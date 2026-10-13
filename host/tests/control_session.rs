use dashboard_config::{ButtonId, ControlAction, ControlMap, ValidatedConfig};
use dashboard_core::control_session::ControlSession;
use dashboard_core::controls::{ControlBatch, ControlEvent};
use dashboard_core::input_model::EncoderBatch;
use dashboard_core::input_trace::Observation;
use dashboard_core::logging::LogStore;
use dashboard_core::pages::Page;
use dashboard_core::render::FpsMode;
use embassy_sync::blocking_mutex::raw::NoopRawMutex;

fn config() -> ValidatedConfig { dashboard_config::validate(dashboard_config::default_config()).unwrap() }

#[test]
fn every_warning_feedback_constructs_draws_and_expires_without_losing_its_meaning() {
    use dashboard_core::dashboard::Overlay;
    use dashboard_core::display::St7789Renderer;
    use dashboard_core::warning_state::WarningFeedback;
    use dashboard_host_validation::AlignedFrame;
    for (feedback, expected, remains_pending) in [
        (WarningFeedback::Pending, "ACK PENDING", true),
        (WarningFeedback::Busy, "ACK BUSY / PENDING", true),
        (WarningFeedback::Accepted, "WARNINGS ACKED", false),
        (WarningFeedback::Partial, "PARTIAL ACK", false),
        (WarningFeedback::Rejected, "ACK REJECTED", false),
        (WarningFeedback::Unavailable, "NO SHOWN WARNINGS", false),
    ] {
        let c = config();
        let mut controls = ControlSession::new(&c, FpsMode::Off, false);
        controls.warning_feedback(feedback, 100);
        let overlay = controls.overlay(&c).expect("feedback must be renderable");
        let Overlay::View(text) = overlay else {
            panic!("wrong feedback overlay");
        };
        assert_eq!(text.as_str(), expected);
        let mut frame = AlignedFrame::boxed();
        overlay.draw(&mut St7789Renderer::new(&mut frame.0));
        assert!(frame.0.iter().any(|byte| *byte != 0));
        controls.expire_feedback(3400);
        assert_eq!(controls.overlay(&c).is_some(), remains_pending);
    }
}
fn apply_action(
    s: &mut ControlSession,
    c: &ValidatedConfig,
    action: ControlAction,
    now: u64,
) -> dashboard_core::control_session::ControlEffects {
    let mut b = ControlBatch::default();
    b.actions
        .push(ControlEvent {
            button: ButtonId::A,
            action,
            observation: Observation::new(now * 1000, now * 1000).unwrap(),
        })
        .unwrap();
    s.apply(
        c,
        &ControlMap::default(),
        b,
        EncoderBatch::default(),
        &LogStore::<NoopRawMutex>::new(),
        now,
    )
}
#[test]
fn brightness_recovery_and_round_trip() {
    let c = config();
    let mut s = ControlSession::new(&c, FpsMode::Off, false);
    apply_action(&mut s, &c, ControlAction::CycleBrightness, 100);
    assert_eq!(s.backlight().compare(), 5000);
    apply_action(&mut s, &c, ControlAction::NextScreen, 200);
    assert_eq!(s.page(), Page::Debug);
    let b = ControlBatch {
        recovery: Observation::new(300000, 300000),
        ..Default::default()
    };
    assert!(
        s.apply(
            &c,
            &ControlMap::default(),
            b,
            EncoderBatch::default(),
            &LogStore::<NoopRawMutex>::new(),
            300
        )
        .recovered
    );
    assert_eq!(s.page(), Page::Dashboard);
    assert_eq!(s.backlight().compare(), 10000);
    assert!(s.popup().is_none());
}
#[test]
fn reset_waits_for_acknowledgment() {
    let c = config();
    let mut s = ControlSession::new(&c, FpsMode::Off, false);
    let effect = apply_action(&mut s, &c, ControlAction::ResetStatistics, 1);
    assert!(effect.reset_requested);
    assert!(effect.reset_observation.is_some());
    assert!(effect.observations.is_none());
    assert!(s.popup().is_none());
    s.statistics_acknowledged(2);
    assert_eq!(s.popup(), Some(dashboard_core::popup::Popup::Reset(2)));
}

fn observation(
    now: u64,
    events: u32,
) -> Observation {
    Observation {
        observed_us: now,
        accepted_us: now,
        events,
    }
}
fn multi_config() -> ValidatedConfig {
    let mut c = dashboard_config::default_config();
    c.views
        .push(dashboard_config::View {
            id: 9,
            name: "Second".try_into().unwrap(),
        })
        .unwrap();
    dashboard_config::validate(c).unwrap()
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ExpectedState {
    page: Page,
    view: u8,
    brightness: dashboard_core::backlight::Brightness,
    compare: u16,
    fps: FpsMode,
    boost_psi: bool,
    popup: Option<dashboard_core::popup::Popup>,
    anchor: Option<u64>,
}
fn matrix_config() -> ValidatedConfig {
    let mut c = multi_config().config().clone();
    c.views
        .push(dashboard_config::View {
            id: 12,
            name: "Third".try_into().unwrap(),
        })
        .unwrap();
    dashboard_config::validate(c).unwrap()
}
impl ExpectedState {
    fn capture(s: &ControlSession) -> Self {
        Self {
            page: s.page(),
            view: s.view_id(),
            brightness: s.backlight().brightness(),
            compare: s.backlight().compare(),
            fps: s.fps(),
            boost_psi: s.boost_psi(),
            popup: s.popup(),
            anchor: s.log_anchor(),
        }
    }
}

fn matrix_setup(
    s: &mut ControlSession,
    c: &ValidatedConfig,
    page: Page,
) -> LogStore<NoopRawMutex> {
    let logs = LogStore::<NoopRawMutex>::new();
    for i in 0..30 {
        logs.push(dashboard_core::logging::Level::Info, i, format_args!("entry"));
    }
    if page != Page::Dashboard {
        apply_action(
            s,
            c,
            if page == Page::Debug {
                ControlAction::Debug
            } else {
                ControlAction::Logs
            },
            0,
        );
    }
    if page == Page::Debug {
        s.backlight_mut().encoder(Page::Debug, -1, 0, 0);
        assert_eq!(s.backlight().compare(), 9725);
    }
    if page == Page::Logs {
        let e = EncoderBatch {
            delta: -2,
            motion: Some(observation(0, 1)),
            ..Default::default()
        };
        s.apply(c, &ControlMap::default(), ControlBatch::default(), e, &logs, 0);
        assert_eq!(s.log_anchor(), Some(28));
    }
    assert!(s.popup().is_none());
    logs
}

/// Independent expected outcomes, including deliberate no-ops. Both matrices
/// compare the entire state and every effect field against this action table.
fn assert_action_result(
    s: &ControlSession,
    effect: &dashboard_core::control_session::ControlEffects,
    before: ExpectedState,
    action: ControlAction,
) {
    use ControlAction::*;
    use dashboard_core::backlight::Brightness;
    use dashboard_core::popup::Popup;
    let page = before.page;
    let compatible = match action {
        None | NextScreen | PreviousScreen | Dashboard | Debug | Logs => true,
        NextView | PreviousView | ToggleBoostUnits | CycleFps | ResetStatistics | CycleBrightness
        | AcknowledgeWarnings => page == Page::Dashboard,
        ToggleBacklight | BrightnessUp | BrightnessDown => page == Page::Debug,
        ScrollOlder | ScrollNewer | LogsLive => page == Page::Logs,
    };
    let mut expected = before.clone();
    let mut flags = (false, false, false, false, false, false);
    let mut accepted = compatible;
    if compatible {
        match action {
            None | AcknowledgeWarnings => accepted = false,
            NextScreen | PreviousScreen | Dashboard | Debug | Logs => {
                expected.page = match action {
                    NextScreen => {
                        match page {
                            Page::Dashboard => Page::Debug,
                            Page::Debug => Page::Logs,
                            Page::Logs => Page::Dashboard,
                        }
                    }
                    PreviousScreen => {
                        match page {
                            Page::Dashboard => Page::Logs,
                            Page::Debug => Page::Dashboard,
                            Page::Logs => Page::Debug,
                        }
                    }
                    Dashboard => Page::Dashboard,
                    Debug => Page::Debug,
                    Logs => Page::Logs,
                    _ => unreachable!(),
                };
                accepted = expected.page != page;
                if accepted {
                    flags.0 = true;
                    expected.popup = Option::None;
                    expected.anchor = Option::None;
                }
            }
            NextView | PreviousView => {
                expected.view = if action == NextView { 9 } else { 12 };
                expected.popup = Some(Popup::View(2000));
                flags.1 = true;
            }
            ToggleBoostUnits => {
                expected.boost_psi = true;
                expected.popup = Some(Popup::BoostUnit(2000));
                flags.2 = true;
            }
            CycleFps => {
                expected.fps = FpsMode::Instant;
                expected.popup = Some(Popup::Fps(2000));
                flags.3 = true;
            }
            ResetStatistics => {
                flags.5 = true;
            }
            CycleBrightness => {
                expected.brightness = Brightness::Dim;
                expected.compare = 5000;
                expected.popup = Some(Popup::Brightness(2000, Brightness::Dim));
            }
            ToggleBacklight => {
                expected.brightness = Brightness::Percent(0);
                expected.popup = Some(Popup::Brightness(2000, Brightness::Percent(0)));
            }
            BrightnessUp => {
                expected.brightness = Brightness::Percent(100);
                expected.compare = 10000;
                expected.popup = Some(Popup::Brightness(2000, Brightness::Percent(100)));
            }
            BrightnessDown => {
                expected.brightness = Brightness::Percent(90);
                expected.compare = 9450;
                expected.popup = Some(Popup::Brightness(2000, Brightness::Percent(90)));
            }
            ScrollOlder => expected.anchor = Some(27),
            ScrollNewer => expected.anchor = Some(29),
            LogsLive => expected.anchor = Option::None,
        }
    }
    assert_eq!(ExpectedState::capture(s), expected, "{page:?}/{action:?}");
    assert_eq!(
        (
            effect.page_changed,
            effect.view_changed,
            effect.units_changed,
            effect.fps_changed,
            effect.recovered,
            effect.reset_requested
        ),
        flags,
        "flags {page:?}/{action:?}"
    );
    assert_eq!(effect.ignored, u32::from(!accepted), "ignored {page:?}/{action:?}");
    let reset = accepted && action == ResetStatistics;
    assert_eq!(
        effect.observations,
        (accepted && !reset).then_some(observation(2_000_000, 1)),
        "observations {page:?}/{action:?}"
    );
    assert_eq!(
        effect.reset_observation,
        reset.then_some(observation(2_000_000, 1)),
        "reset {page:?}/{action:?}"
    );
}
#[test]
fn all_compatible_actions_and_invalid_page_pairs() {
    use ControlAction::*;
    let actions = [
        None,
        NextScreen,
        PreviousScreen,
        Dashboard,
        Debug,
        Logs,
        NextView,
        PreviousView,
        ToggleBoostUnits,
        CycleFps,
        ResetStatistics,
        CycleBrightness,
        ToggleBacklight,
        BrightnessUp,
        BrightnessDown,
        ScrollOlder,
        ScrollNewer,
        LogsLive,
    ];
    for page in Page::ALL {
        for action in actions {
            let c = matrix_config();
            let mut s = ControlSession::new(&c, FpsMode::Off, false);
            let logs = matrix_setup(&mut s, &c, page);
            let before = ExpectedState::capture(&s);
            let mut b = ControlBatch::default();
            b.actions
                .push(ControlEvent {
                    button: ButtonId::A,
                    action,
                    observation: observation(2_000_000, 1),
                })
                .unwrap();
            let effect = s.apply(&c, &ControlMap::default(), b, EncoderBatch::default(), &logs, 2000);
            assert_action_result(&s, &effect, before, action);
        }
    }
}
#[test]
fn default_dashboard_actions_and_view_preservation() {
    let c = multi_config();
    let mut s = ControlSession::new(&c, FpsMode::Off, false);
    assert!(apply_action(&mut s, &c, ControlAction::ToggleBoostUnits, 1).units_changed);
    assert!(s.boost_psi());
    assert!(apply_action(&mut s, &c, ControlAction::CycleFps, 2).fps_changed);
    assert_eq!(s.fps(), FpsMode::Instant);
    assert!(apply_action(&mut s, &c, ControlAction::PreviousView, 3).view_changed);
    assert_eq!(s.view_id(), 9);
    for t in 4..7 {
        assert!(apply_action(&mut s, &c, ControlAction::NextScreen, t).page_changed);
    }
    assert_eq!(s.page(), Page::Dashboard);
    assert_eq!(s.view_id(), 9);
    assert!(apply_action(&mut s, &c, ControlAction::NextView, 7).view_changed);
    assert_eq!(s.view_id(), 1);
    s.expire_feedback(1507);
    assert!(s.popup().is_none());
}
#[test]
fn debug_defaults_saturate_and_toggle() {
    let c = config();
    let mut s = ControlSession::new(&c, FpsMode::Off, false);
    apply_action(&mut s, &c, ControlAction::Debug, 1);
    assert_eq!(apply_action(&mut s, &c, ControlAction::BrightnessUp, 2).ignored, 1);
    assert_eq!(apply_action(&mut s, &c, ControlAction::BrightnessDown, 3).ignored, 0);
    assert_eq!(
        s.backlight().brightness(),
        dashboard_core::backlight::Brightness::Percent(95)
    );
    apply_action(&mut s, &c, ControlAction::ToggleBacklight, 4);
    s.expire_feedback(1504);
    assert_eq!(s.backlight().compare(), 0);
    apply_action(&mut s, &c, ControlAction::ToggleBacklight, 1505);
    assert_ne!(s.backlight().compare(), 0);
}
#[test]
fn logs_actions_use_actual_history() {
    let c = config();
    let mut s = ControlSession::new(&c, FpsMode::Off, false);
    let logs = LogStore::<NoopRawMutex>::new();
    apply_action(&mut s, &c, ControlAction::Logs, 0);
    let action = |s: &mut ControlSession, a, t| {
        let mut b = ControlBatch::default();
        b.actions
            .push(ControlEvent {
                button: ButtonId::A,
                action: a,
                observation: observation(t, 1),
            })
            .unwrap();
        s.apply(&c, &ControlMap::default(), b, EncoderBatch::default(), &logs, t)
    };
    assert_eq!(action(&mut s, ControlAction::ScrollOlder, 1).ignored, 1);
    assert_eq!(s.log_anchor(), None);
    for i in 0..30 {
        logs.push(dashboard_core::logging::Level::Info, i, format_args!("entry"));
    }
    assert_eq!(action(&mut s, ControlAction::ScrollOlder, 2).ignored, 0);
    let anchor = s.log_anchor().unwrap();
    action(&mut s, ControlAction::ScrollOlder, 3);
    assert_eq!(s.log_anchor(), Some(anchor - 1));
    action(&mut s, ControlAction::ScrollNewer, 4);
    assert_eq!(s.log_anchor(), Some(anchor));
    action(&mut s, ControlAction::LogsLive, 5);
    assert!(s.log_anchor().is_none());
    assert_eq!(action(&mut s, ControlAction::LogsLive, 6).ignored, 1);
}
#[test]
fn custom_click_transition_wins_over_motion_and_buttons() {
    let c = multi_config();
    let mut s = ControlSession::new(&c, FpsMode::Off, false);
    let mut map = ControlMap::default();
    map.screens[0].encoder_click = ControlAction::Logs;
    let mut b = ControlBatch::default();
    b.actions
        .push(ControlEvent {
            button: ButtonId::A,
            action: ControlAction::ToggleBoostUnits,
            observation: observation(1, 1),
        })
        .unwrap();
    let e = EncoderBatch {
        delta: 1,
        presses: 8,
        motion: Some(observation(2, 1)),
        button: Some(observation(3, 8)),
    };
    let result = s.apply(&c, &map, b, e, &LogStore::<NoopRawMutex>::new(), 1);
    assert_eq!(s.page(), Page::Logs);
    assert!(!s.boost_psi());
    assert_eq!(s.view_id(), 1);
    assert_eq!(result.observations.unwrap().events, 1);
    assert_eq!(result.ignored, 9);
}
#[test]
fn raw_button_order_and_self_navigation() {
    let c = config();
    let mut s = ControlSession::new(&c, FpsMode::Off, false);
    let mut b = ControlBatch::default();
    for (button, action) in [(ButtonId::Y, ControlAction::Logs), (ButtonId::A, ControlAction::Debug)] {
        b.actions
            .push(ControlEvent {
                button,
                action,
                observation: observation(1, 1),
            })
            .unwrap();
    }
    let result = s.apply(
        &c,
        &ControlMap::default(),
        b,
        EncoderBatch::default(),
        &LogStore::<NoopRawMutex>::new(),
        1,
    );
    assert_eq!(s.page(), Page::Debug);
    assert_eq!(result.ignored, 1);
    let mut b = ControlBatch::default();
    for (button, action) in [
        (ButtonId::A, ControlAction::Debug),
        (ButtonId::B, ControlAction::BrightnessDown),
    ] {
        b.actions
            .push(ControlEvent {
                button,
                action,
                observation: observation(2, 1),
            })
            .unwrap();
    }
    let result = s.apply(
        &c,
        &ControlMap::default(),
        b,
        EncoderBatch::default(),
        &LogStore::<NoopRawMutex>::new(),
        2,
    );
    assert_eq!(result.ignored, 1);
    assert_eq!(
        s.backlight().brightness(),
        dashboard_core::backlight::Brightness::Percent(95)
    );
}
#[test]
fn reverse_timestamps_aggregate_without_loss_and_maximum_batch_is_bounded() {
    let c = multi_config();
    let mut s = ControlSession::new(&c, FpsMode::Off, false);
    let e = EncoderBatch {
        delta: 127,
        presses: 8,
        motion: Some(observation(50, 4)),
        button: Some(observation(10, 8)),
    };
    let result = s.apply(
        &c,
        &ControlMap::default(),
        ControlBatch::default(),
        e,
        &LogStore::<NoopRawMutex>::new(),
        1,
    );
    assert_eq!(
        result.observations,
        Some(Observation {
            observed_us: 10,
            accepted_us: 50,
            events: 12
        })
    );
    assert_eq!(s.view_id(), 9);
    assert_eq!(s.backlight().brightness(), dashboard_core::backlight::Brightness::Off);
}
#[test]
fn malformed_observations_overflow_and_time_regression_have_no_effect() {
    let c = multi_config();
    let mut s = ControlSession::new(&c, FpsMode::Off, false);
    let e = EncoderBatch {
        delta: i32::MAX,
        presses: u8::MAX,
        motion: Some(observation(1, 1)),
        button: Some(observation(1, 1)),
    };
    assert_eq!(
        s.apply(
            &c,
            &ControlMap::default(),
            ControlBatch::default(),
            e,
            &LogStore::<NoopRawMutex>::new(),
            10
        )
        .ignored,
        256
    );
    assert_eq!(apply_action(&mut s, &c, ControlAction::ToggleBoostUnits, 9).ignored, 1);
    assert!(!s.boost_psi());
    let mut b = ControlBatch::default();
    b.actions
        .push(ControlEvent {
            button: ButtonId::A,
            action: ControlAction::CycleFps,
            observation: Observation {
                observed_us: 2,
                accepted_us: 1,
                events: 1,
            },
        })
        .unwrap();
    assert_eq!(
        s.apply(
            &c,
            &ControlMap::default(),
            b,
            EncoderBatch::default(),
            &LogStore::<NoopRawMutex>::new(),
            11
        )
        .ignored,
        1
    );
    assert_eq!(s.fps(), FpsMode::Off);
    let mut b = ControlBatch::default();
    b.actions
        .push(ControlEvent {
            button: ButtonId::A,
            action: ControlAction::CycleFps,
            observation: observation(1, u32::MAX),
        })
        .unwrap();
    let e = EncoderBatch {
        delta: 1,
        motion: Some(observation(1, 1)),
        ..Default::default()
    };
    let result = s.apply(&c, &ControlMap::default(), b, e, &LogStore::<NoopRawMutex>::new(), 12);
    assert_eq!(result.ignored, 1);
    assert_eq!(s.view_id(), 1);
}
#[test]
fn ordinary_feedback_interrupts_pending_off_and_ack_off_dashboard_is_quiet() {
    let c = config();
    let mut s = ControlSession::new(&c, FpsMode::Off, false);
    apply_action(&mut s, &c, ControlAction::CycleBrightness, 1);
    apply_action(&mut s, &c, ControlAction::CycleBrightness, 2);
    assert_eq!(s.backlight().brightness(), dashboard_core::backlight::Brightness::Off);
    apply_action(&mut s, &c, ControlAction::CycleFps, 3);
    assert_eq!(s.backlight().compare(), 5000);
    apply_action(&mut s, &c, ControlAction::ResetStatistics, 4);
    apply_action(&mut s, &c, ControlAction::Debug, 5);
    s.statistics_acknowledged(6);
    assert!(s.popup().is_none());
}

fn frame(sequence: u64) -> dashboard_core::display::pipeline::FrameToken {
    dashboard_core::display::pipeline::FrameToken {
        buffer: dashboard_core::display::pipeline::BufferId::A,
        sequence,
    }
}
#[test]
fn recovery_cancels_all_shutdown_phases_on_every_screen_and_retains_preferences() {
    for page in Page::ALL {
        for phase in 0..7 {
            let c = multi_config();
            let mut s = ControlSession::new(&c, FpsMode::Combined, true);
            apply_action(&mut s, &c, ControlAction::NextView, 0);
            if page != Page::Dashboard {
                apply_action(
                    &mut s,
                    &c,
                    if page == Page::Debug {
                        ControlAction::Debug
                    } else {
                        ControlAction::Logs
                    },
                    0,
                );
            }
            // Existing frame owner may complete a shutdown independently of the UI page.
            if phase > 0 {
                s.backlight_mut().encoder(Page::Dashboard, 0, 1, 100);
            }
            if phase > 1 {
                s.backlight_mut().encoder(Page::Dashboard, 0, 1, 100);
            }
            if phase > 2 {
                s.backlight_mut().frame_submitted(frame(1));
            }
            if phase > 3 {
                s.backlight_mut().frame_completed(frame(1), 100);
                let popup = dashboard_core::popup::Popup::Brightness(100, dashboard_core::backlight::Brightness::Off);
                s.backlight_mut().expire_popup(popup, 2100);
            }
            if phase > 4 {
                s.backlight_mut().frame_submitted(frame(2));
            }
            if phase > 5 {
                s.backlight_mut().frame_completed(frame(2), 2150);
            }
            let b = ControlBatch {
                recovery: Some(observation(2200000, 1)),
                ..Default::default()
            };
            let effect = s.apply(
                &c,
                &ControlMap::default(),
                b,
                EncoderBatch {
                    delta: 1,
                    presses: 1,
                    motion: Some(observation(1, 1)),
                    button: Some(observation(1, 1)),
                },
                &LogStore::<NoopRawMutex>::new(),
                2200,
            );
            assert!(effect.recovered);
            assert_eq!(effect.ignored, 2);
            assert_eq!(s.page(), Page::Dashboard);
            assert_eq!(s.view_id(), 9);
            assert_eq!(s.fps(), FpsMode::Combined);
            assert!(s.boost_psi());
            assert!(s.popup().is_none());
            assert!(s.log_anchor().is_none());
            s.backlight_mut().frame_completed(frame(1), 2300);
            s.backlight_mut().frame_completed(frame(2), 2300);
            s.backlight_mut().frame_unchanged(2300);
            assert_eq!(s.backlight().compare(), 10000);
        }
    }
}
#[test]
fn off_requires_completed_feedback_then_completed_clear() {
    let c = config();
    let mut s = ControlSession::new(&c, FpsMode::Off, false);
    apply_action(&mut s, &c, ControlAction::CycleBrightness, 100);
    apply_action(&mut s, &c, ControlAction::CycleBrightness, 100);
    s.expire_feedback(4000);
    assert!(s.popup().is_some());
    assert_eq!(s.backlight().compare(), 5000);
    s.backlight_mut().frame_submitted(frame(1));
    s.backlight_mut().frame_completed(frame(1), 4500);
    s.expire_feedback(6499);
    assert!(s.popup().is_some());
    s.expire_feedback(6500);
    assert!(s.popup().is_none());
    assert_eq!(s.backlight().compare(), 5000);
    s.backlight_mut().frame_submitted(frame(2));
    s.backlight_mut().frame_completed(frame(1), 6501);
    assert_eq!(s.backlight().compare(), 5000);
    s.backlight_mut().frame_completed(frame(2), 6502);
    assert_eq!(s.backlight().compare(), 0);
}
#[test]
fn single_view_and_none_bindings_are_ignored() {
    let c = config();
    let mut s = ControlSession::new(&c, FpsMode::Off, false);
    assert_eq!(apply_action(&mut s, &c, ControlAction::NextView, 1).ignored, 1);
    assert_eq!(apply_action(&mut s, &c, ControlAction::None, 2).ignored, 1);
    let mut map = ControlMap::default();
    map.screens[0].encoder_rotation = dashboard_config::RotationAction::None;
    map.screens[0].encoder_click = ControlAction::None;
    let e = EncoderBatch {
        delta: 128,
        presses: 8,
        motion: Some(observation(1, 4)),
        button: Some(observation(1, 8)),
    };
    assert_eq!(
        s.apply(
            &c,
            &map,
            ControlBatch::default(),
            e,
            &LogStore::<NoopRawMutex>::new(),
            3
        )
        .ignored,
        12
    );
}

#[test]
fn encoder_custom_actions_respect_every_page_pair_and_count_saturated_clicks() {
    use ControlAction::*;
    let actions = [
        None,
        NextScreen,
        PreviousScreen,
        Dashboard,
        Debug,
        Logs,
        NextView,
        PreviousView,
        ToggleBoostUnits,
        CycleFps,
        ResetStatistics,
        CycleBrightness,
        ToggleBacklight,
        BrightnessUp,
        BrightnessDown,
        ScrollOlder,
        ScrollNewer,
        LogsLive,
    ];
    for page in Page::ALL {
        for action in actions {
            let c = matrix_config();
            let mut s = ControlSession::new(&c, FpsMode::Off, false);
            let logs = matrix_setup(&mut s, &c, page);
            let before = ExpectedState::capture(&s);
            let mut map = ControlMap::default();
            map.screens[page.index()].encoder_click = action;
            let effect = s.apply(
                &c,
                &map,
                ControlBatch::default(),
                EncoderBatch {
                    presses: 1,
                    button: Some(observation(2_000_000, 1)),
                    ..Default::default()
                },
                &logs,
                2000,
            );
            assert_action_result(&s, &effect, before, action);
        }
    }
    let c = config();
    let mut s = ControlSession::new(&c, FpsMode::Off, false);
    apply_action(&mut s, &c, Debug, 0);
    apply_action(&mut s, &c, BrightnessDown, 0);
    let mut map = ControlMap::default();
    map.screens[1].encoder_click = BrightnessUp;
    let e = EncoderBatch {
        presses: 8,
        button: Some(observation(1, 8)),
        ..Default::default()
    };
    let effect = s.apply(
        &c,
        &map,
        ControlBatch::default(),
        e,
        &LogStore::<NoopRawMutex>::new(),
        1,
    );
    assert_eq!(effect.ignored, 7);
    assert_eq!(effect.observations.unwrap().events, 1);
    assert_eq!(s.backlight().compare(), 10000);
}
#[test]
fn missing_and_mismatched_encoder_metadata_are_counted_without_execution() {
    let c = multi_config();
    let mut s = ControlSession::new(&c, FpsMode::Off, false);
    for e in [
        EncoderBatch {
            delta: 1,
            presses: 2,
            ..Default::default()
        },
        EncoderBatch {
            presses: 2,
            button: Some(observation(1, 1)),
            ..Default::default()
        },
    ] {
        let effect = s.apply(
            &c,
            &ControlMap::default(),
            ControlBatch::default(),
            e,
            &LogStore::<NoopRawMutex>::new(),
            1,
        );
        assert_eq!(effect.ignored, if e.delta == 0 { 2 } else { 3 });
        assert!(effect.observations.is_none());
        assert_eq!(s.view_id(), 1);
        assert_eq!(s.backlight().compare(), 10000);
    }
}

#[test]
fn future_observations_and_consumption_overflow_fail_closed() {
    let c = multi_config();
    let mut s = ControlSession::new(&c, FpsMode::Off, false);
    let mut b = ControlBatch {
        recovery: Some(observation(2000, 1)),
        ..Default::default()
    };
    b.actions
        .push(ControlEvent {
            button: ButtonId::Y,
            action: ControlAction::Debug,
            observation: observation(2000, 1),
        })
        .unwrap();
    let e = EncoderBatch {
        delta: 1,
        presses: 1,
        motion: Some(observation(2000, 1)),
        button: Some(observation(2000, 1)),
    };
    let result = s.apply(&c, &ControlMap::default(), b, e, &LogStore::<NoopRawMutex>::new(), 1);
    assert_eq!(result.ignored, 4);
    assert!(!result.recovered);
    assert_eq!(s.page(), Page::Dashboard);
    assert_eq!(s.view_id(), 1);
    assert_eq!(s.backlight().compare(), 10000);
    let b = ControlBatch {
        recovery: Some(observation(1, 1)),
        ..Default::default()
    };
    let result = s.apply(
        &c,
        &ControlMap::default(),
        b,
        EncoderBatch::default(),
        &LogStore::<NoopRawMutex>::new(),
        u64::MAX,
    );
    assert_eq!(result.ignored, 1);
    assert!(!result.recovered);
}

#[test]
fn priority_discard_counts_do_not_subtract_from_a_saturated_total() {
    let c = config();
    let mut s = ControlSession::new(&c, FpsMode::Off, false);
    let b = ControlBatch {
        recovery: Some(observation(1, u32::MAX)),
        ..Default::default()
    };
    let e = EncoderBatch {
        delta: 1,
        motion: Some(observation(1, 1)),
        ..Default::default()
    };
    let effect = s.apply(&c, &ControlMap::default(), b, e, &LogStore::<NoopRawMutex>::new(), 1);
    assert_eq!(effect.ignored, 1);
    assert_eq!(effect.observations.unwrap().events, u32::MAX);
}

#[test]
fn observations_in_current_millisecond_bucket_are_accepted_including_recovery() {
    let c = multi_config();
    let mut s = ControlSession::new(&c, FpsMode::Off, false);
    let mut b = ControlBatch::default();
    b.actions
        .push(ControlEvent {
            button: ButtonId::A,
            action: ControlAction::ToggleBoostUnits,
            observation: observation(1_200_999, 1),
        })
        .unwrap();
    let e = EncoderBatch {
        delta: 1,
        presses: 1,
        motion: Some(observation(1_200_999, 1)),
        button: Some(observation(1_200_999, 1)),
    };
    let effect = s.apply(&c, &ControlMap::default(), b, e, &LogStore::<NoopRawMutex>::new(), 1200);
    assert_eq!(effect.ignored, 0);
    assert_eq!(effect.observations.unwrap().events, 3);
    assert!(s.boost_psi());
    assert_eq!(s.view_id(), 9);
    assert_eq!(s.backlight().compare(), 5000);
    let b = ControlBatch {
        recovery: Some(observation(1_200_999, 1)),
        ..Default::default()
    };
    let effect = s.apply(
        &c,
        &ControlMap::default(),
        b,
        EncoderBatch::default(),
        &LogStore::<NoopRawMutex>::new(),
        1200,
    );
    assert!(effect.recovered);
    assert_eq!(effect.ignored, 0);
    assert_eq!(s.backlight().compare(), 10000);
    assert_eq!(s.view_id(), 9);
}

#[test]
fn acknowledge_emits_intent_without_success_or_statistics_reset() {
    let c = config();
    let mut s = ControlSession::new(&c, FpsMode::Off, false);
    let effect = apply_action(&mut s, &c, ControlAction::AcknowledgeWarnings, 1);
    assert_eq!(effect.ignored, 0);
    assert!(effect.acknowledge_requested);
    assert!(!effect.reset_requested);
    assert!(effect.observations.is_some());
    s.statistics_acknowledged(2);
    assert!(s.popup().is_none());
}

#[test]
fn warning_feedback_remains_pending_until_result_and_never_confirms_reset() {
    use dashboard_host_validation::popup::Popup;
    use dashboard_host_validation::warning_state::WarningFeedback;
    let c = config();
    let mut s = ControlSession::new(&c, FpsMode::Off, false);
    s.warning_feedback(WarningFeedback::Pending, 1);
    s.expire_feedback(10000);
    assert!(matches!(s.popup(), Some(Popup::Warnings(_, WarningFeedback::Pending))));
    s.warning_feedback(WarningFeedback::Busy, 10001);
    s.expire_feedback(20000);
    assert!(matches!(s.popup(), Some(Popup::Warnings(_, WarningFeedback::Busy))));
    s.warning_feedback(WarningFeedback::Accepted, 20001);
    assert!(matches!(s.popup(), Some(Popup::Warnings(_, WarningFeedback::Accepted))));
    s.expire_feedback(23001);
    assert!(s.popup().is_none());
    s.statistics_acknowledged(23002);
    assert!(s.popup().is_none());
}
