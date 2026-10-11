use dashboard_host_validation::acquisition::AcquisitionModel;
use dashboard_host_validation::backlight::Brightness;
use dashboard_host_validation::dashboard::Overlay;
use dashboard_host_validation::display::pipeline::{BufferId, FrameToken};
use dashboard_host_validation::pages::Page;
use dashboard_host_validation::render::FpsMode;
use dashboard_host_validation::samples::SensorId;
use dashboard_host_validation::status_led::{PresentedLed, StatusLed};

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
    assert_eq!(presenter.output(true, true), StatusLed { red: false, blue: true });
    assert!(presenter.complete(first));
    assert_eq!(presenter.output(true, true), StatusLed { red: true, blue: false });
    assert!(presenter.submit(second, Some(Overlay::Brightness(Brightness::Full))));
    assert!(!presenter.complete(first));
    assert_eq!(presenter.output(true, true), StatusLed { red: true, blue: false });
    assert!(presenter.complete(second));
    assert_eq!(presenter.output(true, true), StatusLed { red: false, blue: true });
    assert!(presenter.unchanged(Some(Overlay::Critical(SensorId::Water, false))));
    assert_eq!(
        presenter.output(true, true),
        StatusLed {
            red: false,
            blue: false
        }
    );
}

#[test]
fn visible_warning_blinks_red_without_blue_on_both_phases() {
    for heartbeat in [false, true] {
        assert_eq!(
            StatusLed::for_overlay(Some(Overlay::Critical(SensorId::Water, true)), true, heartbeat),
            StatusLed { red: true, blue: false }
        );
        assert_eq!(
            StatusLed::for_overlay(Some(Overlay::Critical(SensorId::Water, false)), true, heartbeat),
            StatusLed {
                red: false,
                blue: false
            }
        );
    }
}

#[test]
fn hidden_warnings_keep_normal_heartbeat_including_feedback_and_black_screen() {
    let mut model = AcquisitionModel::demo();
    for at in [0, 500, 1000] {
        model.ingest_batch([1.0, 140.0, 120.0, 130.0, 70.0, 1100.0, 11.0, 1.3], at);
    }
    let alerts = model.snapshot().alerts_at(1000);
    assert!(alerts.priority().is_some());
    for phase in [false, true] {
        for page in [Page::Debug, Page::Logs] {
            let visible = Overlay::for_page(page, None, alerts, phase);
            assert_eq!(
                StatusLed::for_overlay(visible, true, phase),
                StatusLed {
                    red: false,
                    blue: phase
                }
            );
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
            assert_eq!(
                StatusLed::for_overlay(visible, true, phase),
                StatusLed {
                    red: false,
                    blue: phase
                }
            );
        }
        let warning = Overlay::for_page(Page::Dashboard, None, alerts, phase);
        assert_eq!(
            StatusLed::for_overlay(warning, false, phase),
            StatusLed {
                red: false,
                blue: phase
            }
        );
    }
}
