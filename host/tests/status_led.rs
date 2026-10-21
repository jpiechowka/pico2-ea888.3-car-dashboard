//! Software LED policy: immutable presentation completion, channel duties and elapsed-time pattern boundaries.

use dashboard_host_validation::acquisition::AcquisitionModel;
use dashboard_host_validation::backlight::Brightness;
use dashboard_host_validation::dashboard::Overlay;
use dashboard_host_validation::display::pipeline::{BufferId, FrameToken};
use dashboard_host_validation::pages::Page;
use dashboard_host_validation::render::FpsMode;
use dashboard_host_validation::samples::SensorId;
use dashboard_host_validation::status_led::{PresentedLed, StatusLed};

/// Default no-warning output remains green even when the backlight is off.
#[test]
fn normal_indicator_is_continuous_green_including_black_screen() {
    let presenter = PresentedLed::new();
    for backlight_on in [false, true] {
        assert_eq!(presenter.output(backlight_on), StatusLed::Green);
    }
}

/// Busy, mismatched and stale completions cannot change the visible phase; matching frames do.
#[test]
fn indicator_changes_only_on_matching_completed_or_unchanged_presentation() {
    let mut presenter = PresentedLed::new();
    let first = FrameToken {
        buffer: BufferId::A,
        sequence: 1,
    };
    let second = FrameToken {
        buffer: BufferId::B,
        sequence: 2,
    };
    let warning = Some(Overlay::Critical(SensorId::Water, true));
    assert!(presenter.submit(first, warning));
    assert!(!presenter.submit(second, None));
    assert!(!presenter.complete(second));
    assert!(!presenter.unchanged(warning));
    assert_eq!(presenter.output(true), StatusLed::Green);
    assert!(presenter.complete(first));
    assert_eq!(presenter.output(true), StatusLed::Red);
    assert!(presenter.submit(second, Some(Overlay::Brightness(Brightness::Full))));
    assert!(!presenter.complete(first));
    assert_eq!(presenter.output(true), StatusLed::Red);
    assert!(presenter.complete(second));
    assert_eq!(presenter.output(true), StatusLed::Green);
    assert!(!presenter.unchanged(Some(Overlay::Critical(SensorId::Water, false))));
    let third = FrameToken {
        buffer: BufferId::A,
        sequence: 3,
    };
    assert!(presenter.submit(third, Some(Overlay::Critical(SensorId::Water, false))));
    assert!(presenter.complete(third));
    assert_eq!(presenter.output(true), StatusLed::Off);
}

/// Every channel's visible warning maps its committed blink phase to red/off without a green interval.
#[test]
fn visible_warning_blinks_red_and_off_without_a_green_phase() {
    for sensor in SensorId::ALL {
        assert_eq!(
            StatusLed::for_overlay(Some(Overlay::Critical(sensor, true)), true),
            StatusLed::Red
        );
        assert_eq!(
            StatusLed::for_overlay(Some(Overlay::Critical(sensor, false)), true),
            StatusLed::Off
        );
    }
}

/// Default policy suppresses dashboard warning indication during button feedback, diagnostics and backlight-off.
#[test]
fn hidden_warnings_are_green_during_feedback_and_on_diagnostic_pages() {
    let mut model = AcquisitionModel::demo();
    for at in [0, 500, 1000] {
        model.ingest_batch([1.0, 140.0, 120.0, 130.0, 70.0, 1100.0, 11.0, 1.3], at);
    }
    let alerts = model.snapshot().alerts_at(1000);
    assert!(alerts.priority().is_some());
    for phase in [false, true] {
        for page in [Page::Debug, Page::Logs] {
            let visible = Overlay::for_page(page, None, alerts, phase);
            assert_eq!(StatusLed::for_overlay(visible, true), StatusLed::Green);
        }
        for feedback in [
            Overlay::Reset,
            Overlay::Fps(FpsMode::Average),
            Overlay::BoostUnit(true),
            Overlay::Brightness(Brightness::Full),
            Overlay::Brightness(Brightness::Dim),
            Overlay::Brightness(Brightness::Off),
        ] {
            let visible = Overlay::for_page(Page::Dashboard, Some(feedback), alerts, phase);
            assert_eq!(StatusLed::for_overlay(visible, true), StatusLed::Green);
        }
        let warning = Overlay::for_page(Page::Dashboard, None, alerts, phase);
        assert_eq!(StatusLed::for_overlay(warning, false), StatusLed::Green);
    }
}

/// Warning masks/run/event metadata change only with the matching newer completed frame token.
#[test]
fn completed_warning_metadata_is_immutable_until_matching_transfer() {
    use dashboard_host_validation::status_led::{PresentedScene, PresentedWarning};
    let first = FrameToken {
        buffer: BufferId::A,
        sequence: 1,
    };
    let second = FrameToken {
        buffer: BufferId::B,
        sequence: 2,
    };
    let mut old = PresentedScene::legacy(Some(Overlay::Critical(SensorId::Oil, true)));
    old.warning = PresentedWarning {
        rules: 1,
        event_serial: 4,
        run_id: 7,
    };
    let mut newer = old;
    newer.warning.event_serial = 5;
    let mut presented = PresentedLed::new();
    assert!(presented.submit(first, old));
    assert_eq!(presented.warning().rules, 0);
    assert!(!presented.complete(second));
    assert!(presented.complete(first));
    assert_eq!(presented.warning(), old.warning);
    assert!(!presented.unchanged(newer));
    assert!(presented.unchanged(old));
    assert!(presented.submit(second, newer));
    assert_eq!(presented.warning(), old.warning);
    assert!(!presented.complete(first));
    assert!(presented.complete(second));
    assert_eq!(presented.warning(), newer.warning);
    assert!(!presented.complete(second));
    assert!(!presented.submit(second, old));
    assert!(!presented.submit(first, old));
}

use dashboard_config::{Color, LedPatternKind, LedSettings};
use dashboard_host_validation::status_led::{LedController, LedDuty, PresentedScene};

/// Returns independent `red`, `green` and `blue` duties in hundredths of a percent; performs no validation.
fn rgb(
    red: u16,
    green: u16,
    blue: u16,
) -> LedDuty {
    LedDuty { red, green, blue }
}
/// Returns normal-pattern duty from `settings` at `at` milliseconds after a fresh epoch at zero.
fn duty(
    settings: &LedSettings,
    at: u64,
) -> LedDuty {
    let mut c = LedController::new();
    c.update(settings, PresentedScene::EMPTY, true, 0, false);
    c.update(settings, PresentedScene::EMPTY, true, at, false)
}
/// RGB565 scaling, percent brightness, blink edges and cubic breathe checkpoints have exact integer duties.
#[test]
fn configured_patterns_have_exact_endpoints_and_independent_channels() {
    let mut s = LedSettings::default();
    for (color, want) in [
        (0xF800, rgb(10000, 0, 0)),
        (0x07E0, rgb(0, 10000, 0)),
        (0x001F, rgb(0, 0, 10000)),
        (0xFFFF, rgb(10000, 10000, 10000)),
        (0, rgb(0, 0, 0)),
        (0x8410, rgb(5161, 5079, 5161)),
    ] {
        s.normal.color = Color(color);
        assert_eq!(duty(&s, 0), want);
    }
    s.normal.color = Color(0xFFFF);
    for (brightness, want) in [(0, 0), (1, 100), (50, 5000), (100, 10000)] {
        s.normal.brightness_percent = brightness;
        assert_eq!(duty(&s, 0), rgb(want, want, want));
    }
    s.normal.kind = LedPatternKind::Off;
    assert_eq!(duty(&s, 0), rgb(0, 0, 0));
    s.normal.kind = LedPatternKind::Blink;
    for period in [200, 400, 10000] {
        for on in [10, 50, 90] {
            s.normal.period_ms = period;
            s.normal.on_percent = on;
            let edge = u64::from(period) * u64::from(on) / 100;
            assert_eq!(duty(&s, edge - 1), rgb(10000, 10000, 10000));
            assert_eq!(duty(&s, edge), rgb(0, 0, 0));
            assert_eq!(duty(&s, u64::from(period)), rgb(10000, 10000, 10000));
        }
    }
    s.normal.kind = LedPatternKind::Breathe;
    s.normal.period_ms = 400;
    for (at, want) in [
        (0, 0),
        (50, 1563),
        (100, 5000),
        (150, 8438),
        (200, 10000),
        (250, 8438),
        (300, 5000),
        (350, 1563),
        (400, 0),
    ] {
        assert_eq!(duty(&s, at), rgb(want, want, want));
    }
}
/// Exhausts all policy flags/pages/feedback/backlight/eligibility combinations against an independent predicate.
#[test]
fn policy_matrix_requires_each_applicable_background_switch() {
    for bits in 0..16 {
        let s = LedSettings {
            during_feedback: bits & 1 != 0,
            on_debug: bits & 2 != 0,
            on_logs: bits & 4 != 0,
            backlight_off: bits & 8 != 0,
            ..LedSettings::default()
        };
        for page in [Page::Dashboard, Page::Debug, Page::Logs] {
            for feedback in [false, true] {
                for lit in [false, true] {
                    for eligible in [false, true] {
                        let scene = PresentedScene {
                            page,
                            eligible,
                            overlay: feedback.then_some(Overlay::Reset),
                            ..PresentedScene::EMPTY
                        };
                        let page_allowed = match page {
                            Page::Dashboard => !feedback || s.during_feedback,
                            Page::Debug => s.on_debug,
                            Page::Logs => s.on_logs,
                        };
                        let warning = eligible && page_allowed && (lit || s.backlight_off);
                        assert_eq!(
                            LedController::new().update(&s, scene, lit, 0, false),
                            if warning { rgb(10000, 0, 0) } else { rgb(0, 10000, 0) },
                            "bits={bits} page={page:?} feedback={feedback} lit={lit} eligible={eligible}"
                        );
                    }
                }
            }
        }
    }
}
/// Committed warning phases override elapsed blink; clock reversal retains output and explicit Off wins.
#[test]
fn committed_phase_warning_off_and_regressed_clock_preserve_policy() {
    let mut s = LedSettings::default();
    let mut scene = PresentedScene {
        eligible: true,
        overlay: Some(Overlay::Critical(SensorId::Oil, false)),
        ..PresentedScene::EMPTY
    };
    let mut c = LedController::new();
    assert_eq!(c.update(&s, scene, true, 100, false), rgb(0, 0, 0));
    assert_eq!(c.update(&s, scene, true, 600, false), rgb(0, 0, 0));
    scene.overlay = Some(Overlay::Critical(SensorId::Oil, true));
    assert_eq!(c.update(&s, scene, true, 600, false), rgb(10000, 0, 0));
    assert_eq!(c.update(&s, PresentedScene::EMPTY, true, 599, false), rgb(10000, 0, 0));
    s.warning.kind = LedPatternKind::Off;
    assert_eq!(c.update(&s, scene, true, 601, false), rgb(0, 0, 0));
    s.warning.kind = LedPatternKind::Blink;
    s.sync_warning_blink = false;
    assert_eq!(c.update(&s, scene, true, 700, false), rgb(0, 0, 0)); // epoch remained 100
    assert_eq!(c.update(&s, scene, true, 900, false), rgb(10000, 0, 0));
}
/// Dense/sparse updates agree near clock saturation; reduced motion holds breathing steady without disabling blink.
#[test]
fn animation_is_cadence_independent_bounded_and_reduced_motion_only_stops_breathing() {
    let mut s = LedSettings::default();
    s.normal.kind = LedPatternKind::Breathe;
    let mut dense = LedController::new();
    let mut sparse = LedController::new();
    let start = u64::MAX - 10000;
    dense.update(&s, PresentedScene::EMPTY, true, start, false);
    sparse.update(&s, PresentedScene::EMPTY, true, start, false);
    for offset in 0..=10000 {
        let out = dense.update(&s, PresentedScene::EMPTY, true, start + offset, false);
        assert!(out.green <= 10000);
        if offset % 997 == 0 || offset == 10000 {
            assert_eq!(
                out,
                sparse.update(&s, PresentedScene::EMPTY, true, start + offset, false)
            );
        }
    }
    assert_eq!(
        dense.update(&s, PresentedScene::EMPTY, true, u64::MAX, true),
        rgb(0, 10000, 0)
    );
    s.normal.kind = LedPatternKind::Blink;
    assert_eq!(
        dense.update(&s, PresentedScene::EMPTY, true, u64::MAX, true),
        rgb(0, 10000, 0)
    );
    assert_eq!(duty(&s, u64::MAX), rgb(0, 10000, 0));
}

/// Switching normal/warning selection restarts blink epochs; repeated identical selections retain them.
#[test]
fn selection_changes_restart_blink_but_visible_phase_and_repaints_do_not() {
    let mut s = LedSettings::default();
    s.normal.kind = LedPatternKind::Blink;
    s.sync_warning_blink = false;
    let warning = PresentedScene {
        eligible: true,
        ..PresentedScene::EMPTY
    };
    let mut c = LedController::new();
    assert_eq!(c.update(&s, PresentedScene::EMPTY, true, 100, false), rgb(0, 10000, 0));
    assert_eq!(c.update(&s, PresentedScene::EMPTY, true, 300, false), rgb(0, 0, 0));
    assert_eq!(c.update(&s, warning, true, 350, false), rgb(10000, 0, 0));
    assert_eq!(c.update(&s, warning, true, 549, false), rgb(10000, 0, 0));
    assert_eq!(c.update(&s, warning, true, 550, false), rgb(0, 0, 0));
    assert_eq!(c.update(&s, PresentedScene::EMPTY, true, 600, false), rgb(0, 10000, 0));
}

/// Odd and boundary breathe periods stay bounded, monotonic within each half and return to zero.
#[test]
fn breathe_extreme_and_odd_periods_are_bounded_and_monotonic_per_half() {
    let mut s = LedSettings::default();
    s.normal.kind = LedPatternKind::Breathe;
    for period in [200, 201, 9999, 10000] {
        s.normal.period_ms = period;
        let mut previous = 0;
        for at in 0..=u64::from(period) / 2 {
            let out = duty(&s, at).green;
            assert!(out >= previous && out <= 10000);
            previous = out;
        }
        assert_eq!(previous, if period == 201 { 9999 } else { 10000 });
        for at in u64::from(period) / 2 + 1..=u64::from(period) {
            let out = duty(&s, at).green;
            assert!(out <= previous);
            previous = out;
        }
        assert_eq!(previous, 0);
    }
}

/// Snoozed active rules still qualify LED status; stale data retains eligibility only through a latch.
#[test]
fn completed_eligibility_retains_snoozed_active_but_expires_unlatched_stale_data() {
    use dashboard_config::{ControlAction, WarningScope};
    use dashboard_host_validation::warning_state::WarningCommand;
    for latch in [false, true] {
        let mut raw = dashboard_config::default_config();
        raw.widgets.truncate(1);
        raw.controls.screens[0].buttons[0].tap = ControlAction::AcknowledgeWarnings;
        raw.widgets[0].warning.enabled = true;
        raw.widgets[0].warning.trigger = 2.;
        raw.widgets[0].warning.clear = 1.;
        raw.widgets[0].warning.on_ms = 1;
        raw.widgets[0].warning.advanced.latch = latch;
        raw.widgets[0].warning.advanced.snooze_s = 5;
        raw.warnings.scope = WarningScope::CurrentView;
        let config = dashboard_config::validate(raw).unwrap();
        let mut model = AcquisitionModel::demo_configured(&config);
        for at in [0, 1] {
            model.ingest_batch([3., 90., 95., 80., 30., 700., 13.8, 1.], at);
        }
        let snap = model.snapshot();
        let other = PresentedScene::capture(&config, &snap, Page::Dashboard, 2, None, 1);
        assert!(!other.eligible);
        model.apply_warning_command(
            WarningCommand {
                request_id: 1,
                run_id: snap.warning_run_id().unwrap(),
                observed_event_serial: snap.warning_event_serial(),
                rules: 1,
            },
            1,
        );
        let snap = model.snapshot();
        let acknowledged = PresentedScene::capture(&config, &snap, Page::Dashboard, 1, None, 1);
        assert!(acknowledged.eligible);
        assert!(!acknowledged.attention);
        assert_eq!(
            LedController::new().update(&LedSettings::default(), acknowledged, true, 1, false),
            rgb(10000, 0, 0)
        );
        let stale = PresentedScene::capture(&config, &snap, Page::Dashboard, 1, None, 10000);
        // The shared contract retains a latch until cleared and acknowledged.
        assert_eq!(stale.eligible, latch);
        assert_eq!(
            LedController::new().update(&LedSettings::default(), stale, true, 10000, false),
            if latch { rgb(10000, 0, 0) } else { rgb(0, 10000, 0) }
        );
    }
}
/// Reduced motion preserves an explicitly configured blink's off interval.
#[test]
fn reduced_motion_keeps_explicit_blink_off_phase() {
    let mut s = LedSettings::default();
    s.normal.kind = LedPatternKind::Blink;
    let mut c = LedController::new();
    c.update(&s, PresentedScene::EMPTY, true, 0, true);
    assert_eq!(c.update(&s, PresentedScene::EMPTY, true, 200, true), rgb(0, 0, 0));
}
