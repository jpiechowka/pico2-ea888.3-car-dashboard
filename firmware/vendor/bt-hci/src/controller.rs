// Local 2026-11-02: command ownership, waiter wakeups and receive bounds; see LOCAL_PATCHES.md.
//! HCI controller

use core::cell::{Cell, RefCell};
use core::convert::Infallible;
use core::future::Future;
use core::mem::MaybeUninit;
use core::slice;

use bt_hci_transport::ReadHciError;
use cmd::controller_baseband::Reset;
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_sync::signal::Signal;
use embedded_io::ErrorType;
use futures_intrusive::sync::LocalManualResetEvent;

use crate::cmd::{Cmd, CmdReturnBuf};
use crate::event::{CommandComplete, CommandCompleteWithStatus, CommandStatus, EventKind};
use crate::param::{RemainingBytes, Status};
use crate::transport::Transport;
use crate::{cmd, data, ControllerToHostPacket, FromHciBytes};

pub mod blocking;

/// Trait representing a HCI controller which supports async operations.
pub trait Controller: ErrorType {
    /// Type of the buffer
    type Buffer<'a>;

    /// Allocate a buffer
    /// Returns implementation-specific receive storage or an allocation error.
    fn alloc_buf(&self) -> Result<Self::Buffer<'_>, Self::Error>;

    /// Write ACL data to the controller.
    /// Sends borrowed `packet`; returns transport submission success or error. Cancellation/partial writes follow the
    /// transport.
    fn write_acl_data(
        &self,
        packet: &data::AclPacket,
    ) -> impl Future<Output = Result<(), Self::Error>>;
    /// Write Sync data to the controller.
    /// Sends borrowed `packet`; returns transport submission success or error. Cancellation/partial writes follow the
    /// transport.
    fn write_sync_data(
        &self,
        packet: &data::SyncPacket,
    ) -> impl Future<Output = Result<(), Self::Error>>;
    /// Write Iso data to the controller.
    /// Sends borrowed `packet`; returns transport submission success or error. Cancellation/partial writes follow the
    /// transport.
    fn write_iso_data(
        &self,
        packet: &data::IsoPacket,
    ) -> impl Future<Output = Result<(), Self::Error>>;

    /// Read a valid HCI packet from the controller.
    /// Fills caller `buf` and returns a borrowing packet or transport/decode error. Cancellation may consume input
    /// or dispatch command responses; it does not imply controller reset.
    fn read<'a>(
        &self,
        buf: &'a mut Self::Buffer<'_>,
    ) -> impl Future<Output = Result<ControllerToHostPacket<'a>, Self::Error>>;
}

/// Marker trait for declaring that a controller supports a given HCI command.
pub trait ControllerCmdSync<C: cmd::SyncCmd + ?Sized>: Controller {
    /// Note: Some implementations may require [`Controller::read()`] to be polled for this to return.
    /// Submits borrowed `cmd` and returns its decoded CommandComplete parameters or HCI/transport error.
    /// Admission may wait without a deadline; cancellation semantics belong to the implementation.
    fn exec(
        &self,
        cmd: &C,
    ) -> impl Future<Output = Result<C::Return, cmd::Error<Self::Error>>>;
}

/// Marker trait for declaring that a controller supports a given async HCI command.
pub trait ControllerCmdAsync<C: cmd::AsyncCmd + ?Sized>: Controller {
    /// Note: Some implementations may require [`Controller::read()`] to be polled for this to return.
    /// Submits borrowed `cmd` and returns CommandStatus success or HCI/transport error, not the later procedure result.
    /// Admission may wait without a deadline; cancellation semantics belong to the implementation.
    fn exec(
        &self,
        cmd: &C,
    ) -> impl Future<Output = Result<(), cmd::Error<Self::Error>>>;
}

/// An external Bluetooth controller with communication via [`Transport`] type `T`.
///
/// The controller state holds a number of command slots that can be used
/// to issue commands and await responses from an underlying controller.
///
/// The contract is that before sending a command, a slot is reserved, which
/// returns a signal handle that can be used to await a response.
///
/// Commands and receive dispatch share one executor. Canceling after slot admission quarantines
/// the opcode without retaining the caller buffer until its response or a successful Reset.
/// If no response arrives and all slots are quarantined, reconstruct the controller after resetting
/// its transport/device. A malformed oversized response returns a read error before publication;
/// applications must terminate the host and recover rather than assume command state is synchronized.
pub struct ExternalController<T, const SLOTS: usize> {
    /// Owned transport used for packet submission and receive dispatch on the same executor.
    transport: T,
    /// Fixed command ownership table, response signals and reported controller allowance.
    slots: ControllerState<SLOTS>,
}

impl<T, const SLOTS: usize> ExternalController<T, SLOTS> {
    /// Create a new instance.
    /// Returns a controller owning `transport`, with SLOTS empty slots and one initial command credit.
    /// Does not initialize or reset the device; zero slots admit no commands.
    pub fn new(transport: T) -> Self {
        Self {
            slots: ControllerState::new(),
            transport,
        }
    }
}

impl<T, const SLOTS: usize> ErrorType for ExternalController<T, SLOTS>
where
    T: ErrorType,
{
    /// Underlying transport error, including converted receive-validation errors.
    type Error = T::Error;
}

impl<T, const SLOTS: usize> Controller for ExternalController<T, SLOTS>
where
    T: Transport,
    T::Error: From<ReadHciError<Infallible>>,
{
    /// Fixed 259-byte receive storage; larger legal protocol packets may exceed this adapter's capacity.
    type Buffer<'a> = [u8; 259];

    #[inline]
    /// Returns a fresh zeroed 259-byte buffer; this fixed-array allocation has no error branch.
    fn alloc_buf(&self) -> Result<Self::Buffer<'_>, Self::Error> { Ok([0u8; 259]) }

    /// Submits borrowed ACL `packet` through the transport and returns its result; failure/cancellation may leave
    /// partial output.
    async fn write_acl_data(
        &self,
        packet: &data::AclPacket<'_>,
    ) -> Result<(), Self::Error> {
        self.transport.write(packet).await?;
        Ok(())
    }

    /// Submits borrowed synchronous `packet` and returns the transport result; failure/cancellation may leave partial
    /// output.
    async fn write_sync_data(
        &self,
        packet: &data::SyncPacket<'_>,
    ) -> Result<(), Self::Error> {
        self.transport.write(packet).await?;
        Ok(())
    }

    /// Submits borrowed ISO `packet` and returns the transport result; failure/cancellation may leave partial output.
    async fn write_iso_data(
        &self,
        packet: &data::IsoPacket<'_>,
    ) -> Result<(), Self::Error> {
        self.transport.write(packet).await?;
        Ok(())
    }

    /// Reads into `buf`, dispatching command completions/status internally, then returns the next other packet.
    /// Returns transport/framing/response-capacity errors; no deadline. Cancellation may consume input or complete
    /// commands already dispatched. Returned payload borrows buf until the caller releases it.
    async fn read<'a>(
        &self,
        buf: &'a mut Self::Buffer<'_>,
    ) -> Result<ControllerToHostPacket<'a>, Self::Error> {
        loop {
            {
                // Safety: we will not hold references across loop iterations.
                let buf = unsafe { slice::from_raw_parts_mut(buf as *mut u8, buf.len()) };
                let value = self.transport.read(&mut buf[..]).await?;
                match value {
                    ControllerToHostPacket::Event(ref event) => {
                        match event.kind {
                            EventKind::CommandComplete => {
                                let e =
                                    CommandComplete::from_hci_bytes_complete(event.data).map_err(ReadHciError::from)?;
                                if !e.has_status() {
                                    self.slots.update_allowance(e.num_hci_cmd_pkts as usize);
                                    return Ok(value);
                                }
                                let e: CommandCompleteWithStatus = e.try_into().map_err(ReadHciError::from)?;
                                self.slots.complete(
                                    e.cmd_opcode,
                                    e.status,
                                    e.num_hci_cmd_pkts as usize,
                                    e.return_param_bytes.as_ref(),
                                )?;
                                continue;
                            }
                            EventKind::CommandStatus => {
                                let e =
                                    CommandStatus::from_hci_bytes_complete(event.data).map_err(ReadHciError::from)?;
                                self.slots
                                    .complete(e.cmd_opcode, e.status, e.num_hci_cmd_pkts as usize, &[])?;
                                continue;
                            }
                            _ => return Ok(value),
                        }
                    }
                    _ => return Ok(value),
                }
            }
        }
    }
}

impl<T, const SLOTS: usize> blocking::Controller for ExternalController<T, SLOTS>
where
    T: crate::transport::blocking::Transport,
    T::Error: From<ReadHciError<Infallible>>,
{
    /// Fixed 259-byte receive storage shared by blocking and try-read operations.
    type Buffer<'a> = [u8; 259];

    #[inline]
    /// Returns a fresh zeroed 259-byte array; this implementation cannot return an allocation error.
    fn alloc_buf(&self) -> Result<Self::Buffer<'_>, Self::Error> { Ok([0u8; 259]) }

    /// Retries borrowed ACL `packet` while Busy; returns success or the first transport error. Spins without a
    /// deadline.
    fn write_acl_data(
        &self,
        packet: &data::AclPacket<'_>,
    ) -> Result<(), Self::Error> {
        loop {
            match self.try_write_acl_data(packet) {
                Err(blocking::TryError::Busy) => {}
                Err(blocking::TryError::Error(e)) => return Err(e),
                Ok(r) => return Ok(r),
            }
        }
    }

    /// Retries borrowed synchronous `packet` while Busy; returns success or the first transport error. Spins without a
    /// deadline.
    fn write_sync_data(
        &self,
        packet: &data::SyncPacket<'_>,
    ) -> Result<(), Self::Error> {
        loop {
            match self.try_write_sync_data(packet) {
                Err(blocking::TryError::Busy) => {}
                Err(blocking::TryError::Error(e)) => return Err(e),
                Ok(r) => return Ok(r),
            }
        }
    }

    /// Retries borrowed ISO `packet` while Busy; returns success or the first transport error. Spins without a
    /// deadline.
    fn write_iso_data(
        &self,
        packet: &data::IsoPacket<'_>,
    ) -> Result<(), Self::Error> {
        loop {
            match self.try_write_iso_data(packet) {
                Err(blocking::TryError::Busy) => {}
                Err(blocking::TryError::Error(e)) => return Err(e),
                Ok(r) => return Ok(r),
            }
        }
    }

    /// Fills `buf`, retrying Busy without a deadline; returns a borrowing packet or first transport/decode error.
    /// Command responses consumed while waiting are already published and are not rolled back.
    fn read<'a>(
        &self,
        buf: &'a mut Self::Buffer<'_>,
    ) -> Result<ControllerToHostPacket<'a>, Self::Error> {
        loop {
            // Safety: we will not hold references across loop iterations.
            let buf = unsafe { &mut *(buf as *mut _) };
            match self.try_read(buf) {
                Err(blocking::TryError::Busy) => {}
                Err(blocking::TryError::Error(e)) => return Err(e),
                Ok(r) => return Ok(r),
            }
        }
    }

    /// Attempts borrowed ACL `packet` once; returns transport success, Busy or error, preserving its partial-effect
    /// contract.
    fn try_write_acl_data(
        &self,
        packet: &data::AclPacket<'_>,
    ) -> Result<(), blocking::TryError<Self::Error>> {
        self.transport.write(packet)?;
        Ok(())
    }

    /// Attempts borrowed synchronous `packet` once; returns transport success, Busy or error, preserving its
    /// partial-effect contract.
    fn try_write_sync_data(
        &self,
        packet: &data::SyncPacket<'_>,
    ) -> Result<(), blocking::TryError<Self::Error>> {
        self.transport.write(packet)?;
        Ok(())
    }

    /// Attempts borrowed ISO `packet` once; returns transport success, Busy or error, preserving its partial-effect
    /// contract.
    fn try_write_iso_data(
        &self,
        packet: &data::IsoPacket<'_>,
    ) -> Result<(), blocking::TryError<Self::Error>> {
        self.transport.write(packet)?;
        Ok(())
    }

    /// Reads into `buf` and dispatches command responses until another packet, Busy or an error is returned.
    /// Returned payload borrows buf; completed command side effects remain even if the eventual result is Busy/error.
    fn try_read<'a>(
        &self,
        buf: &'a mut Self::Buffer<'_>,
    ) -> Result<ControllerToHostPacket<'a>, blocking::TryError<Self::Error>> {
        loop {
            {
                // Safety: we will not hold references across loop iterations.
                let buf = unsafe { slice::from_raw_parts_mut(buf as *mut u8, buf.len()) };
                let value = self.transport.read(&mut buf[..])?;
                match value {
                    ControllerToHostPacket::Event(ref event) => {
                        match event.kind {
                            EventKind::CommandComplete => {
                                let e =
                                    CommandComplete::from_hci_bytes_complete(event.data).map_err(ReadHciError::from)?;
                                if !e.has_status() {
                                    self.slots.update_allowance(e.num_hci_cmd_pkts as usize);
                                    return Ok(value);
                                }
                                let e: CommandCompleteWithStatus = e.try_into().map_err(ReadHciError::from)?;
                                self.slots.complete(
                                    e.cmd_opcode,
                                    e.status,
                                    e.num_hci_cmd_pkts as usize,
                                    e.return_param_bytes.as_ref(),
                                )?;
                                continue;
                            }
                            EventKind::CommandStatus => {
                                let e =
                                    CommandStatus::from_hci_bytes_complete(event.data).map_err(ReadHciError::from)?;
                                self.slots
                                    .complete(e.cmd_opcode, e.status, e.num_hci_cmd_pkts as usize, &[])?;
                                continue;
                            }
                            _ => return Ok(value),
                        }
                    }
                    _ => return Ok(value),
                }
            }
        }
    }
}

impl<T, C, const SLOTS: usize> ControllerCmdSync<C> for ExternalController<T, SLOTS>
where
    T: Transport,
    C: cmd::SyncCmd,
    T::Error: From<ReadHciError<Infallible>>,
{
    /// Admits and writes borrowed `cmd`, returning decoded completion parameters or HCI/transport error.
    /// Concurrent receive polling supplies the result. No deadline; cancellation after admission quarantines
    /// the opcode and removes its buffer pointer, even if submission was incomplete.
    async fn exec(
        &self,
        cmd: &C,
    ) -> Result<C::Return, cmd::Error<Self::Error>> {
        let mut retval: C::ReturnBuf = C::ReturnBuf::new();

        // info!("Executing command with opcode {}", C::OPCODE);
        let (slot, idx) = self.slots.acquire(C::OPCODE, retval.as_mut()).await;
        let _d = OnDrop::new(|| {
            self.slots.release_slot(idx);
        });

        self.transport.write(cmd).await.map_err(cmd::Error::Io)?;

        let result = slot.wait().await;
        let return_param_bytes = RemainingBytes::from_hci_bytes_complete(&retval.as_ref()[..result.len]).unwrap();
        let e = CommandCompleteWithStatus {
            num_hci_cmd_pkts: 0,
            status: result.status,
            cmd_opcode: C::OPCODE,
            return_param_bytes,
        };
        let r = e.to_result::<C>().map_err(cmd::Error::Hci)?;
        // info!("Done executing command with opcode {}", C::OPCODE);
        Ok(r)
    }
}

impl<T, C, const SLOTS: usize> ControllerCmdAsync<C> for ExternalController<T, SLOTS>
where
    T: Transport,
    C: cmd::AsyncCmd,
    T::Error: From<ReadHciError<Infallible>>,
{
    /// Admits and writes borrowed `cmd`, returning its status result or HCI/transport error.
    /// Concurrent receive dispatch is required; cancellation after admission quarantines the opcode until
    /// response/reset.
    async fn exec(
        &self,
        cmd: &C,
    ) -> Result<(), cmd::Error<Self::Error>> {
        let (slot, idx) = self.slots.acquire(C::OPCODE, &mut []).await;
        let _d = OnDrop::new(|| {
            self.slots.release_slot(idx);
        });

        self.transport.write(cmd).await.map_err(cmd::Error::Io)?;

        let result = slot.wait().await;
        result.status.to_result()?;
        Ok(())
    }
}

/// Executor-local controller credits and at most SLOTS outstanding or quarantined commands.
struct ControllerState<const SLOTS: usize> {
    /// Latest controller allowance minus admitted commands; no unsubmitted reservation survives a poll.
    permits: Cell<usize>,
    /// Opcode ownership and, only while Pending, the caller's pinned response buffer.
    slots: RefCell<[CommandSlot; SLOTS]>,
    /// One unread terminal result per live slot; reuse follows the owning command's guard drop.
    signals: [Signal<NoopRawMutex, CommandResponse>; SLOTS],
    /// Intrusive caller-owned waiters; one release wakes all without overwriting another task.
    slot_changed: LocalManualResetEvent,
}

/// Completion metadata published after a bounded copy to the caller's response buffer.
struct CommandResponse {
    /// Controller status, or ControllerBusy when a successful Reset invalidates pending work.
    status: Status,
    /// Initialized response prefix length in bytes, no greater than its admitted capacity.
    len: usize,
}

/// Slot lifetime distinguishes a live response buffer from completed or abandoned opcode ownership.
enum CommandSlot {
    /// No command owns this slot.
    Empty,
    /// Pinned caller response slice remains valid until completion or ownership-guard drop.
    Pending {
        /// Unique command opcode retained until result consumption or cancellation cleanup.
        opcode: u16,
        /// Valid pinned caller response slice; accessed only before the ownership guard removes it.
        event: *mut [u8],
    },
    /// Response was copied and signaled once; opcode remains reserved until owner drop.
    Responded {
        /// Opcode reserved while its signaled response is still owned by the command future.
        opcode: u16,
    },
    /// Caller dropped before response; no pointer remains and late replies cannot target a replacement.
    Abandoned {
        /// Quarantined opcode whose late response must not complete a replacement command.
        opcode: u16,
        /// Original response capacity in bytes, retained without a caller pointer.
        capacity: usize,
    },
}

impl<const SLOTS: usize> Default for ControllerState<SLOTS> {
    /// Returns fresh state with no owned slots and one initial command credit.
    fn default() -> Self { Self::new() }
}

impl<const SLOTS: usize> ControllerState<SLOTS> {
    /// Fresh per-slot signal; const use creates independent interior-mutable storage.
    #[allow(clippy::declare_interior_mutable_const)]
    const EMPTY_SIGNAL: Signal<NoopRawMutex, CommandResponse> = Signal::new();
    /// Initial unowned state for each command slot.
    const EMPTY_SLOT: CommandSlot = CommandSlot::Empty;

    /// Starts with the HCI initial one-command credit and no owned slots or waiters.
    /// Returns initialized executor-local state without touching a device.
    fn new() -> Self {
        Self {
            permits: Cell::new(1),
            slots: RefCell::new([Self::EMPTY_SLOT; SLOTS]),
            signals: [Self::EMPTY_SIGNAL; SLOTS],
            slot_changed: LocalManualResetEvent::new(false),
        }
    }

    /// Applies one completion for `op`, returning InvalidValue before any mutation if matched
    /// `data` exceeds the live response slice. Empty/unmatched or duplicate responses never write
    /// caller memory. Reset has no return bytes and releases abandoned opcodes while failing live
    /// pending commands only on successful Reset; already delivered results remain unchanged. Credits are
    /// replaced by the reported allowance. A duplicate unread response is ignored in its entirety,
    /// including its credits, because no second valid command owns that opcode.
    /// Uses controller `status` and absolute `num_hci_command_packets` allowance; returns Ok after
    /// publication or InvalidValue on invalid response size. No caller bytes are written on that error.
    fn complete(
        &self,
        op: cmd::Opcode,
        status: Status,
        num_hci_command_packets: usize,
        data: &[u8],
    ) -> Result<(), ReadHciError<Infallible>> {
        let mut slots = self.slots.borrow_mut();
        if op == Reset::OPCODE && !data.is_empty() {
            return Err(ReadHciError::InvalidValue);
        }
        // A duplicate cannot replace an unconsumed result or repeat Reset's broadcast side effects.
        if slots
            .iter()
            .any(|slot| matches!(slot, CommandSlot::Responded { opcode } if *opcode == op.to_raw()))
        {
            return Ok(());
        }
        // Validate before copying, signaling, broadcasting Reset, or publishing credits.
        for slot in slots.iter() {
            let capacity = match slot {
                CommandSlot::Pending { opcode, event } if *opcode == op.to_raw() => {
                    if event.is_null() {
                        return Err(ReadHciError::InvalidValue);
                    }
                    // Safety: only Pending retains a slice pinned by the command ownership guard.
                    Some(unsafe { (&**event).len() })
                }
                CommandSlot::Abandoned { opcode, capacity } if *opcode == op.to_raw() => Some(*capacity),
                _ => None,
            };
            if capacity.is_some_and(|capacity| data.len() > capacity) {
                return Err(ReadHciError::InvalidValue);
            }
        }
        let reset_succeeded = op == Reset::OPCODE && status == Status::SUCCESS;
        for (idx, slot) in slots.iter_mut().enumerate() {
            match slot {
                CommandSlot::Pending { opcode, event } if *opcode == op.to_raw() => {
                    if !data.is_empty() {
                        // Safety: Pending owns a valid pinned slice and the length was checked above.
                        unsafe { (&mut (**event))[..data.len()].copy_from_slice(data) };
                    }
                    *slot = CommandSlot::Responded { opcode: op.to_raw() };
                    self.signals[idx].signal(CommandResponse {
                        status,
                        len: data.len(),
                    });
                }
                CommandSlot::Abandoned { opcode, .. } if *opcode == op.to_raw() || reset_succeeded => {
                    *slot = CommandSlot::Empty;
                }
                CommandSlot::Pending { opcode, .. } if reset_succeeded => {
                    *slot = CommandSlot::Responded { opcode: *opcode };
                    self.signals[idx].signal(CommandResponse {
                        status: Status::CONTROLLER_BUSY,
                        len: 0,
                    });
                }
                _ => {}
            }
        }
        drop(slots);
        // Publish allowance before waking credit or slot waiters, including a decrease to zero.
        self.update_allowance(num_hci_command_packets);
        Ok(())
    }

    /// Releases completed `idx`, or removes a canceled Pending pointer while quarantining its opcode.
    /// Cancellation cannot retract a transport write; absent a late response, recovery requires a
    /// controller reset with a free slot or reconstruction. Wakes happen after the table borrow ends.
    fn release_slot(
        &self,
        idx: usize,
    ) {
        let mut slots = self.slots.borrow_mut();
        let freed = match slots[idx] {
            CommandSlot::Pending { opcode, event } => {
                // Safety: the guard is dropped before the command's pinned response buffer.
                let capacity = unsafe { (&*event).len() };
                slots[idx] = CommandSlot::Abandoned { opcode, capacity };
                false
            }
            CommandSlot::Responded { .. } => {
                slots[idx] = CommandSlot::Empty;
                true
            }
            _ => false,
        };
        drop(slots);
        if freed {
            self.wake_slot_waiters();
        }
    }

    /// Pulses every registered intrusive waiter. Resetting the event does not erase each waiter's
    /// completed state, and cancellation removes its node without affecting other registrations.
    fn wake_slot_waiters(&self) {
        self.slot_changed.set();
        self.slot_changed.reset();
    }

    /// Replaces the available command allowance with `credits` (0..=255 from an HCI event).
    /// Pulses after publication so every caller rechecks both credit and slot ownership.
    fn update_allowance(
        &self,
        credits: usize,
    ) {
        self.permits.set(credits);
        self.wake_slot_waiters();
    }

    /// Claims a slot and consumes one credit for `op`/pinned `event` in the same poll.
    /// This non-Sync controller runs on one executor: failed admission and the first wait poll
    /// have no intervening await/yield window. No credit is reserved while waiting, so cancellation
    /// and a later lower allowance cannot leave a stale permit. Reset consumes all available credits
    /// (possibly zero), but still requires a free slot and unique opcode.
    /// Returns the admitted response signal and slot index; cancellation while waiting owns no slot or credit.
    /// The caller must keep event valid until its post-admission release guard runs.
    async fn acquire(
        &self,
        op: cmd::Opcode,
        event: *mut [u8],
    ) -> (&Signal<NoopRawMutex, CommandResponse>, usize) {
        loop {
            let available = self.permits.get();
            if op == Reset::OPCODE || available != 0 {
                if let Some(ret) = self.acquire_slot(op, event) {
                    self.permits.set(if op == Reset::OPCODE { 0 } else { available - 1 });
                    return ret;
                }
            }
            self.slot_changed.wait().await;
        }
    }

    /// Synchronously claims one empty slot for unique `op`, retaining pinned `event` until guard drop.
    /// Returns its signal/index, or None without mutation when opcode or capacity is unavailable.
    fn acquire_slot(
        &self,
        op: cmd::Opcode,
        event: *mut [u8],
    ) -> Option<(&Signal<NoopRawMutex, CommandResponse>, usize)> {
        let mut slots = self.slots.borrow_mut();
        // Make sure there are no existing command with this opcode
        for slot in slots.iter() {
            match slot {
                CommandSlot::Pending { opcode, .. }
                | CommandSlot::Responded { opcode }
                | CommandSlot::Abandoned { opcode, .. }
                    if *opcode == op.to_raw() =>
                {
                    return None;
                }
                _ => {}
            }
        }
        // Reserve our slot
        for (idx, slot) in slots.iter_mut().enumerate() {
            if matches!(slot, CommandSlot::Empty) {
                *slot = CommandSlot::Pending {
                    opcode: op.to_raw(),
                    event,
                };
                self.signals[idx].reset();
                return Some((&self.signals[idx], idx));
            }
        }
        None
    }
}

/// A type to delay the drop handler invocation.
#[must_use = "to delay the drop handler invocation to the end of the scope"]
struct OnDrop<F: FnOnce()> {
    /// Initialized one-shot closure moved out and called exactly once by Drop.
    f: MaybeUninit<F>,
}

impl<F: FnOnce()> OnDrop<F> {
    /// Create a new instance.
    /// Returns a guard owning `f`; dropping the guard invokes f, including during unwinding.
    pub(crate) fn new(f: F) -> Self { Self { f: MaybeUninit::new(f) } }
}

impl<F: FnOnce()> Drop for OnDrop<F> {
    /// Moves out and invokes the owned closure exactly once; a closure panic propagates.
    fn drop(&mut self) { unsafe { self.f.as_ptr().read()() } }
}

#[cfg(test)]
/// Fixed transport fixture for status-free unsolicited command-complete forwarding.
mod tests {
    use bt_hci_transport::{PacketKind, PacketToController, PacketToHost};

    use super::*;

    /// Read-only H4 fixture transport; reads repeat the same source and writes panic.
    pub struct TestTransport<'d> {
        /// Immutable H4 bytes parsed anew for each read call.
        pub rx: &'d [u8],
    }

    #[derive(Clone, Copy, Debug, PartialEq)]
    /// Synthetic transport error that discards detailed parser classification.
    pub struct Error;

    impl core::fmt::Display for Error {
        /// Writes the fixture error's Debug representation to `f`; returns the formatter result.
        fn fmt(
            &self,
            f: &mut core::fmt::Formatter<'_>,
        ) -> core::fmt::Result {
            write!(f, "{:?}", self)
        }
    }

    impl core::error::Error for Error {}

    impl From<ReadHciError<Infallible>> for Error {
        /// Returns the synthetic error for any supplied HCI read failure.
        fn from(_: ReadHciError<Infallible>) -> Self { Self }
    }

    impl ErrorType for TestTransport<'_> {
        /// Synthetic parser/transport failure used by this fixed fixture.
        type Error = Error;
    }
    impl embedded_io::Error for Error {
        /// Returns Other because the fixture does not model a physical I/O error kind.
        fn kind(&self) -> embedded_io::ErrorKind { embedded_io::ErrorKind::Other }
    }
    impl Transport for TestTransport<'_> {
        /// Decodes the fixed fixture through the real packet parser, rejecting unused input bytes.
        /// Returns a decoded packet borrowing caller `rx`, or parser/error on trailing bytes; fixture input is not
        /// advanced.
        async fn read<'a, P: PacketToHost<'a>>(
            &self,
            rx: &'a mut [u8],
        ) -> Result<P, Self::Error> {
            let to_read = rx.len().min(self.rx.len());
            let mut reader = &self.rx[..to_read];
            let kind = PacketKind::read(&mut reader)?;
            let pkt = P::read_hci(kind, &mut reader, rx)?;
            if !reader.is_empty() {
                return Err(Error);
            }
            Ok(pkt)
        }

        /// This unsolicited-event fixture never transmits; polling a write is a fixture error.
        /// Ignores `_val` and always panics; this fixture has no successful write return path.
        async fn write<T: PacketToController>(
            &self,
            _val: &T,
        ) -> Result<(), Self::Error> {
            todo!()
        }
    }

    #[futures_test::test]
    /// Checks a status-free opcode-zero completion is forwarded without error; allocation/decode failure panics.
    pub async fn test_can_handle_unsolicited_command_complete() {
        let t = TestTransport {
            rx: &[
                4, 0x0E, 3, // header
                1, 0, 0, // special command
            ],
        };
        let c: ExternalController<_, 10> = ExternalController::new(t);

        let mut buf = c.alloc_buf().unwrap();
        let pkt = c.read(&mut buf).await;
        assert!(pkt.is_ok());
    }
}

#[cfg(test)]
#[path = "controller_regression_tests.rs"]
mod regression_tests;
