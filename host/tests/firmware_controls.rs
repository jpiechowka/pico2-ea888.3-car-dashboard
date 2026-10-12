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
