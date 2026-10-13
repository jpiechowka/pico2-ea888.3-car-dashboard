use dashboard_config::{ControlAction, ControlMap};
use dashboard_core::control_session::ControlSession;
use dashboard_core::controls::ControlGestures;
use dashboard_core::input_model::EncoderBatch;
use dashboard_core::input_trace::Observation;
use dashboard_core::logging::LogStore;
use dashboard_core::pages::Page;
use dashboard_core::render::FpsMode;
use embassy_sync::blocking_mutex::raw::NoopRawMutex;

#[test]
fn firmware_owns_one_session_and_samples_time_after_encoder_batch() {
    let source = include_str!("../../firmware/pico2/src/main.rs");
    assert!(source.contains("let mut session = ControlSession::new"));
    assert!(!source.contains("process_buttons("));
    let pressed = source.find("let pressed = [btn_a.is_low()").unwrap();
    let batch = source[pressed..].find("ENCODER_EVENTS.take_observed()").unwrap();
    let time = source[pressed..].find("let input_ms = Instant::now()").unwrap();
    let apply = source[pressed..].find("session.apply(").unwrap();
    assert!(batch < time && time < apply);
    assert!(source.contains("gestures.reset();"));
    assert!(source.contains("ENCODER_EVENTS.clear();"));
    assert!(source.contains("session.statistics_acknowledged("));
    assert_eq!(
        source
            .split_whitespace()
            .collect::<String>()
            .matches("session.backlight_mut().frame_completed(")
            .count(),
        2
    );
}

#[test]
fn physical_transition_discards_encoder_and_old_release_then_maintenance_disarms() {
    let c = dashboard_config::validate(dashboard_config::default_config()).unwrap();
    let map = ControlMap::default();
    let logs = LogStore::<NoopRawMutex>::new();
    let mut session = ControlSession::new(&c, FpsMode::Off, false);
    let mut gestures = ControlGestures::new();
    let mut tick = |pressed, now, encoder| {
        let buttons = gestures.update(&map, session.page(), pressed, now);
        session.apply(&c, &map, buttons, encoder, &logs, now)
    };
    tick([false; 4], 0, EncoderBatch::default());
    tick([true, false, false, true], 100, EncoderBatch::default());
    tick([true, false, false, true], 150, EncoderBatch::default());
    tick([true, false, false, false], 200, EncoderBatch::default());
    let effect = tick(
        [true, false, false, false],
        250,
        EncoderBatch {
            delta: 3,
            presses: 2,
            motion: Some(Observation {
                observed_us: 240000,
                accepted_us: 250000,
                events: 3,
            }),
            button: Some(Observation {
                observed_us: 240000,
                accepted_us: 250000,
                events: 2,
            }),
        },
    );
    assert!(effect.page_changed);
    assert_eq!(effect.observations.unwrap().events, 1);
    assert_eq!(effect.ignored, 5);
    drop(tick);
    assert_eq!(session.page(), Page::Debug);
    assert_eq!(session.view_id(), c.config().boot_view);
    assert_eq!(session.backlight().compare(), 10000);
    assert!(!session.boost_psi());
    assert!(session.popup().is_none());
    for (pressed, now) in [([true, false, false, false], 300), ([false; 4], 400), ([false; 4], 450)] {
        let b = gestures.update(&map, session.page(), pressed, now);
        assert!(b.actions.is_empty());
        session.apply(&c, &map, b, EncoderBatch::default(), &logs, now);
    }
    gestures.reset();
    for (pressed, now) in [
        ([false, false, false, true], 500),
        ([false, false, false, true], 3000),
        ([false; 4], 3100),
        ([false; 4], 3150),
    ] {
        let b = gestures.update(&map, session.page(), pressed, now);
        assert!(b.actions.is_empty());
        assert!(b.recovery.is_none());
        let e = session.apply(&c, &map, b, EncoderBatch::default(), &logs, now);
        assert!(!e.page_changed && !e.recovered);
    }
    assert_eq!(session.page(), Page::Debug);
}

#[test]
fn reset_acknowledgment_after_navigation_clears_pending_without_late_popup() {
    let c = dashboard_config::validate(dashboard_config::default_config()).unwrap();
    let map = ControlMap::default();
    let logs = LogStore::<NoopRawMutex>::new();
    let mut s = ControlSession::new(&c, FpsMode::Off, false);
    let mut g = ControlGestures::new();
    let mut requests = dashboard_core::acquisition::ResetRequests::default();
    let mut requested = 0;
    for (p, t) in [
        ([false; 4], 0),
        ([false, true, false, false], 100),
        ([false, true, false, false], 150),
        ([false; 4], 200),
        ([false; 4], 250),
    ] {
        let b = g.update(&map, s.page(), p, t);
        let e = s.apply(&c, &map, b, EncoderBatch::default(), &logs, t);
        if t == 250 {
            assert!(e.reset_requested);
            requested = requests.request();
        }
    }
    assert!(s.popup().is_none());
    let mut b = dashboard_core::controls::ControlBatch::default();
    b.actions
        .push(dashboard_core::controls::ControlEvent {
            button: dashboard_config::ButtonId::Y,
            action: ControlAction::Debug,
            observation: Observation::new(300000, 300000).unwrap(),
        })
        .unwrap();
    s.apply(&c, &map, b, EncoderBatch::default(), &logs, 300);
    assert!(s.popup().is_none());
    assert!(!requests.acknowledge(requested - 1));
    assert!(s.popup().is_none());
    assert!(requests.acknowledge(requested));
    s.statistics_acknowledged(350);
    assert!(s.popup().is_none());
    let b = dashboard_core::controls::ControlBatch {
        recovery: Observation::new(400000, 400000),
        ..Default::default()
    };
    s.apply(&c, &map, b, EncoderBatch::default(), &logs, 400);
    assert!(!requests.acknowledge(requested));
    s.statistics_acknowledged(450);
    assert!(s.popup().is_none());
}

#[test]
fn warning_route_is_separate_capacity_one_and_applies_after_inputs_before_tick() {
    let main = include_str!("../../firmware/pico2/src/main.rs");
    let acquisition = include_str!("../../firmware/pico2/src/tasks/acquisition.rs");
    let service = include_str!("../../crates/dashboard-core/src/diagnostics/service.rs");
    assert!(
        acquisition.contains("Channel<CriticalSectionRawMutex, dashboard_pico2::warning_state::WarningCommand, 1>")
    );
    assert!(acquisition.contains("RESET_STATISTICS: Signal<CriticalSectionRawMutex, u64>"));
    assert!(
        main.find("request_presented(led_presentation.warning())").unwrap()
            < main.find("measurement_receiver.try_changed()").unwrap()
    );
    assert!(main.contains("warning_requests.observe(&measurements)"));
    assert!(main.contains("WARNING_COMMANDS.try_send(command)"));
    assert!(!service.contains("self.session.tick(now)"));
    assert!(!service.contains("self.session.poll(now)"));
    let start = service.find("pub fn step_with_warning").unwrap();
    let code = &service[start..];
    assert!(
        code.find("self.step_inputs(cx, now)").unwrap()
            < code.find("self.apply_warning_command(command, now)").unwrap()
    );
    assert!(
        code.find("self.apply_warning_command(command, now)").unwrap()
            < code.find("self.session.tick_measurements(now)").unwrap()
    );
    assert!(acquisition.contains("model.step_with_warning(cx, now, WARNING_COMMANDS.try_receive().ok())"));
    let start = acquisition.find("model.ingest_batch").unwrap();
    let code = &acquisition[start..];
    assert!(code.find("model.ingest_batch").unwrap() < code.find("model.apply_warning_command(command, now)").unwrap());
    assert!(code.find("model.apply_warning_command(command, now)").unwrap() < code.find("model.tick(now)").unwrap());
}

#[test]
fn completed_diagnostic_scene_and_pending_return_use_the_same_led_owner() {
    use dashboard_core::display::pipeline::{BufferId, FrameToken};
    use dashboard_core::status_led::{LedController, LedDuty, PresentedLed, PresentedScene};
    let mut settings = dashboard_config::LedSettings {
        on_debug: true,
        ..Default::default()
    };
    settings.warning.kind = dashboard_config::LedPatternKind::Solid;
    let mut presented = PresentedLed::new();
    let mut led = LedController::new();
    let token = |sequence| {
        FrameToken {
            buffer: BufferId::A,
            sequence,
        }
    };
    let debug = PresentedScene {
        page: Page::Debug,
        eligible: true,
        ..PresentedScene::EMPTY
    };
    assert!(presented.submit(token(1), debug));
    assert_eq!(
        led.update(&settings, presented.visible(), true, 0, false),
        LedDuty {
            red: 0,
            green: 10000,
            blue: 0
        }
    );
    assert!(presented.complete(token(1)));
    assert_eq!(led.update(&settings, presented.visible(), true, 1, false).red, 10000);
    assert!(presented.submit(token(2), PresentedScene::EMPTY));
    assert!(!presented.complete(token(3)));
    assert_eq!(led.update(&settings, presented.visible(), true, 2, false).red, 10000);
    assert!(presented.complete(token(2)));
    assert_eq!(led.update(&settings, presented.visible(), true, 3, false).green, 10000);
}
