use dashboard_host_validation::acquisition::AcquisitionModel;
use dashboard_host_validation::backlight::Brightness;
use dashboard_host_validation::dashboard::{DashboardCache, DashboardScene, Overlay};
use dashboard_host_validation::display::St7789Renderer;
use dashboard_host_validation::input_trace::{InputTracker, Observation};
use dashboard_host_validation::pages::Page;
use dashboard_host_validation::popup::Popup;
use dashboard_host_validation::render::FpsMode;
use dashboard_host_validation::samples::SensorId;
use dashboard_host_validation::vehicle_status::VehicleStatus;
use dashboard_host_validation::{AlignedFrame, colors};

fn alarm_model() -> AcquisitionModel<'static> {
    let mut model = AcquisitionModel::demo();
    for at in [0, 500, 1000] {
        model.ingest_batch([1.0, 140.0, 120.0, 130.0, 70.0, 1100.0, 11.0, 1.3], at);
    }
    model
}

#[test]
fn diagnostics_never_inherit_a_warning_or_dashboard_feedback() {
    let alerts = alarm_model().snapshot().alerts_at(1000);
    for blink in [false, true] {
        for popup in [
            None,
            Some(Overlay::Reset),
            Some(Overlay::Fps(FpsMode::Average)),
            Some(Overlay::BoostUnit(true)),
            Some(Overlay::Brightness(Brightness::Percent(0))),
            Some(Overlay::Brightness(Brightness::Full)),
            Some(Overlay::Brightness(Brightness::Dim)),
            Some(Overlay::Brightness(Brightness::Off)),
        ] {
            assert_eq!(Overlay::for_page(Page::Logs, popup, alerts, blink), None);
            let expected = if popup == Some(Overlay::Brightness(Brightness::Percent(0))) {
                popup
            } else {
                None
            };
            assert_eq!(Overlay::for_page(Page::Debug, popup, alerts, blink), expected);
        }
        assert_eq!(
            Overlay::for_page(Page::Dashboard, None, alerts, blink),
            Some(Overlay::Critical(SensorId::Water, blink))
        );
    }
}

#[test]
fn feedback_expiration_resumes_warning_at_wall_clock_deadline() {
    let mut model = alarm_model();
    for (popup, overlay, duration) in [
        (Popup::Fps(1000), Overlay::Fps(FpsMode::Average), 3000),
        (Popup::Reset(1000), Overlay::Reset, 3000),
        (Popup::BoostUnit(1000), Overlay::BoostUnit(true), 3000),
        (
            Popup::Brightness(1000, Brightness::Percent(0)),
            Overlay::Brightness(Brightness::Percent(0)),
            1500,
        ),
        (
            Popup::Brightness(1000, Brightness::Full),
            Overlay::Brightness(Brightness::Full),
            1500,
        ),
        (
            Popup::Brightness(1000, Brightness::Dim),
            Overlay::Brightness(Brightness::Dim),
            1500,
        ),
        (
            Popup::Brightness(1000, Brightness::Off),
            Overlay::Brightness(Brightness::Off),
            2000,
        ),
    ] {
        // Many/few intermediate paints cannot change the exact deadline.
        for now in [1000, 1001, 1000 + duration - 1, 1000 + duration, 1000 + duration + 500] {
            // Snapshot age must not clear the alarm while testing the overlay.
            model.ingest_batch([1.0, 140.0, 120.0, 130.0, 70.0, 1100.0, 11.0, 1.3], now);
            let alerts = model.snapshot().alerts_at(now);
            for blink in [false, true] {
                let feedback = (!popup.is_expired_at(now)).then_some(overlay);
                let expected = if now < 1000 + duration {
                    overlay
                } else {
                    Overlay::Critical(SensorId::Water, blink)
                };
                assert_eq!(
                    Overlay::for_page(Page::Dashboard, feedback, alerts, blink),
                    Some(expected)
                );
            }
        }
        model = alarm_model();
    }
    assert!(Popup::Reset(1000).is_expired_at(999));
    assert!(!Popup::Reset(u64::MAX - 3000).is_expired_at(u64::MAX - 1));
    assert!(Popup::Reset(u64::MAX - 3000).is_expired_at(u64::MAX));
}

#[test]
fn warning_feedback_and_page_round_trip_repair_both_buffers() {
    let snapshot = alarm_model().snapshot();
    let mut scene = DashboardScene {
        readings: None,
        layout: None,
        measurements: &snapshot,
        status: VehicleStatus::Demo,
        now_ms: 1000,
        blink_on: true,
        boost_psi: false,
        fps_mode: FpsMode::Off,
        fps_instant: 0.0,
        fps_average: 0.0,
        backgrounds: [colors::RED; 8],
        overlay: None,
    };
    let mut cache = DashboardCache::new();
    let mut frames = [AlignedFrame::boxed(), AlignedFrame::boxed()];
    let mut expected = AlignedFrame::boxed();
    for page in [Page::Dashboard, Page::Debug, Page::Logs, Page::Dashboard] {
        for (buffer, frame) in frames.iter_mut().enumerate() {
            if page == Page::Dashboard {
                cache.draw(&mut St7789Renderer::new(&mut frame.0), buffer, &scene);
                scene.draw_full(&mut St7789Renderer::new(&mut expected.0));
                assert!(frame.0 == expected.0);
            } else {
                cache.invalidate(buffer);
                // Diagnostic renderers clear the whole framebuffer. No warning
                // may be added by the shared page policy after that clear.
                frame.0.fill(0x5A);
                if let Some(overlay) = Overlay::for_page(page, None, snapshot.alerts_at(1000), true) {
                    overlay.draw(&mut St7789Renderer::new(&mut frame.0));
                }
                assert!(frame.0.iter().all(|byte| *byte == 0x5A));
            }
        }
        scene.blink_on = !scene.blink_on;
    }
}

#[test]
fn visible_feedback_and_page_changes_remain_correlated_with_input() {
    let alerts = alarm_model().snapshot().alerts_at(1000);
    for (page, user) in [
        (Page::Dashboard, Some(Overlay::Brightness(Brightness::Percent(0)))),
        (Page::Dashboard, Some(Overlay::Reset)),
        (Page::Dashboard, Some(Overlay::Fps(FpsMode::Off))),
        (Page::Debug, None),
        (Page::Logs, None),
    ] {
        let mut tracker = InputTracker::new();
        assert!(tracker.consume(Observation::new(1, 51).unwrap(), 52));
        let overlay = Overlay::for_page(page, user, alerts, true);
        tracker.exclude_obscured(matches!(overlay, Some(Overlay::Critical(..))));
        assert_eq!(tracker.submitted().unwrap().observed_us, 1);
        assert_eq!(tracker.counters().obscured_events, 0);
    }
}
