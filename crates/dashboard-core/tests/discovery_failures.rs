//! Probe failure provenance and generation fences; all radio events are synthetic.
use dashboard_core::discovery::*;

/// Returns a probe with one connectable peer and an explicit Inspect request at100ms;
/// panics if the synthetic setup cannot issue that request.
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
/// Ensures late completion cannot convert any terminal inspection failure into success.
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
/// Preserves first failure and reader page while enriching terminal context, then fences a new generation.
fn terminal_context_enriches_first_failure_without_replacing_its_cause() {
    let mut probe = inspecting();
    let revision = probe.request.revision;
    let context = ConnectionFailureContext {
        local_error: Some(FailureReason::Capacity),
        disconnect_reason: Some(0x16),
    };
    let mut first = InspectionFailure::new(FailureStage::Mtu, FailureReason::Disconnected);
    first.elapsed_ms = Some(215);
    probe.event(Event::Failure {
        revision,
        failure: first,
    });
    probe.page = 2;
    probe.event(Event::FailureContext { revision, context });
    assert_eq!(probe.failure, Some(first));
    assert_eq!(probe.failure_context, context);
    assert_eq!(probe.page, 2, "late evidence must not move the reader");
    for incoming in [
        ConnectionFailureContext::default(),
        ConnectionFailureContext {
            local_error: Some(FailureReason::Codec),
            disconnect_reason: Some(8),
        },
    ] {
        probe.event(Event::FailureContext {
            revision,
            context: incoming,
        });
        assert_eq!(probe.failure_context, context, "preserve first observed evidence");
    }
    probe.event(Event::Complete {
        revision,
        limited: false,
    });
    assert!(!probe.inspection.complete);
    probe.event(Event::State {
        revision,
        phase: RadioPhase::Paused,
        error: ProbeError::None,
    });
    probe.input(Input::Click, 101).unwrap();
    assert_eq!(probe.failure_context, ConnectionFailureContext::default());
    probe.event(Event::FailureContext { revision, context });
    assert_eq!(
        probe.failure_context,
        ConnectionFailureContext::default(),
        "stale generation ignored"
    );
}

#[test]
/// Confirms late local ingress failure invalidates completed inventory and cannot be erased by another Complete.
fn terminal_context_can_reveal_a_failure_after_inventory_completed() {
    let mut probe = inspecting();
    let revision = probe.request.revision;
    for stage in [
        InventoryStage::Services,
        InventoryStage::Characteristics,
        InventoryStage::Descriptors,
        InventoryStage::Reads,
    ] {
        probe.event(Event::Item {
            revision,
            item: InspectionItem::StageComplete(stage),
        });
    }
    probe.event(Event::Complete {
        revision,
        limited: false,
    });
    let normal = ConnectionFailureContext {
        local_error: None,
        disconnect_reason: Some(0x16),
    };
    probe.event(Event::FailureContext {
        revision,
        context: normal,
    });
    assert!(probe.inspection.complete);
    assert!(probe.failure.is_none());
    let late = ConnectionFailureContext {
        local_error: Some(FailureReason::Capacity),
        ..normal
    };
    probe.event(Event::FailureContext {
        revision,
        context: late,
    });
    assert_eq!(probe.failure_context, late);
    assert_eq!(
        probe.failure,
        Some(InspectionFailure::new(
            FailureStage::ConnectionEvents,
            FailureReason::Capacity
        ))
    );
    assert!(!probe.inspection.complete);
    probe.event(Event::Complete {
        revision,
        limited: false,
    });
    assert!(
        !probe.inspection.complete,
        "late completion cannot erase ingress failure"
    );
}

#[test]
/// Merges local/disconnect evidence independently and rejects both old/current inspection context after
/// cancel/recovery.
fn delayed_terminal_code_merges_with_local_error_and_cannot_cross_cancel_or_recovery() {
    for input in [Input::Back, Input::Recover] {
        let mut probe = inspecting();
        let revision = probe.request.revision;
        probe.event(Event::Failure {
            revision,
            failure: InspectionFailure::new(FailureStage::Services, FailureReason::Disconnected),
        });
        probe.event(Event::FailureContext {
            revision,
            context: ConnectionFailureContext {
                local_error: Some(FailureReason::Codec),
                disconnect_reason: None,
            },
        });
        probe.event(Event::FailureContext {
            revision,
            context: ConnectionFailureContext {
                local_error: None,
                disconnect_reason: Some(0x16),
            },
        });
        assert_eq!(probe.failure_context.local_error, Some(FailureReason::Codec));
        assert_eq!(probe.failure_context.disconnect_reason, Some(0x16));
        probe.input(input, 101).unwrap();
        for late in [revision, probe.request.revision] {
            probe.event(Event::FailureContext {
                revision: late,
                context: ConnectionFailureContext {
                    local_error: Some(FailureReason::Capacity),
                    disconnect_reason: Some(8),
                },
            });
        }
        assert_eq!(probe.failure_context, ConnectionFailureContext::default());
    }
}

#[test]
/// Keeps the first inspection cause through cleanup/late events until an explicit newer Inspect request.
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
/// Verifies Back/Recover exclude old inspection failure and completion evidence from scanning state.
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
/// Treats protected reads as retained evidence, while explicit failures preserve partial rows without completion.
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
                service_start: 1,
                handle: 3,
                failure: Some(FailureReason::Att(5)),
                elapsed_ms: 1,
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
        for stage in [
            InventoryStage::Services,
            InventoryStage::Characteristics,
            InventoryStage::Descriptors,
            InventoryStage::Reads,
        ] {
            probe.event(Event::Item {
                revision,
                item: InspectionItem::StageComplete(stage),
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
/// Exhausts declaration lengths and invalid order/handle boundaries, retaining only valid16/128-bit UUID layouts.
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
/// Checks all raw error-code bytes retain their value and missing timing/ranges stay absent.
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
/// Confirms global scan faults survive peer clearing/recovery without being relabeled to a newer request.
fn global_scan_diagnostics_survive_pause_and_clear_and_keep_the_original_revision() {
    let mut probe = inspecting();
    let failure = ScanFailure {
        kind: ScanErrorKind::InvalidSize,
        revision: 1,
        index: 2,
        total_reports: 3,
        report_bytes: 80,
        remaining_bytes: 12,
        declared_data_len: Some(31),
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
/// Opens non-radio fault details with no peer and proves details navigation cannot issue a connection.
fn non_scan_fault_details_are_reachable_without_a_peer_and_never_connect() {
    let mut probe = Probe::new();
    probe.fault_latch = FaultLatch::from_parts(Fault::Encoder.bit() | Fault::Usb.bit(), Fault::Encoder as u8).unwrap();
    let request = probe.input(Input::Details, 11).expect("X opens global fault evidence");
    assert_eq!(request.action, Action::Scan(false));
    assert_eq!(probe.screen, Screen::Details);
    probe.event(Event::State {
        revision: request.revision,
        phase: RadioPhase::Paused,
        error: ProbeError::None,
    });
    for input in [Input::Click, Input::Details] {
        assert!(
            probe.input(input, 12).is_none(),
            "global evidence cannot initiate inspection"
        );
    }
    assert_eq!(probe.input(Input::Back, 13).unwrap().action, Action::Scan(true));
    assert_eq!(probe.screen, Screen::List);
    assert!(probe.fault_latch.contains(Fault::Encoder));
    assert!(probe.fault_latch.contains(Fault::Usb));
}

#[test]
/// Keeps parser failure evidence accessible without peers and ignores old reports while global details are paused.
fn scan_failure_details_are_reachable_without_any_valid_peer() {
    let mut probe = Probe::new();
    assert!(probe.input(Input::Details, 10).is_none());
    probe.scan_diagnostics.record(ScanFailure {
        kind: ScanErrorKind::InvalidSize,
        revision: 1,
        index: 0,
        total_reports: 1,
        report_bytes: 9,
        remaining_bytes: 9,
        declared_data_len: Some(0),
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
