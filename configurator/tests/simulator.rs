use dashboard_configurator::config::*;
use dashboard_configurator::simulator::ControlSimulator;

#[test]
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
fn invalid_maps_use_accepted_layout_and_cannot_disable_recovery() {
    let mut sim = ControlSimulator::new(validate(default_config()).unwrap());
    let mut map = ControlMap::default();
    map.screens[0].buttons[0].tap = ControlAction::Logs;
    map.screens[2].buttons[3].hold = ControlAction::ToggleBoostUnits;
    sim.inspect_invalid(map);
    assert!(sim.invalid());
    sim.tap(ButtonId::A);
    assert_eq!(sim.session().page(), ControlPage::Logs);
    sim.rotate(-1);
    assert!(sim.session().log_anchor().is_some());
    sim.recover();
    assert_eq!(sim.session().page(), ControlPage::Dashboard);
}

#[test]
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
        let mut sim = ControlSimulator::new(editor.preview().clone());
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
        let mut map = editor.preview().config().controls;
        map.screens[page.index()].buttons[0].tap = action;
        sim.inspect_invalid(map);
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
fn diagnostic_pages_cannot_submit_dashboard_preview_pixels() {
    use dashboard_configurator::preview::{Preview, PreviewError};
    use dashboard_core::display::pipeline::{BufferId, FrameToken};
    let mut sim = ControlSimulator::new(validate(default_config()).unwrap());
    let mut preview = Preview::new();
    let token = FrameToken {
        buffer: BufferId::A,
        sequence: 1,
    };
    preview.render_simulation_submitted(&mut sim, token).unwrap();
    sim.complete_frame(token);
    let pixels = preview.rgb565().to_vec();
    let shown = sim.completed_warning();
    sim.tap(ButtonId::Y);
    assert_eq!(
        preview
            .render_simulation_submitted(
                &mut sim,
                FrameToken {
                    buffer: BufferId::B,
                    sequence: 2
                }
            )
            .unwrap_err(),
        PreviewError::UnsupportedPage(ControlPage::Debug)
    );
    assert_eq!(preview.rgb565(), pixels);
    assert_eq!(sim.completed_warning(), shown);
    assert!(!sim.complete_frame(FrameToken {
        buffer: BufferId::B,
        sequence: 2
    }));
    sim.tap(ButtonId::Y);
    assert_eq!(
        preview
            .render_simulation_submitted(
                &mut sim,
                FrameToken {
                    buffer: BufferId::B,
                    sequence: 3
                }
            )
            .unwrap_err(),
        PreviewError::UnsupportedPage(ControlPage::Logs)
    );
    assert_eq!(preview.rgb565(), pixels);
    assert_eq!(sim.completed_warning(), shown);
    assert!(!sim.complete_frame(FrameToken {
        buffer: BufferId::B,
        sequence: 3
    }));
}

#[test]
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
