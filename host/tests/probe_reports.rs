#[path = "../../firmware/ble-probe/src/reports.rs"]
mod reports;

use bt_hci::FromHciBytes;
use bt_hci::param::LeAdvReports;
use dashboard_core::discovery::{Event, ReportKind, ScanDiagnostics, ScanErrorKind, ScanFailure};

fn decode(
    bytes: &[u8],
    revision: Option<u32>,
) -> (Vec<Event>, ScanDiagnostics) {
    let reports = LeAdvReports::from_hci_bytes_complete(bytes).unwrap();
    let mut events = Vec::new();
    let mut diagnostics = ScanDiagnostics::new();
    reports::parse_reports(
        revision,
        reports.iter(),
        1234,
        |report| events.push(report),
        |failure| diagnostics.record(failure),
    );
    (events, diagnostics)
}

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
            at_ms: 1234
        })
    );
}

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
            at_ms: 1234
        })
    );
}

#[test]
fn disabled_scan_revision_discards_late_reports_and_errors() {
    for bytes in [&[1, 0, 0, 1, 2, 3, 4, 5, 6, 0, 0x9E][..], &[1, 5][..]] {
        let (events, diagnostics) = decode(bytes, None);
        assert!(events.is_empty());
        assert_eq!(diagnostics.total, 0);
        assert_eq!(diagnostics.last, None);
    }
}

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
