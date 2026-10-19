extern crate std;

use super::*;
use crate::regression_tests::MockBus;
use bt_hci::event::le::{LeAdvertisingReport, LeEventPacket};
use bt_hci::{ControllerToHostPacket, FromHciBytes};
use bt_hci_transport::Transport;
use embassy_futures::block_on;

fn advertisement(length: u8) -> std::vec::Vec<u8> {
    let mut packet = std::vec![4, 0x3e, 12 + length, 2, 1, 0, 0, 1, 2, 3, 4, 5, 6, length];
    packet.extend((0..length).map(|i| i + 0x40));
    packet.push(0xc5); // -59 dBm, distinct from nonzero transport padding.
    packet
}

#[test]
fn ring_payload_offsets_wrap_and_padding_preserve_every_legacy_data_length() {
    for length in 0..=31 {
        let packet = advertisement(length);
        // Header at the final word, payload split at the final word, no wrap.
        for start in [0, 4088, 4092] {
            let mut state = BtState::new();
            let (mut runner, driver) = new(&mut state);
            let mut bus = MockBus::ring(start, &[&packet]);
            runner.addr = bus.base;
            runner.b2h_read_pointer = start as u32;
            block_on(runner.handle_irq(&mut bus));
            assert!(!driver.test_faulted());
            assert_eq!(bus.acknowledged, Some(bus.pointer));
            let mut rx = [0; 259];
            let ControllerToHostPacket::Event(event) = block_on(driver.read(&mut rx)).unwrap() else {
                panic!()
            };
            assert_eq!(event.data, &packet[3..], "length={length}, start={start}");
            let le = LeEventPacket::from_hci_bytes_complete(event.data).unwrap();
            let reports = LeAdvertisingReport::from_hci_bytes_complete(le.data).unwrap();
            let report = reports.reports.iter().next().unwrap().unwrap();
            assert_eq!(report.data, &packet[14..14 + usize::from(length)]);
            assert_eq!(report.rssi, -59);
            assert!(driver.rx.borrow_mut().try_receive().is_none());
        }
    }
}

#[test]
fn ring_batch_and_full_queue_retry_keep_packet_order_without_new_irq() {
    let packets: std::vec::Vec<_> = (0..6).map(|n| [4, 0xff, 2, n, 0x5a]).collect();
    let slices: std::vec::Vec<_> = packets.iter().map(|p| &p[..]).collect();
    let mut bus = MockBus::ring(4080, &slices);
    let mut state = BtState::new();
    let (mut runner, driver) = new(&mut state);
    runner.addr = bus.base;
    runner.b2h_read_pointer = 4080;
    block_on(runner.handle_irq(&mut bus));
    assert_eq!(bus.acknowledged, Some(16)); // Four eight-byte ring records.
    block_on(runner.handle_irq(&mut bus));
    assert_eq!(bus.acknowledged, Some(16)); // Queue full, record five retained.
    let mut rx = [0; 259];
    for n in 0..6 {
        let ControllerToHostPacket::Event(event) = block_on(driver.read(&mut rx)).unwrap() else {
            panic!()
        };
        assert_eq!(event.data, &[n, 0x5a]);
        block_on(runner.handle_irq(&mut bus));
    }
    assert_eq!(bus.acknowledged, Some(32));
    assert!(driver.rx.borrow_mut().try_receive().is_none());
    assert!(!driver.test_faulted());
}

#[test]
fn ring_maximum_event_preserves_all_255_bytes_and_drops_padding() {
    let mut packet = std::vec![4, 0xff, 255];
    packet.extend(0..255);
    let mut bus = MockBus::ring(4092, &[&packet]);
    let mut state = BtState::new();
    let (mut runner, driver) = new(&mut state);
    runner.addr = bus.base;
    runner.b2h_read_pointer = 4092;
    block_on(runner.handle_irq(&mut bus));
    assert_eq!(bus.acknowledged, Some(260));
    let mut rx = [0; 259];
    let ControllerToHostPacket::Event(event) = block_on(driver.read(&mut rx)).unwrap() else {
        panic!()
    };
    assert_eq!(event.data, &packet[3..]);
}

#[test]
fn short_report_remains_invalid_instead_of_consuming_ring_padding_as_rssi() {
    for length in 0..=31 {
        let mut packet = advertisement(length);
        packet.pop(); // Includes the observed16-byte report declaring7 data bytes.
        packet[2] -= 1;
        let mut bus = MockBus::ring(4088, &[&packet]);
        let mut state = BtState::new();
        let (mut runner, driver) = new(&mut state);
        runner.addr = bus.base;
        runner.b2h_read_pointer = 4088;
        block_on(runner.handle_irq(&mut bus));
        let mut rx = [0; 259];
        let ControllerToHostPacket::Event(event) = block_on(driver.read(&mut rx)).unwrap() else {
            panic!()
        };
        assert_eq!(event.data, &packet[3..]);
        let le = LeEventPacket::from_hci_bytes_complete(event.data).unwrap();
        let reports = LeAdvertisingReport::from_hci_bytes_complete(le.data).unwrap();
        assert!(matches!(
            reports.reports.iter().next(),
            Some(Err(bt_hci::FromHciBytesError::InvalidSize))
        ));
        assert!(!driver.test_faulted(), "inner report shape is not a transport fault");
    }
}
