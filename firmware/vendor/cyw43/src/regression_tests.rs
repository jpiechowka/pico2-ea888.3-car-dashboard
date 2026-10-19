extern crate std;

use core::cell::Cell;
use core::future::Future;
use core::pin::pin;
use core::task::{Context, Poll, Waker};

use aligned::{A4, Aligned};
use embassy_futures::block_on;

use crate::consts::*;
use crate::runner::{BusType, Runner, SealedBus};

#[derive(Default)]
pub(crate) struct MockBus {
    pub pointer: u32,
    pub header: [u8; 4],
    pub reads: usize,
    pub writes: usize,
    pub f2_reads: usize,
    pub f2_continuous: bool,
    pub ring: Option<std::boxed::Box<[u8; 4096]>>,
    pub base: u32,
    pub acknowledged: Option<u32>,
}

impl MockBus {
    /// Address-backed firmware ring. Nonzero padding must never reach HCI.
    pub(crate) fn ring(start: usize, packets: &[&[u8]]) -> Self {
        let mut ring = std::boxed::Box::new([0xa5; 4096]);
        let mut cursor = start;
        for packet in packets {
            let payload = &packet[1..];
            let length = payload.len();
            let header = [length as u8, (length >> 8) as u8, (length >> 16) as u8, packet[0]];
            for byte in header.iter().chain(payload) {
                ring[cursor] = *byte;
                cursor = (cursor + 1) % 4096;
            }
            cursor = ((cursor + 3) & !3) % 4096;
        }
        Self {
            ring: Some(ring),
            base: 0x180000,
            pointer: cursor as u32,
            ..Default::default()
        }
    }
}

impl SealedBus for MockBus {
    const TYPE: BusType = BusType::Spi;

    async fn init(&mut self, _: bool) {}

    async fn wlan_read(&mut self, buf: &mut Aligned<A4, [u8]>) {
        self.f2_reads += 1;
        buf.fill(0);
    }

    async fn wlan_write(&mut self, _: &Aligned<A4, [u8]>) {
        self.writes += 1;
    }

    async fn bp_read(&mut self, addr: u32, data: &mut [u8]) {
        self.reads += 1;
        if let Some(ring) = &self.ring {
            let offset = addr.checked_sub(self.base + BTSDIO_OFFSET_HOST_READ_BUF).unwrap() as usize;
            data.copy_from_slice(&ring[offset..offset + data.len()]);
            return;
        }
        data.fill(0);
        if data.len() == 4 {
            data.copy_from_slice(&self.header);
        }
    }

    async fn bp_write(&mut self, _: u32, _: &[u8]) {
        self.writes += 1;
    }

    async fn bp_read8(&mut self, _: u32) -> u8 {
        0
    }

    async fn bp_write8(&mut self, _: u32, _: u8) {}

    async fn bp_read16(&mut self, _: u32) -> u16 {
        0
    }

    async fn bp_write16(&mut self, _: u32, _: u16) {}

    async fn bp_read32(&mut self, addr: u32) -> u32 {
        if addr == self.base + BTSDIO_OFFSET_BT2HOST_IN || addr == self.base + BTSDIO_OFFSET_HOST2BT_OUT {
            self.pointer
        } else if self.ring.is_some() && addr == crate::CHIP.sdiod_core_base_address + SDIO_INT_STATUS {
            0 // Polling must still drain a pending ring without a fresh IRQ.
        } else {
            u32::MAX
        }
    }

    async fn bp_write32(&mut self, addr: u32, value: u32) {
        if addr == self.base + BTSDIO_OFFSET_BT2HOST_OUT {
            self.acknowledged = Some(value);
        }
        self.writes += 1;
    }

    async fn read8(&mut self, _: u32, _: u32) -> u8 {
        0
    }

    async fn write8(&mut self, _: u32, _: u32, _: u8) {}

    async fn read16(&mut self, _: u32, _: u32) -> u16 {
        IRQ_F2_PACKET_AVAILABLE
    }

    async fn write16(&mut self, _: u32, _: u32, _: u16) {}

    async fn read32(&mut self, _: u32, _: u32) -> u32 {
        // Stop eventually so a broken unbounded drain fails instead of hanging.
        if self.f2_continuous && self.f2_reads < 100 {
            STATUS_F2_PKT_AVAILABLE | (12 << STATUS_F2_PKT_LEN_SHIFT)
        } else {
            0
        }
    }

    async fn write32(&mut self, _: u32, _: u32, _: u32) {}

    async fn wait_for_event(&mut self) {
        core::future::pending::<()>().await
    }
}

#[test]
fn bluetooth_only_network_channel_constructs() {
    let mut state = crate::State::new();
    let _ = embassy_net_driver_channel::new(
        &mut state.net.ch,
        embassy_net_driver_channel::driver::HardwareAddress::Ethernet([0; 6]),
    );
}

#[test]
fn unsolicited_oversized_wifi_frame_cannot_overrun_inert_channel() {
    let mut state = crate::State::new();
    let (ch, _) = embassy_net_driver_channel::new(
        &mut state.net.ch,
        embassy_net_driver_channel::driver::HardwareAddress::Ethernet([0; 6]),
    );
    let mut runner = Runner::new(
        ch,
        MockBus::default(),
        &state.ioctl_state,
        &state.net.events,
        &state.net.secure_network,
        None,
    );
    let mut packet = Aligned::<A4, _>([0u8; 12 + 4 + crate::MTU + 1]);
    let len = packet.len() as u16;
    packet[..2].copy_from_slice(&len.to_le_bytes());
    packet[2..4].copy_from_slice(&(!len).to_le_bytes());
    packet[5] = CHANNEL_TYPE_DATA;
    packet[7] = 12;
    assert!(crate::structs::SdpcmHeader::parse(&mut packet[..]).is_some());
    runner.rx(&mut packet[..]);
}

#[test]
fn f2_declared_offsets_and_control_lengths_are_checked() {
    for offset in 0..=255 {
        let mut bytes = Aligned::<A4, _>([0u8; 32]);
        bytes[..2].copy_from_slice(&32u16.to_le_bytes());
        bytes[2..4].copy_from_slice(&(!32u16).to_le_bytes());
        bytes[7] = offset;
        assert_eq!(
            crate::structs::SdpcmHeader::parse(&mut bytes[..]).is_some(),
            (12..=32).contains(&offset)
        );
    }
    let mut bytes = Aligned::<A4, _>([0u8; 32]);
    bytes[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(crate::structs::CdcHeader::parse(&mut bytes[..]).is_none());
}

#[test]
fn continuous_f2_traffic_returns_to_boundary() {
    let mut state = crate::State::new();
    let (ch, _) = embassy_net_driver_channel::new(
        &mut state.net.ch,
        embassy_net_driver_channel::driver::HardwareAddress::Ethernet([0; 6]),
    );
    let bus = MockBus {
        f2_continuous: true,
        ..Default::default()
    };
    let mut runner = Runner::new(
        ch,
        bus,
        &state.ioctl_state,
        &state.net.events,
        &state.net.secure_network,
        None,
    );
    block_on(runner.check_status(&mut [0; 512]));
    assert!(runner.bus.f2_reads <= 4, "a maintenance turn must have a bounded drain");
}

#[test]
fn ready_tx_cannot_starve_rx_or_boundary() {
    let mut state = crate::State::new();
    let (ch, _) = embassy_net_driver_channel::new(
        &mut state.net.ch,
        embassy_net_driver_channel::driver::HardwareAddress::Ethernet([0; 6]),
    );
    let mut bt_state = crate::bluetooth::BtState::new();
    let (bt, _driver) = crate::bluetooth::new(&mut bt_state);
    _driver.queue_test_packet();
    let bus = MockBus {
        pointer: 4,
        ..Default::default()
    }; // TX ring full.
    let runner = Runner::new(
        ch,
        bus,
        &state.ioctl_state,
        &state.net.events,
        &state.net.secure_network,
        Some(bt),
    );
    let boundaries = Cell::new(0);
    let mut run = pin!(runner.run_with_boundary(async || {
        boundaries.set(boundaries.get() + 1);
        if boundaries.get() == 4 {
            core::future::pending::<()>().await;
        }
    }));
    let mut cx = Context::from_waker(Waker::noop());
    for _ in 0..20 {
        assert!(matches!(run.as_mut().poll(&mut cx), Poll::Pending));
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    assert_eq!(boundaries.get(), 4);
    // The malformed RX header faults transport despite perpetually ready TX.
    assert!(_driver.test_faulted());
}
