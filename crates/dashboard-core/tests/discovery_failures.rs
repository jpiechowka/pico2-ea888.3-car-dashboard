use dashboard_core::discovery::*;

fn inspecting() -> Probe {
    let mut probe = Probe::new();
    probe.catalog.observe(Advertisement::new(
        Peer {
            address: [1; 6],
            random: false,
        },
        ReportKind::Connectable,
        -50,
        &[],
        100,
    ));
    probe.input(Input::Click, 100).unwrap();
    probe
}

#[test]
fn failed_inspection_cannot_be_marked_complete_by_a_late_completion() {
    for error in [
        ProbeError::Discovery,
        ProbeError::Timeout,
        ProbeError::Disconnected,
        ProbeError::Cleanup,
    ] {
        let mut probe = inspecting();
        probe.event(Event::State {
            revision: probe.request.revision,
            phase: RadioPhase::Paused,
            error,
        });
        probe.event(Event::Complete {
            revision: probe.request.revision,
            limited: false,
        });
        assert!(!probe.inspection.complete, "failure {error:?} relabeled complete");
    }
}

#[test]
fn first_failure_survives_cleanup_and_only_new_requests_clear_it() {
    let mut probe = inspecting();
    let revision = probe.request.revision;
    let mut failure = InspectionFailure::new(FailureStage::Characteristics, FailureReason::Att(0x0E));
    failure.range = Some((1, 7));
    failure.last_accepted = Some((4, 5));
    failure.elapsed_ms = Some(321);
    probe.page = 8;
    probe.event(Event::Failure { revision, failure });
    assert_eq!(probe.page, 0, "failure evidence becomes visible immediately");
    for phase in [RadioPhase::Disconnecting, RadioPhase::Paused] {
        probe.event(Event::State {
            revision,
            phase,
            error: ProbeError::None,
        });
        assert_eq!(probe.failure, Some(failure));
        assert_eq!(probe.error, ProbeError::Discovery);
    }
    probe.event(Event::Failure {
        revision,
        failure: InspectionFailure::new(FailureStage::ClientTask, FailureReason::Disconnected),
    });
    assert_eq!(probe.failure, Some(failure));
    probe.event(Event::Complete {
        revision,
        limited: false,
    });
    assert!(!probe.inspection.complete);
    let next = probe.input(Input::Click, 101).unwrap();
    assert!(next.revision > revision);
    assert_eq!(probe.failure, None);
    assert_eq!(probe.error, ProbeError::None);
    probe.event(Event::Failure { revision, failure });
    assert_eq!(probe.failure, None);
}

#[test]
fn cancellation_recovery_and_scan_cannot_receive_old_failure_evidence() {
    for input in [Input::Back, Input::Recover] {
        let mut probe = inspecting();
        let old = probe.request.revision;
        probe.input(input, 101).unwrap();
        let failure = InspectionFailure::new(FailureStage::Mtu, FailureReason::Timeout);
        for revision in [old, probe.request.revision] {
            probe.event(Event::Failure { revision, failure });
            assert_eq!(probe.failure, None);
        }
        probe.event(Event::Complete {
            revision: old,
            limited: false,
        });
        assert!(!probe.inspection.complete);
    }
}

#[test]
fn successful_and_protected_values_complete_but_failures_preserve_partial_rows() {
    for success in [true, false] {
        let mut probe = inspecting();
        let revision = probe.request.revision;
        let service = Service {
            start: 1,
            end: 7,
            uuid: Uuid::from_wire(&[0x0A, 0x18]).unwrap(),
        };
        probe.event(Event::Item {
            revision,
            item: InspectionItem::Service(service),
        });
        let row = characteristic_from_wire(1, 7, 1, 2, &[2, 3, 0, 0x29, 0x2A]).unwrap();
        probe.event(Event::Item {
            revision,
            item: InspectionItem::Characteristic(row),
        });
        probe.event(Event::Item {
            revision,
            item: InspectionItem::Value(InfoValue {
                uuid: 0x2A29,
                value: Payload::empty(),
                result: ReadResult::Protected,
            }),
        });
        if !success {
            probe.event(Event::Failure {
                revision,
                failure: InspectionFailure::new(FailureStage::DeviceInfo, FailureReason::Timeout),
            });
        }
        probe.event(Event::Complete {
            revision,
            limited: false,
        });
        assert_eq!(probe.inspection.complete, success);
        assert_eq!(probe.inspection.services.len(), 1);
        assert_eq!(probe.inspection.characteristics.len(), 1);
        assert_eq!(probe.inspection.values[0].result, ReadResult::Protected);
    }
}

#[test]
fn characteristic_wire_validation_handles_length_order_bounds_and_uuid_widths() {
    for length in 0..=32 {
        let mut bytes = [0xFF; 32];
        bytes[0] = 0x20;
        bytes[1] = 3;
        bytes[2] = 0;
        let result = characteristic_from_wire(1, 7, 1, 2, &bytes[..length]);
        assert_eq!(result.is_ok(), matches!(length, 5 | 19));
        if let Ok(row) = result {
            assert_eq!((row.declaration, row.handle, row.properties), (2, 3, 0x20));
            assert_eq!(row.uuid.len, (length - 3) as u8);
        }
    }
    for (start, end, previous, decl) in [(0, 7, 0, 2), (7, 1, 1, 2), (1, 7, 1, 1), (1, 7, 3, 3), (1, 7, 1, 7)] {
        assert!(matches!(
            characteristic_from_wire(start, end, previous, decl, &[2, 3, 0, 0, 0]),
            Err(FailureReason::DeclarationOrder)
        ));
    }
    for value in [0, 1, 2, 8, u16::MAX] {
        let [lo, hi] = value.to_le_bytes();
        assert!(matches!(
            characteristic_from_wire(1, 7, 1, 2, &[2, lo, hi, 0, 0]),
            Err(FailureReason::ValueHandle)
        ));
    }
    let last = characteristic_from_wire(1, u16::MAX, 2, 0xFFFE, &[2, 0xFF, 0xFF, 0, 0]).unwrap();
    assert_eq!(last.handle, u16::MAX);
}

#[test]
fn failure_codes_and_missing_measurements_remain_exact() {
    for code in u8::MIN..=u8::MAX {
        for reason in [
            FailureReason::Att(code),
            FailureReason::Hci(code),
            FailureReason::DisconnectCode(code),
        ] {
            let failure = InspectionFailure::new(FailureStage::ClientTask, reason);
            assert_eq!(failure.reason.code(), Some(code));
            assert_eq!(failure.elapsed_ms, None);
            assert_eq!(failure.range, None);
            assert_eq!(failure.last_accepted, None);
        }
    }
    assert_eq!(
        InspectionFailure::new(FailureStage::Connect, FailureReason::Hci(8)).error(),
        ProbeError::Connect
    );
    assert_eq!(
        InspectionFailure::new(FailureStage::Mtu, FailureReason::Timeout).error(),
        ProbeError::Timeout
    );
}

#[test]
fn global_scan_diagnostics_survive_pause_and_clear_and_keep_the_original_revision() {
    let mut probe = inspecting();
    let failure = ScanFailure {
        kind: ScanErrorKind::InvalidSize,
        revision: 1,
        index: 2,
        total_reports: 3,
        at_ms: 90,
    };
    probe.scan_diagnostics.record(failure);
    probe.input(Input::Back, 101);
    probe.input(Input::Recover, 102);
    assert_eq!(probe.scan_diagnostics.last, Some(failure));
    assert_eq!(probe.scan_diagnostics.total, 1);
    assert!(probe.catalog.devices().is_empty());
}

#[test]
fn scan_failure_details_are_reachable_without_any_valid_peer() {
    let mut probe = Probe::new();
    assert!(probe.input(Input::Details, 10).is_none());
    probe.scan_diagnostics.record(ScanFailure {
        kind: ScanErrorKind::InvalidSize,
        revision: 1,
        index: 0,
        total_reports: 1,
        at_ms: 10,
    });
    let request = probe.input(Input::Details, 11).expect("X opens global diagnostics");
    assert_eq!(request.action, Action::Scan(false));
    assert_eq!(probe.screen, Screen::Details);
    probe.event(Event::State {
        revision: request.revision,
        phase: RadioPhase::Paused,
        error: ProbeError::None,
    });
    for input in [Input::Click, Input::Details] {
        assert!(probe.input(input, 12).is_none(), "cannot inspect without a peer");
    }
    probe.event(Event::Report {
        revision: 1,
        report: Advertisement::new(
            Peer {
                address: [1; 6],
                random: false,
            },
            ReportKind::Connectable,
            -20,
            &[],
            12,
        ),
    });
    assert!(probe.catalog.devices().is_empty());
    assert_eq!(probe.input(Input::Back, 13).unwrap().action, Action::Scan(true));
    assert_eq!(probe.screen, Screen::List);
    assert_eq!(probe.scan_diagnostics.total, 1);
}
