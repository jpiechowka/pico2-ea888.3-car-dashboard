//! Independent review regressions: accepted evidence must be internally coherent.
use dashboard_core::discovery::capture::*;
use dashboard_core::discovery::*;

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
#[test]
fn capture_rejects_complete_metadata_with_missing_stages() {
    let mut p = probe();
    p.inspection.complete = true;
    assert!(
        encode_probe(&p, 12).is_err(),
        "capture must reject contradictory completion"
    );
}
#[test]
fn capture_rejects_reversed_boot_timeline() {
    let mut p = probe();
    p.inspection.ended_at_ms = Some(9);
    assert!(
        encode_probe(&p, 8).is_err(),
        "end/capture precede start and peer observation"
    );
}
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
