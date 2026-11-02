//! Parameter validity and exact wire-length regressions without a controller.
extern crate std;

use crate::{FixedSizeValue, FromHciBytes, FromHciBytesError, WriteHci};

/// One-byte fixture whose unsafe validity contract accepts only an exact-sized prefix.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ExactByte(
    /// Single byte accepted only when equal to the fixture sentinel 0x42.
    u8,
);

// SAFETY: transparent u8 has no padding or references; the accepted representation is valid.
unsafe impl FixedSizeValue for ExactByte {
    /// Returns whether the single fixture byte is accepted; a nonexact `data` length intentionally exposes caller
    /// drift.
    fn is_valid(data: &[u8]) -> bool {
        assert_eq!(data.len(), 1, "validator requires exact object bytes");
        data[0] == 0x42
    }
}
// SAFETY: transparent u8 has alignment one.
unsafe impl crate::ByteAlignedValue for ExactByte {}

crate::param! {
    /// Packed fixture ensuring each field validator receives only its own representation.
    struct ExactPair { first: ExactByte, second: u8 }
}

crate::param::param_slice! {
    /// Three-byte record used to exercise guaranteed transparent byte-array layout.
    [PackedRecord; 3] { value[1]: u16 }
}

#[test]
/// Borrowed record casts require array-identical size, stride, alignment and field origin.
/// These assertions guard the explicit representation contract, not a reproduced compiler mislayout.
fn fixed_record_wrappers_preserve_array_layout() {
    const { assert!(core::mem::size_of::<PackedRecord>() == 3) };
    const { assert!(core::mem::align_of::<PackedRecord>() == 1) };
    const { assert!(core::mem::offset_of!(PackedRecord, 0) == 0) };
    let bytes = [2, 0xAA, 0x34, 0x12, 0xBB, 0x78, 0x56, 0xCC];
    let (records, rest) = <&[PackedRecord]>::from_hci_bytes(&bytes).unwrap();
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].value(), Ok(0x1234));
    assert_eq!(records[1].value(), Ok(0x5678));
    assert_eq!(rest, &[0xCC]);
    let mut encoded = [0u8; 7];
    records.write_hci(&mut encoded.as_mut_slice()).unwrap();
    assert_eq!(encoded, bytes[..7]);
}

#[test]
/// Trailing packet fields must not leak into the fixed type's exact-sized validity callback.
fn fixed_validators_receive_only_their_representation() {
    let bytes = [0x42, 99];
    assert_eq!(ExactByte::from_hci_bytes(&bytes), Ok((ExactByte(0x42), &[99][..])));
    assert_eq!(
        <ExactByte as crate::ByteAlignedValue>::ref_from_hci_bytes(&bytes),
        Ok((&ExactByte(0x42), &[99][..]))
    );
    assert_eq!(
        <[ExactByte; 1]>::from_hci_bytes(&bytes),
        Ok(([ExactByte(0x42)], &[99][..]))
    );
    let (pair, rest) = ExactPair::from_hci_bytes(&[0x42, 99, 88]).unwrap();
    assert_eq!(pair.first, ExactByte(0x42));
    assert_eq!(pair.second, 99);
    assert_eq!(rest, &[88]);
}

#[test]
/// Invalid element bits must fail validation before any unsafe typed array is produced.
fn arrays_validate_every_element_before_decoding() {
    for bytes in [[2, 0], [0, 255], [1, 2]] {
        // This assertion intentionally precedes decoding: the broken baseline must not create UB.
        assert!(!<[bool; 2] as FixedSizeValue>::is_valid(&bytes));
        assert_eq!(
            <[bool; 2]>::from_hci_bytes(&bytes),
            Err(FromHciBytesError::InvalidValue)
        );
        assert_eq!(
            <&[bool; 2]>::from_hci_bytes(&bytes),
            Err(FromHciBytesError::InvalidValue)
        );
    }
    assert_eq!(<[bool; 2]>::from_hci_bytes(&[0, 1, 99]), Ok(([false, true], &[99][..])));
    assert!(!<[[bool; 2]; 2] as FixedSizeValue>::is_valid(&[0, 1, 2, 0]));
    assert!(!<[crate::param::DisconnectReason; 1] as FixedSizeValue>::is_valid(&[
        0xFF
    ]));
    assert_eq!(<[bool; 2]>::from_hci_bytes(&[1]), Err(FromHciBytesError::InvalidSize));
}

#[test]
/// Zero-sized valid elements consume only the count byte, retaining the exact trailing bytes.
fn zero_sized_arrays_and_slices_do_not_panic() {
    assert_eq!(<[(); 2]>::from_hci_bytes(&[42]), Ok(([(); 2], &[42][..])));
    assert_eq!(<[bool; 0]>::from_hci_bytes(&[42]), Ok(([], &[42][..])));
    for count in [0, 1, 255] {
        let bytes = [count, 42];
        let (units, rest) = <&[()]>::from_hci_bytes(&bytes).unwrap();
        assert_eq!(units.len(), usize::from(count));
        assert_eq!(rest, &[42]);
    }
}

#[test]
/// Count multiplication and short buffers fail before any typed memory is viewed.
fn element_prefix_rejects_overflow_and_short_input() {
    assert_eq!(
        crate::validated_element_prefix::<u16>(&[], usize::MAX),
        Err(FromHciBytesError::InvalidSize)
    );
    assert_eq!(
        crate::validated_element_prefix::<u16>(&[0], 1),
        Err(FromHciBytesError::InvalidSize)
    );
    assert_eq!(
        crate::validated_element_prefix::<bool>(&[2], 1),
        Err(FromHciBytesError::InvalidValue)
    );
    assert_eq!(
        crate::validated_element_prefix::<bool>(&[2], 0),
        Ok((&[][..], &[2][..]))
    );
    assert_eq!(
        crate::validated_element_prefix::<u16>(&[1, 2, 3], 1),
        Ok((&[1, 2][..], &[3][..]))
    );
}

#[futures_test::test]
/// Async and blocking variable byte parameters encode the byte count, excluding the count itself.
async fn byte_slice_prefix_is_payload_length_in_both_serializers() {
    for length in [0, 1, 31, 255] {
        let payload = std::vec![0xa5; length];
        let mut blocking = std::vec![0; length + 1];
        let mut asynchronous = std::vec![0; length + 1];
        payload.as_slice().write_hci(&mut blocking[..]).unwrap();
        payload.as_slice().write_hci_async(&mut asynchronous[..]).await.unwrap();
        assert_eq!(blocking[0], u8::try_from(length).unwrap());
        assert_eq!(asynchronous[0], u8::try_from(length).unwrap());
        assert_eq!(blocking, asynchronous);
        assert_eq!(&asynchronous[1..], payload.as_slice());
    }
}

#[futures_test::test]
/// A short writer exposes its real error and retains the already-written count; no rollback is promised.
async fn byte_slice_writer_errors_propagate_after_partial_output() {
    let payload = &[0xAA, 0xBB][..];
    let mut blocking = [0xFF];
    let mut asynchronous = [0xFF];
    assert!(payload.write_hci(&mut blocking[..]).is_err());
    assert!(payload.write_hci_async(&mut asynchronous[..]).await.is_err());
    assert_eq!(blocking, [2]);
    assert_eq!(asynchronous, [2]);
}
