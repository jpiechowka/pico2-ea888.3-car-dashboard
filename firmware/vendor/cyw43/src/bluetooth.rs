use core::cell::{Cell, RefCell};
use core::convert::Infallible;
use core::future::Future;
use core::mem::MaybeUninit;

use bt_hci_transport::{PacketKind, PacketToController, ReadHciError, WithIndicator};
use embassy_futures::yield_now;
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_sync::zerocopy_channel;
use embassy_time::{Duration, Timer};
use embedded_io_async::ErrorKind;

use crate::consts::*;
use crate::runner::Bus;
pub use crate::spi::SpiBusCyw43;
use crate::util::round_up;
use crate::{CHIP, util};

pub(crate) struct BtState {
    rx: [BtPacketBuf; 4],
    tx: [BtPacketBuf; 4],
    inner: MaybeUninit<BtStateInnre<'static>>,
}

impl BtState {
    pub const fn new() -> Self {
        Self {
            rx: [const { BtPacketBuf::new() }; 4],
            tx: [const { BtPacketBuf::new() }; 4],
            inner: MaybeUninit::uninit(),
        }
    }
}

struct BtStateInnre<'d> {
    faulted: Cell<bool>,
    rx: zerocopy_channel::Channel<'d, NoopRawMutex, BtPacketBuf>,
    tx: zerocopy_channel::Channel<'d, NoopRawMutex, BtPacketBuf>,
}

/// Bluetooth driver.
pub struct BtDriver<'d> {
    faulted: &'d Cell<bool>,
    rx: RefCell<zerocopy_channel::Receiver<'d, NoopRawMutex, BtPacketBuf>>,
    tx: Mutex<NoopRawMutex, zerocopy_channel::Sender<'d, NoopRawMutex, BtPacketBuf>>,
}

pub(crate) struct BtRunner<'d> {
    pub(crate) tx_chan: zerocopy_channel::Receiver<'d, NoopRawMutex, BtPacketBuf>,
    rx_chan: zerocopy_channel::Sender<'d, NoopRawMutex, BtPacketBuf>,

    // Bluetooth circular buffers
    addr: u32,
    h2b_write_pointer: u32,
    b2h_read_pointer: u32,
    faulted: &'d Cell<bool>,
}

// HCI command <= 3 + 255 bytes, event <= 2 + 255, linked ACL <= 4 + 251.
// The largest rounded payload (260) plus indicator (1) fits 272. The GSPI
// ring header is separate. Larger packets fail explicitly, never truncate.
const BT_HCI_MTU: usize = if cfg!(feature = "bluetooth-only") { 272 } else { 1024 };

/// Represents a packet of size MTU.
pub(crate) struct BtPacketBuf {
    pub(crate) len: usize,
    pub(crate) buf: [u8; BT_HCI_MTU],
}

impl BtPacketBuf {
    /// Create a new packet buffer.
    pub const fn new() -> Self {
        Self {
            len: 0,
            buf: [0; BT_HCI_MTU],
        }
    }
}

pub(crate) fn new<'d>(state: &'d mut BtState) -> (BtRunner<'d>, BtDriver<'d>) {
    // safety: this is a self-referential struct, however:
    // - it can't move while the `'d` borrow is active.
    // - when the borrow ends, the dangling references inside the MaybeUninit will never be used again.
    let state_uninit: *mut MaybeUninit<BtStateInnre<'d>> =
        (&mut state.inner as *mut MaybeUninit<BtStateInnre<'static>>).cast();
    let state = unsafe { &mut *state_uninit }.write(BtStateInnre {
        faulted: Cell::new(false),
        rx: zerocopy_channel::Channel::new(&mut state.rx[..]),
        tx: zerocopy_channel::Channel::new(&mut state.tx[..]),
    });

    let (rx_sender, rx_receiver) = state.rx.split();
    let (tx_sender, tx_receiver) = state.tx.split();

    (
        BtRunner {
            tx_chan: tx_receiver,
            rx_chan: rx_sender,

            addr: 0,
            h2b_write_pointer: 0,
            b2h_read_pointer: 0,
            faulted: &state.faulted,
        },
        BtDriver {
            faulted: &state.faulted,
            rx: RefCell::new(rx_receiver),
            tx: Mutex::new(tx_sender),
        },
    )
}

impl<'a> BtRunner<'a> {
    pub(crate) async fn init_bluetooth(&mut self, bus: &mut impl Bus, firmware: &[u8]) -> Result<(), ()> {
        trace!("init_bluetooth");
        bus.bp_write32(CHIP.bluetooth_base_address + BT2WLAN_PWRUP_ADDR, BT2WLAN_PWRUP_WAKE)
            .await;
        Timer::after(Duration::from_millis(2)).await;
        self.upload_bluetooth_firmware(bus, firmware).await?;
        self.wait_bt_ready(bus).await;
        self.init_bt_buffers(bus).await;
        self.wait_bt_awake(bus).await;
        self.bt_set_host_ready(bus).await;
        self.bt_toggle_intr(bus).await;
        Ok(())
    }

    pub(crate) async fn upload_bluetooth_firmware(&mut self, bus: &mut impl Bus, firmware: &[u8]) -> Result<(), ()> {
        let mut firmware = crate::bluetooth_firmware::Firmware::new(firmware, CHIP.bluetooth_base_address)?;
        // A u8-length record can need three prefix bytes plus end rounding.
        let mut aligned: [u8; 260] = [0; 260];
        while let Some((dest, bytes)) = firmware.next_data()? {
            let start = util::round_down(dest, 4);
            let prefix = (dest - start) as usize;
            let end = prefix + bytes.len();
            let len = (end + 3) & !3;
            if prefix != 0 {
                let old = bus.bp_read32(start).await.to_le_bytes();
                aligned[..prefix].copy_from_slice(&old[..prefix]);
            }
            aligned[prefix..end].copy_from_slice(bytes);
            if end != len {
                let old = bus.bp_read32(start + (end & !3) as u32).await.to_le_bytes();
                aligned[end..len].copy_from_slice(&old[end & 3..]);
            }
            bus.bp_write(start, &aligned[..len]).await;
        }
        Ok(())
    }

    pub(crate) async fn wait_bt_ready(&mut self, bus: &mut impl Bus) {
        trace!("wait_bt_ready");
        let mut success = false;
        for _ in 0..300 {
            let val = bus.bp_read32(BT_CTRL_REG_ADDR).await;
            trace!("BT_CTRL_REG_ADDR = {:08x}", val);
            if val & BTSDIO_REG_FW_RDY_BITMASK != 0 {
                success = true;
                break;
            }
            Timer::after(Duration::from_millis(1)).await;
        }
        assert!(success == true);
    }

    pub(crate) async fn wait_bt_awake(&mut self, bus: &mut impl Bus) {
        trace!("wait_bt_awake");
        let mut success = false;
        for _ in 0..300 {
            let val = bus.bp_read32(BT_CTRL_REG_ADDR).await;
            trace!("BT_CTRL_REG_ADDR = {:08x}", val);
            if val & BTSDIO_REG_BT_AWAKE_BITMASK != 0 {
                success = true;
                break;
            }
            Timer::after(Duration::from_millis(1)).await;
        }
        assert!(success == true);
    }

    pub(crate) async fn bt_set_host_ready(&mut self, bus: &mut impl Bus) {
        trace!("bt_set_host_ready");
        let old_val = bus.bp_read32(HOST_CTRL_REG_ADDR).await;
        // TODO: do we need to swap endianness on this read?
        let new_val = old_val | BTSDIO_REG_SW_RDY_BITMASK;
        bus.bp_write32(HOST_CTRL_REG_ADDR, new_val).await;
    }

    // TODO: use this
    #[allow(dead_code)]
    pub(crate) async fn bt_set_awake(&mut self, bus: &mut impl Bus, awake: bool) {
        trace!("bt_set_awake");
        let old_val = bus.bp_read32(HOST_CTRL_REG_ADDR).await;
        // TODO: do we need to swap endianness on this read?
        let new_val = if awake {
            old_val | BTSDIO_REG_WAKE_BT_BITMASK
        } else {
            old_val & !BTSDIO_REG_WAKE_BT_BITMASK
        };
        bus.bp_write32(HOST_CTRL_REG_ADDR, new_val).await;
    }

    pub(crate) async fn bt_toggle_intr(&mut self, bus: &mut impl Bus) {
        trace!("bt_toggle_intr");
        let old_val = bus.bp_read32(HOST_CTRL_REG_ADDR).await;
        // TODO: do we need to swap endianness on this read?
        let new_val = old_val ^ BTSDIO_REG_DATA_VALID_BITMASK;
        bus.bp_write32(HOST_CTRL_REG_ADDR, new_val).await;
    }

    // TODO: use this
    #[allow(dead_code)]
    pub(crate) async fn bt_set_intr(&mut self, bus: &mut impl Bus) {
        trace!("bt_set_intr");
        let old_val = bus.bp_read32(HOST_CTRL_REG_ADDR).await;
        let new_val = old_val | BTSDIO_REG_DATA_VALID_BITMASK;
        bus.bp_write32(HOST_CTRL_REG_ADDR, new_val).await;
    }

    pub(crate) async fn init_bt_buffers(&mut self, bus: &mut impl Bus) {
        trace!("init_bt_buffers");
        self.addr = bus.bp_read32(WLAN_RAM_BASE_REG_ADDR).await;
        assert!(self.addr != 0);
        trace!("wlan_ram_base_addr = {:08x}", self.addr);
        bus.bp_write32(self.addr + BTSDIO_OFFSET_HOST2BT_IN, 0).await;
        bus.bp_write32(self.addr + BTSDIO_OFFSET_HOST2BT_OUT, 0).await;
        bus.bp_write32(self.addr + BTSDIO_OFFSET_BT2HOST_IN, 0).await;
        bus.bp_write32(self.addr + BTSDIO_OFFSET_BT2HOST_OUT, 0).await;
    }

    async fn bt_bus_request(&mut self, bus: &mut impl Bus) {
        // TODO: CYW43_THREAD_ENTER mutex?
        self.bt_set_awake(bus, true).await;
        self.wait_bt_awake(bus).await;
    }

    fn fail_transport(&mut self) {
        self.faulted.set(true);
        // Wake blocked writers, and wake a reader even when no packet arrived.
        while self.tx_chan.try_receive().is_some() {
            self.tx_chan.receive_done();
        }
        if let Some(buf) = self.rx_chan.try_send() {
            buf.len = 0;
            self.rx_chan.send_done();
        }
    }

    pub(crate) async fn hci_write(&mut self, bus: &mut impl Bus) {
        if self.faulted.get() {
            return;
        }
        self.bt_bus_request(bus).await;

        // NOTE(unwrap): we only call this when we do have a packet in the queue.
        let buf = self.tx_chan.try_receive().unwrap();
        debug!("HCI tx: {:02x}", crate::fmt::Bytes(&buf.buf[..buf.len]));

        let len = buf.len as u32 - 1; // len doesn't include hci type byte
        let rounded_len = round_up(len, 4);
        let total_len = 4 + rounded_len;

        let read_pointer = bus.bp_read32(self.addr + BTSDIO_OFFSET_HOST2BT_OUT).await;
        if !valid_ring_pointer(read_pointer) {
            self.fail_transport();
            return;
        }
        let available = read_pointer.wrapping_sub(self.h2b_write_pointer + 4) % BTSDIO_FWBUF_SIZE;
        if available < total_len {
            warn!(
                "bluetooth tx queue full, retrying. len {} available {}",
                total_len, available
            );
            yield_now().await;
            return;
        }

        // Build header
        let mut header = [0u8; 4];
        header[0] = len as u8;
        header[1] = (len >> 8) as u8;
        header[2] = (len >> 16) as u8;
        header[3] = buf.buf[0]; // HCI type byte

        // Write header
        let addr = self.addr + BTSDIO_OFFSET_HOST_WRITE_BUF + self.h2b_write_pointer;
        bus.bp_write(addr, &header).await;
        self.h2b_write_pointer = (self.h2b_write_pointer + 4) % BTSDIO_FWBUF_SIZE;

        // Write payload.
        // Explicitly clear padding; never send bytes left by a previous packet.
        buf.buf[1 + len as usize..1 + rounded_len as usize].fill(0);
        let payload = &buf.buf[1..][..rounded_len as usize];
        if self.h2b_write_pointer as usize + payload.len() > BTSDIO_FWBUF_SIZE as usize {
            // wraparound
            let n = BTSDIO_FWBUF_SIZE - self.h2b_write_pointer;
            let addr = self.addr + BTSDIO_OFFSET_HOST_WRITE_BUF + self.h2b_write_pointer;
            bus.bp_write(addr, &payload[..n as usize]).await;
            let addr = self.addr + BTSDIO_OFFSET_HOST_WRITE_BUF;
            bus.bp_write(addr, &payload[n as usize..]).await;
        } else {
            // no wraparound
            let addr = self.addr + BTSDIO_OFFSET_HOST_WRITE_BUF + self.h2b_write_pointer;
            bus.bp_write(addr, payload).await;
        }
        self.h2b_write_pointer = (self.h2b_write_pointer + payload.len() as u32) % BTSDIO_FWBUF_SIZE;

        // Update pointer.
        bus.bp_write32(self.addr + BTSDIO_OFFSET_HOST2BT_IN, self.h2b_write_pointer)
            .await;

        self.bt_toggle_intr(bus).await;

        self.tx_chan.receive_done();
    }

    async fn bt_has_work(&mut self, bus: &mut impl Bus) -> bool {
        let int_status = bus.bp_read32(CHIP.sdiod_core_base_address + SDIO_INT_STATUS).await;
        if int_status & I_HMB_FC_CHANGE != 0 {
            bus.bp_write32(
                CHIP.sdiod_core_base_address + SDIO_INT_STATUS,
                int_status & I_HMB_FC_CHANGE,
            )
            .await;
            return true;
        }
        return false;
    }

    pub(crate) async fn handle_irq(&mut self, bus: &mut impl Bus) {
        if !self.faulted.get() {
            // Retry a pending ring even after the IRQ was cleared while the
            // bounded host queue was full. Never wait below the park boundary.
            let _ = self.bt_has_work(bus).await;
            for _ in 0..4 {
                // Check if we have data.
                let write_pointer = bus.bp_read32(self.addr + BTSDIO_OFFSET_BT2HOST_IN).await;
                if !valid_ring_pointer(write_pointer) {
                    self.fail_transport();
                    break;
                }
                let available = write_pointer.wrapping_sub(self.b2h_read_pointer) % BTSDIO_FWBUF_SIZE;
                if available == 0 {
                    break;
                }

                // read header
                let mut header = [0u8; 4];
                let addr = self.addr + BTSDIO_OFFSET_HOST_READ_BUF + self.b2h_read_pointer;
                bus.bp_read(addr, &mut header).await;

                // calc length
                let len = header[0] as u32 | ((header[1]) as u32) << 8 | ((header[2]) as u32) << 16;
                let rounded_len = round_up(len, 4);
                let Some(buf) = self.rx_chan.try_send() else { break };
                if len == 0 || rounded_len as usize >= buf.buf.len() {
                    // Poison the transport once, retain the ring, and stop all
                    // Bluetooth bus traffic until reset. No truncated packet.
                    self.fail_transport();
                    break;
                }
                if available < 4 + rounded_len {
                    self.fail_transport();
                    break;
                }
                self.b2h_read_pointer = (self.b2h_read_pointer + 4) % BTSDIO_FWBUF_SIZE;

                // Obtain a buf from the channel.
                buf.buf[0] = header[3]; // hci packet type
                let payload = &mut buf.buf[1..][..rounded_len as usize];
                if self.b2h_read_pointer as usize + payload.len() > BTSDIO_FWBUF_SIZE as usize {
                    // wraparound
                    let n = BTSDIO_FWBUF_SIZE - self.b2h_read_pointer;
                    let addr = self.addr + BTSDIO_OFFSET_HOST_READ_BUF + self.b2h_read_pointer;
                    bus.bp_read(addr, &mut payload[..n as usize]).await;
                    let addr = self.addr + BTSDIO_OFFSET_HOST_READ_BUF;
                    bus.bp_read(addr, &mut payload[n as usize..]).await;
                } else {
                    // no wraparound
                    let addr = self.addr + BTSDIO_OFFSET_HOST_READ_BUF + self.b2h_read_pointer;
                    bus.bp_read(addr, payload).await;
                }
                self.b2h_read_pointer = (self.b2h_read_pointer + payload.len() as u32) % BTSDIO_FWBUF_SIZE;
                bus.bp_write32(self.addr + BTSDIO_OFFSET_BT2HOST_OUT, self.b2h_read_pointer)
                    .await;

                buf.len = 1 + len as usize;
                debug!("HCI rx: {:02x}", crate::fmt::Bytes(&buf.buf[..buf.len]));

                self.rx_chan.send_done();

                self.bt_toggle_intr(bus).await;
            }
        }
    }
}

/// HCI transport error.
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Debug)]
pub enum Error {
    /// I/O error.
    Io(ErrorKind),
}

impl From<ReadHciError<Infallible>> for Error {
    fn from(e: ReadHciError<Infallible>) -> Self {
        match e {
            ReadHciError::BufferTooSmall => Error::Io(ErrorKind::InvalidInput),
            ReadHciError::InvalidValue => Error::Io(ErrorKind::InvalidData),
            ReadHciError::Read(_) => Error::Io(ErrorKind::BrokenPipe),
        }
    }
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::Debug::fmt(self, f)
    }
}

impl core::error::Error for Error {}

impl<'d> embedded_io_async::ErrorType for BtDriver<'d> {
    type Error = Error;
}

impl embedded_io_async::Error for Error {
    fn kind(&self) -> ErrorKind {
        match self {
            Self::Io(e) => *e,
        }
    }
}

impl<'d> bt_hci_transport::Transport for BtDriver<'d> {
    fn read<'a, P: bt_hci_transport::PacketToHost<'a>>(
        &self,
        rx: &'a mut [u8],
    ) -> impl Future<Output = Result<P, Self::Error>> {
        async {
            if self.faulted.get() {
                return Err(Error::Io(ErrorKind::BrokenPipe));
            }
            let ch = &mut *self.rx.borrow_mut();
            let buf = ch.receive().await;
            if self.faulted.get() {
                ch.receive_done();
                return Err(Error::Io(ErrorKind::BrokenPipe));
            }
            let mut reader = &buf.buf[..buf.len];
            let result = PacketKind::read(&mut reader).and_then(|kind| P::read_hci(kind, &mut reader, rx));
            let trailing = !reader.is_empty();
            ch.receive_done();
            if trailing {
                return Err(Error::Io(ErrorKind::InvalidData));
            }
            result.map_err(Error::from)
        }
    }

    /// Write a complete HCI packet from the tx buffer
    fn write<T: PacketToController>(&self, val: &T) -> impl Future<Output = Result<(), Self::Error>> {
        async {
            // Indicator + rounded payload must fit even on the largest packet.
            if val.size() > (BT_HCI_MTU - 1) & !3 {
                return Err(Error::Io(ErrorKind::InvalidInput));
            }
            if self.faulted.get() {
                return Err(Error::Io(ErrorKind::BrokenPipe));
            }
            let mut guard = self.tx.lock().await;
            let ch = &mut *guard;
            let buf = ch.send().await;
            if self.faulted.get() {
                return Err(Error::Io(ErrorKind::BrokenPipe));
            }
            let buf_len = buf.buf.len();
            let mut slice = &mut buf.buf[..];
            WithIndicator::new(val)
                .write_hci(&mut slice)
                .map_err(|_| Error::Io(ErrorKind::Other))?;
            buf.len = buf_len - slice.len();
            ch.send_done();
            Ok(())
        }
    }
}

fn valid_ring_pointer(pointer: u32) -> bool {
    pointer < BTSDIO_FWBUF_SIZE && pointer % 4 == 0
}

#[cfg(test)]
impl BtDriver<'_> {
    pub(crate) fn test_faulted(&self) -> bool {
        self.faulted.get()
    }

    pub(crate) fn queue_test_packet(&self) {
        let mut tx = self.tx.try_lock().unwrap();
        let buf = tx.try_send().unwrap();
        buf.len = 5;
        buf.buf[..5].copy_from_slice(&[1, 0, 0, 0, 0]);
        tx.send_done();
    }
}

#[cfg(test)]
#[path = "bluetooth_ring_tests.rs"]
mod ring_tests;

#[cfg(test)]
mod tests {
    extern crate std;
    use embassy_futures::block_on;

    use super::*;
    use crate::regression_tests::MockBus;
    use bt_hci_transport::Transport;

    #[test]
    fn maximum_hci_event_and_linked_acl_fit_bluetooth_only_buffers() {
        let mut state = BtState::new();
        let (mut runner, driver) = new(&mut state);
        let packet = runner.rx_chan.try_send().unwrap();
        packet.len = 258; // indicator + event code + u8 parameter length + 255
        packet.buf[..3].copy_from_slice(&[4, 0xff, 255]);
        runner.rx_chan.send_done();
        let mut rx = [0; 259]; // Actual bt-hci 0.10 ExternalController buffer.
        let event = block_on(driver.read::<bt_hci::ControllerToHostPacket<'_>>(&mut rx)).unwrap();
        assert!(matches!(event, bt_hci::ControllerToHostPacket::Event(event) if event.data.len() == 255));
        let payload = [0u8; 251];
        let acl = bt_hci::data::AclPacket::new(
            bt_hci::param::ConnHandle::new(0),
            bt_hci::data::AclPacketBoundary::FirstNonFlushable,
            bt_hci::data::AclBroadcastFlag::PointToPoint,
            &payload,
        );
        block_on(driver.write(&acl)).unwrap();
        assert_eq!(runner.tx_chan.try_receive().unwrap().len, 256);
        runner.tx_chan.receive_done();
        let command = bt_hci::cmd::le::LeSetExtAdvData::new(
            bt_hci::param::AdvHandle::new(0),
            bt_hci::param::Operation::Complete,
            false,
            &payload,
        );
        block_on(driver.write(&command)).unwrap();
        assert_eq!(runner.tx_chan.try_receive().unwrap().len, 259); // 3 + 255 + indicator
        block_on(runner.hci_write(&mut MockBus::default())); // Includes padding to260.
        assert!(runner.tx_chan.try_receive().is_none());
    }

    #[test]
    fn malformed_hci_is_consumed_and_fault_wakes_blocked_transport() {
        use core::{
            pin::pin,
            task::{Context, Poll, Waker},
        };
        let mut state = BtState::new();
        let (mut runner, driver) = new(&mut state);
        let packet = runner.rx_chan.try_send().unwrap();
        packet.len = 1;
        packet.buf[0] = 0xff;
        runner.rx_chan.send_done();
        let mut rx = [0; 259];
        assert!(block_on(driver.read::<bt_hci::ControllerToHostPacket<'_>>(&mut rx)).is_err());
        assert!(driver.rx.borrow_mut().try_receive().is_none());
        for _ in 0..4 {
            driver.queue_test_packet();
        }
        let command = bt_hci::cmd::controller_baseband::Reset::new();
        let mut write = pin!(driver.write(&command));
        let mut second_write = pin!(driver.write(&command));
        let mut read = pin!(driver.read::<bt_hci::ControllerToHostPacket<'_>>(&mut rx));
        let mut cx = Context::from_waker(Waker::noop());
        assert!(write.as_mut().poll(&mut cx).is_pending());
        assert!(second_write.as_mut().poll(&mut cx).is_pending());
        assert!(read.as_mut().poll(&mut cx).is_pending());
        runner.fail_transport();
        assert!(matches!(
            write.as_mut().poll(&mut cx),
            Poll::Ready(Err(Error::Io(ErrorKind::BrokenPipe)))
        ));
        assert!(matches!(
            second_write.as_mut().poll(&mut cx),
            Poll::Ready(Err(Error::Io(ErrorKind::BrokenPipe)))
        ));
        assert!(matches!(
            read.as_mut().poll(&mut cx),
            Poll::Ready(Err(Error::Io(ErrorKind::BrokenPipe)))
        ));
    }

    #[test]
    fn oversized_tx_is_rejected_without_publishing_or_panicking() {
        let mut state = BtState::new();
        let (mut runner, driver) = new(&mut state);
        let payload = [0u8; BT_HCI_MTU];
        let packet = bt_hci::data::AclPacket::new(
            bt_hci::param::ConnHandle::new(0),
            bt_hci::data::AclPacketBoundary::FirstNonFlushable,
            bt_hci::data::AclBroadcastFlag::PointToPoint,
            &payload,
        );
        assert!(matches!(
            block_on(driver.write(&packet)),
            Err(Error::Io(ErrorKind::InvalidInput))
        ));
        assert!(runner.tx_chan.try_receive().is_none());
    }

    #[test]
    fn invalid_ring_pointers_and_packet_lengths_fault_without_advancing() {
        for (pointer, length) in [
            (1, 4),
            (4096, 4),
            (u32::MAX, 4),
            (8, 0),
            ((BT_HCI_MTU + 4) as u32, (BT_HCI_MTU - 3) as u32),
            (8, 0xFF_FFFF),
            (4, 1),
        ] {
            let mut state = BtState::new();
            let (mut runner, driver) = new(&mut state);
            for _ in 0..4 {
                driver.queue_test_packet();
            }
            let mut bus = MockBus {
                pointer,
                header: [length as u8, (length >> 8) as u8, (length >> 16) as u8, 4],
                ..Default::default()
            };
            block_on(runner.handle_irq(&mut bus));
            assert!(driver.test_faulted(), "pointer={pointer}, length={length}");
            assert_eq!(runner.b2h_read_pointer, 0);
            assert!(runner.tx_chan.try_receive().is_none());
            let writes = bus.writes;
            block_on(runner.hci_write(&mut bus));
            block_on(runner.handle_irq(&mut bus));
            assert_eq!(bus.writes, writes, "faulted transport must stop bus work");
        }
    }

    #[test]
    fn maximum_packet_and_ring_wrap_preserve_exact_length() {
        for (start, end, length) in [(0, BT_HCI_MTU as u32, (BT_HCI_MTU - 4) as u32), (4092, 4, 1)] {
            let mut state = BtState::new();
            let (mut runner, driver) = new(&mut state);
            runner.b2h_read_pointer = start;
            let mut bus = MockBus {
                pointer: end,
                header: [length as u8, (length >> 8) as u8, 0, 4],
                ..Default::default()
            };
            block_on(runner.handle_irq(&mut bus));
            assert!(!driver.test_faulted());
            assert_eq!(runner.b2h_read_pointer, end);
            let mut rx = driver.rx.borrow_mut();
            let packet = rx.try_receive().unwrap();
            assert_eq!(packet.len, length as usize + 1);
            assert_eq!(packet.buf[0], 4);
        }
    }

    #[test]
    fn full_host_queue_retains_ring_then_retries_without_new_irq() {
        let mut state = BtState::new();
        let (mut runner, driver) = new(&mut state);
        for _ in 0..4 {
            runner.rx_chan.try_send().unwrap().len = 1;
            runner.rx_chan.send_done();
        }
        let mut bus = MockBus {
            pointer: 8,
            header: [1, 0, 0, 4],
            ..Default::default()
        };
        block_on(runner.handle_irq(&mut bus));
        assert_eq!(runner.b2h_read_pointer, 0);
        driver.rx.borrow_mut().receive_done();
        block_on(runner.handle_irq(&mut bus));
        assert_eq!(runner.b2h_read_pointer, 8);
        assert!(!driver.test_faulted());
    }

    #[test]
    fn firmware_record_alignment_handles_largest_record() {
        let mut state = BtState::new();
        let (mut runner, _) = new(&mut state);
        let mut firmware = [0u8; 3 + 4 + 255 + 4];
        firmware[..3].copy_from_slice(&[1, 0, 2]);
        firmware[3..7].copy_from_slice(&[255, 0, 3, BTFW_HEX_LINE_TYPE_DATA]);
        *firmware.last_mut().unwrap() = BTFW_HEX_LINE_TYPE_END_OF_DATA;
        block_on(runner.upload_bluetooth_firmware(&mut MockBus::default(), &firmware)).unwrap();
    }

    #[test]
    fn firmware_rejects_truncation_without_panicking_or_writing() {
        for firmware in [&[][..], &[255], &[1, 0], &[1, 0, 1, 4, 0, 0, 0, 1]] {
            let mut state = BtState::new();
            let (mut runner, _) = new(&mut state);
            let mut bus = MockBus::default();
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                assert!(block_on(runner.upload_bluetooth_firmware(&mut bus, firmware)).is_err());
            }));
            assert!(result.is_ok(), "truncated firmware panicked");
            assert_eq!(bus.writes, 0);
        }
    }

    #[test]
    fn firmware_validates_later_records_before_any_write() {
        let mut state = BtState::new();
        let (mut runner, _) = new(&mut state);
        let mut bus = MockBus::default();
        // Valid data followed by an unknown record, then EOF.
        let firmware = [1, 0, 3, 1, 0, 0, 0, 42, 1, 0, 0, 99, 1, 0, 0, 0, 1];
        assert!(block_on(runner.upload_bluetooth_firmware(&mut bus, &firmware)).is_err());
        assert_eq!(bus.writes, 0, "partially wrote an invalid image");
    }

    #[test]
    fn firmware_upload_preserves_both_neighbouring_word_fragments() {
        for offset in 0..4u8 {
            for len in [1, 2, 3, 4, 254, 255u8] {
                let mut firmware = std::vec![1, 0, 2, len, 0, offset, 0];
                let data: std::vec::Vec<u8> = (0..len).collect();
                firmware.extend_from_slice(&data);
                firmware.extend_from_slice(&[0, 0, 0, 1]);
                let mut state = BtState::new();
                let (mut runner, _) = new(&mut state);
                let mut bus = MockBus::default();
                block_on(runner.upload_bluetooth_firmware(&mut bus, &firmware)).unwrap();
                let end = usize::from(offset) + usize::from(len);
                let mut expected = std::vec![0xff; (end + 3) & !3];
                expected[usize::from(offset)..end].copy_from_slice(&data);
                assert_eq!(bus.patch_writes, [(CHIP.bluetooth_base_address, expected)]);
            }
        }
    }
}
