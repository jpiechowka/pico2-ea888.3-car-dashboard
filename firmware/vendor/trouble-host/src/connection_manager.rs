//! Single-executor link slots, retained owners, ACL credit/fragment locks and queued ATT/security work.
//! Connecting means controller-established but not yet application-accepted; terminal disconnection
//! is distinct from requested/in-flight close. Refcounts prevent slot reuse, and RefCell/NoopRawMutex
//! provide no cross-thread exclusion. Packet/event queues are bounded and may reject delivery.

use core::cell::{Cell, Ref, RefCell, RefMut};
use core::future::poll_fn;
#[cfg(feature = "security")]
use core::future::Future;
use core::num::NonZeroU16;
use core::task::{Context, Poll};

use bt_hci::param::{AddrKind, BdAddr, ConnHandle, DisconnectReason, LeConnRole, Status};
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_sync::channel::Channel;
use embassy_sync::waitqueue::WakerRegistration;
#[cfg(feature = "security")]
use embassy_time::TimeoutError;

#[cfg(feature = "subrating")]
use crate::connection::SubratingParams;
use crate::connection::{ConnParams, Connection, ConnectionEvent, SecurityLevel};
use crate::host::EventHandler;
use crate::pdu::Pdu;
use crate::prelude::sar::PacketReassembly;
#[cfg(feature = "security")]
use crate::security_manager::{SecurityEventData, SecurityManager};
use crate::{config, Address, Error, Identity, PacketPool};

/// ATT MTU before negotiation, 23 bytes including ATT opcode/parameters.
pub(crate) const DEFAULT_ATT_MTU: u16 = 23;

/// Resolvable private addresses used on a connection.
///
/// Holds the Resolvable Private Addresses (RPAs) for a connection that were
/// resolved by the controller.
#[cfg(feature = "security")]
#[derive(Debug, Default, Clone, Copy)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub(crate) struct ResolvablePrivateAddrs {
    /// The local resolvable private address used on this connection, if any.
    pub(crate) local: Option<BdAddr>,
    /// The peer's resolvable private address used on this connection, if any.
    pub(crate) peer: Option<BdAddr>,
}

#[cfg(feature = "security")]
impl ResolvablePrivateAddrs {
    /// Returns empty resolved-private-address metadata; neither local nor peer RPA is recorded.
    pub(crate) const fn none() -> Self {
        Self {
            local: None,
            peer: None,
        }
    }
}

#[cfg(feature = "att-queued-writes")]
/// One contiguous queued ATT write for a single attribute; handle 0 denotes empty. Backing bytes remain after clear.
pub(crate) struct PrepareWriteState {
    /// Nonzero attribute handle; zero denotes no queued write.
    pub(crate) handle: u16,
    /// Starting attribute byte offset of the first admitted chunk.
    pub(crate) offset: u16,
    /// Initialized queued byte count; excludes attribute starting offset.
    pub(crate) len: u16,
    /// Bounded staged value bytes; only the len prefix belongs to the active write.
    pub(crate) buf: [u8; config::PREPARE_WRITE_BUFFER_SIZE],
}

#[cfg(feature = "att-queued-writes")]
impl PrepareWriteState {
    /// Returns an empty queued-write slot with zeroed backing bytes.
    const fn new() -> Self {
        Self {
            handle: 0,
            offset: 0,
            len: 0,
            buf: [0; config::PREPARE_WRITE_BUFFER_SIZE],
        }
    }

    /// Appends `value` at attribute byte `offset` for nonzero `handle`; returns INVALID_HANDLE or PREPARE_QUEUE_FULL for identity/contiguity/capacity failure. Rejection preserves metadata and bytes. Offset addition is checked; configured buffer length must fit u16.
    fn queue(&mut self, handle: u16, offset: u16, value: &[u8]) -> Result<(), crate::att::AttErrorCode> {
        if handle == 0 {
            return Err(crate::att::AttErrorCode::INVALID_HANDLE);
        }
        let empty = self.handle == 0;
        if !empty && (self.handle != handle || self.offset.checked_add(self.len) != Some(offset)) {
            return Err(crate::att::AttErrorCode::PREPARE_QUEUE_FULL);
        }

        let buf_offset = if empty { 0 } else { usize::from(self.len) };
        if value.len() > self.buf.len() - buf_offset {
            return Err(crate::att::AttErrorCode::PREPARE_QUEUE_FULL);
        }
        let end = buf_offset + value.len();
        // Publish only a fully admitted chunk. A rejected first chunk must not
        // reserve this handle or change the starting offset.
        if empty {
            self.handle = handle;
            self.offset = offset;
        }
        self.buf[buf_offset..end].copy_from_slice(value);
        self.len = end as u16;
        Ok(())
    }

    /// Clears handle/length so the next chunk starts a new write; does not erase old bytes or reset offset.
    fn clear(&mut self) {
        self.handle = 0;
        self.len = 0;
    }
}

#[cfg(all(test, feature = "att-queued-writes"))]
/// Queued-write admission tests for transactional failure, offset overflow and handle 0 sentinel.
mod prepare_tests {
    use super::PrepareWriteState;
    use crate::att::AttErrorCode;

    #[test]
    /// An oversized first chunk reserves nothing; later valid same-handle contiguous chunks append normally.
    fn rejected_first_chunk_preserves_empty_queue_and_later_valid_write() {
        let mut queue = PrepareWriteState::new();
        let oversized = [0u8; crate::config::PREPARE_WRITE_BUFFER_SIZE + 1];
        assert_eq!(queue.queue(1, 7, &oversized), Err(AttErrorCode::PREPARE_QUEUE_FULL));
        assert_eq!((queue.handle, queue.offset, queue.len), (0, 0, 0));
        assert_eq!(queue.queue(2, 0, &[1, 2]), Ok(()));
        assert_eq!(queue.queue(2, 2, &[3]), Ok(()));
        assert_eq!(&queue.buf[..usize::from(queue.len)], &[1, 2, 3]);
    }

    #[test]
    /// Wrapped offsets and a different handle reject without replacing the pending bytes/identity.
    fn offset_wrap_and_wrong_handle_preserve_queued_bytes() {
        let mut queue = PrepareWriteState::new();
        assert_eq!(queue.queue(1, u16::MAX, &[7]), Ok(()));
        assert_eq!(queue.queue(1, 0, &[8]), Err(AttErrorCode::PREPARE_QUEUE_FULL));
        assert_eq!(queue.queue(2, 0, &[9]), Err(AttErrorCode::PREPARE_QUEUE_FULL));
        assert_eq!(
            (queue.handle, queue.offset, queue.len, queue.buf[0]),
            (1, u16::MAX, 1, 7)
        );
    }

    #[test]
    /// Handle0 cannot reserve empty state; zero-length valid chunks retain identity and clear permits another handle.
    fn zero_handle_cannot_alias_an_empty_queue() {
        let mut queue = PrepareWriteState::new();
        assert_eq!(queue.queue(0, 0, &[1]), Err(AttErrorCode::INVALID_HANDLE));
        assert_eq!((queue.handle, queue.len), (0, 0));
        assert_eq!(queue.queue(1, 0, &[]), Ok(()));
        assert_eq!(queue.queue(1, 0, &[2]), Ok(()));
        assert_eq!(queue.len, 1);
        queue.clear();
        assert_eq!(queue.queue(2, 3, &[4]), Ok(()));
    }
}

/// Role-specific accept waiters, close waiter and default per-link controller ACL credit budget.
struct State {
    /// Single central accept waiter.
    central_waker: WakerRegistration,
    /// Single peripheral accept waiter.
    peripheral_waker: WakerRegistration,
    /// Runner waiter for newly requested local closes.
    disconnect_waker: WakerRegistration,
    /// Controller ACL packet budget copied into newly established slots.
    default_link_credits: usize,
}

/// Bounded single-executor application-event queue; full queues can lose attempted notifications.
type EventChannel = Channel<NoopRawMutex, ConnectionEvent, { config::CONNECTION_EVENT_QUEUE_SIZE }>;
/// Bounded single-executor ATT packet queue owning pool storage until consumption/drop.
type GattChannel<P> = Channel<NoopRawMutex, Pdu<P>, { config::L2CAP_RX_QUEUE_SIZE }>;

#[cfg(feature = "gatt")]
#[derive(Clone, Copy, PartialEq, Eq)]
/// Single in-flight ATT request correlation; response ingress marks Received before the client dequeues it.
enum AttTransaction {
    /// No request currently owns response correlation.
    Idle,
    /// A submitted request awaits one matching response.
    Pending(
        /// Request opcode; expected normal response is opcode+1.
        u8,
    ),
    /// Matching response admitted at ingress, awaiting owner consumption/cleanup.
    Received,
}

/// Borrowed fixed link table, shared outbound queue and optional shared security manager; handles retain table slots.
pub(crate) struct ConnectionManager<'d, P: PacketPool> {
    /// Role waiters and default ACL budget under runtime borrow checking.
    state: RefCell<State>,
    /// Host-owned fixed slots; refcounts retain identities across terminal close.
    connections: &'d RefCell<[ConnectionStorage<P::Packet>]>,
    /// Shared queue of retained slot indices and owned packets. Each entry owns one transferred I/O count; dequeue restores its RAII lease and manager drop drains leftovers.
    outbound: Channel<NoopRawMutex, (usize, Pdu<P::Packet>), { config::L2CAP_TX_QUEUE_SIZE }>,
    #[cfg(feature = "security")]
    /// Shared pairing state and bounded bond store for this host.
    pub(crate) security_manager: SecurityManager<'d>,
}

impl<'d, P: PacketPool> ConnectionManager<'d, P> {
    /// Returns a manager borrowing initialized `connections` and, with security, bounded `bond_storage`. Does not reset slots/bonds; outbound queue and initial ACL budget start empty/zero.
    pub(crate) fn new(
        connections: &'d RefCell<[ConnectionStorage<P::Packet>]>,
        #[cfg(feature = "security")] bond_storage: &'d RefCell<heapless::VecView<crate::BondInformation>>,
    ) -> Self {
        Self {
            state: RefCell::new(State {
                central_waker: WakerRegistration::new(),
                peripheral_waker: WakerRegistration::new(),
                disconnect_waker: WakerRegistration::new(),
                default_link_credits: 0,
            }),
            connections,
            outbound: Channel::new(),
            #[cfg(feature = "security")]
            security_manager: SecurityManager::new(bond_storage),
        }
    }

    /// Returns shared local slot `index`; invalid index or conflicting mutable borrow panics.
    pub(crate) fn connection(&self, index: u8) -> Ref<'_, ConnectionStorage<P::Packet>> {
        Ref::map(self.connections.borrow(), |x| &x[index as usize])
    }

    /// Returns exclusive local slot `index`; invalid index or any conflicting table borrow panics.
    pub(crate) fn connection_mut(&self, index: u8) -> RefMut<'_, ConnectionStorage<P::Packet>> {
        RefMut::map(self.connections.borrow_mut(), |x| &mut x[index as usize])
    }

    /// Returns a shared Connecting/Connected slot matching controller `handle`, otherwise None; borrow conflicts panic.
    fn connection_by_handle(&self, handle: ConnHandle) -> Option<Ref<'_, ConnectionStorage<P::Packet>>> {
        Ref::filter_map(self.connections.borrow(), |connections| {
            for storage in connections.iter() {
                if storage.handle == handle
                    && matches!(storage.state, ConnectionState::Connected | ConnectionState::Connecting)
                {
                    return Some(storage);
                }
            }
            None
        })
        .ok()
    }

    /// Returns an exclusive Connecting/Connected slot matching controller `handle`, otherwise None; borrow conflicts panic.
    fn connection_by_handle_mut(&self, handle: ConnHandle) -> Option<RefMut<'_, ConnectionStorage<P::Packet>>> {
        RefMut::filter_map(self.connections.borrow_mut(), |connections| {
            for storage in connections.iter_mut() {
                if storage.handle == handle
                    && matches!(storage.state, ConnectionState::Connected | ConnectionState::Connecting)
                {
                    return Some(storage);
                }
            }
            None
        })
        .ok()
    }

    /// Returns establishment role for retained local `index`; invalid index/borrow conflict panics.
    pub(crate) fn role(&self, index: u8) -> LeConnRole {
        self.connection(index).role
    }

    /// Returns the role for live controller `handle`, or None for absent/closing links.
    pub(crate) fn role_by_handle(&self, handle: ConnHandle) -> Option<LeConnRole> {
        self.connection_by_handle(handle).map(|connection| connection.role)
    }

    /// Returns controller handle for retained local `index`; invalid index/borrow conflict panics.
    pub(crate) fn handle(&self, index: u8) -> ConnHandle {
        self.connection(index).handle
    }

    /// Returns local lifecycle state for retained local `index`; invalid index/borrow conflict panics.
    pub(crate) fn state(&self, index: u8) -> ConnectionState {
        self.connection(index).state
    }

    /// Returns whether local `index` is terminally Disconnected, not merely awaiting close.
    pub(crate) fn is_disconnected(&self, index: u8) -> bool {
        self.connection(index).state == ConnectionState::Disconnected
    }

    /// Returns true when every slot is terminally Disconnected; retained references may still prevent reuse.
    pub(crate) fn all_disconnected(&self) -> bool {
        self.connections
            .borrow()
            .iter()
            .all(|entry| entry.state == ConnectionState::Disconnected)
    }

    /// Returns true only for accepted Connected local `index`; Connecting is false.
    pub(crate) fn is_connected(&self, index: u8) -> bool {
        self.connection(index).state == ConnectionState::Connected
    }

    /// Returns the next event for local `index`, waiting without a link-state exit or deadline; pending cancellation consumes nothing.
    pub(crate) async fn next(&self, index: u8) -> ConnectionEvent {
        poll_fn(|cx| self.connection(index).events.poll_receive(cx)).await
    }

    #[cfg(feature = "gatt")]
    /// Returns the next server ATT packet for local `index`, without a link-state exit; caller concurrently observes terminal events.
    pub(crate) async fn next_gatt(&self, index: u8) -> Pdu<P::Packet> {
        poll_fn(|cx| self.connection(index).gatt.poll_receive(cx)).await
    }

    /// Queues `event` for local `index`, waiting for capacity; cancellation before enqueue drops it. Readiness-to-insertion invariant failure panics.
    pub(crate) async fn post_event(&self, index: u8, event: ConnectionEvent) {
        poll_fn(|cx| self.connection(index).events.poll_ready_to_send(cx)).await;
        self.connection(index).events.try_send(event).unwrap();
    }

    /// A subrate factor of 1 means every connection event is used, the connection is not subrated.
    #[cfg(feature = "subrating")]
    /// Returns subrating metadata for `subrate_factor`>1 with `continuation_number` active events; factors 0/1 return None.
    fn subrating(subrate_factor: u16, continuation_number: u16) -> Option<SubratingParams> {
        (subrate_factor > 1).then_some(SubratingParams {
            subrate_factor,
            continuation_number,
        })
    }

    /// Posts `event` for accepted `handle`; returns NotFound outside Connected or OutOfMemory if full. Parameter/subrate observations update before queue admission, so a rejected notification can still update stored state.
    pub(crate) fn post_handle_event(&self, handle: ConnHandle, event: ConnectionEvent) -> Result<(), Error> {
        for entry in self.connections.borrow_mut().iter_mut() {
            if entry.state == ConnectionState::Connected && handle == entry.handle {
                match event {
                    ConnectionEvent::ConnectionParamsUpdated {
                        conn_interval,
                        peripheral_latency,
                        supervision_timeout,
                    } => {
                        // Changing the connection interval resets subrating.
                        #[cfg(feature = "subrating")]
                        if conn_interval != entry.params.conn_interval {
                            entry.subrating_params = None;
                        }
                        entry.params = ConnParams {
                            conn_interval,
                            peripheral_latency,
                            supervision_timeout,
                        }
                    }
                    // The subrate procedure leaves the connection interval alone.
                    #[cfg(feature = "subrating")]
                    ConnectionEvent::SubratingParamsUpdated {
                        subrate_factor,
                        peripheral_latency,
                        continuation_number,
                        supervision_timeout,
                    } => {
                        entry.params.peripheral_latency = peripheral_latency;
                        entry.params.supervision_timeout = supervision_timeout;
                        entry.subrating_params = Self::subrating(subrate_factor, continuation_number);
                    }
                    ConnectionEvent::ConnectionRateChanged {
                        conn_interval,
                        subrate_factor,
                        peripheral_latency,
                        continuation_number,
                        supervision_timeout,
                    } => {
                        entry.params = ConnParams {
                            conn_interval,
                            peripheral_latency,
                            supervision_timeout,
                        };
                        #[cfg(feature = "subrating")]
                        {
                            entry.subrating_params = Self::subrating(subrate_factor, continuation_number);
                        }
                    }
                    _ => {}
                }

                entry.events.try_send(event).map_err(|_| Error::OutOfMemory)?;
                return Ok(());
            }
        }
        Err(Error::NotFound)
    }

    #[cfg(feature = "gatt")]
    /// Queues owned server ATT `pdu` for Connecting/Connected `handle`; returns NotFound/full-queue OutOfMemory and drops rejected storage.
    pub(crate) fn post_gatt(&self, handle: ConnHandle, pdu: Pdu<P::Packet>) -> Result<(), Error> {
        for entry in self.connections.borrow().iter() {
            if matches!(entry.state, ConnectionState::Connecting | ConnectionState::Connected) && handle == entry.handle
            {
                entry.gatt.try_send(pdu).map_err(|_| Error::OutOfMemory)?;
                return Ok(());
            }
        }
        Err(Error::NotFound)
    }

    #[cfg(feature = "gatt")]
    /// Queues owned client ATT `pdu` for Connecting/Connected `handle`; returns NotFound/full-queue OutOfMemory and drops rejected storage.
    pub(crate) fn post_gatt_client(&self, handle: ConnHandle, pdu: Pdu<P::Packet>) -> Result<(), Error> {
        for entry in self.connections.borrow().iter() {
            if matches!(entry.state, ConnectionState::Connecting | ConnectionState::Connected) && handle == entry.handle
            {
                entry.gatt_client.try_send(pdu).map_err(|_| Error::OutOfMemory)?;
                return Ok(());
            }
        }
        Err(Error::NotFound)
    }

    #[cfg(feature = "gatt")]
    /// Returns exclusive client ownership for accepted local `index`, Busy if already owned, or Disconnected; successful caller must release.
    pub(crate) fn acquire_gatt_client(&self, index: u8) -> Result<(), Error> {
        let mut storage = self.connection_mut(index);
        if storage.state != ConnectionState::Connected {
            return Err(Error::Disconnected);
        }
        if storage.gatt_client_owned {
            return Err(Error::Busy);
        }
        storage.gatt_client_owned = true;
        Ok(())
    }

    #[cfg(feature = "gatt")]
    /// Clears client ownership at local `index`; does not change request correlation or wake a waiter.
    pub(crate) fn release_gatt_client(&self, index: u8) {
        self.connection_mut(index).gatt_client_owned = false;
    }

    #[cfg(feature = "gatt")]
    /// Returns Ok after reserving local `index` request `opcode`; rejects closed links or nonidle correlation with Disconnected/Busy.
    pub(crate) fn begin_att_request(&self, index: u8, opcode: u8) -> Result<(), Error> {
        let mut storage = self.connection_mut(index);
        if storage.state != ConnectionState::Connected {
            return Err(Error::Disconnected);
        }
        if storage.att_transaction != AttTransaction::Idle {
            return Err(Error::Busy);
        }
        storage.att_transaction = AttTransaction::Pending(opcode);
        Ok(())
    }

    #[cfg(feature = "gatt")]
    /// Resets local `index` ATT correlation to Idle after owner cleanup; queued responses are not cleared here.
    pub(crate) fn finish_att_request(&self, index: u8) {
        self.connection_mut(index).att_transaction = AttTransaction::Idle;
    }

    // Validate at ingress, not when the client task eventually polls its queue.
    // The slot remains busy until the sole request owner consumes the response.
    #[cfg(feature = "gatt")]
    /// Validates ingress response `bytes` against pending request on live `handle`; returns Disconnected/UnexpectedGattResponse on mismatch. Exact five-byte error responses must echo request opcode; normal response opcode is request+1. Success marks Received, rejecting duplicates until owner cleanup.
    pub(crate) fn accept_att_response(&self, handle: ConnHandle, bytes: &[u8]) -> Result<(), Error> {
        let mut storage = self.connection_by_handle_mut(handle).ok_or(Error::Disconnected)?;
        let AttTransaction::Pending(request) = storage.att_transaction else {
            return Err(Error::UnexpectedGattResponse);
        };
        let matches = match bytes {
            [crate::att::ATT_ERROR_RSP, opcode, _, _, _] => *opcode == request,
            [opcode, ..] => request.checked_add(1) == Some(*opcode),
            [] => false,
        };
        if !matches {
            return Err(Error::UnexpectedGattResponse);
        }
        storage.att_transaction = AttTransaction::Received;
        Ok(())
    }

    #[cfg(feature = "gatt")]
    /// Returns an owned queued client response for local `index`, or None outside Connecting/Connected. Registers one waiter; pending cancellation leaves the queue unchanged.
    pub(crate) async fn next_gatt_client(&self, index: u8) -> Option<Pdu<P::Packet>> {
        poll_fn(|cx| {
            let mut connections = self.connections.borrow_mut();
            let storage = &mut connections[index as usize];
            if !matches!(storage.state, ConnectionState::Connecting | ConnectionState::Connected) {
                return Poll::Ready(None);
            }
            storage.gatt_client_waker.register(cx.waker());
            match storage.gatt_client.poll_receive(cx) {
                core::task::Poll::Ready(pdu) => core::task::Poll::Ready(Some(pdu)),
                core::task::Poll::Pending if storage.state == ConnectionState::Disconnected => {
                    core::task::Poll::Ready(None)
                }
                core::task::Poll::Pending => core::task::Poll::Pending,
            }
        })
        .await
    }

    #[cfg(feature = "gatt")]
    /// Returns exclusive indication ownership for accepted local `index`, waiting when busy or returning Disconnected. Pending cancellation owns nothing; successful acquisition requires release.
    pub(crate) async fn acquire_indication_slot(&self, index: u8) -> Result<(), Error> {
        poll_fn(|cx| {
            let mut connections = self.connections.borrow_mut();
            let storage = &mut connections[index as usize];
            if storage.state != ConnectionState::Connected {
                return Poll::Ready(Err(Error::Disconnected));
            }
            if !storage.indication_in_flight {
                storage.indication_in_flight = true;
                storage.indication_cfm_received = false;
                return Poll::Ready(Ok(()));
            }
            storage.indication_slot_waker.register(cx.waker());
            Poll::Pending
        })
        .await
    }

    #[cfg(feature = "gatt")]
    /// Clears local `index` indication ownership and confirmation, then wakes the next slot waiter.
    pub(crate) fn release_indication_slot(&self, index: u8) {
        let mut connections = self.connections.borrow_mut();
        let storage = &mut connections[index as usize];
        storage.indication_in_flight = false;
        storage.indication_cfm_received = false;
        storage.indication_slot_waker.wake();
    }

    #[cfg(feature = "gatt")]
    /// Returns Ok after consuming local `index` confirmation latch, or Disconnected. Pending cancellation keeps slot ownership and leaves any future confirmation available.
    pub(crate) async fn wait_indication_confirmation(&self, index: u8) -> Result<(), Error> {
        poll_fn(|cx| {
            let mut connections = self.connections.borrow_mut();
            let storage = &mut connections[index as usize];
            if storage.state != ConnectionState::Connected {
                return Poll::Ready(Err(Error::Disconnected));
            }
            if storage.indication_cfm_received {
                storage.indication_cfm_received = false;
                return Poll::Ready(Ok(()));
            }
            storage.indication_cfm_waker.register(cx.waker());
            Poll::Pending
        })
        .await
    }

    #[cfg(feature = "gatt")]
    /// Latches and wakes confirmation only for accepted live `h` with an indication in flight; unrelated confirmations are ignored.
    pub(crate) fn signal_indication_confirmation(&self, h: ConnHandle) {
        if let Some(mut entry) = self.connection_by_handle_mut(h) {
            if entry.state == ConnectionState::Connected && entry.indication_in_flight {
                entry.indication_cfm_received = true;
                entry.indication_cfm_waker.wake();
            }
        }
    }

    /// Returns retained identity address/type for retained local `index`; invalid index/borrow conflict panics.
    pub(crate) fn peer_address(&self, index: u8) -> Address {
        self.connection(index).peer_identity.addr
    }

    /// Returns retained identity address and optional resolving key for retained local `index`; invalid index/borrow conflict panics.
    pub(crate) fn peer_identity(&self, index: u8) -> Identity {
        self.connection(index).peer_identity
    }

    /// Returns latest stored interval/latency/timeout for retained local `index`; invalid index/borrow conflict panics.
    pub(crate) fn params(&self, index: u8) -> ConnParams {
        self.connection(index).params
    }

    #[cfg(feature = "subrating")]
    /// Returns stored optional subrating parameters for retained local `index`; invalid index/borrow conflict panics.
    pub(crate) fn subrating_params(&self, index: u8) -> Option<SubratingParams> {
        self.connection(index).subrating_params
    }

    /// Stores ATT `mtu` bytes at local `index`; zero resets unexchanged state. Internal caller validates protocol limits.
    pub(crate) fn set_att_mtu(&self, index: u8, mtu: u16) {
        self.connection_mut(index).att_mtu = NonZeroU16::new(mtu);
    }

    /// Sets local `index` listener flag to `listening`; enabling an already-claimed listener panics before mutation.
    pub(crate) fn set_l2cap_listening(&self, index: u8, listening: bool) {
        let mut storage = self.connection_mut(index);
        assert!(!listening || !storage.l2cap_listening, "L2CAP listener already active");
        storage.l2cap_listening = listening;
    }

    /// Requests close with `reason` for local Connecting/Connected `index`, waking runner, active ACL sender and GATT client waiters; other phases are unchanged.
    pub(crate) fn request_disconnect(&self, index: u8, reason: DisconnectReason) {
        let entry = &mut self.connection_mut(index);
        if matches!(entry.state, ConnectionState::Connecting | ConnectionState::Connected) {
            entry.state = ConnectionState::DisconnectRequest(reason);
            entry.link_credit_waker.wake();
            #[cfg(feature = "gatt")]
            entry.gatt_client_waker.wake();
            self.state.borrow_mut().disconnect_waker.wake();
        }
    }

    /// Retains a clone of the first ACL ingress `error` for live `handle`; later errors or absent/closing links do not replace it.
    pub(crate) fn record_local_disconnect_error(&self, handle: ConnHandle, error: &Error) {
        if let Some(mut entry) = self.connection_by_handle_mut(handle) {
            if entry.local_disconnect_error.is_none() {
                entry.local_disconnect_error = Some(error.clone());
            }
        }
    }

    /// Requests live `handle` close with `reason`, waking runner/client/ACL sender; absent or already-closing handles are ignored.
    pub(crate) fn request_handle_disconnect(&self, handle: ConnHandle, reason: DisconnectReason) {
        if let Some(mut entry) = self.connection_by_handle_mut(handle) {
            if matches!(entry.state, ConnectionState::Connecting | ConnectionState::Connected) && handle == entry.handle
            {
                entry.state = ConnectionState::DisconnectRequest(reason);
                entry.link_credit_waker.wake();
                #[cfg(feature = "gatt")]
                entry.gatt_client_waker.wake();
                self.state.borrow_mut().disconnect_waker.wake();
            }
        }
    }

    /// Quarantines all controller-established but unaccepted central links and wakes runner/ACL sender; returns whether any were found. Prevents a later address-matched attempt adopting a canceled result.
    pub(crate) fn cancel_unaccepted_central(&self) -> bool {
        let mut found = false;
        for entry in self.connections.borrow_mut().iter_mut() {
            if entry.role == LeConnRole::Central && entry.state == ConnectionState::Connecting {
                entry.state = ConnectionState::DisconnectRequest(DisconnectReason::RemoteUserTerminatedConn);
                entry.link_credit_waker.wake();
                #[cfg(feature = "gatt")]
                entry.gatt_client_waker.wake();
                found = true;
            }
        }
        if found {
            self.state.borrow_mut().disconnect_waker.wake();
        }
        found
    }

    /// Returns first pending local close request, or Pending; optional `cx` registers the runner waiter. Caller must issue/confirm before polling again; confirmation is not terminal controller disconnection.
    pub(crate) fn poll_disconnecting<'m>(
        &'m self,
        cx: Option<&mut Context<'_>>,
    ) -> Poll<DisconnectRequest<'m, P::Packet>> {
        let mut state = self.state.borrow_mut();
        if let Some(cx) = cx {
            state.disconnect_waker.register(cx.waker());
        }
        core::mem::drop(state);
        for (idx, storage) in self.connections.borrow().iter().enumerate() {
            if let ConnectionState::DisconnectRequest(reason) = storage.state {
                return Poll::Ready(DisconnectRequest {
                    index: idx,
                    handle: storage.handle,
                    reason,
                    connections: self.connections,
                });
            }
        }
        Poll::Pending
    }

    /// Returns a newly retained owner for accepted controller `h`, or None; reference-count saturation panics.
    pub(crate) fn get_connected_handle(&'d self, h: ConnHandle) -> Option<Connection<'d, P>> {
        for (index, storage) in self.connections.borrow_mut().iter_mut().enumerate() {
            if storage.handle == h && storage.state == ConnectionState::Connected {
                storage.inc_ref();
                return Some(Connection::new(index as u8, self));
            }
        }
        None
    }

    /// Returns a live iterator over accepted links; each yielded handle increments its slot reference. Later state changes affect subsequent yields.
    pub(crate) fn connections(&'d self) -> ConnectedIter<'d, P> {
        ConnectedIter {
            manager: self,
            index: 0,
            len: self.connections.as_ptr().len(),
        }
    }

    /// Returns a retained accepted link whose identity matches `peer_address` (including supported address resolution), or None; reference saturation panics.
    pub(crate) fn get_connection_by_peer_address(&'d self, peer_address: Address) -> Option<Connection<'d, P>> {
        for (index, storage) in self.connections.borrow_mut().iter_mut().enumerate() {
            if storage.state == ConnectionState::Connected && storage.peer_identity.match_address(&peer_address) {
                storage.inc_ref();
                return Some(Connection::new(index as u8, self));
            }
        }
        None
    }

    /// Returns callback `f` result under an exclusive borrow of live `h`, or Disconnected; includes Connecting and Connected. Reentrant table borrowing from f panics.
    pub(crate) fn with_connected_handle<F: FnOnce(&mut ConnectionStorage<P::Packet>) -> Result<R, Error>, R>(
        &self,
        h: ConnHandle,
        f: F,
    ) -> Result<R, Error> {
        self.connection_by_handle_mut(h)
            .map(|mut c| f(&mut *c))
            .unwrap_or(Err(Error::Disconnected))
    }

    /// Counts one ACL receive for live `h`, returning Ok or Disconnected; metrics-disabled builds still validate identity.
    pub(crate) fn received(&self, h: ConnHandle) -> Result<(), Error> {
        self.with_connected_handle(h, |storage| {
            #[cfg(feature = "connection-metrics")]
            storage.metrics.received(1);
            Ok(())
        })
    }

    /// Returns whether controller `h` is Connecting or Connected; accepted application ownership is not required.
    pub(crate) fn is_handle_connected(&self, h: ConnHandle) -> bool {
        self.with_connected_handle(h, |_storage| Ok(())).is_ok()
    }

    /// Returns the listener flag for live `h`, or false outside Connecting/Connected.
    pub(crate) fn is_l2cap_listening(&self, h: ConnHandle) -> bool {
        self.with_connected_handle(h, |storage| Ok(storage.l2cap_listening))
            .unwrap_or(false)
    }

    /// Returns callback `f` result under exclusive access to live `h` reassembly, or Disconnected; reentrant table mutation panics.
    pub(crate) fn reassembly<F: FnOnce(&mut PacketReassembly<P::Packet>) -> Result<R, Error>, R>(
        &self,
        h: ConnHandle,
        f: F,
    ) -> Result<R, Error> {
        self.with_connected_handle(h, |storage| f(&mut storage.reassembly))
    }

    /// Applies terminal `reason` to nonterminal `h`; returns NotFound if absent/already terminal. Clears reassembly/ATT queues/locks, wakes blocked operations, retains terminal reason despite full event queue, and resets security/metrics. References and identity remain until reuse.
    pub(crate) fn disconnected(&self, h: ConnHandle, reason: Status) -> Result<(), Error> {
        for (idx, storage) in self.connections.borrow_mut().iter_mut().enumerate() {
            if h == storage.handle && storage.state != ConnectionState::Disconnected {
                storage.state = ConnectionState::Disconnected;
                storage.disconnect_reason = Some(reason);
                storage.reassembly.clear();
                storage.acl_send_locked = false;
                storage.link_credit_waker.wake();
                storage.acl_send_lock_waker.wake();
                let _ = storage.events.try_send(ConnectionEvent::Disconnected { reason });
                #[cfg(feature = "gatt")]
                {
                    storage.gatt.clear();
                    storage.gatt_client.clear();
                    storage.gatt_client_waker.wake();
                    storage.indication_in_flight = false;
                    storage.indication_cfm_received = false;
                    storage.indication_slot_waker.wake();
                    storage.indication_cfm_waker.wake();
                }
                #[cfg(feature = "connection-metrics")]
                storage.metrics.reset();
                #[cfg(feature = "security")]
                {
                    storage.security_level = SecurityLevel::NoEncryption;
                    storage.bondable = false;
                    storage.oob_available = false;
                    #[cfg(feature = "legacy-pairing")]
                    {
                        storage.encryption_key_len = 0;
                    }
                    self.security_manager.disconnect(storage);
                }
                #[cfg(feature = "att-queued-writes")]
                storage.prepare_write.clear();
                storage.l2cap_listening = false;
                return Ok(());
            }
        }
        warn!("[link][disconnect] connection handle {:?} not found", h);
        Err(Error::NotFound)
    }

    /// Records controller-established `handle`, `peer_addr`, `role` and `params` without resolved private-address metadata. Returns slot admission result; application acceptance is a later step.
    pub(crate) fn connect(
        &self,
        handle: ConnHandle,
        peer_addr: Address,
        role: LeConnRole,
        params: ConnParams,
    ) -> Result<(), Error> {
        self.connect_with_rpas(
            handle,
            peer_addr,
            role,
            params,
            #[cfg(feature = "security")]
            ResolvablePrivateAddrs::none(),
        )
    }

    /// Records `handle`, `peer_addr`, `role`, `params` and optional security `resolvable_addrs` in the first Disconnected zero-reference slot. Returns NotFound when full. Resolves stored identity/bond, resets pending work and wakes role acceptance; published phase is Connecting.
    pub(crate) fn connect_with_rpas(
        &self,
        handle: ConnHandle,
        peer_addr: Address,
        role: LeConnRole,
        params: ConnParams,
        #[cfg(feature = "security")] resolvable_addrs: ResolvablePrivateAddrs,
    ) -> Result<(), Error> {
        let mut state = self.state.borrow_mut();
        let default_credits = state.default_link_credits;
        let default_att_mtu = self.default_att_mtu();
        for (idx, storage) in self.connections.borrow_mut().iter_mut().enumerate() {
            if ConnectionState::Disconnected == storage.state && storage.refcount == 0 && storage.io_refcount.get() == 0
            {
                storage.events.clear();
                storage.reassembly.clear();
                storage.state = ConnectionState::Connecting;
                storage.link_credits = default_credits;
                storage.acl_send_locked = false;
                storage.att_mtu = None;
                storage.disconnect_reason = None;
                storage.local_disconnect_error = None;
                #[cfg(feature = "gatt")]
                {
                    storage.gatt_client_owned = false;
                    storage.att_transaction = AttTransaction::Idle;
                }
                storage.handle = handle;
                #[cfg(feature = "security")]
                let identity = self
                    .security_manager
                    .get_peer_bond_information(&peer_addr.into())
                    .map(|bond| bond.identity)
                    .unwrap_or(peer_addr.into());
                #[cfg(not(feature = "security"))]
                let identity = Identity::from(peer_addr);
                storage.peer_identity = identity;
                storage.role = role;
                storage.params = params;
                #[cfg(feature = "subrating")]
                {
                    storage.subrating_params = None;
                }
                #[cfg(feature = "security")]
                {
                    storage.bond_rejected = false;
                    storage.smp_timeout = false;
                }
                #[cfg(feature = "security")]
                {
                    storage.resolvable_addrs = resolvable_addrs;
                }

                match role {
                    LeConnRole::Central => {
                        state.central_waker.wake();
                    }
                    LeConnRole::Peripheral => {
                        state.peripheral_waker.wake();
                    }
                }
                return Ok(());
            }
        }
        warn!("[link][connect] no available slot found for handle {:?}", handle);
        Err(Error::NotFound)
    }

    /// Returns a retained controller-established link matching `role` and optional `peers` identity filter (empty accepts all); optional `cx` registers one waiter per role. Transitions Connecting to Connected. Unexpected existing reference count panics; Pending has no retained ownership.
    pub(crate) fn poll_accept(
        &'d self,
        role: LeConnRole,
        peers: &[Address],
        cx: Option<&mut Context<'_>>,
    ) -> Poll<Connection<'d, P>> {
        let mut state = self.state.borrow_mut();
        if let Some(cx) = cx {
            match role {
                LeConnRole::Central => {
                    state.central_waker.register(cx.waker());
                }
                LeConnRole::Peripheral => {
                    state.peripheral_waker.register(cx.waker());
                }
            }
        }
        core::mem::drop(state);
        for (idx, storage) in self.connections.borrow_mut().iter_mut().enumerate() {
            if let ConnectionState::Connecting = storage.state {
                let handle = storage.handle;
                let r = storage.role;
                if r == role {
                    if !peers.is_empty() {
                        for peer in peers.iter() {
                            if storage.peer_identity.match_address(peer) {
                                storage.state = ConnectionState::Connected;
                                debug!("[link][poll_accept] connection accepted: state: {:?}", storage);
                                assert_eq!(storage.refcount, 0);
                                storage.inc_ref();
                                return Poll::Ready(Connection::new(idx as u8, self));
                            }
                        }
                    } else {
                        storage.state = ConnectionState::Connected;
                        assert_eq!(storage.refcount, 0);
                        debug!("[link][poll_accept] connection accepted: state: {:?}", storage);

                        assert_eq!(storage.refcount, 0);
                        storage.inc_ref();
                        return Poll::Ready(Connection::new(idx as u8, self));
                    }
                }
            }
        }
        Poll::Pending
    }

    /// Logs every slot if `verbose`, otherwise only nonterminal slots; formatting holds a shared table borrow.
    pub(crate) fn log_status(&self, verbose: bool) {
        for (idx, storage) in self.connections.borrow().iter().enumerate() {
            if verbose || storage.state != ConnectionState::Disconnected {
                debug!("[link][idx = {}] state = {:?}", idx, storage);
            }
        }
    }

    /// Retains local `index`; u8 reference exhaustion or invalid index/borrow panics.
    pub(crate) fn inc_ref(&self, index: u8) {
        self.connection_mut(index).inc_ref();
    }

    /// Releases local `index`; last accepted-link owner requests close and wakes runner. Reference underflow or invalid index/borrow panics.
    pub(crate) fn dec_ref(&self, index: u8) {
        self.connection_mut(index)
            .dec_ref(&mut self.state.borrow_mut().disconnect_waker);
    }

    /// Returns the next retained link matching `role` and `peers` (empty accepts all), waiting without deadline. Pending cancellation retains no owner.
    pub(crate) async fn accept(&'d self, role: LeConnRole, peers: &[Address]) -> Connection<'d, P> {
        poll_fn(|cx| self.poll_accept(role, peers, Some(cx))).await
    }

    /// Replaces default and every slot ACL budget with `credits`; intended for initial controller setup, not concurrent active grants. Does not wake credit waiters.
    pub(crate) fn set_link_credits(&self, credits: usize) {
        let mut state = self.state.borrow_mut();
        state.default_link_credits = credits;
        core::mem::drop(state);
        for storage in self.connections.borrow_mut().iter_mut() {
            storage.link_credits = credits;
        }
    }

    /// Returns packet-pool byte capacity minus the four-byte L2CAP header, cast to u16; internal pool configuration must fit that domain.
    pub(crate) const fn default_att_mtu(&self) -> u16 {
        (P::MTU - 4) as u16
    }

    /// Adds completed controller ACL `packets` back to live `handle` and wakes its sender; returns NotFound for absent/closing links. Trusts the controller to report only submitted outstanding packets once; malformed/duplicate completion counts are not validated and can overcredit or overflow.
    pub(crate) fn confirm_sent(&self, handle: ConnHandle, packets: usize) -> Result<(), Error> {
        for storage in self.connections.borrow_mut().iter_mut() {
            match storage.state {
                ConnectionState::Connecting | ConnectionState::Connected if handle == storage.handle => {
                    storage.link_credits += packets;
                    storage.link_credit_waker.wake();
                    return Ok(());
                }
                _ => {}
            }
        }
        Err(Error::NotFound)
    }

    /// Returns exclusive ACL-fragment ownership for live `handle`, Pending while busy (registering optional `cx`), NotFound, or NoPermits at I/O-reference exhaustion. Successful lock retains its exact slot until drop; pending polling retains no ownership.
    pub(crate) fn poll_acquire_acl_send_lock(
        &self,
        handle: ConnHandle,
        cx: Option<&mut Context<'_>>,
    ) -> Poll<Result<AclSendLock<'_, P>, Error>> {
        let mut connections = self.connections.borrow_mut();
        if let Some((index, storage)) = connections.iter_mut().enumerate().find(|(_, storage)| {
            storage.handle == handle
                && matches!(storage.state, ConnectionState::Connecting | ConnectionState::Connected)
        }) {
            match storage.poll_acquire_acl_lock(cx) {
                Poll::Ready(Ok(())) => Poll::Ready(Ok(AclSendLock {
                    lease: IoLease {
                        index,
                        connections: self.connections,
                    },
                })),
                Poll::Ready(Err(error)) => Poll::Ready(Err(error)),
                Poll::Pending => Poll::Pending,
            }
        } else {
            warn!("[link][poll_acquire_acl_send_lock] connection {:?} not found", handle);
            Poll::Ready(Err(Error::NotFound))
        }
    }

    /// Returns negotiated ATT MTU bytes or 23 before exchange for retained local `index`; invalid index/borrow conflict panics.
    pub(crate) fn get_att_mtu(&self, index: u8) -> u16 {
        self.connection(index).att_mtu()
    }

    /// Returns whether nonzero ATT MTU was stored for retained local `index`; invalid index/borrow conflict panics.
    pub(crate) fn is_att_mtu_exchanged(&self, index: u8) -> bool {
        self.connection(index).att_mtu.is_some()
    }

    /// Returns a lease for live local `index`, Disconnected after close begins, or NoPermits at I/O-count exhaustion; invalid indices panic.
    fn retain_io(&self, index: usize) -> Result<IoLease<'_, P>, Error> {
        let mut connections = self.connections.borrow_mut();
        let storage = &mut connections[index];
        if !matches!(storage.state, ConnectionState::Connecting | ConnectionState::Connected) {
            return Err(Error::Disconnected);
        }
        storage.retain_io()?;
        Ok(IoLease {
            index,
            connections: self.connections,
        })
    }

    /// Returns a lease for the currently live `handle`, NotFound if absent, or NoPermits at I/O-count exhaustion. Capture before awaiting so handle reuse cannot retarget work.
    pub(crate) fn retain_handle(&self, handle: ConnHandle) -> Result<IoLease<'_, P>, Error> {
        let index = self
            .connections
            .borrow()
            .iter()
            .position(|storage| {
                storage.handle == handle
                    && matches!(storage.state, ConnectionState::Connecting | ConnectionState::Connected)
            })
            .ok_or(Error::NotFound)?;
        self.retain_io(index)
    }

    /// Queues owned `pdu` for retained local `index`, waiting for shared capacity. Returns Disconnected/NoPermits before admission; cancellation drops packet and lease. Success means queued, not transmitted; later close makes the runner discard it.
    pub(crate) async fn send(&self, index: u8, pdu: Pdu<P::Packet>) -> Result<(), Error> {
        let lease = self.retain_io(usize::from(index))?;
        self.outbound.send((lease.index, pdu)).await;
        // Queue admission transfers this exact I/O count. No await lies between
        // successful insertion and transfer; cancellation before insertion drops it.
        core::mem::forget(lease);
        Ok(())
    }

    /// Returns Ok after queueing owned `pdu` for retained local `index`; Disconnected/NoPermits reject admission and full queue returns OutOfMemory. Rejection drops packet and any acquired lease.
    pub(crate) fn try_send(&self, index: u8, pdu: Pdu<P::Packet>) -> Result<(), Error> {
        let lease = self.retain_io(usize::from(index))?;
        self.outbound
            .try_send((lease.index, pdu))
            .map_err(|_| Error::OutOfMemory)?;
        core::mem::forget(lease);
        Ok(())
    }

    /// Returns Ok after queueing owned `pdu` for currently live `handle`; NotFound/NoPermits/full-queue OutOfMemory drop the packet. Captured slot identity survives subsequent handle reuse.
    pub(crate) fn try_outbound(&self, handle: ConnHandle, pdu: Pdu<P::Packet>) -> Result<(), Error> {
        let lease = self.retain_handle(handle)?;
        self.outbound
            .try_send((lease.index, pdu))
            .map_err(|_| Error::OutOfMemory)?;
        core::mem::forget(lease);
        Ok(())
    }

    /// Queues owned `pdu` using already-borrowed live `storage`; returns InvalidValue for foreign storage, Disconnected, NoPermits or full-queue OutOfMemory. Exact table origin is checked using addresses only; no raw pointer is dereferenced. Failure restores the count through `storage`, so this is valid inside an existing table RefMut callback.
    pub(crate) fn try_outbound_storage(
        &self,
        storage: &ConnectionStorage<P::Packet>,
        pdu: Pdu<P::Packet>,
    ) -> Result<(), Error> {
        let table = self.connections.as_ptr();
        let stride = core::mem::size_of::<ConnectionStorage<P::Packet>>();
        // ConnectionStorage is nonzero-sized. Compare addresses only while the
        // caller owns a valid element borrow; never create a second table borrow.
        let offset = core::ptr::from_ref(storage)
            .addr()
            .checked_sub(table.cast::<ConnectionStorage<P::Packet>>().addr())
            .ok_or(Error::InvalidValue)?;
        if !offset.is_multiple_of(stride) || offset / stride >= table.len() {
            return Err(Error::InvalidValue);
        }
        if !matches!(storage.state, ConnectionState::Connecting | ConnectionState::Connected) {
            return Err(Error::Disconnected);
        }
        storage.retain_io()?;
        let rollback = crate::host::OnDrop::new(|| storage.release_io());
        self.outbound
            .try_send((offset / stride, pdu))
            .map_err(|_| Error::OutOfMemory)?;
        rollback.defuse();
        Ok(())
    }

    /// Returns the next retained slot lease and owned packet. Caller must retain the lease through exact-slot ACL-lock acquisition; pending cancellation consumes nothing.
    pub(crate) async fn outbound(&self) -> (IoLease<'_, P>, Pdu<P::Packet>) {
        let (index, pdu) = self.outbound.receive().await;
        // Restore the count transferred at enqueue before returning ownership.
        (
            IoLease {
                index,
                connections: self.connections,
            },
            pdu,
        )
    }

    /// Returns accepted `conn` negotiated/default ATT MTU, or pool-derived maximum if no accepted slot matches.
    pub(crate) fn get_att_mtu_handle(&self, conn: ConnHandle) -> u16 {
        for storage in self.connections.borrow_mut().iter_mut() {
            match storage.state {
                ConnectionState::Connected if storage.handle == conn => {
                    return storage.att_mtu();
                }
                _ => {}
            }
        }
        self.default_att_mtu()
    }

    /// Negotiates `mtu` for live `conn`, returning/storing min(peer,pool maximum) with 23-byte floor; returns input unchanged for absent links. Connecting links participate before application acceptance.
    pub(crate) fn exchange_att_mtu(&self, conn: ConnHandle, mtu: u16) -> u16 {
        debug!("exchange_att_mtu: {}, current default: {}", mtu, self.default_att_mtu());
        let default_att_mtu = self.default_att_mtu();
        // Both states are live link-layer connections; the transition
        // to `Connected` is just the application polling `accept()` and
        // is unrelated to ATT activity. `handle_acl` already accepts
        // both via `connection_by_handle_mut`.
        for storage in self.connections.borrow_mut().iter_mut() {
            match storage.state {
                ConnectionState::Connecting | ConnectionState::Connected if storage.handle == conn => {
                    let negotiated = if mtu < DEFAULT_ATT_MTU || default_att_mtu < DEFAULT_ATT_MTU {
                        DEFAULT_ATT_MTU
                    } else {
                        default_att_mtu.min(mtu)
                    };
                    storage.att_mtu = NonZeroU16::new(negotiated);
                    return storage.att_mtu();
                }
                _ => {}
            }
        }
        mtu
    }

    /// Submits boolean `confirm` for accepted local `index`; returns security-phase/queue errors, Disconnected, or NotSupported without security.
    pub(crate) fn pass_key_confirm(&self, index: u8, confirm: bool) -> Result<(), Error> {
        #[cfg(feature = "security")]
        {
            let connection = self.connection(index);
            if connection.state == ConnectionState::Connected {
                self.security_manager
                    .handle_pass_key_confirm(confirm, self, &connection)
            } else {
                Err(Error::Disconnected)
            }
        }
        #[cfg(not(feature = "security"))]
        Err(Error::NotSupported)
    }

    /// Submits numeric `pass_key` for accepted local `index`; caller enforces 0..=999999. Returns security-phase/queue errors, Disconnected, or NotSupported without security.
    pub(crate) fn pass_key_input(&self, index: u8, pass_key: u32) -> Result<(), Error> {
        #[cfg(feature = "security")]
        {
            let connection = self.connection(index);
            if connection.state == ConnectionState::Connected {
                self.security_manager.handle_pass_key_input(pass_key, self, &connection)
            } else {
                Err(Error::Disconnected)
            }
        }
        #[cfg(not(feature = "security"))]
        Err(Error::NotSupported)
    }

    /// Initiates security for unencrypted local `index`, with `user_initiated` selecting active request policy. Returns Ok if already encrypted, otherwise security/link errors or NotSupported without feature.
    pub(crate) fn request_security(&self, index: u8, user_initiated: bool) -> Result<(), Error> {
        #[cfg(feature = "security")]
        {
            let current_level = self.get_security_level(index)?;
            if current_level != SecurityLevel::NoEncryption {
                return Ok(());
            }
            self.security_manager
                .initiate(self, &self.connection(index), user_initiated)
        }
        #[cfg(not(feature = "security"))]
        Err(Error::NotSupported)
    }

    #[cfg(feature = "security")]
    /// Returns whether stored identity for local `index` matches retained bond information marked bonded; does not inspect encryption state.
    pub(crate) fn is_bonded_peer(&self, index: u8) -> bool {
        let storage = self.connection(index);
        self.security_manager
            .get_peer_bond_information(&storage.peer_identity)
            .is_some_and(|b| b.is_bonded)
    }

    #[cfg(feature = "security")]
    /// Returns Ok when local `index` is encrypted or its shared security procedure succeeds; otherwise returns link/security errors. Uses resolved peer address when available; cancellation stops waiting without canceling the shared procedure.
    pub(crate) async fn try_enable_encryption(&self, index: u8) -> Result<(), Error> {
        let address = {
            let storage = self.connection(index);
            if storage.state != ConnectionState::Connected {
                return Err(Error::Disconnected);
            } else if storage.security_level != SecurityLevel::NoEncryption {
                return Ok(());
            }
            storage
                .resolvable_addrs
                .peer
                .map(|a| Address::new(AddrKind::RANDOM, a))
                .unwrap_or(storage.peer_identity.addr)
        };

        if !self.security_manager.is_pairing_in_progress(address) {
            self.request_security(index, false)?;
        }
        match self.security_manager.wait_finished(address).await {
            Ok(()) => Ok(()),
            Err(Error::Busy) => {
                // Another connection is now pairing. Check if pairing succeeded first.
                if self.get_security_level(index)? != SecurityLevel::NoEncryption {
                    Ok(())
                } else {
                    Err(Error::Busy)
                }
            }
            Err(e) => Err(e),
        }
    }

    /// Returns accepted local `index` level (NoEncryption without feature), or Disconnected.
    pub(crate) fn get_security_level(&self, index: u8) -> Result<SecurityLevel, Error> {
        let storage = self.connection(index);
        match storage.state {
            ConnectionState::Connected => {
                #[cfg(feature = "security")]
                {
                    Ok(storage.security_level)
                }
                #[cfg(not(feature = "security"))]
                Ok(SecurityLevel::NoEncryption)
            }
            _ => Err(Error::Disconnected),
        }
    }

    #[cfg(feature = "legacy-pairing")]
    /// Returns stored key length bytes for accepted local `index`, or Disconnected; caller must separately check encryption state.
    pub(crate) fn get_encryption_key_len(&self, index: u8) -> Result<u8, Error> {
        let storage = self.connection(index);
        match storage.state {
            ConnectionState::Connected => Ok(storage.encryption_key_len),
            _ => Err(Error::Disconnected),
        }
    }

    /// Returns accepted local `index` bonding preference (false without security), or Disconnected.
    pub(crate) fn get_bondable(&self, index: u8) -> Result<bool, Error> {
        let storage = self.connection(index);
        match storage.state {
            ConnectionState::Connected => {
                #[cfg(feature = "security")]
                {
                    Ok(storage.bondable)
                }
                #[cfg(not(feature = "security"))]
                Ok(false)
            }
            _ => Err(Error::Disconnected),
        }
    }

    /// Stores `bondable` for accepted local `index`; returns Disconnected or NotSupported without security. Active pairing has already sampled its capability flags.
    pub(crate) fn set_bondable(&self, index: u8, bondable: bool) -> Result<(), Error> {
        #[cfg(feature = "security")]
        {
            let mut storage = self.connection_mut(index);
            match storage.state {
                ConnectionState::Connected => {
                    storage.bondable = bondable;
                    Ok(())
                }
                _ => Err(Error::Disconnected),
            }
        }
        #[cfg(not(feature = "security"))]
        Err(Error::NotSupported)
    }

    #[cfg(feature = "security")]
    /// Stores OOB capability `available` for accepted local `index`; returns Disconnected otherwise. Pairing setup consumes this preference.
    pub(crate) fn set_oob_available(&self, index: u8, available: bool) -> Result<(), Error> {
        let mut storage = self.connection_mut(index);
        match storage.state {
            ConnectionState::Connected => {
                storage.oob_available = available;
                Ok(())
            }
            _ => Err(Error::Disconnected),
        }
    }

    #[cfg(feature = "security")]
    /// Submits `local_oob` and `peer_oob` to accepted local `index` active pairing; returns Disconnected or security-state/crypto/queue errors.
    pub(crate) fn provide_oob_data(
        &self,
        index: u8,
        local_oob: crate::security_manager::OobData,
        peer_oob: crate::security_manager::OobData,
    ) -> Result<(), Error> {
        let storage = self.connection(index);
        if storage.state == ConnectionState::Connected {
            self.security_manager
                .handle_oob_data_received(local_oob, peer_oob, self, &storage)
        } else {
            Err(Error::Disconnected)
        }
    }

    /// Routes owned SMP `pdu` for Connecting/Connected `handle`; returns security processing result, or Ok when absent/timed-out/feature-disabled. `event_handler` is currently unused. Packet ownership ends or transfers during dispatch.
    pub(crate) fn handle_security_channel(
        &self,
        handle: ConnHandle,
        pdu: Pdu<P::Packet>,
        event_handler: &dyn EventHandler,
    ) -> Result<(), Error> {
        #[cfg(feature = "security")]
        {
            for storage in self.connections.borrow().iter() {
                match storage.state {
                    // Also handle SMP while the connection is still `Connecting`.
                    // A central can send its Pairing Request as soon as the link is
                    // up, before the peripheral has `accept()`ed the connection; if
                    // we only match `Connected` here that packet is silently dropped
                    // and pairing never starts.
                    ConnectionState::Connected | ConnectionState::Connecting if storage.handle == handle => {
                        if storage.smp_timeout {
                            warn!("Ignoring security channel packet after SMP timeout");
                            return Ok(());
                        } else if let Err(error) = self.security_manager.handle_l2cap_command(pdu, self, storage) {
                            error!("Failed to handle security manager packet, {:?}", error);
                            return Err(error);
                        }
                        break;
                    }
                    _ => (),
                }
            }
        }
        Ok(())
    }

    /// Returns shared security handling result for HCI `event`; security-disabled builds ignore it and return Ok.
    pub(crate) fn handle_security_hci_event(&self, event: bt_hci::event::EventPacket) -> Result<(), Error> {
        #[cfg(feature = "security")]
        {
            self.security_manager.handle_hci_event(event, self)?;
        }
        Ok(())
    }

    /// Returns shared security handling result for LE `event`; security-disabled builds ignore it and return Ok.
    pub(crate) fn handle_security_hci_le_event(&self, event: bt_hci::event::le::LeEventPacket) -> Result<(), Error> {
        #[cfg(feature = "security")]
        {
            self.security_manager.handle_hci_le_event(event, self)?;
        }
        Ok(())
    }

    #[cfg(feature = "security")]
    /// Executes `_event` security work through `host`: key reply, central encryption, timeout latch or resolving-list/identity update. Returns controller/transport errors; missing identities are logged/ignored, missing keys request authentication-failure close. Cancellation can leave a submitted command or partial state update.
    pub(crate) async fn handle_security_event<'h, C>(
        &self,
        host: &crate::host::BleHost<'h, C, P>,
        _event: crate::security_manager::SecurityEventData,
    ) -> Result<(), crate::BleHostError<C::Error>>
    where
        C: crate::ControllerCmdSync<bt_hci::cmd::le::LeLongTermKeyRequestReply>
            + crate::ControllerCmdAsync<bt_hci::cmd::le::LeEnableEncryption>,
    {
        #[cfg(feature = "central")]
        use bt_hci::cmd::le::LeEnableEncryption;
        use bt_hci::cmd::le::LeLongTermKeyRequestReply;

        match _event {
            crate::security_manager::SecurityEventData::SendLongTermKey(handle, ediv, rand) => {
                let identity = self.connection_by_handle(handle).map(|x| x.peer_identity);
                if let Some(identity) = identity {
                    // Match EDIV/Rand against stored bonds to find the correct LTK.
                    // During active legacy pairing (STK phase), the STK is stored with EDIV=0, Rand=0.
                    let bond = self.security_manager.get_peer_bond_information(&identity);
                    #[cfg(not(feature = "legacy-pairing"))]
                    let ltk = bond.map(|b| b.ltk);
                    #[cfg(feature = "legacy-pairing")]
                    let ltk = bond.and_then(|b| {
                        if b.ediv == ediv && b.rand == rand {
                            Some(b.ltk)
                        } else {
                            None
                        }
                    });
                    if let Some(ltk) = ltk {
                        let _ = host
                            .command(LeLongTermKeyRequestReply::new(handle, ltk.to_le_bytes()))
                            .await?;
                    } else {
                        warn!("[host] Long term key request reply failed, no long term key");
                        // Send disconnect event to the controller
                        self.request_handle_disconnect(handle, DisconnectReason::AuthenticationFailure);
                    }
                } else {
                    warn!("[host] Long term key request reply failed, unknown peer")
                }
            }
            crate::security_manager::SecurityEventData::EnableEncryption(handle, bond_info) => {
                let role = self.connection_by_handle(handle).map(|x| x.role);
                if let Some(role) = role {
                    #[cfg(feature = "central")]
                    if LeConnRole::Central == role {
                        #[cfg(feature = "legacy-pairing")]
                        let (ediv, rand) = (bond_info.ediv, bond_info.rand);
                        #[cfg(not(feature = "legacy-pairing"))]
                        let (ediv, rand) = (0, [0; 8]);
                        host.async_command(LeEnableEncryption::new(handle, rand, ediv, bond_info.ltk.to_le_bytes()))
                            .await?;
                    }
                } else {
                    warn!("[host] Enable encryption failed, unknown peer")
                }
            }
            crate::security_manager::SecurityEventData::Timeout => {
                warn!("[host] Pairing timeout");
                if let Some(peer_address) = self.security_manager.peer_address() {
                    for (index, storage) in self.connections.borrow_mut().iter_mut().enumerate() {
                        if storage.state == ConnectionState::Connected
                            && storage.peer_identity.match_address(&peer_address)
                        {
                            storage.smp_timeout = true;
                            break;
                        }
                    }
                }
                self.security_manager.cancel_timeout();
            }
            crate::security_manager::SecurityEventData::TimerChange => (),
            #[cfg(feature = "security")]
            crate::security_manager::SecurityEventData::BondAdded(handle, identity) => {
                host.resolving_list_state()
                    .borrow_mut()
                    .push(crate::host::ResolvingListUpdate::Add(identity));
                // Update the connection's stored peer identity to reflect the identity
                // address distributed during key exchange, which may differ from the
                // address seen at connection time (e.g. a temporary RPA).
                if let Some(mut connection) = self.connection_by_handle_mut(handle) {
                    connection.peer_identity = identity;
                }
            }
        }
        Ok(())
    }

    #[cfg(feature = "security")]
    /// Returns the shared security event/timer future; Result distinguishes queued event from timer expiry. Caller owns subsequent event handling.
    pub(crate) fn poll_security_events(
        &self,
    ) -> impl Future<Output = Result<SecurityEventData, TimeoutError>> + use<'_, P> {
        self.security_manager.poll_events()
    }

    #[cfg(feature = "connection-metrics")]
    /// Returns `f` applied to borrowed counters at local `index`; reentrant mutable table access panics.
    pub(crate) fn metrics<F: FnOnce(&Metrics) -> R, R>(&self, index: u8, f: F) -> R {
        f(&self.connection(index).metrics)
    }

    #[cfg(feature = "att-queued-writes")]
    /// Queues `value` for ATT `handle` at byte `offset` in local `index`; returns ATT identity/contiguity/capacity admission errors without partial publication.
    pub(crate) fn prepare_write(
        &self,
        index: u8,
        handle: u16,
        offset: u16,
        value: &[u8],
    ) -> Result<(), crate::att::AttErrorCode> {
        self.connection_mut(index).prepare_write.queue(handle, offset, value)
    }

    #[cfg(feature = "att-queued-writes")]
    /// Returns callback `f` result for local `index` queued-write snapshot under immutable borrow; reentrant mutation panics.
    pub(crate) fn with_prepare_write<F, R>(&self, index: u8, f: F) -> R
    where
        F: FnOnce(&PrepareWriteState) -> R,
    {
        f(&self.connection(index).prepare_write)
    }

    #[cfg(feature = "att-queued-writes")]
    /// Clears local `index` queued-write identity/length, retaining backing bytes.
    pub(crate) fn clear_prepare_write(&self, index: u8) {
        self.connection_mut(index).prepare_write.clear();
    }
}

impl<P: PacketPool> Drop for ConnectionManager<'_, P> {
    /// Drains transferred outbound counts first, then resets borrowed slots to release inbound queues/reassembly. Safe borrowed owners cannot outlive this manager; no controller operation occurs.
    fn drop(&mut self) {
        while let Ok((index, _pdu)) = self.outbound.try_receive() {
            drop(IoLease::<P> {
                index,
                connections: self.connections,
            });
        }
        for storage in self.connections.borrow_mut().iter_mut() {
            *storage = ConnectionStorage::new();
        }
    }
}

/// Live accepted-link iterator; each yielded handle retains its slot and must be dropped independently.
pub struct ConnectedIter<'d, P: PacketPool> {
    /// Originating table used to retain each returned owner.
    manager: &'d ConnectionManager<'d, P>,
    /// Next table index to inspect; advances even when a slot is skipped.
    index: usize,
    /// Fixed table length captured at iterator creation.
    len: usize,
}

impl<'d, P: PacketPool> Iterator for ConnectedIter<'d, P> {
    /// Retained application-accepted connection owner.
    type Item = Connection<'d, P>;

    /// Returns the next currently accepted slot owner, skipping other states; exhaustion returns None. Reference overflow panics; iteration is not an atomic table snapshot.
    fn next(&mut self) -> Option<Self::Item> {
        while self.index < self.len {
            let idx = self.index;
            self.index += 1;
            if self.manager.is_connected(idx as u8) {
                self.manager.inc_ref(idx as u8);
                return Some(Connection::new(idx as u8, self.manager));
            }
        }
        None
    }
}

/// Borrowed runner work item for a local disconnect request; confirmation only advances to in-flight close.
pub struct DisconnectRequest<'a, P> {
    /// Local table slot whose requested close was observed.
    index: usize,
    /// Controller handle captured for command submission.
    handle: ConnHandle,
    /// Local reason to submit and retain in the in-flight state.
    reason: DisconnectReason,
    /// Borrowed table used by confirmation; does not itself retain a slot owner.
    connections: &'a RefCell<[ConnectionStorage<P>]>,
}

impl<P> DisconnectRequest<'_, P> {
    /// Returns the controller link handle captured when this work item was polled.
    pub fn handle(&self) -> ConnHandle {
        self.handle
    }

    /// Returns the locally requested HCI disconnect reason.
    pub fn reason(&self) -> DisconnectReason {
        self.reason
    }

    /// Advances a still-pending request to Disconnecting; preserves terminal state if controller completion raced ahead. Does not itself issue HCI commands.
    pub fn confirm(self) {
        let mut connections = self.connections.borrow_mut();
        let storage = &mut connections[self.index];
        // Only transition if still in DisconnectRequest. The HCI
        // disconnect_complete handler (`disconnected()`) may race ahead and
        // set state = Disconnected; in that case we must NOT overwrite it
        // back to Disconnecting, or the slot becomes unreusable
        // (terminal state and both reference counts zero permit reuse).
        if !matches!(storage.state, ConnectionState::DisconnectRequest(_)) {
            return;
        }
        storage.state = ConnectionState::Disconnecting(self.reason);
    }
}
/// Reusable link slot retaining identity/diagnostics until zero-reference terminal cleanup permits reallocation.
pub struct ConnectionStorage<P> {
    /// Controller/application/close lifecycle phase.
    pub state: ConnectionState,
    /// Controller-assigned numeric handle; retained after close and reusable by another slot.
    pub handle: ConnHandle,
    /// Central/peripheral role from establishment.
    pub role: LeConnRole,
    /// Resolved/stored peer identity, updated when pairing distributes an identity address.
    pub peer_identity: Identity,
    /// Retained interval/latency/supervision observation.
    pub params: ConnParams,
    #[cfg(feature = "subrating")]
    /// Optional retained factor/continuation; factor 1 is represented as None.
    pub subrating_params: Option<SubratingParams>,
    /// Negotiated nonzero ATT MTU bytes; None means unexchanged/default 23.
    pub att_mtu: Option<NonZeroU16>,
    /// Terminal controller status, retained even if event delivery failed.
    pub(crate) disconnect_reason: Option<Status>,
    /// First ACL ingress failure requesting local close; survives terminal completion.
    pub(crate) local_disconnect_error: Option<Error>,
    /// Unreserved controller ACL packet credits; separate from dynamic-channel K-frame credits.
    pub link_credits: usize,
    /// Single sender waiting for controller ACL credits.
    pub link_credit_waker: WakerRegistration,
    /// True while one sender owns ACL-fragment serialization for this link.
    pub acl_send_locked: bool,
    /// Single competing ACL sender waiting for lock release.
    pub acl_send_lock_waker: WakerRegistration,
    /// Retained application link owners; slot reuse also requires zero I/O owners and Disconnected.
    pub refcount: u8,
    /// Retained ACL locks/credit grants, separate from application acceptance and automatic disconnect. Forgotten owners keep this bounded slot unavailable.
    io_refcount: Cell<u8>,
    #[cfg(feature = "connection-metrics")]
    /// Wrapping packet/blocked counters and monotonic observation times.
    pub metrics: Metrics,
    #[cfg(feature = "security")]
    /// Current observed encryption/authentication level.
    pub security_level: SecurityLevel,
    #[cfg(feature = "security")]
    /// Application bonding preference sampled by pairing setup; initially false.
    pub bondable: bool,
    #[cfg(feature = "security")]
    /// Application OOB capability preference sampled by pairing setup.
    pub oob_available: bool,
    #[cfg(feature = "security")]
    /// Stored-bond encryption was rejected for this connection attempt.
    pub bond_rejected: bool,
    #[cfg(feature = "security")]
    /// SMP timeout latch suppressing later SMP packets until link reuse.
    pub smp_timeout: bool,
    #[cfg(feature = "security")]
    /// Controller-provided local/peer RPAs, separate from distributed identity addresses.
    pub resolvable_addrs: ResolvablePrivateAddrs,
    #[cfg(feature = "legacy-pairing")]
    /// Negotiated key length bytes; zero before negotiation and meaningful with security state.
    pub encryption_key_len: u8,
    /// Shared Boolean admitting registered dynamic-channel peer requests.
    pub l2cap_listening: bool,
    /// Bounded application-event queue; retained terminal reason covers overflow loss.
    pub events: EventChannel,
    /// Single ACL/L2CAP partial packet assembly, released on terminal close/reuse.
    pub reassembly: PacketReassembly<P>,
    #[cfg(feature = "gatt")]
    /// Server-side inbound ATT queue owning packet allocations.
    pub gatt: GattChannel<P>,
    #[cfg(feature = "gatt")]
    /// Client-side correlated ATT response queue owning packet allocations.
    pub(crate) gatt_client: GattChannel<P>,
    #[cfg(feature = "gatt")]
    /// Client waiter also woken on local/terminal close.
    pub(crate) gatt_client_waker: WakerRegistration,
    #[cfg(feature = "gatt")]
    /// True while one GATT client owns this link ATT request channel.
    gatt_client_owned: bool,
    #[cfg(feature = "gatt")]
    /// Single request opcode/correlation phase, rejecting unsolicited/duplicate responses.
    att_transaction: AttTransaction,
    #[cfg(feature = "gatt")]
    /// Exclusive server indication ownership until confirmation/cancel cleanup.
    pub(crate) indication_in_flight: bool,
    #[cfg(feature = "gatt")]
    /// Waiter for an existing indication owner to release its slot.
    pub(crate) indication_slot_waker: WakerRegistration,
    #[cfg(feature = "gatt")]
    /// Unconsumed ATT Handle Value Confirmation latch.
    pub(crate) indication_cfm_received: bool,
    #[cfg(feature = "gatt")]
    /// Waiter for confirmation or terminal disconnection.
    pub(crate) indication_cfm_waker: WakerRegistration,
    #[cfg(feature = "att-queued-writes")]
    /// Single-attribute contiguous ATT prepared-write staging area.
    pub(crate) prepare_write: PrepareWriteState,
}

/// Connection metrics
#[cfg(feature = "connection-metrics")]
#[derive(Debug)]
pub struct Metrics {
    /// Number of ACL packets sent for this connection.
    pub num_sent: usize,
    /// Number of ACL packets received on this connection.
    pub num_received: usize,
    /// Time of last sent packet.
    pub last_sent: embassy_time::Instant,
    /// Time of last received packet.
    pub last_received: embassy_time::Instant,
    /// Number of times a sender was blocked from sending.
    pub blocked_sends: usize,
}

#[cfg(feature = "connection-metrics")]
impl Metrics {
    /// Returns zero counters and Instant::MIN sent/received timestamps; MIN means no packet observed yet.
    pub(crate) const fn new() -> Self {
        Self {
            num_sent: 0,
            num_received: 0,
            last_sent: embassy_time::Instant::MIN,
            last_received: embassy_time::Instant::MIN,
            blocked_sends: 0,
        }
    }

    /// Adds `num` confirmed ACL packets with wrapping count and records current monotonic time.
    pub(crate) fn sent(&mut self, num: usize) {
        self.num_sent = self.num_sent.wrapping_add(num);
        self.last_sent = embassy_time::Instant::now();
    }

    /// Adds `num` received ACL packets with wrapping count and records current monotonic time.
    pub(crate) fn received(&mut self, num: usize) {
        self.num_received = self.num_received.wrapping_add(num);
        self.last_received = embassy_time::Instant::now();
    }

    /// Increments wrapping blocked-poll count; repeated credit/lock waits count separately.
    pub(crate) fn blocked_send(&mut self) {
        self.blocked_sends = self.blocked_sends.wrapping_add(1);
    }

    /// Resets counts and last-observation times to initial zero/MIN state.
    pub(crate) fn reset(&mut self) {
        *self = Self::new();
    }
}

#[cfg(feature = "connection-metrics")]
#[cfg(feature = "defmt")]
impl defmt::Format for Metrics {
    /// Writes metrics to defmt `f`, rendering elapsed milliseconds since stored times; MIN is not labeled unavailable here.
    fn format(&self, f: defmt::Formatter<'_>) {
        defmt::write!(
            f,
            "sent = {}, since_sent = {} ms, recvd = {}, since_recvd = {} ms, blocked sends = {}",
            self.num_sent,
            self.last_sent.elapsed().as_millis(),
            self.num_received,
            self.last_received.elapsed().as_millis(),
            self.blocked_sends,
        );
    }
}

impl<P> ConnectionStorage<P> {
    /// Returns an unowned terminal slot with empty queues, zero/default identity and no negotiated/security state.
    pub(crate) const fn new() -> ConnectionStorage<P> {
        ConnectionStorage {
            state: ConnectionState::Disconnected,
            handle: ConnHandle(0),
            role: LeConnRole::Central,
            peer_identity: Identity {
                addr: Address {
                    kind: AddrKind::PUBLIC,
                    addr: BdAddr([0; 6]),
                },
                #[cfg(feature = "security")]
                irk: None,
            },
            params: ConnParams::new(),
            #[cfg(feature = "subrating")]
            subrating_params: None,
            att_mtu: None,
            disconnect_reason: None,
            local_disconnect_error: None,
            link_credits: 0,
            link_credit_waker: WakerRegistration::new(),
            acl_send_locked: false,
            acl_send_lock_waker: WakerRegistration::new(),
            refcount: 0,
            io_refcount: Cell::new(0),
            #[cfg(feature = "connection-metrics")]
            metrics: Metrics::new(),
            #[cfg(feature = "security")]
            security_level: SecurityLevel::NoEncryption,
            #[cfg(feature = "security")]
            bond_rejected: false,
            #[cfg(feature = "security")]
            smp_timeout: false,
            #[cfg(feature = "security")]
            resolvable_addrs: ResolvablePrivateAddrs::none(),
            #[cfg(feature = "legacy-pairing")]
            encryption_key_len: 0,
            l2cap_listening: false,
            events: EventChannel::new(),
            #[cfg(feature = "gatt")]
            gatt: GattChannel::new(),
            #[cfg(feature = "gatt")]
            gatt_client: GattChannel::new(),
            #[cfg(feature = "gatt")]
            gatt_client_waker: WakerRegistration::new(),
            #[cfg(feature = "gatt")]
            gatt_client_owned: false,
            #[cfg(feature = "gatt")]
            att_transaction: AttTransaction::Idle,
            #[cfg(feature = "gatt")]
            indication_in_flight: false,
            #[cfg(feature = "gatt")]
            indication_slot_waker: WakerRegistration::new(),
            #[cfg(feature = "gatt")]
            indication_cfm_received: false,
            #[cfg(feature = "gatt")]
            indication_cfm_waker: WakerRegistration::new(),
            reassembly: PacketReassembly::new(),
            #[cfg(feature = "security")]
            bondable: false,
            #[cfg(feature = "security")]
            oob_available: false,
            #[cfg(feature = "att-queued-writes")]
            prepare_write: PrepareWriteState::new(),
        }
    }

    /// Returns Ok after retaining one I/O owner, or NoPermits without mutation at 255 owners.
    fn retain_io(&self) -> Result<(), Error> {
        self.io_refcount
            .set(self.io_refcount.get().checked_add(1).ok_or(Error::NoPermits)?);
        Ok(())
    }

    /// Releases one I/O owner without application disconnect effects; underflow indicates an internal ownership bug and panics.
    fn release_io(&self) {
        self.io_refcount.set(
            self.io_refcount
                .get()
                .checked_sub(1)
                .expect("I/O lease count underflow"),
        );
    }

    /// Returns Ok after acquiring ACL ownership and retaining one I/O reference, NoPermits on exhaustion, or Pending while busy. Optional `cx` registers the competing sender; failure preserves ownership and credits.
    fn poll_acquire_acl_lock(&mut self, cx: Option<&mut Context<'_>>) -> Poll<Result<(), Error>> {
        if !self.acl_send_locked {
            self.retain_io()?;
            self.acl_send_locked = true;
            Poll::Ready(Ok(()))
        } else {
            if let Some(cx) = cx {
                self.acl_send_lock_waker.register(cx.waker());
            }
            #[cfg(feature = "connection-metrics")]
            self.metrics.blocked_send();
            Poll::Pending
        }
    }

    /// Retains one owner; panics before mutation at u8 reference-count exhaustion.
    fn inc_ref(&mut self) {
        self.refcount = unwrap!(
            self.refcount.checked_add(1),
            "Too many references to the same connection"
        );
    }

    /// Releases one owner; last Connected owner requests RemoteUserTerminatedConn close and wakes `waker` plus the active ACL sender. Underflow panics; other lifecycle phases remain unchanged.
    fn dec_ref(&mut self, waker: &mut WakerRegistration) {
        self.refcount = unwrap!(
            self.refcount.checked_sub(1),
            "bug: dropping a connection with refcount 0"
        );
        if self.refcount == 0 && self.state == ConnectionState::Connected {
            self.state = ConnectionState::DisconnectRequest(DisconnectReason::RemoteUserTerminatedConn);
            self.link_credit_waker.wake();
            waker.wake();
        }
    }

    /// Returns true after reserving `n` ACL packets; insufficient credits return false without mutation.
    fn take_link_credits(&mut self, n: usize) -> bool {
        if let Some(credits) = self.link_credits.checked_sub(n) {
            self.link_credits = credits;
            true
        } else {
            false
        }
    }

    /// Returns negotiated ATT MTU bytes, or 23 when unexchanged.
    fn att_mtu(&self) -> u16 {
        // Default ATT MTU is 23
        self.att_mtu.map_or(DEFAULT_ATT_MTU, |mtu| mtu.get())
    }
}

impl<P> core::fmt::Debug for ConnectionStorage<P> {
    /// Writes lifecycle/handle/role/peer address/reference count and optional metrics to `f`; returns formatting errors. Stored IRKs are deliberately excluded from incidental diagnostics.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let mut d = f.debug_struct("ConnectionStorage");
        let d = d
            .field("state", &self.state)
            .field("handle", &self.handle)
            .field("role", &self.role)
            .field("peer_address", &self.peer_identity.addr)
            .field("refcount", &self.refcount);
        #[cfg(feature = "connection-metrics")]
        let d = d.field("metrics", &self.metrics);
        d.finish()
    }
}

#[cfg(feature = "defmt")]
impl<P> defmt::Format for ConnectionStorage<P> {
    /// Writes link lifecycle/handle/credits/peer address/reference/reassembly and optional metrics through defmt `f`; excludes stored IRKs.
    fn format(&self, f: defmt::Formatter<'_>) {
        defmt::write!(
            f,
            "state = {}, conn = {}, flow = {}",
            self.state,
            self.handle,
            self.link_credits,
        );

        defmt::write!(
            f,
            ", role = {}, peer = {}, ref = {}, sar = {}",
            self.role,
            self.peer_identity.addr,
            self.refcount,
            self.reassembly,
        );

        #[cfg(feature = "connection-metrics")]
        defmt::write!(f, ", {}", self.metrics);
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
/// Link lifecycle separating controller establishment, application acceptance, requested close and terminal completion.
pub enum ConnectionState {
    /// Terminal/unused; slot reuse additionally requires zero references.
    Disconnected,
    /// Local close requested, not yet acknowledged by runner command handling.
    DisconnectRequest(
        /// HCI reason to submit to the controller.
        DisconnectReason,
    ),
    /// Runner submitted close; terminal controller event is still outstanding.
    Disconnecting(
        /// Locally submitted HCI reason, not necessarily the final event status.
        DisconnectReason,
    ),
    /// Controller established the link, but application acceptance has not retained it.
    Connecting,
    /// Application accepted and retained the live link.
    Connected,
}

/// Affine retention of one local slot, independent of application Connection owners.
/// Numeric controller handles may be reused elsewhere; this index cannot be reused until all leases drop.
pub(crate) struct IoLease<'a, P: PacketPool> {
    /// Exact retained table position, never resolved again through a controller handle.
    index: usize,
    /// Table whose I/O count was incremented before constructing this lease.
    connections: &'a RefCell<[ConnectionStorage<P::Packet>]>,
}

impl<'a, P: PacketPool> IoLease<'a, P> {
    /// Returns the retained slot while Connecting/Connected, or None after close begins; borrow conflicts panic.
    fn live(&self) -> Option<RefMut<'_, ConnectionStorage<P::Packet>>> {
        RefMut::filter_map(self.connections.borrow_mut(), |connections| {
            let storage = &mut connections[self.index];
            matches!(storage.state, ConnectionState::Connecting | ConnectionState::Connected).then_some(storage)
        })
        .ok()
    }

    /// Returns the original controller handle, including after terminal close; retaining the slot prevents replacement.
    pub(crate) fn handle(&self) -> ConnHandle {
        self.connections.borrow()[self.index].handle
    }

    /// Returns an independently retained ACL lock for this exact slot, Disconnected/NoPermits, or Pending while busy. Optional `cx` registers the waiter; pending cancellation retains only this lease.
    pub(crate) fn poll_acquire(&self, cx: Option<&mut Context<'_>>) -> Poll<Result<AclSendLock<'a, P>, Error>> {
        let Some(mut storage) = self.live() else {
            return Poll::Ready(Err(Error::Disconnected));
        };
        match storage.poll_acquire_acl_lock(cx) {
            Poll::Ready(Ok(())) => Poll::Ready(Ok(AclSendLock {
                lease: IoLease {
                    index: self.index,
                    connections: self.connections,
                },
            })),
            Poll::Ready(Err(error)) => Poll::Ready(Err(error)),
            Poll::Pending => Poll::Pending,
        }
    }
}

impl<P: PacketPool> Drop for IoLease<'_, P> {
    /// Releases exact-slot retention without initiating application disconnect; count underflow is an internal invariant panic.
    fn drop(&mut self) {
        let mut connections = self.connections.borrow_mut();
        let storage = &mut connections[self.index];
        storage.release_io();
    }
}

/// Exclusive outbound ACL-fragment ownership for one retained local slot; drop releases serialization.
pub struct AclSendLock<'a, P: PacketPool> {
    /// Prevents slot reuse while this sender or its independently retained grants remain alive.
    lease: IoLease<'a, P>,
}

impl<'a, P: PacketPool> AclSendLock<'a, P> {
    /// Returns Ok for this exact Connecting/Connected slot and registers `cx` for close/credit changes, or Disconnected. The temporary table borrow ends before caller polls its transport; ACL ownership makes this the sole active sender waiter.
    pub(crate) fn check_live(&self, cx: &Context<'_>) -> Result<(), Error> {
        let Some(mut storage) = self.lease.live() else {
            return Err(Error::Disconnected);
        };
        storage.link_credit_waker.register(cx.waker());
        Ok(())
    }

    /// Returns whether the retained live slot has at least `n_acl` unreserved controller ACL packet credits.
    pub(crate) fn has_link_credits(&self, n_acl: usize) -> bool {
        self.lease
            .live()
            .map(|storage| storage.link_credits >= n_acl)
            .unwrap_or(false)
    }

    /// Reserves `packets` ACL credits for the retained slot; returns a refund-on-drop grant, Disconnected, NoPermits at I/O-count exhaustion, or Pending while insufficient. Optional `cx` registers its credit waiter. Zero packets still retain one lease; cancellation before success reserves nothing. A grant can outlive this lock.
    pub(crate) fn poll_request_to_send(
        &self,
        packets: usize,
        cx: Option<&mut Context<'_>>,
    ) -> Poll<Result<PacketGrant<'a, P>, Error>> {
        let Some(mut storage) = self.lease.live() else {
            return Poll::Ready(Err(Error::Disconnected));
        };
        if storage.link_credits >= packets {
            // Retain before debiting: exhaustion leaves both credits and ownership unchanged.
            storage.retain_io()?;
            let taken = storage.take_link_credits(packets);
            debug_assert!(taken);
            Poll::Ready(Ok(PacketGrant {
                lease: IoLease {
                    index: self.lease.index,
                    connections: self.lease.connections,
                },
                packets,
            }))
        } else {
            #[cfg(feature = "connection-metrics")]
            storage.metrics.blocked_send();
            if let Some(cx) = cx {
                storage.link_credit_waker.register(cx.waker());
            }
            Poll::Pending
        }
    }
}

impl<P: PacketPool> Drop for AclSendLock<'_, P> {
    /// Unlocks only the retained live slot and wakes a competing sender; terminal slots receive no effects before lease release.
    fn drop(&mut self) {
        if let Some(mut storage) = self.lease.live() {
            if core::mem::replace(&mut storage.acl_send_locked, false) {
                storage.acl_send_lock_waker.wake();
            }
        }
    }
}

/// Reserved controller ACL packet credits; confirms consume entitlement and drop refunds unused packets only to the original live slot.
pub struct PacketGrant<'a, P: PacketPool> {
    /// Independent retention, allowing this grant to outlive the ACL lock that issued it.
    lease: IoLease<'a, P>,
    /// Reserved ACL packet count not yet confirmed as submitted.
    packets: usize,
}

impl<P: PacketPool> PacketGrant<'_, P> {
    /// Subtracts submitted `sent` ACL packets with saturation and records metrics only on the original live slot; caller must not confirm more packets than actually submitted.
    pub(crate) fn confirm(&mut self, sent: usize) {
        self.packets = self.packets.saturating_sub(sent);
        #[cfg(feature = "connection-metrics")]
        {
            if let Some(mut storage) = self.lease.live() {
                storage.metrics.sent(sent);
            }
        }
    }
}

impl<P: PacketPool> Drop for PacketGrant<'_, P> {
    /// Refunds unconfirmed credits only to the retained live slot, then releases retention. Valid controller completion accounting bounds the refund; malformed duplicate completions can invalidate that assumption.
    fn drop(&mut self) {
        if self.packets > 0 {
            if let Some(mut storage) = self.lease.live() {
                storage.link_credits += self.packets;
                storage.link_credit_waker.wake();
            }
        }
    }
}

#[cfg(test)]
/// Native link lifecycle, handle-reuse, queued GATT and disconnect acknowledgment regressions.
pub(crate) mod tests {
    use super::*;
    extern crate std;

    use std::boxed::Box;

    use bt_hci::param::{AddrKind, BdAddr};
    use embassy_futures::block_on;

    use crate::prelude::*;

    /// First distinct six-byte little-endian peer fixture address; tests choose its address kind explicitly.
    pub const ADDR_1: [u8; 6] = [0x11, 0x22, 0x33, 0x44, 0x55, 0x66];
    /// Second distinct six-byte little-endian peer fixture address for filtering and reuse tests.
    pub const ADDR_2: [u8; 6] = [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF];

    #[test]
    #[cfg(feature = "gatt")]
    /// Queued client ATT bytes are withheld immediately after local close, before terminal controller acknowledgment.
    fn local_disconnect_stops_queued_gatt_before_controller_ack() {
        let mgr = setup();
        let handle = ConnHandle::new(0);
        mgr.connect(
            handle,
            Address::new(AddrKind::RANDOM, BdAddr::new(ADDR_1)),
            LeConnRole::Central,
            ConnParams::new(),
        )
        .unwrap();
        let Poll::Ready(conn) = mgr.poll_accept(LeConnRole::Central, &[], None) else {
            panic!()
        };
        mgr.post_gatt_client(handle, Pdu::new(DefaultPacketPool::allocate().unwrap(), 1))
            .unwrap();
        conn.disconnect();
        assert!(
            block_on(conn.next_gatt_client()).is_none(),
            "queued bytes must not escape a failed epoch"
        );
        mgr.disconnected(handle, Status::SUCCESS).unwrap();
    }

    /// Returns a leaked three-slot manager and optional ten-bond store for isolated unit tests; no controller or device is opened.
    pub fn setup() -> &'static ConnectionManager<'static, DefaultPacketPool> {
        let storage: &RefCell<[_]> = Box::leak(Box::new(RefCell::new([const { ConnectionStorage::new() }; 3])));
        #[cfg(feature = "security")]
        let bond_storage: &RefCell<heapless::VecView<_>> = Box::leak(Box::new(RefCell::new(heapless::Vec::<
            crate::security_manager::BondInformation,
            10,
        >::new())));
        let mgr = ConnectionManager::new(
            storage,
            #[cfg(feature = "security")]
            bond_storage,
        );
        Box::leak(Box::new(mgr))
    }

    #[test]
    /// Peripheral admission waits for establishment, then retains reported role and peer identity.
    fn peripheral_connection_established() {
        let mgr = setup();
        assert!(mgr.poll_accept(LeConnRole::Peripheral, &[], None).is_pending());

        unwrap!(mgr.connect(
            ConnHandle::new(0),
            Address::new(AddrKind::RANDOM, BdAddr::new(ADDR_1)),
            LeConnRole::Peripheral,
            ConnParams::new(),
        ));

        let Poll::Ready(handle) = mgr.poll_accept(LeConnRole::Peripheral, &[], None) else {
            panic!("expected connection to be accepted");
        };
        assert_eq!(handle.role(), LeConnRole::Peripheral);
        assert_eq!(
            handle.peer_address(),
            Address::new(AddrKind::RANDOM, BdAddr::new(ADDR_1))
        );

        handle.disconnect();
    }

    #[test]
    /// Central admission waits for establishment, then retains reported role and peer identity.
    fn central_connection_established() {
        let mgr = setup();

        assert!(mgr.poll_accept(LeConnRole::Central, &[], None).is_pending());

        unwrap!(mgr.connect(
            ConnHandle::new(0),
            Address::new(AddrKind::RANDOM, BdAddr::new(ADDR_2)),
            LeConnRole::Central,
            ConnParams::new(),
        ));

        let Poll::Ready(handle) = mgr.poll_accept(LeConnRole::Central, &[], None) else {
            panic!("expected connection to be accepted");
        };
        assert_eq!(handle.role(), LeConnRole::Central);
        assert_eq!(
            handle.peer_address(),
            Address::new(AddrKind::RANDOM, BdAddr::new(ADDR_2))
        );
    }

    #[test]
    /// Terminal controller close racing ahead of runner confirmation remains terminal and stops further close work.
    fn controller_disconnects_before_host() {
        let mgr = setup();

        unwrap!(mgr.connect(
            ConnHandle::new(3),
            Address::new(AddrKind::RANDOM, BdAddr::new(ADDR_1)),
            LeConnRole::Central,
            ConnParams::new(),
        ));

        unwrap!(mgr.connect(
            ConnHandle::new(2),
            Address::new(AddrKind::RANDOM, BdAddr::new(ADDR_2)),
            LeConnRole::Peripheral,
            ConnParams::new(),
        ));

        let Poll::Ready(central) = mgr.poll_accept(LeConnRole::Central, &[], None) else {
            panic!("expected connection to be accepted");
        };

        let Poll::Ready(peripheral) = mgr.poll_accept(LeConnRole::Peripheral, &[], None) else {
            panic!("expected connection to be accepted");
        };

        assert_eq!(ConnHandle::new(3), central.handle());
        assert_eq!(ConnHandle::new(2), peripheral.handle());

        // Disconnect request from us
        peripheral.disconnect();

        // Polling should return the disconnecting handle
        let Poll::Ready(req) = mgr.poll_disconnecting(None) else {
            panic!("expected connection to be accepted");
        };

        // If nothing happens, polling should behave the same way
        let Poll::Ready(req) = mgr.poll_disconnecting(None) else {
            panic!("expected connection to be accepted");
        };

        // Disconnection event from host arrives before we confirm
        unwrap!(mgr.disconnected(ConnHandle::new(2), Status::UNSPECIFIED));

        // This should be a noop
        req.confirm();

        // Polling should not return anything
        assert!(mgr.poll_disconnecting(None).is_pending());
    }

    #[test]
    /// Runner acknowledgment advances close state and later controller completion emits the terminal reason.
    fn controller_disconnects_after_host() {
        let mgr = setup();

        unwrap!(mgr.connect(
            ConnHandle::new(3),
            Address::new(AddrKind::RANDOM, BdAddr::new(ADDR_1)),
            LeConnRole::Central,
            ConnParams::new(),
        ));

        unwrap!(mgr.connect(
            ConnHandle::new(2),
            Address::new(AddrKind::RANDOM, BdAddr::new(ADDR_2)),
            LeConnRole::Peripheral,
            ConnParams::new(),
        ));

        let Poll::Ready(central) = mgr.poll_accept(LeConnRole::Central, &[], None) else {
            panic!("expected connection to be accepted");
        };

        let Poll::Ready(peripheral) = mgr.poll_accept(LeConnRole::Peripheral, &[], None) else {
            panic!("expected connection to be accepted");
        };

        assert_eq!(ConnHandle::new(3), central.handle());
        assert_eq!(ConnHandle::new(2), peripheral.handle());

        // Disconnect request from us
        peripheral.disconnect();

        // Polling should return the disconnecting handle
        let Poll::Ready(req) = mgr.poll_disconnecting(None) else {
            panic!("expected connection to be accepted");
        };

        // This should remove it from the list
        req.confirm();

        // Polling should not return anything
        assert!(mgr.poll_disconnecting(None).is_pending());

        // Disconnection event from host arrives before we confirm
        unwrap!(mgr.disconnected(ConnHandle::new(2), Status::UNSPECIFIED));

        // Check that we get an event
        use crate::connection::ConnectionEvent;
        assert!(matches!(
            block_on(peripheral.next()),
            ConnectionEvent::Disconnected {
                reason: Status::UNSPECIFIED
            }
        ));

        // Polling should not return anything
        assert!(mgr.poll_disconnecting(None).is_pending());
    }

    #[test]
    /// Retained disconnected owners preserve old identity even when the numeric controller handle belongs to a new slot.
    fn referenced_handle_not_reused() {
        let mgr = setup();

        assert!(mgr.poll_accept(LeConnRole::Peripheral, &[], None).is_pending());

        let handle = ConnHandle::new(42);
        unwrap!(mgr.connect(
            handle,
            Address::new(AddrKind::RANDOM, BdAddr::new(ADDR_1)),
            LeConnRole::Peripheral,
            ConnParams::new()
        ));

        let Poll::Ready(conn) = mgr.poll_accept(LeConnRole::Peripheral, &[], None) else {
            panic!("expected connection to be accepted");
        };
        assert_eq!(conn.role(), LeConnRole::Peripheral);
        assert_eq!(conn.peer_address(), Address::new(AddrKind::RANDOM, BdAddr::new(ADDR_1)));

        unwrap!(mgr.disconnected(handle, Status::UNSPECIFIED));

        // New incoming connection reusing handle
        let handle = ConnHandle::new(42);
        unwrap!(mgr.connect(
            handle,
            Address::new(AddrKind::RANDOM, BdAddr::new(ADDR_2)),
            LeConnRole::Peripheral,
            ConnParams::new()
        ));

        let Poll::Ready(conn2) = mgr.poll_accept(LeConnRole::Peripheral, &[], None) else {
            panic!("expected connection to be accepted");
        };

        // Ensure existing connection doesnt panic things
        assert_eq!(conn.handle(), ConnHandle::new(42));
        assert_eq!(conn.role(), LeConnRole::Peripheral);
        assert_eq!(conn.peer_address(), Address::new(AddrKind::RANDOM, BdAddr::new(ADDR_1)));
        assert!(!conn.is_connected());

        assert_eq!(conn2.handle(), ConnHandle::new(42));
        assert_eq!(conn2.role(), LeConnRole::Peripheral);
        assert_eq!(
            conn2.peer_address(),
            Address::new(AddrKind::RANDOM, BdAddr::new(ADDR_2))
        );
        assert!(conn2.is_connected());
    }

    #[test]
    /// Terminal close resolves the new live slot rather than an old retained slot with the same controller handle.
    fn disconnect_correct_handle() {
        let mgr = setup();

        assert!(mgr.poll_accept(LeConnRole::Peripheral, &[], None).is_pending());

        let handle = ConnHandle::new(42);
        unwrap!(mgr.connect(
            handle,
            Address::new(AddrKind::RANDOM, BdAddr::new(ADDR_1)),
            LeConnRole::Peripheral,
            ConnParams::new()
        ));

        let Poll::Ready(conn) = mgr.poll_accept(LeConnRole::Peripheral, &[], None) else {
            panic!("expected connection to be accepted");
        };
        assert_eq!(conn.role(), LeConnRole::Peripheral);
        assert_eq!(conn.peer_address(), Address::new(AddrKind::RANDOM, BdAddr::new(ADDR_1)));

        unwrap!(mgr.disconnected(handle, Status::UNSPECIFIED));

        // New incoming connection reusing handle
        let handle = ConnHandle::new(42);
        unwrap!(mgr.connect(
            handle,
            Address::new(AddrKind::RANDOM, BdAddr::new(ADDR_2)),
            LeConnRole::Peripheral,
            ConnParams::new()
        ));

        let Poll::Ready(conn2) = mgr.poll_accept(LeConnRole::Peripheral, &[], None) else {
            panic!("expected connection to be accepted");
        };

        assert_eq!(conn2.handle(), ConnHandle::new(42));
        assert_eq!(conn2.role(), LeConnRole::Peripheral);
        assert_eq!(
            conn2.peer_address(),
            Address::new(AddrKind::RANDOM, BdAddr::new(ADDR_2))
        );
        assert!(conn2.is_connected());

        unwrap!(mgr.disconnected(handle, Status::UNSPECIFIED));

        assert!(!conn2.is_connected());
    }

    #[test]
    /// Last-owner drop publishes one close request, which disappears from pending iteration after runner confirmation.
    fn disconnecting_iterator_invalid() {
        let mgr = setup();

        assert!(mgr.poll_accept(LeConnRole::Peripheral, &[], None).is_pending());

        unwrap!(mgr.connect(
            ConnHandle::new(3),
            Address::new(AddrKind::RANDOM, BdAddr::new(ADDR_1)),
            LeConnRole::Peripheral,
            ConnParams::new()
        ));

        let Poll::Ready(handle) = mgr.poll_accept(LeConnRole::Peripheral, &[], None) else {
            panic!("expected connection to be accepted");
        };
        assert_eq!(handle.role(), LeConnRole::Peripheral);
        assert_eq!(
            handle.peer_address(),
            Address::new(AddrKind::RANDOM, BdAddr::new(ADDR_1))
        );

        assert!(mgr.poll_disconnecting(None).is_pending());

        // Disconnect request from us
        drop(handle);

        // Polling should return the disconnecting handle
        let Poll::Ready(req) = mgr.poll_disconnecting(None) else {
            panic!("expected connection to be accepted");
        };

        //        unwrap!(mgr.disconnected(ConnHandle::new(3)));

        req.confirm();

        assert!(mgr.poll_disconnecting(None).is_pending());
    }

    #[test]
    /// Handle liveness includes Connecting/Connected and excludes absent or locally closing links.
    fn nonexisting_handle_is_disconnected() {
        let mgr = setup();

        assert!(!mgr.is_handle_connected(ConnHandle::new(5)));

        unwrap!(mgr.connect(
            ConnHandle::new(3),
            Address::new(AddrKind::RANDOM, BdAddr::new(ADDR_1)),
            LeConnRole::Peripheral,
            ConnParams::new()
        ));

        assert!(mgr.is_handle_connected(ConnHandle::new(3)));

        let Poll::Ready(handle) = mgr.poll_accept(LeConnRole::Peripheral, &[], None) else {
            panic!("expected connection to be accepted");
        };

        assert!(mgr.is_handle_connected(ConnHandle::new(3)));

        handle.disconnect();

        assert!(!mgr.is_handle_connected(ConnHandle::new(3)));
    }
    /// Establishes and accepts `handle` on `mgr` for a caller-selected peer `address`; returns its retained owner.
    fn accepted_link(
        mgr: &'static ConnectionManager<'static, DefaultPacketPool>,
        handle: ConnHandle,
        address: [u8; 6],
    ) -> Connection<'static, DefaultPacketPool> {
        mgr.connect(
            handle,
            Address::new(AddrKind::PUBLIC, BdAddr::new(address)),
            LeConnRole::Central,
            ConnParams::new(),
        )
        .unwrap();
        let Poll::Ready(connection) = mgr.poll_accept(LeConnRole::Central, &[], None) else {
            panic!()
        };
        connection
    }

    /// Queued bytes from a terminal peer must never resolve to another live peer using the same controller handle.
    #[test]
    fn queued_outbound_retains_original_peer_identity() {
        let mgr = setup();
        let handle = ConnHandle::new(7);
        let old = accepted_link(mgr, handle, ADDR_1);
        mgr.try_send(old.index(), Pdu::new(DefaultPacketPool::allocate().unwrap(), 1))
            .unwrap();
        mgr.disconnected(handle, Status::SUCCESS).unwrap();
        let new = accepted_link(mgr, handle, ADDR_2);
        let (lease, _pdu) = block_on(mgr.outbound());
        let acquired = lease.poll_acquire(None);
        assert!(matches!(
            acquired,
            Poll::Ready(Err(Error::Disconnected | Error::NotFound))
        ));
        assert!(!mgr.connection(new.index()).acl_send_locked);
    }

    /// Queue ownership transfers exactly one count; failed immediate sends and canceled capacity waits release their temporary lease and packet.
    #[test]
    fn outbound_queue_transfer_failure_and_cancellation_balance_leases() {
        use core::{future::Future, pin::pin, task::Waker};
        let mgr = setup();
        let connection = accepted_link(mgr, ConnHandle::new(7), ADDR_1);
        let index = connection.index();
        for _ in 0..config::L2CAP_TX_QUEUE_SIZE {
            mgr.try_send(index, Pdu::new(DefaultPacketPool::allocate().unwrap(), 1))
                .unwrap();
        }
        assert_eq!(
            usize::from(mgr.connection(index).io_refcount.get()),
            config::L2CAP_TX_QUEUE_SIZE
        );
        assert_eq!(
            mgr.try_send(index, Pdu::new(DefaultPacketPool::allocate().unwrap(), 1)),
            Err(Error::OutOfMemory)
        );
        assert_eq!(
            usize::from(mgr.connection(index).io_refcount.get()),
            config::L2CAP_TX_QUEUE_SIZE
        );
        {
            let mut pending = pin!(mgr.send(index, Pdu::new(DefaultPacketPool::allocate().unwrap(), 1)));
            assert!(pending
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
                .is_pending());
            assert_eq!(
                usize::from(mgr.connection(index).io_refcount.get()),
                config::L2CAP_TX_QUEUE_SIZE + 1
            );
        }
        assert_eq!(
            usize::from(mgr.connection(index).io_refcount.get()),
            config::L2CAP_TX_QUEUE_SIZE
        );
        for remaining in (0..config::L2CAP_TX_QUEUE_SIZE).rev() {
            let queued = block_on(mgr.outbound());
            assert_eq!(usize::from(mgr.connection(index).io_refcount.get()), remaining + 1);
            drop(queued);
            assert_eq!(usize::from(mgr.connection(index).io_refcount.get()), remaining);
        }
        block_on(mgr.send(index, Pdu::new(DefaultPacketPool::allocate().unwrap(), 1))).unwrap();
        mgr.disconnected(connection.handle(), Status::SUCCESS).unwrap();
        assert_eq!(
            mgr.try_send(index, Pdu::new(DefaultPacketPool::allocate().unwrap(), 1)),
            Err(Error::Disconnected)
        );
        assert_eq!(
            block_on(mgr.send(index, Pdu::new(DefaultPacketPool::allocate().unwrap(), 1))),
            Err(Error::Disconnected)
        );
        drop(block_on(mgr.outbound()));
        assert_eq!(mgr.connection(index).io_refcount.get(), 0);
    }

    /// Already-borrowed enqueue validates origin/state/count and rolls back queue failure without reborrowing the table.
    #[test]
    fn borrowed_storage_enqueue_is_transactional_and_checks_origin() {
        let mgr = setup();
        let other = setup();
        let handle = ConnHandle::new(7);
        let connection = accepted_link(mgr, handle, ADDR_1);
        let foreign = accepted_link(other, handle, ADDR_2);
        other
            .with_connected_handle(handle, |storage| {
                assert_eq!(
                    mgr.try_outbound_storage(storage, Pdu::new(DefaultPacketPool::allocate().unwrap(), 1)),
                    Err(Error::InvalidValue)
                );
                assert_eq!(storage.io_refcount.get(), 0);
                Ok(())
            })
            .unwrap();
        mgr.with_connected_handle(handle, |storage| {
            storage.io_refcount.set(u8::MAX);
            assert_eq!(
                mgr.try_outbound_storage(storage, Pdu::new(DefaultPacketPool::allocate().unwrap(), 1)),
                Err(Error::NoPermits)
            );
            assert_eq!(storage.io_refcount.get(), u8::MAX);
            storage.io_refcount.set(0);
            for _ in 0..config::L2CAP_TX_QUEUE_SIZE {
                mgr.try_outbound_storage(storage, Pdu::new(DefaultPacketPool::allocate().unwrap(), 1))
                    .unwrap();
            }
            assert_eq!(
                mgr.try_outbound_storage(storage, Pdu::new(DefaultPacketPool::allocate().unwrap(), 1)),
                Err(Error::OutOfMemory)
            );
            assert_eq!(usize::from(storage.io_refcount.get()), config::L2CAP_TX_QUEUE_SIZE);
            Ok(())
        })
        .unwrap();
        for _ in 0..config::L2CAP_TX_QUEUE_SIZE {
            drop(block_on(mgr.outbound()));
        }
        mgr.disconnected(handle, Status::SUCCESS).unwrap();
        let storage = mgr.connection_mut(connection.index());
        assert_eq!(
            mgr.try_outbound_storage(&storage, Pdu::new(DefaultPacketPool::allocate().unwrap(), 1)),
            Err(Error::Disconnected)
        );
        assert_eq!(storage.io_refcount.get(), 0);
        drop(storage);
        drop(foreign);
    }

    /// Dropping a manager drains transferred queue counts even when the backing table outlives it.
    #[test]
    fn manager_drop_releases_queued_io_counts() {
        let storage = RefCell::new([ConnectionStorage::new()]);
        #[cfg(feature = "security")]
        let bonds = RefCell::new(heapless::Vec::<crate::BondInformation, 1>::new());
        {
            let mgr = ConnectionManager::<DefaultPacketPool>::new(
                &storage,
                #[cfg(feature = "security")]
                &bonds,
            );
            let handle = ConnHandle::new(7);
            mgr.connect(
                handle,
                Address::new(AddrKind::PUBLIC, BdAddr::new(ADDR_1)),
                LeConnRole::Central,
                ConnParams::new(),
            )
            .unwrap();
            mgr.try_outbound(handle, Pdu::new(DefaultPacketPool::allocate().unwrap(), 1))
                .unwrap();
            assert_eq!(storage.borrow()[0].io_refcount.get(), 1);
        }
        assert_eq!(storage.borrow()[0].io_refcount.get(), 0);
    }

    /// Native layout observation for the lease/queue changes; target-image checks separately establish Cortex-M33 footprint.
    #[test]
    fn io_lease_layout_observation() {
        use core::mem::size_of;
        std::println!(
            "slot={} lock={} grant={} queue_before={} queue_after={} capacity={}",
            size_of::<ConnectionStorage<<DefaultPacketPool as PacketPool>::Packet>>(),
            size_of::<AclSendLock<'_, DefaultPacketPool>>(),
            size_of::<PacketGrant<'_, DefaultPacketPool>>(),
            size_of::<(ConnHandle, Pdu<<DefaultPacketPool as PacketPool>::Packet>)>(),
            size_of::<(usize, Pdu<<DefaultPacketPool as PacketPool>::Packet>)>(),
            config::L2CAP_TX_QUEUE_SIZE
        );
    }

    /// Incidental connection diagnostics must be identical for different IRKs on the same public peer address.
    #[cfg(feature = "security")]
    #[test]
    fn connection_diagnostics_do_not_format_identity_keys() {
        let mut storage = ConnectionStorage::<()>::new();
        storage.peer_identity.irk = crate::IdentityResolvingKey::new(0x112233445566778899aabbccddeeff00);
        let first = std::format!("{:?}", storage);
        storage.peer_identity.irk = crate::IdentityResolvingKey::new(0x887766554433221100ffeeddccbbaa99);
        let second = std::format!("{:?}", storage);
        assert_eq!(first, second);
        assert!(first.contains("peer_address"));
    }

    /// An old ACL lock cannot unlock a different live slot with the reused controller handle.
    #[test]
    fn acl_lock_drop_does_not_unlock_reused_handle() {
        let mgr = setup();
        let handle = ConnHandle::new(7);
        let old = accepted_link(mgr, handle, ADDR_1);
        let Poll::Ready(Ok(old_lock)) = mgr.poll_acquire_acl_send_lock(handle, None) else {
            panic!()
        };
        mgr.disconnected(handle, Status::SUCCESS).unwrap();
        let new = accepted_link(mgr, handle, ADDR_2);
        assert_ne!(old.index(), new.index());
        let Poll::Ready(Ok(new_lock)) = mgr.poll_acquire_acl_send_lock(handle, None) else {
            panic!()
        };
        drop(old_lock);
        assert!(mgr.connection(new.index()).acl_send_locked);
        drop(new_lock);
        assert!(!mgr.connection(new.index()).acl_send_locked);
    }

    /// Canceling an old ACL reservation cannot refund packets into a new peer's reused handle.
    #[test]
    fn packet_grant_drop_does_not_credit_reused_handle() {
        let mgr = setup();
        mgr.set_link_credits(3);
        let handle = ConnHandle::new(7);
        let old = accepted_link(mgr, handle, ADDR_1);
        let Poll::Ready(Ok(old_lock)) = mgr.poll_acquire_acl_send_lock(handle, None) else {
            panic!()
        };
        let Poll::Ready(Ok(grant)) = old_lock.poll_request_to_send(1, None) else {
            panic!()
        };
        mgr.disconnected(handle, Status::SUCCESS).unwrap();
        let new = accepted_link(mgr, handle, ADDR_2);
        assert_ne!(old.index(), new.index());
        assert_eq!(mgr.connection(new.index()).link_credits, 3);
        drop(grant);
        assert_eq!(mgr.connection(new.index()).link_credits, 3);
        assert!(!old_lock.has_link_credits(1));
    }

    /// A grant independently retains a terminal slot after its lock and application owner drop; release permits exact-slot reuse.
    #[test]
    fn packet_grant_outlives_lock_and_blocks_slot_reuse() {
        let mgr = setup();
        mgr.set_link_credits(3);
        let handle = ConnHandle::new(7);
        let old = accepted_link(mgr, handle, ADDR_1);
        let index = old.index();
        let Poll::Ready(Ok(lock)) = mgr.poll_acquire_acl_send_lock(handle, None) else {
            panic!()
        };
        let Poll::Ready(Ok(grant)) = lock.poll_request_to_send(1, None) else {
            panic!()
        };
        drop(lock);
        assert_eq!(mgr.connection(index).io_refcount.get(), 1);
        assert!(!mgr.connection(index).acl_send_locked);
        mgr.disconnected(handle, Status::SUCCESS).unwrap();
        drop(old);
        let other = accepted_link(mgr, handle, ADDR_2);
        assert_ne!(other.index(), index);
        drop(grant);
        assert_eq!(mgr.connection(index).io_refcount.get(), 0);
        let reused = accepted_link(mgr, ConnHandle::new(8), ADDR_1);
        assert_eq!(reused.index(), index);
    }

    /// A pending credit request exits on disconnection and cannot start using a new peer with the same numeric handle.
    #[test]
    fn pending_acl_request_stays_with_disconnected_slot() {
        let mgr = setup();
        let handle = ConnHandle::new(7);
        let old = accepted_link(mgr, handle, ADDR_1);
        let Poll::Ready(Ok(lock)) = mgr.poll_acquire_acl_send_lock(handle, None) else {
            panic!()
        };
        assert!(lock.poll_request_to_send(1, None).is_pending());
        mgr.disconnected(handle, Status::SUCCESS).unwrap();
        drop(old);
        mgr.set_link_credits(3);
        let new = accepted_link(mgr, handle, ADDR_2);
        assert!(matches!(
            lock.poll_request_to_send(1, None),
            Poll::Ready(Err(Error::Disconnected))
        ));
        assert_eq!(mgr.connection(new.index()).link_credits, 3);
    }

    /// Real zero-packet leases exhaust the bounded count without debiting credits or locking on failure; final release restores admission.
    #[test]
    fn io_lease_exhaustion_is_transactional() {
        let mgr = setup();
        mgr.set_link_credits(3);
        let handle = ConnHandle::new(7);
        let connection = accepted_link(mgr, handle, ADDR_1);
        let index = connection.index();
        let Poll::Ready(Ok(lock)) = mgr.poll_acquire_acl_send_lock(handle, None) else {
            panic!()
        };
        let mut grants = std::vec::Vec::new();
        for _ in 0..254 {
            let Poll::Ready(Ok(grant)) = lock.poll_request_to_send(0, None) else {
                panic!()
            };
            grants.push(grant);
        }
        assert_eq!(mgr.connection(index).io_refcount.get(), 255);
        assert!(matches!(
            lock.poll_request_to_send(1, None),
            Poll::Ready(Err(Error::NoPermits))
        ));
        assert_eq!(mgr.connection(index).link_credits, 3);
        drop(lock);
        let Poll::Ready(Ok(lock)) = mgr.poll_acquire_acl_send_lock(handle, None) else {
            panic!()
        };
        drop(grants.pop());
        let Poll::Ready(Ok(grant)) = lock.poll_request_to_send(0, None) else {
            panic!()
        };
        grants.push(grant);
        drop(lock);
        assert_eq!(mgr.connection(index).io_refcount.get(), 254);
        drop(grants);
        assert_eq!(mgr.connection(index).io_refcount.get(), 0);
        assert_eq!(mgr.connection(index).link_credits, 3);
    }

    /// ACL traffic before application acceptance uses independent I/O ownership and does not violate acceptance's zero-owner invariant.
    #[test]
    fn connecting_acl_lease_does_not_prevent_acceptance() {
        let mgr = setup();
        let handle = ConnHandle::new(7);
        mgr.connect(
            handle,
            Address::new(AddrKind::PUBLIC, BdAddr::new(ADDR_1)),
            LeConnRole::Central,
            ConnParams::new(),
        )
        .unwrap();
        let Poll::Ready(Ok(lock)) = mgr.poll_acquire_acl_send_lock(handle, None) else {
            panic!()
        };
        let Poll::Ready(Ok(grant)) = lock.poll_request_to_send(0, None) else {
            panic!()
        };
        let Poll::Ready(connection) = mgr.poll_accept(LeConnRole::Central, &[], None) else {
            panic!()
        };
        assert_eq!(mgr.connection(connection.index()).refcount, 1);
        assert_eq!(mgr.connection(connection.index()).io_refcount.get(), 2);
        drop(grant);
        drop(lock);
        assert!(connection.is_connected());
    }

    /// Reusing a slot starts with no OOB capability or negotiated key-length observation from its previous peer.
    #[cfg(feature = "security")]
    #[test]
    fn reused_slot_resets_peer_security_preferences() {
        let mgr = setup();
        let handle = ConnHandle::new(7);
        let old = accepted_link(mgr, handle, ADDR_1);
        let index = old.index();
        old.set_oob_available(true).unwrap();
        #[cfg(feature = "legacy-pairing")]
        {
            mgr.connection_mut(index).encryption_key_len = 16;
        }
        mgr.disconnected(handle, Status::SUCCESS).unwrap();
        drop(old);
        let new = accepted_link(mgr, handle, ADDR_2);
        assert_eq!(new.index(), index);
        assert!(!mgr.connection(index).oob_available);
        #[cfg(feature = "legacy-pairing")]
        assert_eq!(new.encryption_key_len().unwrap(), 0);
    }

    /// A controller connection-rate event updates stored parameters even when notification queue admission fails.
    #[test]
    fn connection_rate_event_updates_retained_parameters() {
        let mgr = setup();
        let handle = ConnHandle::new(7);
        let conn = accepted_link(mgr, handle, ADDR_1);
        let event = ConnectionEvent::ConnectionRateChanged {
            conn_interval: embassy_time::Duration::from_millis(60),
            subrate_factor: 3,
            peripheral_latency: 2,
            continuation_number: 1,
            supervision_timeout: embassy_time::Duration::from_secs(4),
        };
        mgr.post_handle_event(handle, event).unwrap();
        assert_eq!(conn.params().conn_interval, embassy_time::Duration::from_millis(60));
        assert_eq!(conn.params().peripheral_latency, 2);
        assert_eq!(conn.params().supervision_timeout, embassy_time::Duration::from_secs(4));
        #[cfg(feature = "subrating")]
        assert_eq!(
            conn.subrating_params(),
            Some(SubratingParams {
                subrate_factor: 3,
                continuation_number: 1
            })
        );
        while mgr
            .post_handle_event(
                handle,
                ConnectionEvent::PhyUpdated {
                    tx_phy: bt_hci::param::PhyKind::Le1M,
                    rx_phy: bt_hci::param::PhyKind::Le1M,
                },
            )
            .is_ok()
        {}
        let result = mgr.post_handle_event(
            handle,
            ConnectionEvent::ConnectionRateChanged {
                conn_interval: embassy_time::Duration::from_millis(80),
                subrate_factor: 1,
                peripheral_latency: 0,
                continuation_number: 0,
                supervision_timeout: embassy_time::Duration::from_secs(5),
            },
        );
        assert_eq!(result, Err(Error::OutOfMemory));
        assert_eq!(conn.params().conn_interval, embassy_time::Duration::from_millis(80));
        assert_eq!(conn.params().supervision_timeout, embassy_time::Duration::from_secs(5));
        #[cfg(feature = "subrating")]
        assert_eq!(conn.subrating_params(), None);
    }
}
