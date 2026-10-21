//! Observed BLE name provenance across raw packet replacement, explicit inspection and peer identity boundaries.

use dashboard_core::discovery::*;

/// Public-address peer fixture in controller byte order.
const PEER: Peer = Peer {
    address: [1, 2, 3, 4, 5, 6],
    random: false,
};

/// Returns a -60dBm `kind` report for `peer` with bounded raw `data` captured at `at_ms` milliseconds.
fn report(
    peer: Peer,
    kind: ReportKind,
    data: &[u8],
    at_ms: u64,
) -> Advertisement {
    Advertisement::new(peer, kind, -60, data, at_ms)
}

/// Retained display name survives nameless reports while the separately stored raw advertisement remains current.
#[test]
fn name_survives_packets_without_name_and_raw_evidence_stays_current() {
    let mut catalog = Catalog::new();
    catalog.observe(report(PEER, ReportKind::Connectable, b"\x03\x09TV", 1));
    catalog.observe(report(PEER, ReportKind::Connectable, &[2, 1, 6], 2));
    let device = &catalog.devices()[0];
    assert_eq!(device.name(), "TV");
    assert_eq!(device.advertisement.as_slice(), &[2, 1, 6]);
    assert_eq!(device.advertisement_meta.unwrap().at_ms, 2);
}

/// An empty shortened-name response cannot erase a nonempty shortened advertisement name.
#[test]
fn empty_short_response_does_not_hide_nonempty_short_advertisement() {
    let mut catalog = Catalog::new();
    catalog.observe(report(PEER, ReportKind::Connectable, b"\x03\x08TV", 1));
    catalog.observe(report(PEER, ReportKind::ScanResponse, &[1, 8], 2));
    assert_eq!(catalog.devices()[0].name(), "TV");
}

/// Returns an explicitly inspecting probe with one `service` UUID and readable Device Name at handle 3.
/// Fixture selection/UUID admission failure panics; no radio I/O occurs.
fn inspecting(service: u16) -> Probe {
    let mut p = Probe::new();
    p.event(Event::Report {
        revision: 1,
        report: report(PEER, ReportKind::Connectable, &[2, 1, 6], 1),
    });
    let revision = p.input(Input::Click, 2).unwrap().revision;
    for item in [
        InspectionItem::Service(Service {
            start: 1,
            end: 5,
            uuid: Uuid::from_wire(&service.to_le_bytes()).unwrap(),
        }),
        InspectionItem::Characteristic(Characteristic {
            service_start: 1,
            declaration: 2,
            handle: 3,
            properties: 2,
            uuid: Uuid::from_wire(&[0, 0x2A]).unwrap(),
        }),
    ] {
        p.event(Event::Item { revision, item });
    }
    p
}

/// Publishes current-generation Device Name `data` with `result` into `p`; non-Ok outcomes carry Disconnected evidence.
fn value(
    p: &mut Probe,
    result: ReadResult,
    data: Payload<64>,
) {
    p.event(Event::Item {
        revision: p.request.revision,
        item: InspectionItem::Value(InfoValue {
            service_start: 1,
            handle: 3,
            uuid: 0x2A00,
            value: data,
            result,
            failure: (result != ReadResult::Ok).then_some(FailureReason::Disconnected),
            elapsed_ms: 1,
        }),
    });
}

/// Publishes cleanup-complete inspection termination into `p` for `revision` at `at_ms` milliseconds.
fn end(
    p: &mut Probe,
    revision: u32,
    at_ms: u64,
) {
    p.event(Event::InspectionEnd {
        revision,
        at_ms,
        cleanup_complete: true,
        disconnect_reason: Some(0x16),
    });
}

/// GAP name becomes reusable only after clean inspection end, survives scan return and is cleared by recovery.
#[test]
fn explicit_admitted_gap_name_is_retained_after_return_to_scan() {
    let mut p = inspecting(0x1800);
    value(&mut p, ReadResult::Ok, Payload::new(b"Light"));
    assert!(p.selected_device().unwrap().name().is_empty());
    let revision = p.request.revision;
    end(&mut p, revision, 5);
    assert_eq!(p.selected_device().unwrap().name(), "Light");
    p.input(Input::Back, 6);
    p.event(Event::Report {
        revision: p.request.revision,
        report: report(PEER, ReportKind::Connectable, &[2, 1, 6], 7),
    });
    assert_eq!(p.selected_device().unwrap().name(), "Light");
    p.input(Input::Recover, 8);
    p.event(Event::Report {
        revision: p.request.revision,
        report: report(PEER, ReportKind::Connectable, &[2, 1, 6], 9),
    });
    assert!(p.selected_device().unwrap().name().is_empty());
}

/// Failed/empty/truncated/wrong-service names and stale-generation/time completions cannot name the peer.
#[test]
fn failed_empty_truncated_foreign_or_stale_gap_evidence_does_not_name_peer() {
    for (service, result, data) in [
        (0x1800, ReadResult::Protected, Payload::new(b"No")),
        (0x1800, ReadResult::Unavailable, Payload::new(b"No")),
        (0x1800, ReadResult::Error, Payload::new(b"No")),
        (0x1800, ReadResult::Ok, Payload::empty()),
        (0x1800, ReadResult::Ok, Payload::new(&[b'X'; 65])),
        (0x180A, ReadResult::Ok, Payload::new(b"Wrong service")),
    ] {
        let mut p = inspecting(service);
        value(&mut p, result, data);
        let revision = p.request.revision;
        end(&mut p, revision, 5);
        assert!(p.selected_device().unwrap().name().is_empty());
    }
    for (revision, at) in [(1, 5), (2, 0)] {
        let mut p = inspecting(0x1800);
        value(&mut p, ReadResult::Ok, Payload::new(b"No"));
        end(&mut p, revision, at);
        assert!(p.selected_device().unwrap().name().is_empty());
    }
}

/// Complete names outrank short names; regressed/malformed packets and differing address types cannot overwrite
/// identity.
#[test]
fn cache_respects_complete_names_time_and_exact_peer_identity() {
    let mut catalog = Catalog::new();
    catalog.observe(report(PEER, ReportKind::Connectable, b"\x03\x09TV", 10));
    catalog.observe(report(PEER, ReportKind::Connectable, b"\x02\x08T", 11));
    catalog.observe(report(PEER, ReportKind::Connectable, b"\x04\x09OLD", 9));
    let random = Peer { random: true, ..PEER };
    catalog.observe(report(random, ReportKind::ScanResponse, b"\x04\x09BAD", 12));
    assert_eq!(catalog.devices().len(), 1);
    catalog.observe(report(random, ReportKind::Connectable, &[2, 1, 6], 13));
    assert_eq!(catalog.devices()[0].name(), "TV");
    assert!(catalog.devices()[1].name().is_empty());
    catalog.observe(report(PEER, ReportKind::ScanResponse, b"\x04\x09NEW", 14));
    assert_eq!(catalog.devices()[0].name(), "NEW");
    catalog.observe(report(PEER, ReportKind::ScanResponse, &[2, 1, 6, 20, 9, b'X'], 15));
    assert_eq!(catalog.devices()[0].name(), "NEW");
}
