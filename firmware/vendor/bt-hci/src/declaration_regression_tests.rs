//! Wire-boundary fixtures from Bluetooth Core 6.2 Vol 4 Part E; no radio operations.
extern crate std;

use super::*;
use crate::cmd::Cmd;
use crate::cmd::le::*;

/// RF test opcodes are distinct from Read Supported States and DHKey generation.
#[test]
fn rf_test_opcodes_match_sections_7_8_28_and_29() {
    assert_eq!(LeTransmitterTest::OPCODE.to_raw(), 0x201E);
    assert_eq!(LeTransmitterTestV3::OPCODE.to_raw(), 0x2050);
    assert_eq!(LeReceiverTestV3::OPCODE.to_raw(), 0x204F);
}

/// Section7.8.2 returns two lengths and two one-byte packet counts after status.
#[test]
fn buffer_size_v2_decodes_exact_six_byte_return() {
    let value = LeReadBufferSizeV2Return::from_hci_bytes_complete(&[0xFB, 0, 4, 0x20, 1, 7]).unwrap();
    assert_eq!({ value.le_acl_data_packet_length }, 251);
    assert_eq!(value.total_num_le_acl_data_packets, 4);
    assert_eq!({ value.iso_data_packet_length }, 288);
    assert_eq!({ value.total_num_iso_data_packets }, 7);
}

/// Check validity before decoding so the broken baseline never constructs an invalid enum.
#[test]
fn phy_options_validate_both_discriminant_bytes() {
    for high in 1..=255 {
        assert!(!PhyOptions::is_valid(&[0, high]));
        assert!(!AdvPhyOptions::is_valid(&[0, high]));
    }
    for value in 0..=255 {
        assert_eq!(PhyOptions::is_valid(&[value, 0]), value <= 2);
        assert_eq!(AdvPhyOptions::is_valid(&[value, 0]), value <= 4);
    }
    // These are naturally two-byte aligned. Owned decoding deliberately supports odd addresses.
    assert_eq!(core::mem::align_of::<PhyOptions>(), 2);
    assert_eq!(core::mem::align_of::<AdvPhyOptions>(), 2);
    let bytes = [99, 2, 0, 99];
    assert_eq!(
        PhyOptions::from_hci_bytes(&bytes[1..]),
        Ok((PhyOptions::S8CodingPreferred, &bytes[3..]))
    );
}

/// A complete extended report must reject every truncated prefix, including missing secondary PHY.
#[test]
fn extended_report_truncation_returns_error_without_panicking() {
    let mut bytes = [0; 25];
    bytes[9] = 1; // Primary PHY = LE1M; secondary PHY = absent.
    bytes[23] = 0; // Empty advertising data.
    for end in 0..24 {
        assert!(LeExtAdvReport::from_hci_bytes(&bytes[..end]).is_err(), "prefix {end}");
    }
    let (report, rest) = LeExtAdvReport::from_hci_bytes(&bytes).unwrap();
    assert!(report.secondary_adv_phy.is_none());
    assert_eq!(rest, &[0]);
}

/// Establish corrected record admission before attempting malformed enum data.
#[test]
fn periodic_reports_reject_invalid_typed_fields() {
    let valid = [2, 0, 0, 0, 0, 0, 1, 0xAA, 0, 0, 0, 1, 0, 1, 0xBB];
    let (reports, _) = LePeriodicAdvertisingResponseReports::from_hci_bytes(&valid).unwrap();
    // The unsafe column-major baseline fails here while every interpreted enum is still valid.
    assert_eq!(reports.get(0).unwrap().data, &[0xAA]);
    for (offset, invalid) in [(3, 3), (5, 2)] {
        let mut bytes = [1, 0, 0, 0xFF, 1, 0, 0];
        bytes[offset] = invalid;
        assert!(LePeriodicAdvertisingResponseReports::from_hci_bytes(&bytes).is_err());
    }
}

/// A one-record fixture is independent of column/interleaved layout; missing payload must fail admission.
#[test]
fn periodic_and_cs_payload_lengths_are_admitted_before_get() {
    let periodic = [1, 0, 0, 0xFF, 1, 0, 2, 0xAA, 0xBB];
    assert!(LePeriodicAdvertisingResponseReports::from_hci_bytes(&periodic[..8]).is_err());
    let (reports, rest) = LePeriodicAdvertisingResponseReports::from_hci_bytes(&periodic).unwrap();
    assert!(rest.is_empty());
    assert_eq!(reports.get(0).unwrap().data, &[0xAA, 0xBB]);
    assert!(reports.get(1).is_none());
    let cs = [1, 0, 1, 2, 0xAA, 0xBB];
    assert!(LeCsSubeventStepData::from_hci_bytes(&cs[..5]).is_err());
    let (steps, rest) = LeCsSubeventStepData::from_hci_bytes(&cs).unwrap();
    assert!(rest.is_empty());
    assert_eq!(steps.get(0).unwrap().step_data, &[0xAA, 0xBB]);
    assert!(steps.get(1).is_none());
}

/// Section5.2 interleaves each complete record, including its variable payload.
#[test]
fn repeated_reports_follow_interleaved_parameter_order() {
    // All old column-decoded enum positions are valid too, keeping the baseline safe.
    let periodic = [2, 0, 0, 0, 0, 0, 1, 0xAA, 0, 0, 0, 1, 0, 1, 0xBB];
    let (reports, rest) = LePeriodicAdvertisingResponseReports::from_hci_bytes(&periodic).unwrap();
    assert_eq!(reports.get(0).unwrap().data, &[0xAA]);
    assert_eq!(reports.get(1).unwrap().data, &[0xBB]);
    assert_eq!(reports.get(1).unwrap().response_slot, 1);
    assert!(rest.is_empty());
    let cs = [2, 0, 1, 1, 0xAA, 2, 3, 1, 0xBB];
    let (steps, rest) = LeCsSubeventStepData::from_hci_bytes(&cs).unwrap();
    assert_eq!(steps.get(0).unwrap().step_data, &[0xAA]);
    assert_eq!(steps.get(1).unwrap().step_data, &[0xBB]);
    assert_eq!(steps.get(1).unwrap().step_mode, 2);
    assert!(rest.is_empty());
}

/// Each record emits its length once; both serializers preserve the independent wire fixture.
#[futures_test::test]
async fn variable_record_serializers_emit_one_length_and_propagate_errors() {
    let bytes = [0, 0, 0xFF, 1, 0, 2, 0xAA, 0xBB];
    let record = LePeriodicAdvertisingResponseReport::from_hci_bytes_complete(&bytes).unwrap();
    assert_eq!(record.size(), bytes.len());
    let mut output = [0; 8];
    record.write_hci(&mut output[..]).unwrap();
    assert_eq!(output, bytes);
    output.fill(0);
    record.write_hci_async(&mut output[..]).await.unwrap();
    assert_eq!(output, bytes);
    assert!(record.write_hci(&mut output[..7]).is_err());
    assert!(record.write_hci_async(&mut output[..7]).await.is_err());
    let invalid = LePeriodicAdvertisingResponseReport {
        data_length: 1,
        ..record
    };
    output.fill(0xCC);
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| invalid.write_hci(&mut output[..]))).is_err());
    assert_eq!(output, [0xCC; 8]);
    let cs_bytes = [2, 3, 1, 0xAA];
    let cs = LeCsSubeventStepEntry::from_hci_bytes_complete(&cs_bytes).unwrap();
    assert_eq!(cs.size(), 4);
    cs.write_hci(&mut output[..4]).unwrap();
    assert_eq!(&output[..4], &cs_bytes);
    cs.write_hci_async(&mut output[..4]).await.unwrap();
    assert_eq!(&output[..4], &cs_bytes);
}

/// Containers admit only complete records and preserve following bytes, including count zero.
#[test]
fn variable_record_containers_preserve_tail_and_reject_every_short_prefix() {
    let periodic = [2, 0, 0, 0, 0, 0, 1, 0xAA, 0, 0, 0, 1, 0, 1, 0xBB];
    for end in 0..periodic.len() {
        assert!(LePeriodicAdvertisingResponseReports::from_hci_bytes(&periodic[..end]).is_err());
    }
    let mut with_tail = periodic.to_vec();
    with_tail.push(0xCC);
    let (reports, rest) = LePeriodicAdvertisingResponseReports::from_hci_bytes(&with_tail).unwrap();
    assert_eq!(reports.iter().count(), 2);
    assert_eq!(rest, &[0xCC]);
    let (empty, rest) = LePeriodicAdvertisingResponseReports::from_hci_bytes(&[0, 0xCC]).unwrap();
    assert!(empty.is_empty());
    assert_eq!(rest, &[0xCC]);
    let cs = [2, 0, 1, 1, 0xAA, 2, 3, 1, 0xBB];
    for end in 0..cs.len() {
        assert!(LeCsSubeventStepData::from_hci_bytes(&cs[..end]).is_err());
    }
    let (empty, rest) = LeCsSubeventStepData::from_hci_bytes(&[0, 0xCC]).unwrap();
    assert!(empty.is_empty());
    assert_eq!(rest, &[0xCC]);
}

/// Verifies invalid local lengths in `value` panic before either serializer can publish bytes.
fn rejects_before_write(value: &impl WriteHci) {
    use core::future::Future;
    use core::task::{Context, Waker};
    use std::panic::{AssertUnwindSafe, catch_unwind};
    assert!(catch_unwind(AssertUnwindSafe(|| value.size())).is_err());
    let mut bytes = [0xcc; 300];
    assert!(catch_unwind(AssertUnwindSafe(|| value.write_hci(&mut bytes[..]))).is_err());
    assert_eq!(bytes, [0xcc; 300]);
    assert!(
        catch_unwind(AssertUnwindSafe(|| {
            let mut work = core::pin::pin!(value.write_hci_async(&mut bytes[..]));
            let _ = work.as_mut().poll(&mut Context::from_waker(Waker::noop()));
        }))
        .is_err()
    );
    assert_eq!(bytes, [0xcc; 300]);
}

/// Custom command parameters and nested subevents must not wrap their one-byte lengths.
#[test]
fn custom_parameter_lengths_reject_before_partial_output() {
    let large = [0; 256];
    rejects_before_write(&LeTransmitterTestV3Params {
        tx_channel: 1,
        test_data_length: 1,
        packet_payload: 0,
        phy: PhyKind::Le1M,
        cte_length: 0,
        cte_type: CteKind::AoA,
        switching_pattern: &large,
    });
    rejects_before_write(&LeReceiverTestV3Params {
        rx_channel: 1,
        phy: PhyKind::Le1M,
        modulation_index: 0,
        expected_cte_length: 0,
        expected_cte_type: CteKind::AoA,
        slot_durations: 1,
        switching_pattern: &large,
    });
    rejects_before_write(&LeSetupIsoDataPathParams {
        handle: ConnHandle::new(1),
        data_path_direction: DataPathDirection::Input,
        data_path_id: DataPathId::HCI,
        codec_id: CodecId {
            coding_format: 0,
            company_id: 0,
            vendor_specific_codec_id: 0,
        },
        controller_delay: super::super::ExtDuration::from_u32(0),
        codec_configuration: &large,
    });
    let empty = LePeriodicAdvSubeventData {
        subevent: 0,
        response_slot_start: 0,
        response_slot_count: 0,
        subevent_data: &[],
    };
    let too_many = [empty; 256];
    rejects_before_write(&too_many.as_slice());
    let nested = [LePeriodicAdvSubeventData {
        subevent_data: &large,
        ..empty
    }];
    rejects_before_write(&nested.as_slice());
}

/// Variable-length successful CIG returns contain only the declared handles (§7.8.97/.98).
#[test]
fn cig_returns_accept_one_handle() {
    assert!(LeSetCigParametersReturn::from_hci_bytes_complete(&[7, 1, 0x34, 2]).is_ok());
    assert!(LeSetCigParametersTestReturn::from_hci_bytes_complete(&[7, 1, 0x34, 2]).is_ok());
}

/// Zero groups is the standard response for controllers supporting only RCV (§7.8.156).
#[test]
fn minimum_interval_return_accepts_zero_groups() {
    assert!(LeReadMinimumSupportedConnectionIntervalReturn::from_hci_bytes_complete(&[60, 0]).is_ok());
}

/// BIG handle is one byte and each CIG test configuration includes NSE (§7.8.98/.106).
#[test]
fn iso_handle_and_test_record_wire_widths() {
    assert_eq!(core::mem::size_of::<BigHandle>(), 1);
    assert_eq!(core::mem::size_of::<CisConfigTest>(), 14);
}

/// ExactSizeIterator counts successful prefix items and at most one terminal error.
#[test]
fn advertisement_iterators_count_actual_yields() {
    let legacy = LeAdvReports::from_hci_bytes_complete(&[3]).unwrap();
    assert_eq!(legacy.iter().len(), 1);
    let extended = LeExtAdvReports::from_hci_bytes_complete(&[3]).unwrap();
    assert_eq!(extended.iter().len(), 1);
}

/// Checks `value` through both writers against independently transcribed wire bytes `expected`.
async fn assert_wire(value: &impl WriteHci, expected: &[u8]) {
    assert_eq!(value.size(), expected.len());
    let mut bytes = [0xCC; 255];
    value.write_hci(&mut bytes[..expected.len()]).unwrap();
    assert_eq!(&bytes[..expected.len()], expected);
    bytes.fill(0xCC);
    value.write_hci_async(&mut bytes[..expected.len()]).await.unwrap();
    assert_eq!(&bytes[..expected.len()], expected);
    assert!(value.write_hci(&mut bytes[..expected.len() - 1]).is_err());
    assert!(value.write_hci_async(&mut bytes[..expected.len() - 1]).await.is_err());
}

/// §7.8.97 puts SCA/packing/framing before two latency values; records are interleaved (§5.2).
#[futures_test::test]
async fn normal_cig_command_matches_normative_field_order() {
    let bytes = [
        7, 0, 1, 0, 0, 2, 0, 6, 1, 0, 10, 0, 20, 0, 2, 1, 3, 0, 4, 0, 1, 2, 5, 6, 2, 7, 0, 8, 0, 2, 1, 9, 10,
    ];
    let value = LeSetCigParametersParams::from_hci_bytes_complete(&bytes).unwrap();
    assert_eq!(value.worst_case_sca, 6);
    assert_eq!(value.packing, Packing::Interleaved);
    assert_eq!(value.cis_configs.len(), 2);
    assert_eq!(value.cis_configs[1].cis_id, CisId::new(2));
    assert_wire(&value, &bytes).await;
}

/// §7.8.98 includes NSE immediately after CIS_ID; §7.8.99 contains no CIG_ID prefix.
#[futures_test::test]
async fn test_cig_and_create_cis_match_normative_records() {
    let bytes = [
        7, 0, 1, 0, 0, 2, 0, 1, 2, 4, 0, 6, 1, 0, 1, 2, 3, 4, 0, 5, 0, 6, 0, 7, 0, 1, 2, 1, 2,
    ];
    let value = LeSetCigParametersTestParams::from_hci_bytes_complete(&bytes).unwrap();
    assert_eq!(value.cis_configs[0].nse, 3);
    assert_wire(&value, &bytes).await;
    let cis_bytes = [2, 0x34, 2, 0x56, 3, 0x35, 2, 0x57, 3];
    let cis = LeCreateCisParams::from_hci_bytes_complete(&cis_bytes).unwrap();
    assert_eq!(cis.cis_configs.len(), 2);
    assert_eq!({ cis.cis_configs[1].acl_handle }, ConnHandle::new(0x357));
    assert_wire(&cis, &cis_bytes).await;
}

/// §7.8.104 ends at the 16-byte broadcast code, without a BIS handle list.
#[futures_test::test]
async fn create_big_test_has_only_specified_fixed_fields() {
    let mut bytes = [0; 36];
    bytes[..20].copy_from_slice(&[7, 8, 1, 0, 1, 0, 4, 0, 1, 20, 0, 20, 0, 1, 0, 0, 1, 1, 0, 0]);
    bytes[20..].copy_from_slice(&[0xAA; 16]);
    let value = LeCreateBigTestParams::from_hci_bytes_complete(&bytes).unwrap();
    assert_eq!(value.num_bis, 1);
    assert_wire(&value, &bytes).await;
}

/// §7.8.106 has distinct BIG and periodic-sync handles plus exactly Num_BIS indices.
#[futures_test::test]
async fn big_sync_encodes_both_handles_and_rejects_mismatched_count() {
    let value = LeBigCreateSyncParams {
        big_sync_handle: BigHandle(7),
        sync_handle: SyncHandle(0x234),
        encryption: EncryptionMode::Unencrypted,
        broadcast_code: BroadcastCode::new([0xAA; 16]),
        mse: 3,
        big_sync_timeout: 0x123,
        num_bis: 2,
        bis: &[1, 3],
    };
    let mut bytes = [0xAA; 26];
    bytes[..4].copy_from_slice(&[7, 0x34, 2, 0]);
    bytes[20..].copy_from_slice(&[3, 0x23, 1, 2, 1, 3]);
    assert_wire(&value, &bytes).await;
    rejects_before_write(&LeBigCreateSyncParams { num_bis: 1, ..value });
}

/// RF test PHY is an encoded selection, not a bitmask; RX disallows TX-only S=2 selection.
#[futures_test::test]
async fn rf_test_phy_selection_is_not_a_mask() {
    let tx = LeTransmitterTestV3Params {
        tx_channel: 2,
        test_data_length: 10,
        packet_payload: 0,
        phy: PhyKind::LeCoded,
        cte_length: 0,
        cte_type: CteKind::AoA,
        switching_pattern: &[1, 2],
    };
    assert_wire(&tx, &[2, 10, 0, 3, 0, 0, 2, 1, 2]).await;
    assert_wire(
        &LeTransmitterTestV3Params {
            phy: PhyKind::LeCodedS2,
            ..tx
        },
        &[2, 10, 0, 4, 0, 0, 2, 1, 2],
    )
    .await;
    let rx = LeReceiverTestV3Params {
        rx_channel: 2,
        phy: PhyKind::LeCoded,
        modulation_index: 0,
        expected_cte_length: 0,
        expected_cte_type: CteKind::AoA,
        slot_durations: 1,
        switching_pattern: &[1, 2],
    };
    assert_wire(&rx, &[2, 3, 0, 0, 0, 1, 2, 1, 2]).await;
    rejects_before_write(&LeReceiverTestV3Params {
        phy: PhyKind::LeCodedS2,
        ..rx
    });
}

/// Counted responses admit exact prefixes, preserve tails and reject truncation/capacity excess.
#[futures_test::test]
async fn counted_returns_validate_streams_and_bound_storage() {
    use crate::{ReadHci, ReadHciError};
    let bytes = [7, 2, 0x34, 2, 0x35, 2];
    for end in 0..bytes.len() {
        assert!(LeSetCigParametersReturn::from_hci_bytes(&bytes[..end]).is_err());
    }
    let (cig, rest) = LeSetCigParametersReturn::from_hci_bytes(&[7, 1, 0x34, 2, 0xCC]).unwrap();
    assert_eq!(rest, &[0xCC]);
    assert_eq!(cig.cis_handles[0], ConnHandle::new(0x234));
    assert_wire(&cig, &[7, 1, 0x34, 2]).await;
    assert!(LeSetCigParametersReturn::from_hci_bytes(&[7, 32]).is_err());
    assert!(LeReadMinimumSupportedConnectionIntervalReturn::from_hci_bytes(&[60, 42]).is_err());
    let mut max = [0; 64];
    max[0] = 7;
    max[1] = 31;
    assert_eq!(
        LeSetCigParametersReturn::from_hci_bytes_complete(&max).unwrap().num_cis,
        31
    );
    let mut max_groups = [0; 248];
    max_groups[0] = 60;
    max_groups[1] = 41;
    assert_eq!(
        LeReadMinimumSupportedConnectionIntervalReturn::from_hci_bytes_complete(&max_groups)
            .unwrap()
            .num_groups,
        41
    );
    let groups = [5, 2, 5, 0, 80, 0, 5, 0, 80, 0, 0x20, 3, 8, 0];
    let value = LeReadMinimumSupportedConnectionIntervalReturn::from_hci_bytes_complete(&groups).unwrap();
    assert_eq!({ value.groups[1].stride }.as_micros(), 1000);
    assert_wire(&value, &groups).await;
    for end in 0..groups.len() {
        assert!(LeReadMinimumSupportedConnectionIntervalReturn::from_hci_bytes(&groups[..end]).is_err());
    }
    let mut buf = [0; 64];
    let mut reader = &bytes[..];
    assert_eq!(
        LeSetCigParametersReturn::read_hci(&mut reader, &mut buf)
            .unwrap()
            .num_cis,
        2
    );
    assert!(reader.is_empty());
    let mut reader = &bytes[..];
    assert_eq!(
        LeSetCigParametersReturn::read_hci_async(&mut reader, &mut buf)
            .await
            .unwrap()
            .num_cis,
        2
    );
    assert!(reader.is_empty());
    let mut reader = &bytes[..];
    assert!(matches!(
        LeSetCigParametersReturn::read_hci(&mut reader, &mut buf[..1]),
        Err(ReadHciError::BufferTooSmall)
    ));
    assert_eq!(reader, &bytes); // No header consumed when scratch cannot hold it.
    let mut reader = &[7, 32, 0xCC][..];
    assert!(
        LeSetCigParametersReturn::read_hci_async(&mut reader, &mut buf)
            .await
            .is_err()
    );
    assert_eq!(reader, &[0xCC]); // Excess count rejected before payload I/O.
    let mut reader = &bytes[..];
    assert!(matches!(
        LeSetCigParametersReturn::read_hci(&mut reader, &mut buf[..3]),
        Err(ReadHciError::BufferTooSmall)
    ));
    assert_eq!(reader, &bytes[2..]); // Length-dependent scratch failure consumes the header only.
    rejects_before_write(&LeSetCigParametersReturn { num_cis: 32, ..cig });
    assert_eq!(<LeSetCigParametersReturn as ReadHci>::MAX_LEN, 64);
    assert_eq!(
        <LeReadMinimumSupportedConnectionIntervalReturn as ReadHci>::MAX_LEN,
        248
    );
    assert!(core::mem::size_of::<LeReadMinimumSupportedConnectionIntervalReturn>() <= 248);
}

/// Late malformed records yield the valid prefix plus one error; valid reports remain exact.
#[test]
fn advertisement_iterator_exact_size_survives_each_next() {
    for extended in [false, true] {
        let mut record = std::vec![0; if extended { 24 } else { 10 }];
        if extended {
            record[9] = 1;
        }
        for valid in 0..=2 {
            for malformed in [false, true] {
                let mut bytes = std::vec![if malformed { 3 } else { valid as u8 }];
                for _ in 0..valid {
                    bytes.extend_from_slice(&record);
                }
                let expected = valid + usize::from(malformed);
                if extended {
                    let reports = LeExtAdvReports::from_hci_bytes_complete(&bytes).unwrap();
                    let mut iter = reports.iter();
                    for left in (1..=expected).rev() {
                        assert_eq!(iter.len(), left);
                        assert_eq!(iter.size_hint(), (left, Some(left)));
                        assert_eq!(iter.next().unwrap().is_err(), malformed && left == 1);
                    }
                    assert_eq!(iter.len(), 0);
                    assert!(iter.next().is_none());
                    assert!(iter.next().is_none());
                } else {
                    let reports = LeAdvReports::from_hci_bytes_complete(&bytes).unwrap();
                    let mut iter = reports.iter();
                    for left in (1..=expected).rev() {
                        assert_eq!(iter.len(), left);
                        assert_eq!(iter.size_hint(), (left, Some(left)));
                        assert_eq!(iter.next().unwrap().is_err(), malformed && left == 1);
                    }
                    assert_eq!(iter.len(), 0);
                    assert!(iter.next().is_none());
                    assert!(iter.next().is_none());
                }
            }
        }
    }
}

/// In-memory transport for actual ExternalController variable-return integration.
#[derive(Default)]
struct DeclarationTransport {
    /// A single injected complete H4 event, consumed by the reader.
    incoming: core::cell::RefCell<Option<std::vec::Vec<u8>>>,
    /// Submission count; no hardware I/O occurs.
    writes: core::cell::Cell<usize>,
}

/// Synthetic decode error at the transport boundary.
#[derive(Debug)]
struct DeclarationIoError;
impl core::fmt::Display for DeclarationIoError {
    /// Writes the synthetic error to `f`, returning its formatting result.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("synthetic transport decode failure")
    }
}
impl core::error::Error for DeclarationIoError {}
impl embedded_io::Error for DeclarationIoError {
    /// Returns the generic error kind used by this in-memory fixture.
    fn kind(&self) -> embedded_io::ErrorKind {
        embedded_io::ErrorKind::Other
    }
}
impl From<crate::ReadHciError<core::convert::Infallible>> for DeclarationIoError {
    /// Returns a synthetic boundary error for any packet decoding failure.
    fn from(_: crate::ReadHciError<core::convert::Infallible>) -> Self {
        Self
    }
}
impl embedded_io::ErrorType for &DeclarationTransport {
    /// Fixture-only decode failure, with no hardware transport involved.
    type Error = DeclarationIoError;
}
impl crate::transport::Transport for &DeclarationTransport {
    /// Waits for injected bytes, then returns a packet decoded using scratch `rx`.
    /// Returns malformed-packet errors; cancellation after dequeue consumes that fixture event.
    async fn read<'a, P: crate::transport::PacketToHost<'a>>(&self, rx: &'a mut [u8]) -> Result<P, DeclarationIoError> {
        let bytes = core::future::poll_fn(|_| {
            self.incoming
                .borrow_mut()
                .take()
                .map(core::task::Poll::Ready)
                .unwrap_or(core::task::Poll::Pending)
        })
        .await;
        let mut input = bytes.as_slice();
        let kind = crate::transport::PacketKind::read(&mut input)?;
        P::read_hci(kind, &mut input, rx).map_err(Into::into)
    }
    /// Counts one submission and returns success; wire serialization is tested separately.
    async fn write<T: crate::transport::PacketToController>(&self, _: &T) -> Result<(), DeclarationIoError> {
        self.writes.set(self.writes.get() + 1);
        Ok(())
    }
}

/// Runs a real generic command through its slot, response copy, typed decoding and release.
/// Returns `cmd`'s result after injecting Command Complete `status` and return `body`.
/// Panics if submission or completion fails to reach its expected poll state.
fn controller_return<C: crate::cmd::SyncCmd>(
    cmd: &C, status: u8, body: &[u8],
) -> Result<C::Return, crate::cmd::Error<DeclarationIoError>> {
    use core::future::Future;
    use core::task::{Context, Poll, Waker};

    use crate::controller::{Controller, ControllerCmdSync, ExternalController};
    let transport = DeclarationTransport::default();
    let controller = ExternalController::<_, 1>::new(&transport);
    let mut command = core::pin::pin!(controller.exec(cmd));
    let mut cx = Context::from_waker(Waker::noop());
    assert!(command.as_mut().poll(&mut cx).is_pending());
    assert_eq!(transport.writes.get(), 1);
    let opcode = C::OPCODE.to_raw().to_le_bytes();
    let mut bytes = std::vec![
        4,
        0x0E,
        u8::try_from(4 + body.len()).unwrap(),
        1,
        opcode[0],
        opcode[1],
        status
    ];
    bytes.extend_from_slice(body);
    *transport.incoming.borrow_mut() = Some(bytes);
    let mut buf = controller.alloc_buf().unwrap();
    {
        let mut receive = core::pin::pin!(controller.read(&mut buf));
        assert!(receive.as_mut().poll(&mut cx).is_pending());
    }
    let Poll::Ready(result) = command.as_mut().poll(&mut cx) else {
        panic!("completion must wake command");
    };
    result
}

/// Real controller execution accepts variable CIG/group prefixes and rejects malformed successful returns.
#[test]
fn external_controller_executes_variable_length_returns() {
    let params =
        LeSetCigParametersParams::from_hci_bytes_complete(&[7, 0, 1, 0, 0, 2, 0, 6, 1, 0, 10, 0, 20, 0, 0]).unwrap();
    let cig = LeSetCigParameters::from(params);
    let returned = controller_return(&cig, 0, &[7, 1, 0x34, 2]).unwrap();
    assert_eq!(returned.num_cis, 1);
    assert_eq!(returned.cis_handles[0], ConnHandle::new(0x234));
    assert_eq!(controller_return(&cig, 0, &[7, 0]).unwrap().num_cis, 0);
    assert!(controller_return(&cig, 0, &[7, 2, 0x34, 2]).is_err());
    assert!(controller_return(&cig, 0, &[7, 0, 0xCC]).is_err());
    assert!(controller_return(&cig, 0x0C, &[]).is_err());
    let minimum = LeReadMinimumSupportedConnectionInterval::new();
    assert_eq!(controller_return(&minimum, 0, &[60, 0]).unwrap().num_groups, 0);
    let groups = [5, 2, 5, 0, 80, 0, 5, 0, 80, 0, 0x20, 3, 8, 0];
    let returned = controller_return(&minimum, 0, &groups).unwrap();
    assert_eq!(returned.num_groups, 2);
    assert_eq!({ returned.groups[1].stride }.as_micros(), 1000);
    assert!(controller_return(&minimum, 0, &groups[..13]).is_err());
    assert!(controller_return(&minimum, 0, &[60, 42]).is_err());
}

/// Core7.3.8 has two little-endian u16 counts after the separately handled status.
#[test]
fn stored_link_key_return_has_two_full_width_counts() {
    use crate::cmd::controller_baseband::ReadStoredLinkKeyReturn;
    let value = ReadStoredLinkKeyReturn::from_hci_bytes_complete(&[0xCD, 0xAB, 0x34, 0x12]).unwrap();
    assert_eq!({ value.max_num_keys } as u32, 0xABCD);
    assert_eq!({ value.num_keys_read } as u32, 0x1234);
    for bytes in [&[][..], &[0][..], &[0, 0][..], &[0, 0, 0][..], &[0, 0, 0, 0, 0][..]] {
        assert!(ReadStoredLinkKeyReturn::from_hci_bytes_complete(bytes).is_err());
    }
}

/// Actual HCI completion consumes status once and returns both complete count words.
#[test]
fn external_controller_reads_stored_link_key_counts() {
    use crate::cmd::controller_baseband::ReadStoredLinkKey;
    let command = ReadStoredLinkKey::new(crate::param::BdAddr::new([1; 6]), true);
    for (bytes, max, read) in [
        ([0xCD, 0xAB, 0x34, 0x12], 0xABCD, 0x1234),
        ([0; 4], 0, 0),
        ([255; 4], 65535, 65535),
    ] {
        let value = controller_return(&command, 0, &bytes).unwrap();
        assert_eq!({ value.max_num_keys } as u32, max);
        assert_eq!({ value.num_keys_read } as u32, read);
    }
    assert!(matches!(
        controller_return(&command, 0x0C, &[]),
        Err(crate::cmd::Error::Hci(crate::param::Error::CMD_DISALLOWED))
    ));
}
