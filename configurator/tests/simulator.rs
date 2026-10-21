//! Synthetic input, acquisition ownership and completed-presentation state regressions.
//! These host fixtures do not establish GPIO, DMA, radio or physical display behavior.

use dashboard_configurator::config::*;
use dashboard_configurator::simulator::ControlSimulator;

#[test]
/// Candidate controls follow candidate view membership/order and proof identity while accepted session and acquisition
/// remain unchanged.
fn unapplied_json_controls_use_candidate_view_membership_and_order() {
    let accepted = validate(default_config()).unwrap();
    let mut sim = ControlSimulator::new(accepted.clone());
    let source = sim.source_key();
    let snapshot = sim.snapshot();
    let accepted_view = sim.session().view_id();
    let mut raw = accepted.config().clone();
    raw.views.clear();
    for id in [9, 4, 7] {
        raw.views
            .push(View {
                orientation: None,
                id,
                name: format!("VIEW {id}").as_str().try_into().unwrap(),
            })
            .unwrap();
    }
    raw.boot_view = 4;
    for w in &mut raw.widgets {
        w.view = 4;
    }
    let mut editor = dashboard_configurator::editor::Editor::default();
    let handle = editor.open_json().unwrap();
    let token = editor
        .set_json_text(handle, serde_json::to_string(&raw).unwrap())
        .unwrap();
    editor.finish_json_parse(token).unwrap();
    let candidate = editor.json_draft().unwrap().preview_proof().unwrap();
    sim.inspect_candidate(candidate.clone());
    assert_eq!(
        sim.display_session().view_id(),
        4,
        "candidate boot view replaces removed accepted view"
    );
    for wanted in [7, 9, 4] {
        sim.rotate(1);
        assert_eq!(sim.display_session().view_id(), wanted, "candidate declared view order");
    }
    assert_eq!(sim.source_key(), source);
    assert_eq!(sim.snapshot(), snapshot);
    assert_eq!(sim.session().view_id(), accepted_view);
    sim.inspect_candidate(candidate.clone());
    assert_eq!(
        sim.display_session().view_id(),
        4,
        "same current proof preserves inspection"
    );
    sim.rotate(1);
    sim.inspect_candidate(candidate);
    assert_eq!(sim.display_session().view_id(), 7);
    raw.views.swap(0, 2);
    raw.boot_view = 9;
    let token = editor
        .set_json_text(handle, serde_json::to_string(&raw).unwrap())
        .unwrap();
    editor.finish_json_parse(token).unwrap();
    let revised = editor.json_draft().unwrap().preview_proof().unwrap();
    sim.inspect_candidate(revised);
    assert_eq!(
        sim.display_session().view_id(),
        9,
        "changed owned candidate with unchanged controls installs current proof"
    );
    sim.rotate(1);
    assert_eq!(sim.display_session().view_id(), 7);
    sim.inspect_invalid(raw.controls);
    assert!(sim.inspection_candidate().is_none());
    assert_eq!(
        sim.display_session().view_id(),
        accepted_view,
        "invalid/unparsed fallback retires former candidate even with identical bindings"
    );
    assert_eq!(sim.snapshot(), snapshot);
}

#[test]
/// Scenario injection uses shared sample admission, leaves unrelated channels missing, and distinguishes UI test-range
/// limits from diagnostic admission.
fn scenarios_use_real_gate_without_refreshing_unselected_channels() {
    use dashboard_configurator::simulator::{InputOutcome, Scenario, SimulationInput};
    use dashboard_core::samples::{SampleError, SensorId};
    let mut sim = ControlSimulator::new(validate(default_config()).unwrap());
    for scenario in Scenario::ALL {
        sim.set_scenario(scenario).unwrap();
        let start = sim.now_ms();
        sim.scenario_step(start + 50).unwrap();
        assert_eq!(sim.projection().scenario, scenario);
        assert_eq!(sim.snapshot().mode, dashboard_core::samples::DataMode::Demo);
    }
    sim.set_scenario(Scenario::Missing).unwrap();
    let start = sim.now_ms();
    assert_eq!(
        sim.inject(SimulationInput::Channel {
            channel: Channel::Water,
            value: 95.,
            at_ms: start + 1
        }),
        InputOutcome::Accepted
    );
    assert_eq!(sim.snapshot().sensor(SensorId::Oil).value(), None);
    let good = sim.snapshot().sensor(SensorId::Water).value();
    assert_eq!(
        sim.inject(SimulationInput::Channel {
            channel: Channel::Water,
            value: f32::NAN,
            at_ms: start + 2
        }),
        InputOutcome::Rejected(SampleError::InvalidValue)
    );
    assert_eq!(sim.snapshot().sensor(SensorId::Water).value(), good);
    assert_eq!(
        sim.inject(SimulationInput::Channel {
            channel: Channel::Water,
            value: 99.,
            at_ms: start
        }),
        InputOutcome::Rejected(SampleError::OutOfOrder)
    );
    assert_eq!(
        sim.inject(SimulationInput::Channel {
            channel: Channel::Boost,
            value: 11.,
            at_ms: start + 3
        }),
        InputOutcome::Accepted
    );
    assert_eq!(sim.custom_input(Channel::Boost, "11"), InputOutcome::TestingRange);
    assert_eq!(sim.diagnostic_input(Channel::Boost, "11"), InputOutcome::Accepted);
    assert_eq!(
        sim.diagnostic_input(Channel::Boost, "Infinity"),
        InputOutcome::Rejected(SampleError::InvalidValue)
    );
    assert_eq!(sim.custom_input(Channel::Boost, "NaN"), InputOutcome::NotFinite);
}

#[test]
/// Cancelling held inputs produces no release action; inspection-only acknowledgment/reset cannot dispatch to accepted
/// acquisition.
fn held_input_cancel_never_synthesizes_tap_hold_or_accepted_candidate_ack() {
    use dashboard_core::input_model::EncoderBatch;
    let mut sim = ControlSimulator::new(validate(default_config()).unwrap());
    let start = sim.now_ms();
    sim.physical_at([true, false, false, false], EncoderBatch::default(), start + 1);
    sim.physical_at([true, false, false, false], EncoderBatch::default(), start + 51);
    sim.cancel_inputs();
    sim.physical_at([false; 4], EncoderBatch::default(), start + 5000);
    assert!(!sim.session().boost_psi());
    let mut map = ControlMap::default();
    map.screens[0].buttons[0].tap = ControlAction::AcknowledgeWarnings;
    map.screens[0].buttons[1].tap = ControlAction::ResetStatistics;
    sim.inspect_invalid(map);
    sim.tap(ButtonId::A);
    sim.tap(ButtonId::B);
    assert_eq!(sim.inspection_outcome(), Some("Inspection only; reset not dispatched"));
    assert!(!sim.reset_pending());
    assert!(sim.pending_warning_command().is_none());
}

#[test]
/// Reset acknowledgment requires the pending token and applies its statistics reset only once.
fn reset_ack_requires_exact_latest_token_and_only_resets_once() {
    let mut sim = ControlSimulator::new(validate(default_config()).unwrap());
    sim.tap(ButtonId::B);
    let token = sim.pending_reset_token().unwrap();
    assert!(!sim.acknowledge_reset_token(token + 1));
    assert!(sim.reset_pending());
    assert!(sim.acknowledge_reset_token(token));
    assert!(!sim.acknowledge_reset_token(token));
    assert_eq!(sim.snapshot().reset_generation, token);
}

#[test]
/// Restart preserves reset-token monotonicity so an old acknowledgment cannot reset the new acquisition owner.
fn source_restart_does_not_recycle_old_reset_acknowledgment_tokens() {
    let mut sim = ControlSimulator::new(validate(default_config()).unwrap());
    sim.tap(ButtonId::B);
    let old = sim.pending_reset_token().unwrap();
    sim.restart(sim.config().clone());
    sim.tap(ButtonId::B);
    let current = sim.pending_reset_token().unwrap();
    assert!(current > old);
    assert!(!sim.acknowledge_reset_token(old));
    assert_eq!(sim.snapshot().reset_generation, 0);
    assert!(sim.acknowledge_reset_token(current));
    assert_eq!(sim.snapshot().reset_generation, current);
}

#[test]
/// Strict browser completion mode requires feedback and clear frame completions before Off, including before any
/// earlier submission and after restart.
fn strict_browser_transport_requires_pixels_even_before_first_submission_and_after_restart() {
    use dashboard_configurator::preview::Preview;
    use dashboard_core::display::pipeline::{BufferId, FrameToken};
    let mut sim = ControlSimulator::new(validate(default_config()).unwrap());
    sim.require_frame_completion();
    for restart in [false, true] {
        if restart {
            sim.restart(sim.config().clone());
        }
        sim.hold(ButtonId::X);
        sim.hold(ButtonId::X);
        sim.advance(2500);
        assert!(sim.session().backlight().compare() > 0);
        let mut p = Preview::new();
        let feedback = FrameToken {
            buffer: BufferId::A,
            sequence: 1,
        };
        p.render_simulation_submitted(&mut sim, feedback).unwrap();
        assert!(sim.complete_frame(feedback));
        sim.advance(2500);
        assert!(sim.session().backlight().compare() > 0);
        let clear = FrameToken {
            buffer: BufferId::A,
            sequence: 2,
        };
        p.render_simulation_submitted(&mut sim, clear).unwrap();
        assert!(sim.complete_frame(clear));
        assert_eq!(sim.session().backlight().compare(), 0);
        p.render_simulation(&sim).unwrap();
        assert!(p.rgb565().iter().all(|byte| *byte == 0));
    }
}

#[test]
/// Synthetic warning fixtures cross configured thresholds only after real dwell and leave context-dependent rules
/// unavailable when unsupported.
fn warning_scenario_crosses_configured_threshold_with_real_dwell_and_context_limits() {
    use dashboard_configurator::simulator::{Scenario, WarningFixture};
    use dashboard_core::alarms::{RuleIndex, Status};
    let mut raw = default_config();
    raw.widgets.truncate(1);
    raw.widgets[0].warning.enabled = true;
    raw.widgets[0].warning.trigger = 4.;
    raw.widgets[0].warning.clear = 3.;
    raw.widgets[0].warning.on_ms = 500;
    let mut sim = ControlSimulator::new(validate(raw).unwrap());
    sim.set_scenario(Scenario::Warnings).unwrap();
    let start = sim.now_ms();
    for elapsed in [0, 250] {
        sim.scenario_step(start + elapsed).unwrap();
    }
    assert_eq!(
        sim.snapshot().rule_status_at(RuleIndex::new(0).unwrap(), sim.now_ms()),
        Status::Pending
    );
    sim.scenario_step(start + 500).unwrap();
    assert_eq!(
        sim.snapshot().rule_status_at(RuleIndex::new(0).unwrap(), sim.now_ms()),
        Status::Active
    );
    assert_eq!(
        sim.snapshot()
            .sensor(dashboard_core::samples::SensorId::Boost)
            .maximum(),
        Some(10.)
    );
    let mut raw = default_config();
    raw.widgets.clear();
    let mut widget = default_widget(
        Channel::Battery,
        1,
        1,
        Placement {
            column: 0,
            row: 0,
            span: Span::OneByOne,
        },
    );
    widget.warning.enabled = true;
    raw.widgets.push(widget).unwrap();
    let mut contextual = ControlSimulator::new(validate(raw).unwrap());
    contextual.set_scenario(Scenario::Warnings).unwrap();
    assert!(matches!(contextual.warning_fixture(0), WarningFixture::Unavailable(_)));
    for delta in [0, 250, 500, 750] {
        contextual.scenario_step(start + delta).unwrap();
    }
    assert_eq!(
        contextual
            .snapshot()
            .rule_status_at(RuleIndex::new(0).unwrap(), contextual.now_ms()),
        Status::Unavailable
    );
}

#[test]
/// Missing/stale/fault scenarios preserve factual gaps; rejected payloads retain previous extrema while recording
/// invalid quality.
fn missing_stale_fault_and_rejected_samples_keep_factual_gaps_and_raw_extrema() {
    use dashboard_configurator::simulator::{InputOutcome, Scenario, SimulationInput};
    use dashboard_core::samples::{Quality, SampleError, SensorId};
    let mut sim = ControlSimulator::new(validate(default_config()).unwrap());
    sim.set_scenario(Scenario::Missing).unwrap();
    let start = sim.now_ms();
    sim.scenario_step(start + 10000).unwrap();
    assert_eq!(sim.snapshot().sensor(SensorId::Water).history().count(), 0);
    sim.set_scenario(Scenario::Stale).unwrap();
    sim.scenario_step(start + 1).unwrap();
    let good = sim.snapshot().sensor(SensorId::Water).value();
    sim.scenario_step(start + 10000).unwrap();
    assert_eq!(sim.snapshot().value_at(SensorId::Water, sim.now_ms()), None);
    assert_eq!(sim.snapshot().sensor(SensorId::Water).value(), good);
    assert!(sim.snapshot().sensor(SensorId::Water).history().count() < 60);
    sim.set_scenario(Scenario::SourceFault).unwrap();
    sim.scenario_step(start + 1).unwrap();
    sim.scenario_step(start + 51).unwrap();
    assert_eq!(
        sim.snapshot().quality(SensorId::Water, sim.now_ms()),
        Quality::TransportError
    );
    for (offset, value) in [(52, f32::NEG_INFINITY), (53, 1_000_001.)] {
        let before = sim.snapshot().sensor(SensorId::Water).maximum();
        assert_eq!(
            sim.inject(SimulationInput::Channel {
                channel: Channel::Water,
                value,
                at_ms: start + offset
            }),
            InputOutcome::Rejected(SampleError::InvalidValue)
        );
        assert_eq!(sim.snapshot().sensor(SensorId::Water).maximum(), before);
        assert_eq!(
            sim.snapshot().quality(SensorId::Water, sim.now_ms()),
            Quality::InvalidPayload
        );
    }
}

#[test]
/// Sampled physical inputs honor the exact two-second Y recovery boundary and cancellation prevents simultaneous held
/// inputs from firing later.
fn physical_simultaneous_cancel_and_y_recovery_use_actual_elapsed_boundary() {
    use dashboard_core::input_model::EncoderBatch;
    let mut sim = ControlSimulator::new(validate(default_config()).unwrap());
    sim.tap(ButtonId::Y);
    let start = sim.now_ms();
    let y = [false, false, false, true];
    sim.physical_at(y, EncoderBatch::default(), start + 1);
    sim.physical_at(y, EncoderBatch::default(), start + 51);
    sim.physical_at(y, EncoderBatch::default(), start + 2000);
    assert_eq!(sim.session().page(), ControlPage::Debug);
    sim.physical_at(y, EncoderBatch::default(), start + 2001);
    assert_eq!(sim.session().page(), ControlPage::Dashboard);
    sim.cancel_inputs();
    let start = sim.now_ms();
    let simultaneous = [true, true, false, false];
    sim.physical_at(simultaneous, EncoderBatch::default(), start + 1);
    sim.physical_at(simultaneous, EncoderBatch::default(), start + 51);
    sim.cancel_inputs();
    sim.physical_at([false; 4], EncoderBatch::default(), start + 5000);
    assert!(!sim.reset_pending());
    assert!(!sim.session().boost_psi());
}

#[test]
/// Inspection navigation leaves accepted page, acquisition snapshot and reset ownership unchanged when bindings resume.
fn candidate_navigation_does_not_mutate_accepted_session_or_statistics() {
    let mut sim = ControlSimulator::new(validate(default_config()).unwrap());
    let start = sim.now_ms();
    sim.advance_to(start, Some([1., 90., 95., 80., 30., 700., 13.8, 1.]));
    let before = sim.snapshot();
    let mut map = ControlMap::default();
    map.screens[0].buttons[0].tap = ControlAction::Logs;
    sim.inspect_invalid(map);
    sim.tap(ButtonId::A);
    assert_eq!(sim.session().page(), ControlPage::Dashboard);
    assert_eq!(sim.snapshot(), before);
    assert!(!sim.reset_pending());
    sim.resume_accepted_bindings();
    assert_eq!(sim.session().page(), ControlPage::Dashboard);
    assert_eq!(sim.snapshot(), before);
}

#[test]
/// Debug and Logs submit distinct shared rasters, accept completion once and do not publish Dashboard warning rules.
fn diagnostic_pages_submit_their_shared_pixels_and_complete_once() {
    use dashboard_configurator::preview::Preview;
    use dashboard_core::display::pipeline::{BufferId, FrameToken};
    let mut sim = ControlSimulator::new(validate(default_config()).unwrap());
    let mut preview = Preview::new();
    sim.tap(ButtonId::Y);
    assert_eq!(sim.session().page(), ControlPage::Debug);
    let debug = FrameToken {
        buffer: BufferId::A,
        sequence: 1,
    };
    preview.render_simulation_submitted(&mut sim, debug).unwrap();
    let debug_pixels = preview.rgb565().to_vec();
    assert!(sim.complete_frame(debug));
    assert!(!sim.complete_frame(debug));
    sim.tap(ButtonId::Y);
    assert_eq!(sim.session().page(), ControlPage::Logs);
    let logs = FrameToken {
        buffer: BufferId::A,
        sequence: 2,
    };
    preview.render_simulation_submitted(&mut sim, logs).unwrap();
    assert_ne!(preview.rgb565(), debug_pixels);
    assert!(sim.complete_frame(logs));
    assert_eq!(sim.completed_warning().rules, 0);
}

#[test]
/// Gesture navigation retains unit choice, protected recovery restores Full, reset feedback waits for acknowledgment
/// and restart clears session choices.
fn gestures_navigate_preserve_choices_and_recover_from_off() {
    let config = validate(default_config()).unwrap();
    let mut sim = ControlSimulator::new(config.clone());
    sim.tap(ButtonId::A);
    assert!(sim.session().boost_psi());
    sim.tap(ButtonId::Y);
    assert_eq!(sim.session().page(), ControlPage::Debug);
    sim.hold(ButtonId::X);
    sim.advance(2500);
    assert_eq!(sim.session().backlight().compare(), 0);
    sim.recover();
    assert_eq!(sim.session().page(), ControlPage::Dashboard);
    assert!(sim.session().boost_psi());
    assert_eq!(
        sim.session().backlight().brightness(),
        dashboard_core::backlight::Brightness::Full
    );
    assert!(sim.session().backlight().compare() > 0);
    sim.tap(ButtonId::B);
    assert!(sim.reset_pending());
    assert!(!matches!(
        sim.session().popup(),
        Some(dashboard_core::popup::Popup::Reset(_))
    ));
    sim.acknowledge_reset();
    assert!(matches!(
        sim.session().popup(),
        Some(dashboard_core::popup::Popup::Reset(_))
    ));
    sim.restart(config);
    assert!(!sim.session().boost_psi());
}

#[test]
/// Invalid-map inspection uses accepted layout for navigation and log scrolling while preserving protected recovery.
fn invalid_maps_use_accepted_layout_and_cannot_disable_recovery() {
    let mut sim = ControlSimulator::new(validate(default_config()).unwrap());
    let mut map = ControlMap::default();
    map.screens[0].buttons[0].tap = ControlAction::Logs;
    map.screens[2].buttons[3].hold = ControlAction::ToggleBoostUnits;
    sim.inspect_invalid(map);
    assert!(sim.invalid());
    sim.tap(ButtonId::A);
    assert_eq!(sim.display_session().page(), ControlPage::Logs);
    sim.rotate(-1);
    assert!(sim.display_session().log_anchor().is_some());
    sim.recover();
    assert_eq!(sim.session().page(), ControlPage::Dashboard);
}

#[test]
/// Incompatible inspection actions increment the ignored count without replacing accepted layout or disabling recovery.
fn incompatible_candidate_actions_are_counted_and_layout_is_accepted() {
    let accepted = validate(default_config()).unwrap();
    let mut sim = ControlSimulator::new(accepted.clone());
    let mut map = accepted.config().controls;
    map.screens[0].buttons[0].tap = ControlAction::ScrollOlder;
    sim.inspect_invalid(map);
    sim.tap(ButtonId::A);
    assert_eq!(sim.ignored(), 1);
    assert_eq!(sim.session().page(), ControlPage::Dashboard);
    assert_eq!(sim.config(), &accepted);
    sim.recover();
    assert_eq!(
        sim.session().backlight().brightness(),
        dashboard_core::backlight::Brightness::Full
    );
}

#[test]
/// Every control action is exercised through a permitted page gesture, including unavailable unseen-warning
/// acknowledgment and reset acknowledgment.
fn every_catalogue_action_runs_through_gestures_and_shared_session() {
    use dashboard_configurator::editor::{Edit, Editor};
    use dashboard_core::backlight::Brightness;
    use dashboard_core::render::FpsMode;
    let mut editor = Editor::default();
    editor.apply(Edit::AddView { name: "Second".into() }).unwrap();
    for action in ControlAction::ALL {
        let page = if action.allowed_on(ControlPage::Dashboard) {
            ControlPage::Dashboard
        } else if action.allowed_on(ControlPage::Debug) {
            ControlPage::Debug
        } else {
            ControlPage::Logs
        };
        let mut raw = editor.preview().config().clone();
        raw.controls.screens[page.index()].buttons[0].tap = action;
        let mut sim = ControlSimulator::new(validate(raw).unwrap());
        if page != ControlPage::Dashboard {
            sim.tap(ButtonId::Y);
        }
        if page == ControlPage::Logs {
            sim.tap(ButtonId::Y);
        }
        if action == ControlAction::BrightnessUp {
            sim.rotate(-2);
        }
        if matches!(action, ControlAction::ScrollNewer | ControlAction::LogsLive) {
            sim.rotate(-2);
        }
        let brightness = sim.session().backlight().compare();
        let old_anchor = sim.session().log_anchor();
        let old_view = sim.session().view_id();
        sim.tap(ButtonId::A);
        if let Some(destination) = action.destination(page) {
            assert_eq!(sim.session().page(), destination, "{action:?}");
        } else {
            match action {
                ControlAction::None => assert_eq!(sim.session().page(), page),
                // Summary mode cannot acknowledge an unseen warning.
                ControlAction::AcknowledgeWarnings => {
                    assert!(!sim.reset_pending());
                    assert!(matches!(
                        sim.session().popup(),
                        Some(dashboard_core::popup::Popup::Warnings(
                            _,
                            dashboard_core::warning_state::WarningFeedback::Unavailable
                        ))
                    ));
                    assert_eq!(sim.session().page(), page);
                }
                ControlAction::NextView | ControlAction::PreviousView => assert_ne!(sim.session().view_id(), old_view),
                ControlAction::ToggleBoostUnits => assert!(sim.session().boost_psi()),
                ControlAction::CycleFps => assert_eq!(sim.session().fps(), FpsMode::Instant),
                ControlAction::ResetStatistics => {
                    assert!(sim.reset_pending());
                    sim.acknowledge_reset();
                    assert!(matches!(
                        sim.session().popup(),
                        Some(dashboard_core::popup::Popup::Reset(_))
                    ));
                }
                ControlAction::CycleBrightness => assert_eq!(sim.session().backlight().brightness(), Brightness::Dim),
                ControlAction::ToggleBacklight => {
                    sim.advance(2500);
                    assert_eq!(sim.session().backlight().compare(), 0);
                }
                ControlAction::BrightnessUp => assert!(sim.session().backlight().compare() > brightness),
                ControlAction::BrightnessDown => assert!(sim.session().backlight().compare() < brightness),
                ControlAction::ScrollOlder | ControlAction::ScrollNewer => {
                    assert_ne!(sim.session().log_anchor(), old_anchor)
                }
                ControlAction::LogsLive => assert_eq!(sim.session().log_anchor(), None),
                _ => unreachable!("navigation handled above"),
            }
        }
    }
}

#[test]
/// Summary-mode Off waits for its feedback interval before clearing, and protected recovery restores visible output
/// without a stale popup.
fn off_feedback_clear_and_recovery_follow_presentation_rules() {
    let mut sim = ControlSimulator::new(validate(default_config()).unwrap());
    sim.hold(ButtonId::X); // Full -> Dim
    sim.hold(ButtonId::X); // Dim -> Off, still presenting feedback
    assert_eq!(
        sim.session().backlight().brightness(),
        dashboard_core::backlight::Brightness::Off
    );
    assert!(sim.session().backlight().compare() > 0);
    sim.advance(1700);
    assert!(sim.session().backlight().compare() > 0);
    sim.advance(400);
    assert_eq!(sim.session().backlight().compare(), 0);
    sim.recover();
    assert!(sim.session().backlight().compare() > 0);
    assert_eq!(sim.session().popup(), None);
}

#[test]
/// Persistent samples retain extrema/history, long no-sample advancement expires data, and restart allocates a fresh
/// empty acquisition origin.
fn persistent_samples_keep_history_and_restart_resets_origin() {
    use dashboard_core::samples::SensorId;
    let cfg = validate(default_config()).unwrap();
    let mut sim = ControlSimulator::new(cfg.clone());
    let start = sim.now_ms();
    for (offset, water) in [(0, 95.0), (2000, 110.0), (4000, 90.0)] {
        sim.advance_to(start + offset, Some([1., 95., water, 80., 30., 700., 13.8, 1.]));
    }
    let snap = sim.snapshot();
    assert_eq!(snap.sequence, 3);
    assert_eq!(snap.sensor(SensorId::Water).minimum(), Some(90.));
    assert_eq!(snap.sensor(SensorId::Water).maximum(), Some(110.));
    assert_eq!(snap.sensor(SensorId::Water).history().count(), 3);
    let identity = snap.sensor(SensorId::Water).graph_cache_identity();
    sim.advance_to(start + 1_000_000_000, None);
    assert_eq!(sim.snapshot().sequence, 3);
    assert_ne!(sim.snapshot().sensor(SensorId::Water).graph_cache_identity(), identity);
    assert_eq!(sim.snapshot().sensor(SensorId::Water).history().bounds(), None);
    assert_eq!(sim.snapshot().value_at(SensorId::Water, sim.now_ms()), None);
    let mut second = ControlSimulator::new(cfg.clone());
    for (offset, water) in [(0, 95.0), (2000, 110.0), (4000, 90.0)] {
        second.advance_to(start + offset, Some([1., 95., water, 80., 30., 700., 13.8, 1.]));
    }
    assert_ne!(
        second.snapshot().sensor(SensorId::Water).graph_cache_identity(),
        identity
    );
    sim.restart(cfg);
    assert_eq!(sim.snapshot().sequence, 0);
    assert_eq!(sim.snapshot().sensor(SensorId::Water).maximum(), None);
    assert_ne!(sim.snapshot().sensor(SensorId::Water).graph_cache_identity(), identity);
}

#[test]
/// Simulator-owned acquisition matches borrowed shared acquisition across valid, nonfinite and out-of-order
/// observations.
fn owned_and_borrowed_ingestion_match_including_rejected_values() {
    use dashboard_core::acquisition::AcquisitionModel;
    let cfg = validate(default_config()).unwrap();
    let mut borrowed = AcquisitionModel::demo_configured(&cfg);
    let mut sim = ControlSimulator::new(cfg.clone());
    let start = sim.now_ms();
    for (offset, water) in [(0, 95.0), (250, f32::NAN), (200, 99.0), (500, 110.0)] {
        let values = [1., 95., water, 80., 30., 700., 13.8, 1.];
        borrowed.ingest_batch(values, start + offset);
        borrowed.tick(start + offset);
        sim.advance_to(start + offset, Some(values));
        assert_eq!(sim.snapshot(), borrowed.snapshot());
    }
}

#[test]
/// Owned acquisition retains its immutable threshold/dwell contract; a separately configured replacement starts fresh
/// and evaluates independently.
fn boxed_owner_binds_threshold_and_dwell_to_its_immutable_config() {
    use dashboard_core::acquisition::{AcquisitionModel, OwnedAcquisitionModel};
    use dashboard_core::alarms::{RuleIndex, Status};
    let mut raw = default_config();
    raw.widgets[0].warning.enabled = true;
    raw.widgets[0].warning.trigger = 1.5;
    raw.widgets[0].warning.clear = 1.;
    raw.widgets[0].warning.on_ms = 500;
    let original = validate(raw.clone()).unwrap();
    let mut borrowed = AcquisitionModel::demo_configured(&original);
    let mut owned = OwnedAcquisitionModel::demo_configured(Box::new(original.clone()));
    let rule = RuleIndex::new(0).unwrap();
    let high = [2., 90., 95., 80., 30., 700., 13.8, 1.];
    for at in [0, 250] {
        borrowed.ingest_batch(high, at);
        owned.ingest_batch(high, at);
        assert_eq!(owned.snapshot(), borrowed.snapshot());
    }
    raw.widgets[0].warning.trigger = 3.;
    raw.widgets[0].warning.clear = 2.5;
    let mut replacement = OwnedAcquisitionModel::demo_configured(Box::new(validate(raw).unwrap()));
    assert_eq!(replacement.snapshot().sequence, 0);
    assert_eq!(owned.snapshot().rule_status_at(rule, 250), Status::Pending);
    for at in [500, 750, 1000] {
        borrowed.ingest_batch(high, at);
        owned.ingest_batch(high, at);
        replacement.ingest_batch(high, at);
        assert_eq!(owned.snapshot(), borrowed.snapshot());
        assert_eq!(owned.snapshot().rule_status_at(rule, at), Status::Active);
        assert_eq!(replacement.snapshot().rule_status_at(rule, at), Status::Normal);
    }
}

#[test]
/// Reset request preserves statistics until owner acknowledgment, duplicate acknowledgment is inert and elapsed time
/// expires feedback.
fn reset_acknowledgment_resets_owned_statistics_once_and_time_expires_feedback() {
    use dashboard_core::samples::SensorId;
    let mut sim = ControlSimulator::new(validate(default_config()).unwrap());
    let start = sim.now_ms();
    sim.advance_to(start, Some([3., 90., 95., 80., 30., 700., 13.8, 1.]));
    sim.advance_to(start + 250, Some([1., 90., 95., 80., 30., 700., 13.8, 1.]));
    sim.tap(ButtonId::B);
    assert!(sim.reset_pending());
    assert_eq!(sim.snapshot().sensor(SensorId::Boost).maximum(), Some(3.));
    sim.acknowledge_reset();
    assert_eq!(sim.snapshot().reset_generation, 1);
    assert_eq!(sim.snapshot().sensor(SensorId::Boost).maximum(), Some(1.));
    sim.acknowledge_reset();
    assert_eq!(sim.snapshot().reset_generation, 1);
    sim.advance_to(sim.now_ms() + 10, None);
    sim.advance_to(sim.now_ms() + 3000, None);
    assert_eq!(sim.session().popup(), None);
}

#[test]
/// Warning acknowledgment requires a matching completed presentation and then producer processing before accepted
/// feedback.
fn warnings_require_render_acceptance_and_matching_completion_before_ack() {
    use dashboard_configurator::preview::Preview;
    use dashboard_core::display::pipeline::{BufferId, FrameToken};
    use dashboard_core::warning_state::WarningFeedback;
    let mut raw = default_config();
    raw.widgets.truncate(1);
    raw.widgets[0].warning.enabled = true;
    raw.widgets[0].warning.trigger = 2.;
    raw.widgets[0].warning.clear = 1.;
    raw.widgets[0].warning.on_ms = 1;
    raw.widgets[0].warning.advanced.latch = true;
    raw.controls.screens[0].buttons[0].tap = ControlAction::AcknowledgeWarnings;
    let mut sim = ControlSimulator::new(validate(raw).unwrap());
    let now = sim.now_ms();
    let values = [3., 90., 95., 80., 30., 700., 13.8, 1.];
    sim.advance_to(now, Some(values));
    sim.advance_to(now + 1, Some(values));
    sim.tap(ButtonId::A);
    assert!(
        sim.pending_warning_command().is_none(),
        "summary cannot acknowledge unseen warning"
    );
    sim.advance(3001);
    sim.advance_to(sim.now_ms(), Some(values));
    sim.advance_to(sim.now_ms() + 1, Some(values));
    let mut preview = Preview::new();
    let token = FrameToken {
        buffer: BufferId::A,
        sequence: 1,
    };
    let wrong = FrameToken {
        buffer: BufferId::B,
        sequence: 1,
    };
    preview.render_simulation_submitted(&mut sim, token).unwrap();
    assert_eq!(sim.completed_warning().rules, 0);
    assert!(!sim.complete_frame(wrong));
    assert!(sim.complete_frame(token));
    assert_eq!(sim.completed_warning().rules, 1);
    sim.tap(ButtonId::A);
    assert!(sim.pending_warning_command().is_some());
    assert!(matches!(
        sim.session().popup(),
        Some(dashboard_core::popup::Popup::Warnings(_, WarningFeedback::Pending))
    ));
    sim.producer_step(None);
    assert!(sim.pending_warning_command().is_none());
    assert!(matches!(
        sim.session().popup(),
        Some(dashboard_core::popup::Popup::Warnings(_, WarningFeedback::Accepted))
    ));
}

#[test]
/// Rendered transport cannot shut off from elapsed time alone: exact feedback completion and a subsequent clear
/// completion are required.
fn rendered_transport_off_waits_for_feedback_and_clear_completions() {
    use dashboard_configurator::preview::Preview;
    use dashboard_core::display::pipeline::{BufferId, FrameToken};
    let mut sim = ControlSimulator::new(validate(default_config()).unwrap());
    let mut preview = Preview::new();
    let token = |sequence| {
        FrameToken {
            buffer: BufferId::A,
            sequence,
        }
    };
    preview.render_simulation_submitted(&mut sim, token(1)).unwrap();
    sim.complete_frame(token(1));
    sim.click();
    sim.click();
    sim.advance(5000);
    assert!(sim.session().backlight().compare() > 0);
    preview.render_simulation_submitted(&mut sim, token(2)).unwrap();
    sim.advance(5000);
    assert!(sim.session().backlight().compare() > 0);
    assert!(!sim.complete_frame(token(1)));
    assert!(sim.complete_frame(token(2)));
    sim.advance(2000);
    assert!(sim.session().backlight().compare() > 0);
    preview.render_simulation_submitted(&mut sim, token(3)).unwrap();
    assert!(sim.session().backlight().compare() > 0);
    assert!(sim.complete_frame(token(3)));
    assert_eq!(sim.session().backlight().compare(), 0);
}

#[test]
/// Dashboard, Debug and Logs submit distinct rasters and reject completion for the wrong buffer identity.
fn diagnostic_pages_submit_real_shared_rasters_instead_of_dashboard_pixels() {
    use dashboard_configurator::preview::Preview;
    use dashboard_core::display::pipeline::{BufferId, FrameToken};
    let mut sim = ControlSimulator::new(validate(default_config()).unwrap());
    let mut preview = Preview::new();
    let mut previous = Vec::new();
    for (sequence, page) in [
        (1, ControlPage::Dashboard),
        (2, ControlPage::Debug),
        (3, ControlPage::Logs),
    ] {
        assert_eq!(sim.session().page(), page);
        let token = FrameToken {
            buffer: BufferId::A,
            sequence,
        };
        preview.render_simulation_submitted(&mut sim, token).unwrap();
        assert!(preview.rgb565() != previous);
        assert!(!sim.complete_frame(FrameToken {
            buffer: BufferId::B,
            sequence
        }));
        assert!(sim.complete_frame(token));
        previous = preview.rgb565().to_vec();
        sim.tap(ButtonId::Y);
    }
}
#[test]
/// Pending frames prevent buffer overwrite; rejected/replayed preparations cannot advance completed warning identity or
/// paging.
fn deadline_interruption_and_dropped_preparation_never_change_completed_identity() {
    use dashboard_configurator::preview::{Preview, PreviewError};
    use dashboard_core::display::pipeline::{BufferId, FrameToken};
    let mut raw = default_config();
    raw.widgets.clear();
    raw.warnings.page_ms = 1000;
    raw.warnings.mode = WarningMode::CycleOne;
    for i in 0..4 {
        let mut w = default_widget(
            Channel::Oil,
            i + 1,
            1,
            Placement {
                column: i as u8,
                row: 0,
                span: Span::OneByOne,
            },
        );
        w.warning.on_ms = 1;
        w.warning.off_ms = 1;
        if i == 0 {
            w.warning.trigger = 140.;
            w.warning.clear = 138.;
            w.warning.advanced.priority = Priority::Critical;
        }
        raw.widgets.push(w).unwrap();
    }
    let mut sim = ControlSimulator::new(validate(raw).unwrap());
    let mut preview = Preview::new();
    let token = |sequence| {
        FrameToken {
            buffer: BufferId::A,
            sequence,
        }
    };
    let start = sim.now_ms();
    for offset in [0, 1] {
        sim.advance_to(start + offset, Some([1., 150., 95., 80., 30., 700., 13.8, 1.]));
    }
    preview.render_simulation_submitted(&mut sim, token(1)).unwrap();
    sim.complete_frame(token(1));
    let old = sim.completed_warning();
    for (offset, oil) in [(997, 135.), (998, 135.), (1000, 150.), (1001, 150.)] {
        sim.advance_to(start + offset, Some([1., oil, 95., 80., 30., 700., 13.8, 1.]));
    }
    preview.render_simulation_submitted(&mut sim, token(2)).unwrap();
    assert_eq!(preview.warning_page().unwrap().rules, 1);
    let submitted_rgb = preview.rgb565().to_vec();
    let submitted_rgba = preview.rgba().to_vec();
    assert_eq!(sim.completed_warning(), old);
    // A pending transfer cannot overwrite either browser buffer.
    sim.advance_to(start + 1002, None);
    assert_eq!(
        preview.render_simulation_submitted(&mut sim, token(3)).unwrap_err(),
        PreviewError::PendingFrame
    );
    assert_eq!(preview.warning_page().unwrap().rules, 1);
    assert_eq!(preview.rgb565(), submitted_rgb);
    assert_eq!(preview.rgba(), submitted_rgba);
    assert_eq!(sim.completed_warning(), old);
    assert!(!sim.complete_frame(token(3)));
    assert!(sim.complete_frame(token(2)));
    assert_eq!(sim.completed_warning().rules, 1);
    assert!(sim.completed_warning().event_serial > old.event_serial);
    let completed = sim.completed_warning();
    assert_eq!(
        preview.render_simulation_submitted(&mut sim, token(2)).unwrap_err(),
        PreviewError::PendingFrame
    );
    assert_eq!(preview.rgb565(), submitted_rgb);
    assert_eq!(preview.rgba(), submitted_rgba);
    assert_eq!(preview.warning_page().unwrap().rules, 1);
    assert_eq!(sim.completed_warning(), completed);
    preview.render_simulation_submitted(&mut sim, token(3)).unwrap();
    sim.complete_frame(token(3));
    assert_eq!(sim.completed_warning().rules, 2);
    for (offset, sequence, rules) in [(2001, 4, 4), (3001, 5, 8)] {
        sim.advance_to(start + offset, Some([1., 150., 95., 80., 30., 700., 13.8, 1.]));
        preview.render_simulation_submitted(&mut sim, token(sequence)).unwrap();
        sim.complete_frame(token(sequence));
        assert_eq!(sim.completed_warning().rules, rules);
    }
}

#[test]
/// Transport faults and stale ticks preserve an established warning latch while completed presentation remains absent.
fn injected_fault_and_stale_tick_use_owner_qualification_and_preserve_latch() {
    use dashboard_core::alarms::RuleIndex;
    use dashboard_core::samples::{Quality, SensorId};
    let mut raw = default_config();
    raw.widgets.truncate(1);
    raw.widgets[0].warning.enabled = true;
    raw.widgets[0].warning.on_ms = 1;
    raw.widgets[0].warning.advanced.latch = true;
    raw.controls.screens[0].buttons[0].tap = ControlAction::AcknowledgeWarnings;
    let mut sim = ControlSimulator::new(validate(raw).unwrap());
    let start = sim.now_ms();
    for at in [start, start + 1] {
        sim.advance_to(at, Some([3., 90., 95., 80., 30., 700., 13.8, 1.]));
    }
    sim.inject_fault(SensorId::Boost, Quality::TransportError, start + 2);
    let e = sim
        .snapshot()
        .warning_eligibility(RuleIndex::new(0).unwrap(), start + 2);
    assert!(e.latched);
    assert_eq!(e.label(), "LATCHED / DATA STALE");
    sim.advance_to(start + 10000, None);
    assert!(
        sim.snapshot()
            .warning_eligibility(RuleIndex::new(0).unwrap(), sim.now_ms())
            .latched
    );
    assert_eq!(sim.completed_warning().rules, 0);
}

#[test]
/// LED warning state follows exact completed frames and returns to configured normal only after completed backlight
/// shutdown.
fn led_follows_completed_warning_and_actual_backlight_shutdown() {
    use dashboard_configurator::preview::Preview;
    use dashboard_core::display::pipeline::{BufferId, FrameToken};
    use dashboard_core::status_led::LedDuty;
    let mut raw = default_config();
    raw.widgets.truncate(1);
    raw.widgets[0].warning.enabled = true;
    raw.widgets[0].warning.on_ms = 1;
    raw.widgets[0].warning.trigger = 2.;
    raw.widgets[0].warning.clear = 1.;
    raw.widgets[0].warning.advanced.latch = true;
    raw.controls.screens[0].buttons[0].tap = ControlAction::AcknowledgeWarnings;
    raw.led.warning.kind = dashboard_config::LedPatternKind::Solid;
    raw.led.during_feedback = true;
    let mut sim = ControlSimulator::new(validate(raw).unwrap());
    let green = LedDuty {
        red: 0,
        green: 10000,
        blue: 0,
    };
    let red = LedDuty {
        red: 10000,
        green: 0,
        blue: 0,
    };
    assert_eq!(sim.led_duty(), green);
    let now = sim.now_ms();
    let v = [3., 90., 95., 80., 30., 700., 13.8, 1.];
    sim.advance_to(now, Some(v));
    sim.advance_to(now + 1, Some(v));
    let mut p = Preview::new();
    let token = |sequence| {
        FrameToken {
            buffer: BufferId::A,
            sequence,
        }
    };
    p.render_simulation_submitted(&mut sim, token(1)).unwrap();
    assert_eq!(sim.led_duty(), green);
    assert!(!sim.complete_frame(token(2)));
    assert!(sim.complete_frame(token(1)));
    assert_eq!(sim.led_duty(), red);
    sim.click();
    sim.click();
    p.render_simulation_submitted(&mut sim, token(2)).unwrap();
    assert!(sim.complete_frame(token(2)));
    assert_eq!(sim.led_duty(), red);
    sim.advance(2000);
    p.render_simulation_submitted(&mut sim, token(3)).unwrap();
    assert_eq!(sim.led_duty(), red);
    assert!(sim.complete_frame(token(3)));
    assert_eq!(sim.session().backlight().compare(), 0);
    assert_eq!(sim.led_duty(), green);
    assert!(!sim.complete_frame(token(3)));
}

#[test]
/// Summary navigation and elapsed time do not fabricate warning completion; reduced motion preserves the configured
/// steady normal colour.
fn summary_navigation_does_not_fabricate_completed_led_warnings() {
    use dashboard_config::{Color, LedPatternKind};
    let mut raw = default_config();
    raw.led.normal.color = Color(0x001F);
    raw.led.normal.kind = LedPatternKind::Breathe;
    raw.reduced_motion = true;
    let mut sim = ControlSimulator::new(validate(raw).unwrap());
    assert_eq!(sim.led_duty().blue, 10000);
    sim.tap(ButtonId::Y);
    sim.advance(5000);
    assert_eq!(sim.led_duty().blue, 10000);
    assert_eq!(sim.completed_warning().rules, 0);
}
#[test]
/// Resuming accepted bindings preserves acquisition time/history and selected view while restoring accepted control
/// behavior.
fn unchanged_config_invalid_resume_restores_bindings_without_restarting_state() {
    use dashboard_configurator::config::*;
    use dashboard_configurator::simulator::ControlSimulator;
    let accepted = validate(default_config()).unwrap();
    let mut sim = ControlSimulator::new(accepted.clone());
    sim.advance_to(sim.now_ms() + 250, Some([1., 90., 95., 80., 30., 700., 13.8, 1.]));
    let now = sim.now_ms();
    let view = sim.session().view_id();
    let snapshot = sim.snapshot();
    let mut map = accepted.config().controls;
    map.screens[0].buttons[0].tap = ControlAction::None;
    sim.inspect_invalid(map);
    sim.resume_accepted_bindings();
    assert!(!sim.invalid());
    assert_eq!(sim.now_ms(), now);
    assert_eq!(sim.session().view_id(), view);
    assert_eq!(
        sim.snapshot()
            .sensor(dashboard_core::samples::SensorId::Boost)
            .history()
            .count(),
        snapshot
            .sensor(dashboard_core::samples::SensorId::Boost)
            .history()
            .count()
    );
    sim.tap(ButtonId::A);
    assert!(sim.session().boost_psi());
}

#[test]
/// Separate diagnostic scratch rendering cannot overwrite an unconsumed timing frame, which remains completable only
/// under its original token.
fn appearance_inspection_scratch_never_overwrites_retained_timing_candidate() {
    use dashboard_configurator::config::*;
    use dashboard_configurator::editor::*;
    use dashboard_configurator::preview::*;
    use dashboard_core::display::pipeline::{BufferId, FrameToken};
    let mut editor = Editor::new(default_config()).unwrap();
    let mut sim = dashboard_configurator::simulator::ControlSimulator::new(editor.preview().clone());
    let mut timing = Preview::new();
    let token = FrameToken {
        buffer: BufferId::A,
        sequence: 19,
    };
    let bytes = timing.render_simulation_submitted(&mut sim, token).unwrap().to_vec();
    let completed = sim.completed_warning();
    // Model an unavailable canvas: no complete_frame is issued yet.
    editor
        .set_raw(editor.field_token(), RawField::ControlHoldMilliseconds, "bad".into())
        .unwrap();
    let draft = editor.diagnostic().unwrap();
    let mut inspection = Preview::new();
    inspection
        .render_diagnostic(
            editor.preview(),
            &draft,
            editor.selection().view,
            PreviewOptions {
                sample_ms: 120000,
                warnings: true,
                ..PreviewOptions::default()
            },
        )
        .unwrap();
    assert_eq!(timing.rgba(), bytes);
    assert_eq!(sim.completed_warning(), completed);
    assert!(!sim.complete_frame(FrameToken { sequence: 20, ..token }));
    editor.discard_raw().unwrap();
    // The consumer retries the exact retained slice under its original token.
    let consumed = timing.rgba().to_vec();
    assert_eq!(consumed, bytes);
    assert!(sim.complete_frame(token));
    assert!(!sim.complete_frame(token));
}
