use dashboard_core::display::pipeline::{BufferId, FrameToken};
use dashboard_core::display::{FrameBuffer, St7789Renderer};
use dashboard_core::startup::*;
use dashboard_core::vehicle_status::VehicleStatus;
const TOKEN: FrameToken = FrameToken {
    buffer: BufferId::A,
    sequence: 1,
};

#[test]
fn startup_led_metadata_completes_without_acknowledgeable_warning() {
    use dashboard_core::status_led::{PresentedLed, PresentedScene, PresentedWarning};
    use dashboard_core::warning_state::WarningRequests;
    let scene = PresentedScene {
        eligible: true,
        attention: true,
        warning: PresentedWarning {
            rules: 3,
            event_serial: 2,
            run_id: 1,
        },
        ..PresentedScene::EMPTY
    };
    assert_eq!(scene.startup(false), PresentedScene::EMPTY);
    let mut led = PresentedLed::new();
    assert!(led.submit(TOKEN, scene.startup(true)));
    assert!(!led.complete(FrameToken { sequence: 9, ..TOKEN }));
    assert!(led.complete(TOKEN));
    assert_eq!(led.warning(), PresentedWarning::default());
    let mut requests = WarningRequests::new();
    assert!(requests.request_presented(led.warning()).is_err());
}
#[test]
fn completion_and_deadlines() {
    let p = StartupProfile::DEFAULT;
    let mut s = Startup::new(p, 100);
    s.submitted(TOKEN);
    s.completed(FrameToken { sequence: 2, ..TOKEN }, 200);
    assert_eq!(s.elapsed(300), 0);
    s.completed(TOKEN, 300);
    s.completed(TOKEN, 400);
    s.advance(p, 899, false, false);
    assert_eq!(s.phase(), Phase::Status);
    s.advance(p, 900, false, false);
    assert_eq!(s.phase(), Phase::Splash);
    s.completed(TOKEN, 901);
    assert_eq!(s.elapsed(1000), 0);
    s.advance(p, 3900, false, false);
    assert_eq!(s.exit(), Some(Exit::Finished));
}
#[test]
fn duration_zero_and_time_faults() {
    let p = StartupProfile {
        duration_ms: 0,
        ..StartupProfile::DEFAULT
    };
    let mut s = Startup::new(p, 0);
    assert_eq!(s.phase(), Phase::Status);
    s.advance(p, 1100, false, false);
    assert!(!s.active());
    let mut s = Startup::new(p, u64::MAX - 10);
    s.advance(p, 0, false, false);
    assert_eq!(s.exit(), Some(Exit::Clock));
    let mut s = Startup::new(p, 0);
    s.advance(p, u64::MAX, false, false);
    assert!(!s.active());
}
#[test]
fn warnings_faults_and_recovery() {
    let p = StartupProfile::DEFAULT;
    let mut s = Startup::new(p, 0);
    s.advance(p, 1, false, true);
    assert!(s.active());
    s.advance(
        StartupProfile {
            interrupt_on_attention: true,
            ..p
        },
        2,
        false,
        true,
    );
    assert_eq!(s.exit(), Some(Exit::Attention));
    for reason in [Exit::Skip, Exit::Recovery, Exit::Fault] {
        let mut s = Startup::new(p, 0);
        s.finish(reason);
        assert!(!s.active());
        assert_eq!(s.exit(), Some(reason));
    }
    let mut s = Startup::new(p, 0);
    s.advance(p, 1, true, false);
    assert_eq!(s.exit(), Some(Exit::Fault));
    assert!(core::mem::size_of::<Startup>() <= 96);
}
#[test]
fn profiles_are_exact_and_bounded() {
    for theme in StartupTheme::ALL {
        let p = StartupProfile {
            theme,
            ..StartupProfile::DEFAULT
        };
        let bytes = p.encode().unwrap();
        assert_eq!(StartupProfile::decode(&bytes), Some(p));
        for n in 0..16 {
            assert!(StartupProfile::decode(&bytes[..n]).is_none());
        }
        let mut bad = bytes;
        bad[15] = 1;
        assert!(StartupProfile::decode(&bad).is_none());
    }
    assert!(
        StartupProfile {
            duration_ms: 10001,
            ..StartupProfile::DEFAULT
        }
        .encode()
        .is_none()
    );
}
fn pixels(
    p: StartupProfile,
    t: u64,
    reduced: bool,
) -> Box<FrameBuffer> {
    let mut f = Box::new(FrameBuffer::new());
    draw(
        &mut St7789Renderer::new(&mut f),
        Phase::Splash,
        p,
        t,
        reduced,
        StartupFacts {
            vehicle: VehicleStatus::Demo,
            config_ok: true,
            settings_loaded: false,
            configured_sources: 0,
        },
    );
    f
}
#[test]
fn every_theme_is_time_based_and_holds_without_motion() {
    for theme in StartupTheme::ALL {
        let p = StartupProfile {
            theme,
            ..StartupProfile::DEFAULT
        };
        assert!(pixels(p, 0, false) != pixels(p, 350, false), "{theme:?} must move");
        assert!(pixels(p, 0, false) == pixels(p, 2000, false));
        assert!(pixels(p, 1, true) == pixels(p, u64::MAX, true));
        let once = StartupProfile {
            loop_animation: false,
            ..p
        };
        assert!(pixels(once, 1999, false) == pixels(once, u64::MAX, false));
    }
}
#[test]
fn startup_taps_include_unmapped_buttons_and_cancel_held_release() {
    use dashboard_core::controls::ControlGestures;
    use dashboard_core::view_config::{ControlAction, ControlMap, ControlPage};
    let mut map = ControlMap::default();
    map.screens[0].buttons[0].tap = ControlAction::None;
    let mut g = ControlGestures::new();
    g.update_startup(&map, ControlPage::Dashboard, [false; 4], 0);
    g.update_startup(&map, ControlPage::Dashboard, [true, false, false, false], 10);
    g.update_startup(&map, ControlPage::Dashboard, [true, false, false, false], 60);
    g.update_startup(&map, ControlPage::Dashboard, [false; 4], 80);
    assert!(
        g.update_startup(&map, ControlPage::Dashboard, [false; 4], 130)
            .startup_skip
    );
    g.update_startup(&map, ControlPage::Dashboard, [true, false, false, false], 200);
    g.suppress_held();
    g.update(&map, ControlPage::Dashboard, [true, false, false, false], 250);
    g.update(&map, ControlPage::Dashboard, [false; 4], 280);
    assert!(
        g.update(&map, ControlPage::Dashboard, [false; 4], 330)
            .actions
            .is_empty()
    );
}

#[test]
fn boot_held_y_never_recovers_until_a_fresh_press() {
    use dashboard_core::controls::ControlGestures;
    use dashboard_core::view_config::{ControlMap, ControlPage};
    let map = ControlMap::default();
    let mut g = ControlGestures::new();
    let y = [false, false, false, true];
    g.update_startup(&map, ControlPage::Dashboard, y, 0);
    let held = g.update_startup(&map, ControlPage::Dashboard, y, 5000);
    assert!(held.recovery.is_none() && !held.startup_skip);
    g.update_startup(&map, ControlPage::Dashboard, [false; 4], 5100);
    assert!(
        !g.update_startup(&map, ControlPage::Dashboard, [false; 4], 5150)
            .startup_skip
    );
    g.update_startup(&map, ControlPage::Dashboard, y, 5200);
    g.update_startup(&map, ControlPage::Dashboard, y, 5250);
    assert!(
        g.update_startup(&map, ControlPage::Dashboard, y, 7199)
            .recovery
            .is_none()
    );
    assert!(
        g.update_startup(&map, ControlPage::Dashboard, y, 7200)
            .recovery
            .is_some()
    );
    assert!(
        g.update_startup(&map, ControlPage::Dashboard, y, 7300)
            .recovery
            .is_none()
    );
    g.update(&map, ControlPage::Dashboard, [false; 4], 7400);
    assert!(
        g.update(&map, ControlPage::Dashboard, [false; 4], 7450)
            .actions
            .is_empty()
    );
}

#[test]
fn exact_preview_boundaries_and_status_rendering() {
    let p = StartupProfile::DEFAULT;
    assert_eq!(Startup::preview(p, 599), (Phase::Status, 599));
    assert_eq!(Startup::preview(p, 600), (Phase::Splash, 0));
    assert_eq!(Startup::preview(p, 3099), (Phase::Splash, 2499));
    assert_eq!(Startup::preview(p, 3100), (Phase::Dashboard, 0));
    let mut frame = Box::new(FrameBuffer::new());
    for vehicle in [
        VehicleStatus::Demo,
        VehicleStatus::Disabled,
        VehicleStatus::Starting,
        VehicleStatus::ResetRequired,
        VehicleStatus::Stale,
        VehicleStatus::Data,
    ] {
        draw(
            &mut St7789Renderer::new(&mut frame),
            Phase::Status,
            StartupProfile {
                status: StartupStatus::Detailed,
                ..p
            },
            u64::MAX,
            false,
            StartupFacts {
                vehicle,
                config_ok: false,
                settings_loaded: true,
                configured_sources: 8,
            },
        );
        assert!(frame.iter().any(|v| *v != 0));
    }
}
