//! Independent review regressions: accepted evidence must be internally coherent.
use dashboard_core::discovery::capture::*;
use dashboard_core::discovery::*;

/// Returns a selected public-peer inspection started at 11ms after a 10ms advertisement; fixture request failure
/// panics.
fn probe() -> Probe {
    let mut p = Probe::new();
    p.catalog.observe(Advertisement::new(
        Peer {
            address: [1; 6],
            random: false,
        },
        ReportKind::Connectable,
        -50,
        &[],
        10,
    ));
    p.input(Input::Click, 11).unwrap();
    p
}
/// Encodes `p` at boot-relative `at` milliseconds with stock build metadata into 8192 temporary bytes.
/// Returns encoded length or exact capture admission/capacity failure; discards successful bytes.
fn encode_probe(
    p: &Probe,
    at: u64,
) -> Result<usize, CaptureError> {
    encode(
        p,
        BuildIdentity {
            source_digest: [1; 32],
            cpu_hz: 150_000_000,
            spi_hz: 75_000_000,
            profile: BuildProfile::Stock,
            source_identity_available: true,
        },
        at,
        &mut [0; 8192],
    )
}
/// A generic completion event cannot claim complete inventory without individual stage acknowledgments.
#[test]
fn completion_without_all_stage_acknowledgments_is_not_complete() {
    let mut p = probe();
    p.event(Event::Complete {
        revision: p.request.revision,
        limited: false,
    });
    assert!(
        !p.inspection.complete,
        "zero completed stages cannot claim complete inventory"
    );
}
/// Manually asserted complete metadata with missing stages is rejected by capture admission.
#[test]
fn capture_rejects_complete_metadata_with_missing_stages() {
    let mut p = probe();
    p.inspection.complete = true;
    assert!(
        encode_probe(&p, 12).is_err(),
        "capture must reject contradictory completion"
    );
}
/// Capture/end times preceding peer observation or inspection start are rejected.
#[test]
fn capture_rejects_reversed_boot_timeline() {
    let mut p = probe();
    p.inspection.ended_at_ms = Some(9);
    assert!(
        encode_probe(&p, 8).is_err(),
        "end/capture precede start and peer observation"
    );
}

/// Global scan errors may predate the selected peer but never the capture's future, including CRC-repaired binary
/// imports.
#[test]
fn global_scan_time_may_predate_peer_but_cannot_follow_capture() {
    let mut p = probe();
    for at_ms in [0, 9, 10, 12, 13] {
        p.scan_diagnostics.last = Some(ScanFailure {
            kind: ScanErrorKind::InvalidSize,
            revision: 1,
            index: 0,
            total_reports: 1,
            report_bytes: 3,
            remaining_bytes: 3,
            declared_data_len: None,
            at_ms,
        });
        assert_eq!(encode_probe(&p, 12).is_ok(), at_ms <= 12, "scan at {at_ms}");
    }
    p.scan_diagnostics.last.as_mut().unwrap().at_ms = 1001;
    let mut buffer = [0; 8192];
    let len = encode(
        &p,
        BuildIdentity {
            source_digest: [1; 32],
            cpu_hz: 150_000_000,
            spi_hz: 75_000_000,
            profile: BuildProfile::Stock,
            source_identity_available: true,
        },
        2000,
        &mut buffer,
    )
    .unwrap();
    let offsets: Vec<_> = buffer[..len]
        .windows(8)
        .enumerate()
        .filter_map(|(offset, bytes)| (bytes == 1001u64.to_le_bytes()).then_some(offset))
        .collect();
    assert_eq!(offsets.len(), 1, "unique scan timestamp in this fixture");
    let offset = offsets[0];
    buffer[offset..offset + 8].copy_from_slice(&2001u64.to_le_bytes());
    let body = capture_store::crc32(&buffer[24..len]);
    buffer[16..20].copy_from_slice(&body.to_le_bytes());
    let header = capture_store::crc32(&buffer[..20]);
    buffer[20..24].copy_from_slice(&header.to_le_bytes());
    assert_eq!(decode(&buffer[..len]).unwrap_err(), CaptureError::InvalidState);
}
/// A passive snapshot cannot attach cleanup/disconnect evidence without an inspection attempt.
#[test]
fn passive_capture_rejects_orphan_cleanup_evidence() {
    let mut p = probe();
    p.inspection = Inspection::new();
    p.request.action = Action::Scan(false);
    p.inspection.cleanup_complete = true;
    p.inspection.disconnect_reason = Some(0x13);
    assert!(
        encode_probe(&p, 12).is_err(),
        "passive capture cannot claim connection cleanup"
    );
}
/// Live admission rejects crossing declaration/value ranges, counts malformed rows and keeps the valid prefix saveable.
#[test]
fn live_model_rejects_crossing_characteristic_declaration_ranges() {
    let mut p = probe();
    let uuid = Uuid::from_wire(&[0x0A, 0x18]).unwrap();
    p.inspection.accept(InspectionItem::Service(Service {
        start: 1,
        end: 20,
        uuid,
    }));
    for (declaration, handle) in [(2, 8), (4, 9)] {
        p.inspection.accept(InspectionItem::Characteristic(Characteristic {
            service_start: 1,
            declaration,
            handle,
            properties: 2,
            uuid,
        }));
    }
    assert_eq!(
        p.inspection.characteristics.len(),
        1,
        "live admission must agree with codec relations"
    );
    assert_eq!(p.inspection.malformed, 1);
    assert!(encode_probe(&p, 12).is_ok(), "valid prefix remains saveable");
}

/// Recomputed envelope CRCs cannot admit contradictory revision/count/time/cleanup/stage bookkeeping.
#[test]
fn crc_valid_imports_reject_inconsistent_bookkeeping() {
    let p = probe();
    let mut buffer = [0; 8192];
    let len = encode(
        &p,
        BuildIdentity {
            source_digest: [1; 32],
            cpu_hz: 150_000_000,
            spi_hz: 75_000_000,
            profile: BuildProfile::Stock,
            source_identity_available: true,
        },
        12,
        &mut buffer,
    )
    .unwrap();
    // Schema v1 offsets for this deliberately minimal frozen layout. Mutate
    // fields rather than CRCs, then independently recompute both envelope CRCs.
    assert_eq!(buffer[78], 1); // gatt_requested
    assert_eq!(&buffer[129..137], &11u64.to_le_bytes()); // started_at_ms
    for (at, bytes) in [
        (74, 0u32.to_le_bytes().to_vec()),  // zero request revision
        (104, 0u32.to_le_bytes().to_vec()), // impossible zero peer reports
        (66, 9u64.to_le_bytes().to_vec()),  // capture before observation/start
        (88, 11u64.to_le_bytes().to_vec()), // first after last
        (113, 9u64.to_le_bytes().to_vec()), // packet before first observation
        (139, vec![1]),                     // cleanup with no terminal timestamp
        (141, vec![1]),                     // characteristics completed without services
        (152, vec![1]),                     // complete with no stage acknowledgments
    ] {
        let mut damaged = buffer[..len].to_vec();
        damaged[at..at + bytes.len()].copy_from_slice(&bytes);
        let body = capture_store::crc32(&damaged[24..]);
        damaged[16..20].copy_from_slice(&body.to_le_bytes());
        let header = capture_store::crc32(&damaged[..20]);
        damaged[20..24].copy_from_slice(&header.to_le_bytes());
        assert_eq!(decode(&damaged).unwrap_err(), CaptureError::InvalidState, "field {at}");
    }
}

/// Pending/failed attempts retain valid partial evidence; returning to fresh passive details clears old inspection
/// state.
#[test]
fn partial_failure_and_passive_snapshots_keep_valid_evidence() {
    let mut p = probe();
    assert!(encode_probe(&p, 12).is_ok(), "pending attempt has unavailable end/link");
    p.failure = Some(InspectionFailure::new(FailureStage::Connect, FailureReason::Timeout));
    p.error = ProbeError::Timeout;
    p.inspection.ended_at_ms = Some(12);
    p.inspection.cleanup_complete = true;
    assert!(encode_probe(&p, 13).is_ok(), "failed attempt has no established link");
    p.input(Input::Back, 14);
    p.input(Input::Details, 15);
    assert!(encode_probe(&p, 16).is_ok(), "fresh passive view clears old inspection");
}

/// Independently completed reads remain saveable despite descriptor limits, and all acknowledged stages permit
/// completion.
#[test]
fn valid_completed_and_independently_limited_stages_remain_saveable() {
    let mut p = probe();
    p.inspection.stages = [true, true, false, true];
    p.inspection.omitted = 1;
    assert!(
        encode_probe(&p, 12).is_ok(),
        "descriptor limit does not erase completed reads"
    );
    p.inspection.stages = [true; 4];
    p.inspection.omitted = 0;
    p.event(Event::Complete {
        revision: p.request.revision,
        limited: false,
    });
    assert!(p.inspection.complete);
    assert!(encode_probe(&p, 12).is_ok());
}
