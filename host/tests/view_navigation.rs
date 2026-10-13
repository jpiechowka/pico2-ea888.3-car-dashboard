use dashboard_host_validation::acquisition::AcquisitionModel;
use dashboard_host_validation::backlight::{Backlight, Brightness};
use dashboard_host_validation::dashboard::Overlay;
use dashboard_host_validation::pages::Page;
use dashboard_host_validation::popup::Popup;
use dashboard_host_validation::view_config::*;
use dashboard_host_validation::view_state::{ViewSelection, WarningPager};

fn config(
    mode: WarningMode,
    scope: WarningScope,
) -> ValidatedConfig {
    let mut cfg = default_config();
    cfg.views
        .push(View {
            id: 9,
            name: "Second".try_into().unwrap(),
        })
        .unwrap();
    cfg.warnings = WarningPresentation {
        mode,
        scope,
        page_ms: 500,
    };
    cfg.widgets.clear();
    for i in 0..5u16 {
        let mut w = default_widget(
            Channel::Oil,
            i + 1,
            if i < 4 { 1 } else { 9 },
            Placement {
                column: i as u8 % 4,
                row: 0,
                span: Span::OneByOne,
            },
        );
        w.warning.on_ms = 1;
        w.warning.message = format!("OIL {i}").as_str().try_into().unwrap();
        cfg.widgets.push(w).unwrap();
    }
    validate(cfg).unwrap()
}

#[test]
fn rotation_wraps_without_overflow_and_respects_diagnostic_pages() {
    let cfg = config(WarningMode::CycleOne, WarningScope::AllViews);
    let mut views = ViewSelection::new(&cfg);
    assert_eq!(views.id(), 1);
    assert!(!views.rotate(&cfg, Page::Logs, 1));
    assert!(!views.rotate(&cfg, Page::Debug, 1));
    assert!(!views.rotate(&cfg, Page::Dashboard, i32::MIN));
    assert!(views.rotate(&cfg, Page::Dashboard, i32::MAX));
    assert_eq!(views.id(), 9);
    assert!(!views.rotate(&cfg, Page::Debug, -1));
    assert_eq!(views.id(), 9);
    assert!(views.rotate(&cfg, Page::Dashboard, -1));
    assert_eq!(views.id(), 1);
    let one = validate(default_config()).unwrap();
    assert!(!ViewSelection::new(&one).rotate(&one, Page::Dashboard, 1));
}

#[test]
fn warning_pages_cycle_on_time_and_scope_resets_without_frame_counting() {
    for mode in [WarningMode::CycleOne, WarningMode::ListThree] {
        let cfg = config(mode, WarningScope::AllViews);
        let mut model = AcquisitionModel::demo_configured(&cfg);
        for at in [0, 1] {
            model.ingest_batch([1., 150., 95., 80., 30., 700., 13.8, 1.], at);
        }
        let snap = model.snapshot();
        let mut pager = WarningPager::new();
        let first = pager.select(&cfg, &snap, 1, 1, true).unwrap();
        assert_eq!(first.lines()[0].as_str(), "OIL 0");
        assert_eq!(first.lines().len(), if mode == WarningMode::CycleOne { 1 } else { 3 });
        for now in [2, 50, 70, 499, 500] {
            assert_eq!(pager.select(&cfg, &snap, 1, now, true).unwrap(), first);
        }
        let next = pager.select(&cfg, &snap, 1, 501, false).unwrap();
        assert_eq!(
            next.lines()[0].as_str(),
            if mode == WarningMode::CycleOne {
                "OIL 1"
            } else {
                "OIL 3"
            }
        );
        assert!(!next.blink);
        assert!(pager.select(&cfg, &snap, 1, 3002, true).is_none());
        assert!(
            pager.select(&cfg, &snap, 1, 3001, true).is_none(),
            "backwards presentation time fails closed"
        );
    }
    let cfg = config(WarningMode::ListThree, WarningScope::CurrentView);
    let mut model = AcquisitionModel::demo_configured(&cfg);
    for at in [0, 1] {
        model.ingest_batch([1., 150., 95., 80., 30., 700., 13.8, 1.], at);
    }
    let mut pager = WarningPager::new();
    assert_eq!(
        pager.select(&cfg, &model.snapshot(), 9, 1, true).unwrap().lines()[0].as_str(),
        "OIL 4"
    );
    assert_eq!(
        pager.select(&cfg, &model.snapshot(), 1, 2, true).unwrap().lines()[0].as_str(),
        "OIL 0"
    );
}

#[test]
fn feedback_diagnostics_and_shutdown_have_one_overlay_policy() {
    let cfg = config(WarningMode::CycleOne, WarningScope::AllViews);
    let mut model = AcquisitionModel::demo_configured(&cfg);
    for at in [0, 1] {
        model.ingest_batch([1., 150., 95., 80., 30., 700., 13.8, 1.], at);
    }
    let mut pager = WarningPager::new();
    let warning = pager.select(&cfg, &model.snapshot(), 1, 1, true);
    assert!(matches!(
        Overlay::configured_for_page(Page::Dashboard, None, warning),
        Some(Overlay::Warnings(_))
    ));
    assert_eq!(
        Overlay::configured_for_page(Page::Dashboard, Some(Overlay::Reset), warning),
        Some(Overlay::Reset)
    );
    assert_eq!(Overlay::configured_for_page(Page::Debug, None, warning), None);
    assert_eq!(
        Overlay::configured_for_page(Page::Logs, Some(Overlay::Reset), warning),
        None
    );
    let mut views = ViewSelection::new(&cfg);
    let mut bl = Backlight::new();
    bl.encoder(Page::Dashboard, 0, 2, 0); // Dim then pending Off
    assert_eq!(bl.brightness(), Brightness::Off);
    let effect = views.encoder(&cfg, &mut bl, 1, 0, 1);
    assert!(effect.motion_applied);
    assert!(matches!(effect.popup, Some(Popup::View(..))));
    assert_eq!(bl.brightness(), Brightness::Dim);
    // Motion cancels only the previous Off; a click in this batch can start a new one.
    let effect = views.encoder(&cfg, &mut bl, 1, 1, 2);
    assert!(matches!(effect.popup, Some(Popup::Brightness(_, Brightness::Off))));
    assert_eq!(bl.brightness(), Brightness::Off);
    let popup = effect.popup.unwrap();
    bl.frame_unchanged(2);
    assert!(bl.expire_popup(popup, 2002));
    bl.frame_unchanged(2002);
    assert_eq!(bl.compare(), 0);
    let effect = views.encoder(&cfg, &mut bl, 1, 0, 2003);
    assert!(
        !effect.motion_applied,
        "invisible rotation is not a visible input event"
    );
    assert_eq!(views.id(), 9);
    assert_eq!(bl.compare(), 0, "rotation must not wake a black screen");
    let waking = views.encoder(&cfg, &mut bl, 1, 1, 2004);
    assert!(
        !waking.motion_applied,
        "a subsequent wake click cannot retroactively make dark motion visible"
    );
    assert!(waking.press_applied);
    assert_eq!(bl.brightness(), Brightness::Full);
}

#[test]
fn set_churn_preserves_cursor_deadline_and_configured_view_order() {
    let mut cfg = config(WarningMode::CycleOne, WarningScope::AllViews).config().clone();
    cfg.widgets[1].warning.trigger = 140.;
    cfg.widgets[1].warning.clear = 138.;
    cfg.widgets[1].warning.off_ms = 1;
    // A document may interleave widget records; view order still wins.
    cfg.widgets.as_mut_slice().rotate_right(1);
    let cfg = validate(cfg).unwrap();
    let mut model = AcquisitionModel::demo_configured(&cfg);
    let mut pager = WarningPager::new();
    for at in [0, 1] {
        model.ingest_batch([1., 150., 95., 80., 30., 700., 13.8, 1.], at);
    }
    assert_eq!(
        pager.select(&cfg, &model.snapshot(), 1, 1, true).unwrap().lines()[0].as_str(),
        "OIL 0"
    );
    for at in [250, 251] {
        model.ingest_batch([1., 135., 95., 80., 30., 700., 13.8, 1.], at);
        assert_eq!(
            pager.select(&cfg, &model.snapshot(), 1, at, true).unwrap().lines()[0].as_str(),
            "OIL 0"
        );
    }
    assert_eq!(
        pager.select(&cfg, &model.snapshot(), 1, 501, true).unwrap().lines()[0].as_str(),
        "OIL 2"
    );
    // New arrivals don't reset the next deadline or replace the active cursor.
    for at in [600, 601] {
        model.ingest_batch([1., 150., 95., 80., 30., 700., 13.8, 1.], at);
        assert_eq!(
            pager.select(&cfg, &model.snapshot(), 1, at, true).unwrap().lines()[0].as_str(),
            "OIL 2"
        );
    }
    assert_eq!(
        pager.select(&cfg, &model.snapshot(), 1, 1001, true).unwrap().lines()[0].as_str(),
        "OIL 3"
    );
}

#[test]
fn multiple_warning_pixels_and_indicator_follow_the_same_completed_overlay() {
    use dashboard_host_validation::configured_scene::Layout;
    use dashboard_host_validation::dashboard::{DashboardCache, DashboardScene};
    use dashboard_host_validation::display::St7789Renderer;
    use dashboard_host_validation::display::pipeline::{BufferId, FrameToken};
    use dashboard_host_validation::render::FpsMode;
    use dashboard_host_validation::status_led::{PresentedLed, StatusLed};
    use dashboard_host_validation::vehicle_status::VehicleStatus;
    use dashboard_host_validation::view_state::PopupText;
    use dashboard_host_validation::{AlignedFrame, colors};
    let mut cfg = config(WarningMode::ListThree, WarningScope::AllViews).config().clone();
    for widget in &mut cfg.widgets {
        widget.warning.message = "ABCDEFGHIJKLMNOPQRST".try_into().unwrap();
    }
    let cfg = validate(cfg).unwrap();
    let mut model = AcquisitionModel::demo_configured(&cfg);
    for at in [0, 1] {
        model.ingest_batch([1., 150., 95., 80., 30., 700., 13.8, 1.], at);
    }
    let snapshot = model.snapshot();
    let mut pager = WarningPager::new();
    let mut scene = DashboardScene {
        readings: None,
        layout: Layout::new(&cfg, 1, false),
        measurements: &snapshot,
        status: VehicleStatus::Demo,
        now_ms: 1,
        blink_on: true,
        boost_psi: false,
        fps_mode: FpsMode::Off,
        fps_instant: 0.,
        fps_average: 0.,
        backgrounds: [colors::RED; 8],
        overlay: None,
    };
    let mut cache = DashboardCache::new();
    let mut frames = [AlignedFrame::boxed(), AlignedFrame::boxed()];
    let mut reference = AlignedFrame::boxed();
    let mut led = PresentedLed::new();
    let first = FrameToken {
        buffer: BufferId::A,
        sequence: 1,
    };
    let wrong = FrameToken {
        buffer: BufferId::B,
        sequence: 1,
    };
    let warning = Overlay::Warnings(pager.select(&cfg, &snapshot, 1, 1, true).unwrap());
    assert!(led.submit(first, Some(warning)));
    assert_eq!(led.output(true), StatusLed::Green);
    assert!(!led.complete(wrong));
    assert!(led.complete(first));
    assert_eq!(led.output(true), StatusLed::Red);
    let feedback = Overlay::View(PopupText::new("Second").unwrap());
    let second = FrameToken {
        buffer: BufferId::B,
        sequence: 2,
    };
    assert!(led.submit(second, Some(feedback)));
    assert!(!led.complete(first));
    assert_eq!(
        led.output(true),
        StatusLed::Red,
        "queued view must not change panel indicator early"
    );
    assert!(led.complete(second));
    assert_eq!(led.output(true), StatusLed::Green);
    for overlay in [
        Some(warning),
        Some(feedback),
        Some(Overlay::Warnings(pager.select(&cfg, &snapshot, 1, 501, false).unwrap())),
        None,
    ] {
        scene.overlay = overlay;
        scene.draw_full(&mut St7789Renderer::new(&mut reference.0));
        for (slot, frame) in frames.iter_mut().enumerate() {
            cache.draw(&mut St7789Renderer::new(&mut frame.0), slot, &scene);
            assert!(frame.0 == reference.0);
        }
        assert!(led.unchanged(overlay));
        assert_eq!(
            led.output(true),
            match overlay.and_then(Overlay::warning_phase) {
                Some(true) => StatusLed::Red,
                Some(false) => StatusLed::Off,
                None => StatusLed::Green,
            }
        );
        assert_eq!(led.output(false), StatusLed::Green);
    }
    assert!(PopupText::new("ABCDEFGHIJKLMNOPQRSTU").is_none());
    assert!(PopupText::new("bad\nmessage").is_none());
    assert!(PopupText::new("żółć").is_none());
}

#[test]
fn rotation_cancels_only_old_off_completion_even_when_new_off_is_requested() {
    use dashboard_host_validation::display::pipeline::{BufferId, FrameToken};
    let cfg = config(WarningMode::CycleOne, WarningScope::AllViews);
    let mut views = ViewSelection::new(&cfg);
    let mut bl = Backlight::new();
    let popup = bl.encoder(Page::Dashboard, 0, 2, 0).popup.unwrap();
    let first = FrameToken {
        buffer: BufferId::A,
        sequence: 1,
    };
    let clear = FrameToken {
        buffer: BufferId::B,
        sequence: 2,
    };
    bl.frame_submitted(first);
    bl.frame_completed(first, 10);
    assert!(!bl.expire_popup(popup, 2009));
    assert!(bl.expire_popup(popup, 2010));
    bl.frame_submitted(clear);
    let effect = views.encoder(&cfg, &mut bl, 1, 1, 2011);
    assert_eq!(effect.popup, Some(Popup::Brightness(2011, Brightness::Off)));
    bl.frame_completed(clear, 2012);
    assert!(bl.compare() > 0);
    assert!(
        !bl.expire_popup(effect.popup.unwrap(), 9000),
        "new Off still awaits its own presented popup"
    );
    let current = FrameToken {
        buffer: BufferId::A,
        sequence: 3,
    };
    bl.frame_submitted(current);
    bl.frame_completed(current, 9001);
    assert!(bl.expire_popup(effect.popup.unwrap(), 11001));
    bl.frame_unchanged(11001);
    assert_eq!(bl.compare(), 0);
}

#[test]
fn all64_warnings_are_reached_in_both_modes_and_feedback_restarts_cleanly() {
    for mode in [WarningMode::CycleOne, WarningMode::ListThree] {
        let mut cfg = default_config();
        cfg.views.clear();
        cfg.widgets.clear();
        cfg.warnings.mode = mode;
        cfg.warnings.page_ms = 500;
        for view in 1..=8 {
            cfg.views
                .push(View {
                    id: view,
                    name: "View".try_into().unwrap(),
                })
                .unwrap();
            for cell in 0..8 {
                let id = (view as u16 - 1) * 8 + cell as u16;
                let mut w = default_widget(
                    Channel::Oil,
                    id + 1,
                    view,
                    Placement {
                        column: cell % 4,
                        row: cell / 4,
                        span: Span::OneByOne,
                    },
                );
                w.warning.on_ms = 1;
                w.warning.message = format!("R{id:02}").as_str().try_into().unwrap();
                cfg.widgets.push(w).unwrap();
            }
        }
        let cfg = validate(cfg).unwrap();
        let mut model = AcquisitionModel::demo_configured(&cfg);
        let values = [1., 150., 95., 80., 30., 700., 13.8, 1.];
        model.ingest_batch(values, 0);
        model.ingest_batch(values, 1);
        let mut pager = WarningPager::new();
        let per_page: usize = if mode == WarningMode::CycleOne { 1 } else { 3 };
        let pages = 64usize.div_ceil(per_page);
        let mut seen = std::collections::BTreeSet::new();
        for p in 0..pages {
            let now = 1 + p as u64 * 500;
            if p != 0 {
                model.ingest_batch(values, now);
            }
            let page = pager.select(&cfg, &model.snapshot(), 1, now, true).unwrap();
            assert_eq!(usize::from(page.number), p + 1);
            assert_eq!(usize::from(page.total), pages);
            for line in page.lines() {
                assert!(seen.insert(line.as_str().to_owned()));
            }
        }
        assert_eq!(seen.len(), 64);
        let now = 1 + pages as u64 * 500;
        model.ingest_batch(values, now);
        pager.pause(now);
        assert_eq!(
            pager
                .select(&cfg, &model.snapshot(), 1, now + 1, false)
                .unwrap()
                .lines()[0]
                .as_str(),
            "R00"
        );
        assert!(pager.select(&cfg, &model.snapshot(), 1, u64::MAX, false).is_none());
    }
}

#[test]
fn warning_palettes_follow_actual_rule_and_list_mode_even_for_a_single_line() {
    use dashboard_host_validation::AlignedFrame;
    use dashboard_host_validation::display::St7789Renderer;
    use dashboard_host_validation::widgets::draw_warning_page;
    for mode in [WarningMode::CycleOne, WarningMode::ListThree] {
        let base = config(mode, WarningScope::AllViews);
        let mut raw = base.config().clone();
        // Only the other view's rule is enabled: never borrow current-view colour.
        for w in &mut raw.widgets {
            w.warning.enabled = w.view == 9;
        }
        raw.widgets[4].colors.warning = WarningColors {
            foreground: Color(0),
            background: Color(0x07FF),
        };
        raw.warning_colors = WarningColors {
            foreground: Color(0),
            background: Color(0xFFE0),
        };
        let config = validate(raw).unwrap();
        let mut model = AcquisitionModel::demo_configured(&config);
        for at in [0, 1] {
            model.ingest_batch([1., 150., 95., 80., 30., 700., 13.8, 1.], at);
        }
        let mut pager = WarningPager::new();
        let expected = if mode == WarningMode::CycleOne {
            config.config().widgets[4].colors.warning
        } else {
            config.config().warning_colors
        };
        for (at, blink) in [(1, true), (2, false)] {
            let page = pager.select(&config, &model.snapshot(), 1, at, blink).unwrap();
            assert_eq!(page.lines().len(), 1);
            assert_eq!(page.lines()[0].as_str(), "OIL 4");
            assert_eq!(page.colors, expected);
            let mut frame = AlignedFrame::boxed();
            draw_warning_page(&mut St7789Renderer::new(&mut frame.0), page);
            let at = (60 * 320 + 60) * 2;
            let background = if blink {
                expected.background
            } else {
                expected.foreground
            };
            assert_eq!(&frame.0[at..at + 2], &background.0.to_be_bytes());
        }
        let mut old = pager.select(&config, &model.snapshot(), 1, 3, true).unwrap();
        let before = old;
        old.colors.background = Color(0xF800);
        assert_ne!(old, before, "overlay retained key must include palette");
    }
}
