//! Exact legacy HCI report decoding, partial-batch diagnostics and callback compatibility.

/// Production decoder imported without its hardware callback adapter.
#[path = "../../firmware/ble-probe/src/reports.rs"]
mod reports;

use bt_hci::FromHciBytes;
use bt_hci::param::LeAdvReports;
use dashboard_core::discovery::{Event, ReportKind, ScanDiagnostics, ScanErrorKind, ScanFailure};

/// Returns owned events and diagnostics from counted HCI `bytes` at 1234 ms.
/// `revision` enables that scan generation; None suppresses reports and errors.
/// Panics if the outer batch envelope cannot be decoded.
fn decode(
    bytes: &[u8],
    revision: Option<u32>,
) -> (Vec<Event>, ScanDiagnostics) {
    let reports = LeAdvReports::from_hci_bytes_complete(bytes).unwrap();
    let mut events = Vec::new();
    let mut diagnostics = ScanDiagnostics::new();
    reports::parse_reports(
        revision,
        reports,
        1234,
        |report| events.push(report),
        |failure| diagnostics.record(failure),
    );
    (events, diagnostics)
}

/// Accepted random-address reports preserve reversed wire address, payload, RSSI and generation.
#[test]
fn accepted_report_preserves_address_payload_rssi_and_scan_identity() {
    let (events, diagnostics) = decode(&[1, 0, 1, 1, 2, 3, 4, 5, 6, 3, 2, 1, 6, 0x9E], Some(7));
    assert_eq!(events.len(), 1);
    let Event::Report { revision, report } = &events[0] else {
        panic!("expected advertisement")
    };
    assert_eq!(*revision, 7);
    assert_eq!(report.peer.address, [6, 5, 4, 3, 2, 1]);
    assert!(report.peer.random);
    assert_eq!(report.kind, ReportKind::Connectable);
    assert_eq!(report.data.as_slice(), [2, 1, 6]);
    assert_eq!(report.rssi, -98);
    assert_eq!(report.at_ms, 1234);
    assert_eq!(diagnostics.total, 0);
}

/// All 256 RSSI encodings, including unavailable 127, survive parsing as raw evidence.
#[test]
fn every_rssi_byte_is_delivered_without_becoming_a_parse_failure() {
    for rssi in i8::MIN..=i8::MAX {
        let (events, diagnostics) = decode(&[1, 4, 0, 1, 2, 3, 4, 5, 6, 0, rssi as u8], Some(7));
        assert_eq!(events.len(), 1, "RSSI {rssi}");
        let Event::Report { report, .. } = &events[0] else {
            panic!("expected advertisement")
        };
        assert_eq!(report.rssi, rssi);
        assert_eq!(report.kind, ReportKind::ScanResponse);
        assert_eq!(diagnostics.total, 0);
    }
}

/// Invalid event-kind bytes produce one value failure with exact cursor and generation evidence.
#[test]
fn invalid_event_kind_retains_failure_category_and_batch_context() {
    let (events, diagnostics) = decode(&[1, 5, 0, 1, 2, 3, 4, 5, 6, 0, 0x9E], Some(9));
    assert!(events.is_empty());
    assert_eq!(
        (diagnostics.total, diagnostics.invalid_size, diagnostics.invalid_value),
        (1, 0, 1)
    );
    assert_eq!(
        diagnostics.last,
        Some(ScanFailure {
            kind: ScanErrorKind::InvalidValue,
            revision: 9,
            index: 0,
            total_reports: 1,
            report_bytes: 10,
            remaining_bytes: 10,
            declared_data_len: Some(0),
            at_ms: 1234
        })
    );
}

/// Missing RSSI and incomplete declared data both reject the record as a size failure.
#[test]
fn missing_rssi_and_truncated_data_are_size_failures() {
    for bytes in [
        &[1, 0, 0, 1, 2, 3, 4, 5, 6, 0][..],
        &[1, 0, 0, 1, 2, 3, 4, 5, 6, 2, 0xAA][..],
    ] {
        let (events, diagnostics) = decode(bytes, Some(9));
        assert!(events.is_empty());
        assert_eq!(
            (diagnostics.total, diagnostics.invalid_size, diagnostics.invalid_value),
            (1, 1, 0)
        );
        assert_eq!(diagnostics.last.unwrap().kind, ScanErrorKind::InvalidSize);
    }
}

/// A malformed second record preserves the first event and stops once without guessing a third.
#[test]
fn malformed_second_report_preserves_valid_prefix_and_counts_one_failed_batch() {
    let (events, diagnostics) = decode(
        &[3, 0, 0, 1, 2, 3, 4, 5, 6, 0, 0xC6, 5, 0, 6, 5, 4, 3, 2, 1, 0, 0x9E],
        Some(11),
    );
    assert_eq!(events.len(), 1);
    assert_eq!(diagnostics.total, 1);
    assert_eq!(
        diagnostics.last,
        Some(ScanFailure {
            kind: ScanErrorKind::InvalidValue,
            revision: 11,
            index: 1,
            total_reports: 3,
            report_bytes: 20,
            remaining_bytes: 10,
            declared_data_len: Some(0),
            at_ms: 1234
        })
    );
}

/// Disabled scan admission suppresses both late valid reports and malformed-batch diagnostics.
#[test]
fn disabled_scan_revision_discards_late_reports_and_errors() {
    for bytes in [&[1, 0, 0, 1, 2, 3, 4, 5, 6, 0, 0x9E][..], &[1, 5][..]] {
        let (events, diagnostics) = decode(bytes, None);
        assert!(events.is_empty());
        assert_eq!(diagnostics.total, 0);
        assert_eq!(diagnostics.last, None);
    }
}

/// Policy filtering is silent, while malformed AD payload bytes remain valid raw HCI evidence.
#[test]
fn filtering_and_ad_decode_failures_are_not_hci_parse_failures() {
    for bytes in [
        [1, 3, 0, 1, 2, 3, 4, 5, 6, 0, 0x9E],
        [1, 0, 2, 1, 2, 3, 4, 5, 6, 0, 0x9E],
    ] {
        let (events, diagnostics) = decode(&bytes, Some(1));
        assert!(events.is_empty());
        assert_eq!(diagnostics.total, 0);
    }
    let (events, diagnostics) = decode(&[1, 1, 0, 1, 2, 3, 4, 5, 6, 1, 0xFF, 0x9E], Some(1));
    assert_eq!(events.len(), 1);
    assert_eq!(diagnostics.total, 0);
}

/// Returns one public connectable wire record without a batch count, with RSSI -94 dBm.
/// `length` sets payload size and its one-byte declaration (caller must keep it <=255);
/// `identity` sets the first wire address byte. Payload bytes are ascending modulo 256.
fn record(
    length: usize,
    identity: u8,
) -> Vec<u8> {
    let mut bytes = vec![0, 0, identity, 2, 3, 4, 5, 6, length as u8];
    bytes.extend((0..length).map(|n| n as u8));
    bytes.push(0xA2);
    bytes
}

/// Every short header prefix reports its exact size and distinguishes absent length from zero.
#[test]
fn short_prefixes_distinguish_absent_data_length_from_present_zero() {
    let complete = record(0, 1);
    for length in 0..complete.len() {
        let mut bytes = vec![1];
        bytes.extend(&complete[..length]);
        let (events, diagnostics) = decode(&bytes, Some(31));
        assert!(events.is_empty());
        let failure = diagnostics.last.unwrap();
        assert_eq!(failure.kind, ScanErrorKind::InvalidSize);
        assert_eq!(
            (failure.report_bytes, failure.remaining_bytes),
            (length as u16, length as u16)
        );
        assert_eq!(failure.declared_data_len, (length >= 9).then_some(0));
        assert_eq!(
            (failure.revision, failure.index, failure.total_reports, failure.at_ms),
            (31, 0, 1, 1234)
        );
    }
}

/// Payload lengths 0..=31 retain complete bytes; every data/RSSI truncation reports exact context.
#[test]
fn every_legal_data_length_accepts_complete_data_and_explains_every_truncation() {
    for length in 0..=31 {
        let complete = record(length, 1);
        let mut bytes = vec![1];
        bytes.extend(&complete);
        let (events, diagnostics) = decode(&bytes, Some(1));
        assert_eq!(events.len(), 1);
        assert_eq!(diagnostics.total, 0);
        let Event::Report { report, .. } = &events[0] else {
            panic!()
        };
        assert_eq!(report.data.as_slice(), &complete[9..9 + length]);
        for retained in 9..complete.len() {
            let mut bytes = vec![1];
            bytes.extend(&complete[..retained]);
            let (events, diagnostics) = decode(&bytes, Some(1));
            assert!(events.is_empty());
            let failure = diagnostics.last.unwrap();
            assert_eq!(failure.kind, ScanErrorKind::InvalidSize);
            assert_eq!(
                (failure.report_bytes, failure.remaining_bytes),
                (retained as u16, retained as u16)
            );
            assert_eq!(failure.declared_data_len, Some(length as u8));
        }
    }
}

/// Two valid records remain delivered when a later record fails, with the remaining cursor intact.
#[test]
fn later_failure_retains_multiple_prior_reports_and_exact_remaining_bytes() {
    let mut bytes = vec![4];
    bytes.extend(record(0, 1));
    bytes.extend(record(31, 2));
    bytes.extend(&record(7, 3)[..12]);
    let (events, diagnostics) = decode(&bytes, Some(42));
    assert_eq!(events.len(), 2);
    assert_eq!(diagnostics.total, 1);
    let failure = diagnostics.last.unwrap();
    assert_eq!((failure.report_bytes, failure.remaining_bytes), (63, 12));
    assert_eq!(failure.declared_data_len, Some(7));
    assert_eq!((failure.index, failure.total_reports), (2, 4));
}

/// Seven records filling the 255-byte HCI parameter limit preserve every address and payload size.
#[test]
fn maximum_legal_event_batch_preserves_all_records_and_no_failure() {
    let lengths = [31, 31, 31, 31, 31, 28, 0];
    let mut bytes = vec![lengths.len() as u8];
    for (index, length) in lengths.into_iter().enumerate() {
        bytes.extend(record(length, index as u8));
    }
    assert_eq!(
        bytes.len() + 1,
        255,
        "subevent plus batch fills the HCI parameter limit"
    );
    let (events, diagnostics) = decode(&bytes, Some(1));
    assert_eq!(events.len(), lengths.len());
    assert_eq!(diagnostics.total, 0);
    for (index, event) in events.iter().enumerate() {
        let Event::Report { report, .. } = event else { panic!() };
        assert_eq!(report.data.as_slice().len(), lengths[index]);
        assert_eq!(report.peer.address[5], index as u8);
    }
}

/// Count exhaustion, trailing bytes, missing records and oversized payloads match the locked iterator.
#[test]
fn count_and_trailing_byte_behavior_matches_the_exact_dependency_iterator() {
    for bytes in [
        vec![0],
        [vec![0], record(0, 1)].concat(),
        [vec![1], record(0, 1), vec![0xFF]].concat(),
        [vec![2], record(0, 1)].concat(),
        [vec![2], record(0, 1), vec![0]].concat(),
        [vec![1], record(32, 1)].concat(),
    ] {
        let parsed = LeAdvReports::from_hci_bytes_complete(&bytes).unwrap();
        let expected: Vec<_> = parsed.iter().collect();
        let (events, diagnostics) = decode(&bytes, Some(1));
        assert_eq!(events.len(), expected.iter().filter(|r| r.is_ok()).count());
        assert_eq!(
            diagnostics.total as usize,
            expected.iter().filter(|r| r.is_err()).count()
        );
        if let Some(failure) = diagnostics.last {
            assert_eq!(failure.report_bytes, (bytes.len() - 1) as u16);
            assert_eq!(failure.remaining_bytes, (bytes.len() - 11) as u16);
            assert_eq!(failure.declared_data_len, None);
        }
    }
}

/// The full-batch callback forwards once to legacy iterator handlers without dropping valid records.
#[test]
fn full_report_callback_preserves_existing_iterator_only_handlers() {
    use core::cell::Cell;

    use trouble_host::prelude::EventHandler;
    /// Iterator-only handler observing the default full-batch compatibility adapter.
    struct LegacyHandler {
        /// Number of callback invocations.
        calls: Cell<usize>,
        /// Successfully decoded entries in the most recent callback.
        entries: Cell<usize>,
    }
    impl EventHandler for LegacyHandler {
        /// Consumes `reports`, counting successful entries and recording one callback invocation.
        fn on_adv_reports(
            &self,
            reports: bt_hci::param::LeAdvReportsIter<'_>,
        ) {
            self.calls.set(self.calls.get() + 1);
            self.entries.set(reports.filter(|r| r.is_ok()).count());
        }
    }
    let handler = LegacyHandler {
        calls: Cell::new(0),
        entries: Cell::new(0),
    };
    let bytes = [vec![2], record(0, 1), record(31, 2)].concat();
    handler.on_adv_report_data(LeAdvReports::from_hci_bytes_complete(&bytes).unwrap());
    assert_eq!(handler.calls.get(), 1);
    assert_eq!(handler.entries.get(), 2);
}
