#[allow(dead_code)]
#[path = "../../firmware/ble-probe/src/bridge.rs"]
mod bridge;
#[path = "../../firmware/ble-probe/src/status.rs"]
mod status;

use dashboard_core::discovery::{capture_ui as cap, *};
use status::{Fault, Status};

fn levels(
    status: &Status,
    model: &Probe,
) -> [bool; 3] {
    status.publish(model, &cap::CaptureUi::new());
    status.levels(0, false)
}

#[test]
fn scanning_drives_only_the_active_low_blue_channel() {
    let status = Status::new();
    let mut model = Probe::new();
    model.phase = RadioPhase::Scanning;
    assert_eq!(levels(&status, &model), [true, true, false]);
}

fn start(status: &Status) -> Probe {
    let mut model = Probe::new();
    status.event(
        &mut model,
        Event::State {
            revision: 1,
            phase: RadioPhase::Scanning,
            error: ProbeError::None,
        },
    );
    status.event(
        &mut model,
        Event::Report {
            revision: 1,
            report: Advertisement::new(
                Peer {
                    address: [1, 2, 3, 4, 5, 6],
                    random: false,
                },
                ReportKind::Connectable,
                -40,
                &[2, 1, 2],
                10,
            ),
        },
    );
    status.input(&mut model, Input::Click, 20).unwrap();
    model
}
fn event(
    status: &Status,
    model: &mut Probe,
    item: InspectionItem,
) {
    status.event(
        model,
        Event::Item {
            revision: model.request.revision,
            item,
        },
    );
}
fn inventory(
    status: &Status,
    model: &mut Probe,
) {
    event(
        status,
        model,
        InspectionItem::Link(LinkInfo {
            connected_at_ms: 25,
            connect_elapsed_ms: 5,
            interval_us: 30_000,
            latency: 0,
            supervision_timeout_us: 4_000_000,
            att_mtu: Some(23),
        }),
    );
    event(
        status,
        model,
        InspectionItem::Service(Service {
            start: 1,
            end: 4,
            uuid: Uuid::from_wire(&[0, 0x18]).unwrap(),
        }),
    );
    event(
        status,
        model,
        InspectionItem::Characteristic(Characteristic {
            service_start: 1,
            declaration: 2,
            handle: 3,
            properties: 2,
            uuid: Uuid::from_wire(&[0, 0x2A]).unwrap(),
        }),
    );
    for stage in [
        InventoryStage::Services,
        InventoryStage::Characteristics,
        InventoryStage::Descriptors,
        InventoryStage::Reads,
    ] {
        event(status, model, InspectionItem::StageComplete(stage));
    }
}
fn end(
    status: &Status,
    model: &mut Probe,
    cleanup_complete: bool,
) {
    status.event(
        model,
        Event::InspectionEnd {
            revision: model.request.revision,
            at_ms: 50,
            cleanup_complete,
            disconnect_reason: Some(0x16),
        },
    );
}
fn complete(
    status: &Status,
    model: &mut Probe,
    limited: bool,
) {
    status.event(
        model,
        Event::Complete {
            revision: model.request.revision,
            limited,
        },
    );
    status.event(
        model,
        Event::State {
            revision: model.request.revision,
            phase: RadioPhase::Paused,
            error: ProbeError::None,
        },
    );
}

#[test]
fn connection_and_inventory_cannot_turn_green_before_terminal_cleanup() {
    let status = Status::new();
    let mut model = start(&status);
    for phase in [
        RadioPhase::Stopping,
        RadioPhase::Connecting,
        RadioPhase::Discovering,
        RadioPhase::Disconnecting,
    ] {
        model.phase = phase;
        assert_eq!(levels(&status, &model), [true, true, false]);
        assert_eq!(status.levels(500, false), [true, true, true]);
    }
    inventory(&status, &mut model);
    complete(&status, &mut model, false);
    assert_eq!(levels(&status, &model), [false, false, true]);
    end(&status, &mut model, true);
    assert_eq!(levels(&status, &model), [true, false, true]);
    // Local normal termination reason is evidence, not an inspection failure.
    model.inspection.disconnect_reason = Some(0x13);
    assert_eq!(levels(&status, &model), [true, false, true]);
}

#[test]
fn completed_live_probe_returns_to_blue_on_back_and_amber_when_paused() {
    let status = Status::new();
    let mut model = start(&status);
    inventory(&status, &mut model);
    end(&status, &mut model, true);
    complete(&status, &mut model, false);
    assert_eq!(levels(&status, &model), [true, false, true]);
    status.input(&mut model, Input::Back, 100);
    let revision = model.request.revision;
    status.event(
        &mut model,
        Event::State {
            revision,
            phase: RadioPhase::Scanning,
            error: ProbeError::None,
        },
    );
    assert_eq!(levels(&status, &model), [true, true, false]);
    status.input(&mut model, Input::Back, 101);
    let revision = model.request.revision;
    status.event(
        &mut model,
        Event::State {
            revision,
            phase: RadioPhase::Paused,
            error: ProbeError::None,
        },
    );
    assert_eq!(levels(&status, &model), [false, false, true]);
}

#[test]
fn rejected_reads_and_truncation_cannot_hide_behind_complete_enumeration() {
    for (result, failure, truncated) in [
        (ReadResult::Protected, Some(FailureReason::Att(5)), false),
        (ReadResult::Unavailable, Some(FailureReason::Att(2)), false),
        (ReadResult::Error, Some(FailureReason::Timeout), false),
        (ReadResult::Ok, None, true),
    ] {
        let status = Status::new();
        let mut model = start(&status);
        inventory(&status, &mut model);
        let mut value = Payload::new(b"test");
        value.truncated = truncated;
        event(
            &status,
            &mut model,
            InspectionItem::Value(InfoValue {
                service_start: 1,
                handle: 3,
                uuid: 0x2A00,
                value,
                result,
                failure,
                elapsed_ms: 10,
            }),
        );
        end(&status, &mut model, true);
        complete(&status, &mut model, false);
        assert!(
            model.inspection.complete,
            "enumeration and per-read outcome are distinct"
        );
        assert_eq!(levels(&status, &model), [false, true, true]);
    }
}

#[test]
fn successful_att_reply_with_invalid_standard_field_length_is_not_clean() {
    for (service, uuid, required) in [
        (0x1800u16, 0x2A01u16, 2),
        (0x1800, 0x2A04, 8),
        (0x180A, 0x2A23, 8),
        (0x180A, 0x2A50, 7),
        (0x1801, 0x2B2A, 16),
    ] {
        for length in 0..=64 {
            let status = Status::new();
            let mut model = start(&status);
            inventory(&status, &mut model);
            model.inspection.services[0].uuid = Uuid::from_wire(&service.to_le_bytes()).unwrap();
            model.inspection.characteristics[0].uuid = Uuid::from_wire(&uuid.to_le_bytes()).unwrap();
            event(
                &status,
                &mut model,
                InspectionItem::Value(InfoValue {
                    service_start: 1,
                    handle: 3,
                    uuid,
                    value: Payload::new(&[0; 64][..length]),
                    result: ReadResult::Ok,
                    failure: None,
                    elapsed_ms: 10,
                }),
            );
            end(&status, &mut model, true);
            complete(&status, &mut model, false);
            assert_eq!(model.inspection.values.len(), 1, "retain raw evidence");
            assert_eq!(
                levels(&status, &model),
                if length == required {
                    [true, false, true]
                } else {
                    [false, true, true]
                },
                "UUID {uuid:04X}, length {length}"
            );
        }
    }
}

#[test]
fn every_probe_failure_latches_across_retry_and_only_y_recovery_clears() {
    for error in [
        ProbeError::Scan,
        ProbeError::Connect,
        ProbeError::Timeout,
        ProbeError::Discovery,
        ProbeError::Capacity,
        ProbeError::Disconnected,
        ProbeError::Cleanup,
        ProbeError::Transport,
        ProbeError::InvalidPeer,
    ] {
        let status = Status::new();
        let mut model = start(&status);
        let revision = model.request.revision;
        status.event(
            &mut model,
            Event::State {
                revision,
                phase: RadioPhase::Paused,
                error,
            },
        );
        assert_eq!(levels(&status, &model), [false, true, true]);
        status.input(&mut model, Input::Click, 30);
        inventory(&status, &mut model);
        end(&status, &mut model, true);
        complete(&status, &mut model, false);
        assert_eq!(levels(&status, &model), [false, true, true]);
        status.input(&mut model, Input::Recover, 100);
        assert_eq!(
            status.levels(0, false),
            [true, true, false],
            "clear also removes stale green activity"
        );
        assert!(model.catalog.devices().is_empty());
        let revision = model.request.revision;
        status.event(
            &mut model,
            Event::State {
                revision,
                phase: RadioPhase::Scanning,
                error: ProbeError::None,
            },
        );
        assert_eq!(levels(&status, &model), [true, true, false]);
    }
}

#[test]
fn cancel_and_recovery_reject_late_failure_and_completion_generations() {
    for input in [Input::Back, Input::Recover] {
        let status = Status::new();
        let mut model = start(&status);
        let revision = model.request.revision;
        status.input(&mut model, input, 30);
        status.event(
            &mut model,
            Event::Failure {
                revision,
                failure: InspectionFailure::new(FailureStage::Connect, FailureReason::Timeout),
            },
        );
        status.event(
            &mut model,
            Event::InspectionEnd {
                revision,
                at_ms: 40,
                cleanup_complete: false,
                disconnect_reason: None,
            },
        );
        status.event(
            &mut model,
            Event::Complete {
                revision,
                limited: true,
            },
        );
        let current = model.request.revision;
        status.event(
            &mut model,
            Event::State {
                revision: current,
                phase: RadioPhase::Scanning,
                error: ProbeError::None,
            },
        );
        assert_eq!(levels(&status, &model), [true, true, false]);
    }
}

#[test]
fn short_invalid_limited_and_missing_stage_results_are_red() {
    for case in 0..5 {
        let status = Status::new();
        let mut model = start(&status);
        inventory(&status, &mut model);
        match case {
            0 => model.inspection.stages[2] = false,
            1 => {
                event(
                    &status,
                    &mut model,
                    InspectionItem::Characteristic(Characteristic {
                        service_start: 1,
                        declaration: 3,
                        handle: 3,
                        properties: 2,
                        uuid: Uuid::from_wire(&[0, 0x2A]).unwrap(),
                    }),
                )
            }
            2 => model.inspection.omitted = u32::MAX,
            _ => {}
        }
        end(&status, &mut model, case != 3);
        complete(&status, &mut model, case == 4);
        assert_eq!(levels(&status, &model), [false, true, true]);
    }
}

#[test]
fn clean_green_requires_matching_peer_link_timestamps_and_all_stages() {
    for case in 0..7 {
        let status = Status::new();
        let mut model = start(&status);
        inventory(&status, &mut model);
        end(&status, &mut model, true);
        complete(&status, &mut model, false);
        match case {
            0 => model.inspection.ended_at_ms = None,
            1 => model.inspection.ended_at_ms = Some(19),
            2 => model.inspection.cleanup_complete = false,
            3 => model.inspection.link = None,
            4 => model.inspection.link.as_mut().unwrap().att_mtu = None,
            5 => model.selected = 10,
            _ => model.inspection.stages[0] = false,
        }
        assert_eq!(levels(&status, &model), [false, false, true]);
    }
}

#[test]
fn faults_win_every_activity_update_even_after_diagnostic_counters_saturate() {
    for fault in [
        Fault::Probe,
        Fault::Scan,
        Fault::Capacity,
        Fault::Storage,
        Fault::Usb,
        Fault::Encoder,
        Fault::Timing,
        Fault::Watchdog,
        Fault::Advertisement,
    ] {
        let status = Status::new();
        let mut model = Probe::new();
        model.phase = RadioPhase::Scanning;
        model.dropped_reports = u32::MAX;
        model.malformed_reports = u32::MAX;
        // Lifetime counters alone do not relatch after a clear.
        status.input(&mut model, Input::Recover, 10);
        status.record(fault);
        assert_eq!(levels(&status, &model), [false, true, true]);
        status.input(&mut model, Input::Recover, 11);
        assert_eq!(levels(&status, &model), [true, true, false]);
        status.record(fault);
        status.record(fault);
        assert_eq!(status.levels(999, false), [false, true, true]);
    }
}

#[test]
fn concurrent_activity_publication_never_loses_a_fault_after_recovery() {
    use std::sync::{Arc, Barrier};
    let status = Arc::new(Status::new());
    let barrier = Arc::new(Barrier::new(2));
    let other = status.clone();
    let sync = barrier.clone();
    let worker = std::thread::spawn(move || {
        sync.wait();
        for _ in 0..10_000 {
            other.record(Fault::Scan);
        }
    });
    let mut model = Probe::new();
    status.input(&mut model, Input::Recover, 0);
    barrier.wait();
    for _ in 0..10_000 {
        status.publish(&model, &cap::CaptureUi::new());
    }
    worker.join().unwrap();
    assert_eq!(status.levels(0, false), [false, true, true]);
}

#[test]
fn optional_encoder_and_normal_usb_reset_are_neutral_but_actual_failures_latch() {
    let status = Status::new();
    let model = Probe::new();
    status.encoder_failed(false);
    status.usb_failed(false);
    assert_eq!(levels(&status, &model), [true, true, false]);
    status.encoder_failed(true);
    assert_eq!(status.levels(0, false), [false, true, true]);
    let status = Status::new();
    status.usb_failed(true);
    assert_eq!(status.levels(0, false), [false, true, true]);
}

#[test]
fn blinking_uses_elapsed_milliseconds_and_fatal_fault_overrides_recovery() {
    let status = Status::new();
    for (now, blue_high) in [
        (0, false),
        (499, false),
        (500, true),
        (999, true),
        (1000, false),
        (1_000_500, true),
    ] {
        assert_eq!(status.levels(now, false), [true, true, blue_high]);
    }
    let mut model = Probe::new();
    status.input(&mut model, Input::Recover, 0);
    for (now, red_high) in [(0, false), (124, false), (125, true), (249, true), (250, false)] {
        assert_eq!(status.levels(now, true), [red_high, true, true]);
    }
    model.phase = RadioPhase::Fault;
    assert_eq!(levels(&status, &model), [false, true, true]);
    assert_eq!(status.levels(125, false), [true, true, true]);
    // No multiplication, deadline addition or stateful catch-up can overflow.
    assert_eq!(Status::new().levels(u64::MAX, false), [true, true, true]);
}

#[test]
fn capture_activity_and_saved_browsing_never_present_saved_evidence_as_live_success() {
    let status = Status::new();
    let mut model = start(&status);
    inventory(&status, &mut model);
    end(&status, &mut model, true);
    complete(&status, &mut model, false);
    let mut ui = cap::CaptureUi::new();
    let context = cap::Context {
        save: cap::SaveAvailability::Ready,
        revision: model.request.revision,
    };
    let Some(cap::Action::Refresh { id }) = status.capture_input(&mut ui, cap::Input::XHold, 50, context) else {
        panic!()
    };
    status.publish(&model, &ui);
    assert_eq!(status.levels(0, false), [false, true, false]);
    status.capture_complete(
        &mut ui,
        id,
        cap::Completion::Listed([cap::Slot::Saved { generation: 1 }; 4]),
    );
    let Some(cap::Action::Load { id, .. }) = status.capture_input(&mut ui, cap::Input::Click, 60, context) else {
        panic!()
    };
    status.capture_complete(&mut ui, id, cap::Completion::Loaded { pages: 3 });
    status.publish(&model, &ui);
    assert_eq!(status.levels(0, false), [false, false, true]);
}

#[test]
fn capture_failure_and_timeout_survive_back_and_hidden_completion() {
    for timeout in [false, true] {
        let status = Status::new();
        let mut ui = cap::CaptureUi::new();
        let context = cap::Context {
            save: cap::SaveAvailability::Ready,
            revision: 1,
        };
        let Some(cap::Action::Refresh { id }) = status.capture_input(&mut ui, cap::Input::XHold, 0, context) else {
            panic!()
        };
        if timeout {
            status.capture_tick(&mut ui, 30_000);
        } else {
            status.capture_input(&mut ui, cap::Input::Back, 1, context);
            status.capture_complete(&mut ui, id, cap::Completion::Failed(cap::UiError::Storage));
        }
        status.publish(&Probe::new(), &ui);
        assert_eq!(status.levels(0, false), [false, true, true]);
    }
}

#[test]
fn empty_slots_and_admission_guards_are_neutral_corrupt_or_unknown_slots_are_red() {
    for (slot, red) in [
        (cap::Slot::Empty, false),
        (cap::Slot::Saved { generation: 1 }, false),
        (cap::Slot::Corrupt, true),
        (cap::Slot::Unsupported, true),
        (cap::Slot::Unavailable, true),
        (cap::Slot::SavedBlocked { generation: 1 }, true),
    ] {
        let status = Status::new();
        let mut ui = cap::CaptureUi::new();
        let ctx = cap::Context {
            save: cap::SaveAvailability::NoPeer,
            revision: 1,
        };
        let Some(cap::Action::Refresh { id }) = status.capture_input(&mut ui, cap::Input::XHold, 0, ctx) else {
            panic!()
        };
        status.capture_complete(&mut ui, id, cap::Completion::Listed([slot; 4]));
        status.publish(&Probe::new(), &ui);
        assert_eq!(
            status.levels(0, false),
            if red { [false, true, true] } else { [true, true, false] }
        );
    }
    let status = Status::new();
    for error in [cap::UiError::Empty, cap::UiError::NoPeer, cap::UiError::Busy] {
        status.capture_error(error);
    }
    assert_eq!(status.levels(0, false), [true, true, false]);
}

#[test]
fn malformed_advertisement_is_latched_even_if_a_valid_packet_immediately_replaces_it() {
    let status = Status::new();
    let mut model = Probe::new();
    for bytes in [&[3, 1][..], &[2, 1, 2][..]] {
        status.event(
            &mut model,
            Event::Report {
                revision: 1,
                report: Advertisement::new(
                    Peer {
                        address: [1; 6],
                        random: false,
                    },
                    ReportKind::Connectable,
                    -40,
                    bytes,
                    10,
                ),
            },
        );
    }
    assert_eq!(levels(&status, &model), [false, true, true]);
}

#[test]
fn actual_report_channel_overflow_relatches_after_clear_with_a_saturated_counter() {
    use core::sync::atomic::Ordering;
    let report = Event::Report {
        revision: 1,
        report: Advertisement::new(
            Peer {
                address: [7; 6],
                random: true,
            },
            ReportKind::Connectable,
            -50,
            &[2, 1, 2],
            1,
        ),
    };
    let mut model = Probe::new();
    while bridge::REPORTS.try_receive().is_ok() {}
    bridge::STATUS.input(&mut model, Input::Recover, 0);
    bridge::DROPPED_REPORTS.store(u32::MAX, Ordering::Relaxed);
    for _ in 0..32 {
        bridge::emit_report(report);
    }
    assert_eq!(bridge::STATUS.levels(0, false), [true, true, false]);
    bridge::emit_report(report);
    assert_eq!(bridge::STATUS.levels(0, false), [false, true, true]);
    assert_eq!(bridge::DROPPED_REPORTS.load(Ordering::Relaxed), u32::MAX);
    bridge::STATUS.input(&mut model, Input::Recover, 1);
    assert_eq!(bridge::STATUS.levels(0, false), [true, true, false]);
    bridge::emit_report(report);
    assert_eq!(bridge::STATUS.levels(0, false), [false, true, true]);
    while bridge::REPORTS.try_receive().is_ok() {}
}

#[test]
fn idle_stale_peer_rejection_latches_and_request_exhaustion_remains_fatal() {
    let status = Status::new();
    let mut model = start(&status);
    model.phase = RadioPhase::Paused;
    assert!(status.input(&mut model, Input::Click, 30_011).is_none());
    assert_eq!(levels(&status, &model), [false, true, true]);
    model.request.revision = u32::MAX;
    assert!(status.input(&mut model, Input::Recover, 31_000).is_none());
    assert_eq!(levels(&status, &model), [false, true, true]);
    assert_eq!(status.levels(125, false), [true, true, true]);
}

#[test]
fn late_storage_failure_after_recovery_is_new_evidence_but_stale_reply_is_ignored() {
    let status = Status::new();
    let mut model = Probe::new();
    let mut ui = cap::CaptureUi::new();
    let context = cap::Context {
        save: cap::SaveAvailability::Ready,
        revision: 1,
    };
    let Some(cap::Action::Refresh { id }) = status.capture_input(&mut ui, cap::Input::XHold, 0, context) else {
        panic!()
    };
    assert_eq!(
        status.capture_input(&mut ui, cap::Input::Recover, 1, context),
        Some(cap::Action::Recover)
    );
    status.input(&mut model, Input::Recover, 1);
    status.capture_complete(&mut ui, id + 1, cap::Completion::Failed(cap::UiError::Storage));
    status.publish(&model, &ui);
    assert_eq!(
        status.levels(0, false),
        [false, true, false],
        "pending operation keeps its ownership fence"
    );
    status.capture_complete(&mut ui, id, cap::Completion::Failed(cap::UiError::Storage));
    status.publish(&model, &ui);
    assert_eq!(status.levels(0, false), [false, true, true]);
}
