//! Independent HCI framing oracles; no controller, radio or device is used.
extern crate std;

use crate::cmd::Cmd;
use crate::cmd::le::LeSetExtAdvData;
use crate::data::{
    AclBroadcastFlag,
    AclPacket,
    AclPacketBoundary,
    IsoDataLoadHeader,
    IsoPacket,
    IsoPacketStatus,
    SyncPacket,
};
use crate::event::{EventPacket, InquiryResultIter};
use crate::param::{AdvHandle, ConnHandle, Operation};
use crate::{ControllerToHostPacket, FromHciBytes, FromHciBytesError, ReadHci, ReadHciError, WriteHci};

#[test]
/// An event consumes exactly its two-byte header plus declared data, leaving the next event intact.
fn event_decode_preserves_unconsumed_packet_tail() {
    let bytes = [0x10, 1, 0x77, 0x10, 1, 0x88];
    let (event, rest) = EventPacket::from_hci_bytes(&bytes).unwrap();
    assert_eq!(event.data, &[0x77]);
    assert_eq!(rest, &bytes[3..]);
    assert!(matches!(
        EventPacket::from_hci_bytes_complete(&bytes),
        Err(FromHciBytesError::InvalidSize)
    ));
    let (_, rest) = ControllerToHostPacket::from_hci_bytes(&[4, 0x10, 1, 0x77, 42]).unwrap();
    assert_eq!(rest, &[42]);
    assert!(EventPacket::from_hci_bytes(&[0x10, 2, 0x77]).is_err());
}

#[futures_test::test]
/// SCO uses a three-byte header; reported size and both writers must agree with the independent bytes.
async fn synchronous_packet_size_matches_wire() {
    for length in [0, 1, 255] {
        let mut wire = std::vec![0xaa; length + 3];
        wire[..3].copy_from_slice(&[1, 0, length as u8]);
        let packet = SyncPacket::from_hci_bytes_complete(&wire).unwrap();
        assert_eq!(packet.size(), wire.len());
        let mut blocking = std::vec![0; wire.len()];
        let mut asynchronous = std::vec![0; wire.len()];
        packet.write_hci(&mut blocking[..]).unwrap();
        packet.write_hci_async(&mut asynchronous[..]).await.unwrap();
        assert_eq!(blocking, wire);
        assert_eq!(asynchronous, wire);
    }
}

#[test]
/// Public read maxima cover the full specified length field, including headers but excluding H4 except at the wrapper.
fn packet_read_maxima_cover_specified_length_fields() {
    assert_eq!(<AclPacket as ReadHci>::MAX_LEN, 65_539);
    assert_eq!(<SyncPacket as ReadHci>::MAX_LEN, 258);
    assert_eq!(<IsoPacket as ReadHci>::MAX_LEN, 16_387);
    assert_eq!(<ControllerToHostPacket as ReadHci>::MAX_LEN, 65_540);
}

#[test]
/// ISO reserved length bits do not become payload bytes; the remaining packet stays separate.
fn iso_length_uses_only_fourteen_defined_bits() {
    let bytes = [1, 0x10, 1, 0xC0, 0xAA, 42];
    let (packet, rest) = IsoPacket::from_hci_bytes(&bytes).unwrap();
    assert_eq!(packet.data(), &[0xAA]);
    assert_eq!(rest, &[42]);
    assert_eq!(packet.data_load_len(), 1);
}

#[test]
/// A byte-slice length cannot wrap; rejection precedes this writer's length prefix.
fn oversized_slice_fails_before_output() {
    let oversized = [0xAA; 256];
    let mut output = [0x55; 300];
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        oversized.as_slice().write_hci(&mut output[..]).unwrap();
    }));
    assert!(result.is_err());
    assert_eq!(output, [0x55; 300]);
}

#[test]
/// Command-wide length admission precedes its three-byte header, excluding any enclosing H4 writer.
fn oversized_command_fails_before_output() {
    let oversized = [0xAA; 256];
    let mut output = [0x55; 300];
    let cmd = LeSetExtAdvData::new(AdvHandle::new(0), Operation::Complete, false, &oversized[..252]);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        cmd.write_hci(&mut output[..]).unwrap();
    }));
    assert!(result.is_err());
    assert_eq!(output, [0x55; 300]);
    let admitted = LeSetExtAdvData::new(AdvHandle::new(0), Operation::Complete, false, &oversized[..251]);
    assert_eq!(admitted.header(), [0x37, 0x20, 255]);
}

#[test]
/// Async serializers reject oversize inputs on first poll without emitting their own prefix.
fn oversized_async_writers_fail_before_output() {
    use core::future::Future;
    let oversized = [0xAA; 256];
    for command in [false, true] {
        let mut output = [0x55; 300];
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut context = futures_test::task::noop_context();
            if command {
                let cmd = LeSetExtAdvData::new(AdvHandle::new(0), Operation::Complete, false, &oversized[..252]);
                let mut future = core::pin::pin!(cmd.write_hci_async(&mut output[..]));
                let _ = future.as_mut().poll(&mut context);
            } else {
                let slice = oversized.as_slice();
                let mut future = core::pin::pin!(slice.write_hci_async(&mut output[..]));
                let _ = future.as_mut().poll(&mut context);
            }
        }));
        assert!(result.is_err(), "command={command}");
        assert_eq!(output, [0x55; 300]);
    }
}

#[test]
/// A retained ACL builder cannot truncate a 65,536-byte payload into a zero-length header.
fn oversized_acl_builder_rejects_length() {
    let payload = std::vec![0xaa; 65_536];
    assert!(
        std::panic::catch_unwind(|| {
            AclPacket::new(
                ConnHandle::new(1),
                AclPacketBoundary::FirstNonFlushable,
                AclBroadcastFlag::PointToPoint,
                &payload,
            )
        })
        .is_err()
    );
}

#[test]
/// Multiple inquiry records are interleaved by Core Vol4E 5.2, without column arrays.
fn inquiry_records_are_interleaved() {
    let bytes = [
        1, 2, 3, 4, 5, 6, 1, 0, 0, 0x20, 4, 0, 0x34, 0x12, 11, 12, 13, 14, 15, 16, 2, 0, 0, 0x30, 5, 1, 0x78, 0x56,
    ];
    let mut records = InquiryResultIter::new_standard(&bytes, 2);
    let first = records.next().unwrap();
    assert_eq!(first.bd_addr.raw(), [1, 2, 3, 4, 5, 6]);
    assert_eq!(first.class_of_device, Some([0x20, 4, 0]));
    assert_eq!(records.next().unwrap().bd_addr.raw(), [11, 12, 13, 14, 15, 16]);
    assert!(records.next().is_none());
}

#[test]
/// An arbitrary public response count cannot overflow address or field offsets.
fn inquiry_count_arithmetic_is_bounded() {
    let bytes = [1, 2, 3, 4, 5, 6, 1, 0, 0, 0x20, 4, 0, 0x34, 0x12];
    let mut arbitrary = InquiryResultIter::new_standard(&bytes[..14], usize::MAX);
    assert!(arbitrary.next().is_some());
    assert!(arbitrary.next().is_none());
}

#[futures_test::test]
/// ACL endpoints roundtrip without overflow; short destination errors consume only the header.
async fn acl_boundary_roundtrips_and_short_buffer_is_explicit() {
    for length in [0, 1, 255, 256, 65_535] {
        let payload = std::vec![0xa5; length];
        let packet = AclPacket::new(
            ConnHandle::new(0x123),
            AclPacketBoundary::Continuing,
            AclBroadcastFlag::PointToPoint,
            &payload,
        );
        let mut wire = std::vec![0; length + 4];
        packet.write_hci(&mut wire[..]).unwrap();
        assert_eq!(&wire[..4], &[0x23, 0x11, length as u8, (length >> 8) as u8]);
        assert_eq!(AclPacket::from_hci_bytes_complete(&wire).unwrap(), packet);
        let mut backing = std::vec![0; length];
        assert_eq!(AclPacket::read_hci(&wire[..], &mut backing).unwrap(), packet);
        assert_eq!(
            AclPacket::read_hci_async(&wire[..], &mut backing).await.unwrap(),
            packet
        );
        let mut async_wire = std::vec![0; length + 4];
        packet.write_hci_async(&mut async_wire[..]).await.unwrap();
        assert_eq!(async_wire, wire);
        if length != 0 {
            let mut reader = &wire[..];
            assert!(matches!(
                AclPacket::read_hci(&mut reader, &mut backing[..length - 1]),
                Err(ReadHciError::BufferTooSmall)
            ));
            assert_eq!(reader, &wire[4..]);
        }
    }
}

#[futures_test::test]
/// SDU status bits never inflate the length, and raw reserved/status bits survive both writers.
async fn iso_sdu_metadata_remains_separate_and_roundtrips() {
    for (status, expected) in [
        (0u16, Some(IsoPacketStatus::Correct)),
        (1, Some(IsoPacketStatus::PossiblyInvalid)),
        (2, Some(IsoPacketStatus::PartiallyLost)),
        (3, None),
    ] {
        let word = 0x3ABC | (status << 14);
        let wire = [0x34, 0x12, word as u8, (word >> 8) as u8, 0x42];
        let (header, rest) = IsoDataLoadHeader::from_hci_bytes(false, &wire).unwrap();
        assert_eq!(header.sdu_len(), 0xABC);
        assert_eq!(header.packet_status(), expected);
        assert_eq!(rest, &[0x42]);
        let mut output = [0; 4];
        header.write_hci(&mut output[..]).unwrap();
        assert_eq!(output, wire[..4]);
        output.fill(0x55);
        header.write_hci_async(&mut output[..]).await.unwrap();
        assert_eq!(output, wire[..4]);
    }
    let timestamped = [1, 2, 3, 4, 0x34, 0x12, 1, 0];
    for length in 0..timestamped.len() {
        assert!(IsoDataLoadHeader::from_hci_bytes(true, &timestamped[..length]).is_err());
    }
    let (header, rest) = IsoDataLoadHeader::from_hci_bytes(true, &timestamped).unwrap();
    assert_eq!(header.timestamp, Some(0x0403_0201));
    assert_eq!(header.sdu_len(), 1);
    assert!(rest.is_empty());
}

#[test]
/// Every partial inquiry record is bounded, and invalid page mode cannot corrupt adjacent fields.
fn inquiry_truncation_and_invalid_field_preserve_independent_evidence() {
    let wire = [1, 2, 3, 4, 5, 6, 0xFF, 0, 0x20, 4, 0, 0x34, 0x12, 0xF0];
    for length in 0..=wire.len() {
        let mut iter = InquiryResultIter::new_with_rssi(&wire[..length], usize::MAX);
        let item = iter.next();
        assert_eq!(item.is_some(), length >= 6);
        if let Some(item) = item {
            assert!(item.page_scan_repetition_mode.is_none());
            assert_eq!(item.class_of_device.is_some(), length >= 11);
            assert_eq!(item.clock_offset.is_some(), length >= 13);
            assert_eq!(item.rssi, (length == 14).then_some(-16));
        }
        assert!(iter.next().is_none());
    }
    assert!(InquiryResultIter::new_with_rssi(&wire, 0).next().is_none());
}
