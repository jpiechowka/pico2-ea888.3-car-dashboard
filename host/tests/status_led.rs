use dashboard_host_validation::acquisition::AcquisitionModel;
use dashboard_host_validation::backlight::Brightness;
use dashboard_host_validation::dashboard::Overlay;
use dashboard_host_validation::display::pipeline::{BufferId, FrameToken};
use dashboard_host_validation::pages::Page;
use dashboard_host_validation::render::FpsMode;
use dashboard_host_validation::samples::SensorId;
use dashboard_host_validation::status_led::{PresentedLed, StatusLed};

#[test]
fn normal_indicator_is_continuous_green_including_black_screen() {
    let presenter = PresentedLed::new();
    for backlight_on in [false, true] {
        assert_eq!(presenter.output(backlight_on), StatusLed::Green);
    }
}

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
