//! Named-view navigation and warning rotation share completed feedback and raw warning state.
use dashboard_host_validation::acquisition::AcquisitionModel;
use dashboard_host_validation::backlight::{Backlight, Brightness};
use dashboard_host_validation::dashboard::Overlay;
use dashboard_host_validation::pages::Page;
use dashboard_host_validation::popup::Popup;
use dashboard_host_validation::view_config::*;
use dashboard_host_validation::view_state::{ViewSelection, WarningPager};

/// Returns two validated views with five oil warnings, one-millisecond activation and 500 ms pages.
/// `mode` selects page capacity; `scope` selects current/all-view eligibility. Panics on fixture invalidity.
fn config(
    mode: WarningMode,
    scope: WarningScope,
) -> ValidatedConfig {
    let mut cfg = default_config();
    cfg.views
        .push(View {
            id: 9,
            name: "Second".try_into().unwrap(),
            orientation: None,
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

/// Extreme rotation wraps without overflow and changes selection only on Dashboard with multiple views.
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

/// Warning pages advance by elapsed time, reset on scope/view changes and reject stale/reversed time.
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

/// Feedback outranks warnings; navigation cancels old Off, while invisible rotation never wakes the display.
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

/// Warning-set churn preserves scheduled cursor/deadline and view order while new episodes interrupt.
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
    // New episodes interrupt immediately without replacing the retained rotation cursor.
    for at in [600, 601] {
        model.ingest_batch([1., 150., 95., 80., 30., 700., 13.8, 1.], at);
        assert_eq!(
            pager.select(&cfg, &model.snapshot(), 1, at, true).unwrap().lines()[0].as_str(),
            if at == 600 { "OIL 2" } else { "OIL 0" }
        );
    }
    assert_eq!(
        pager.select(&cfg, &model.snapshot(), 1, 1001, true).unwrap().lines()[0].as_str(),
        "OIL 3"
    );
}

/// Retained warning pixels and RGB phase follow the same completed overlay, never a queued replacement.
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
    for (sequence, overlay) in [
        Some(warning),
        Some(feedback),
        Some(Overlay::Warnings(pager.select(&cfg, &snapshot, 1, 501, false).unwrap())),
        None,
    ]
    .into_iter()
    .enumerate()
    {
        scene.overlay = overlay;
        scene.draw_full(&mut St7789Renderer::new(&mut reference.0));
        for (slot, frame) in frames.iter_mut().enumerate() {
            cache.draw(&mut St7789Renderer::new(&mut frame.0), slot, &scene);
            assert!(frame.0 == reference.0);
        }
        if !led.unchanged(overlay) {
            let token = FrameToken {
                buffer: BufferId::A,
                sequence: 10 + sequence as u64,
            };
            assert!(led.submit(token, overlay));
            assert!(led.complete(token));
        }
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

/// Rotation cancels the old clear completion without satisfying a newly requested Off popup's dwell.
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

/// Both paging modes visit all 64 rules once per cycle and restart cleanly after feedback interruption.
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
                    orientation: None,
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

/// Single-warning mode uses its rule palette; list mode uses the global palette even for one line.
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

/// Stable priority ordering still rotates through lower-priority rules in both paging modes.
#[test]
fn priorities_are_stable_and_lower_priorities_rotate() {
    for mode in [WarningMode::CycleOne, WarningMode::ListThree] {
        let mut raw = config(mode, WarningScope::AllViews).config().clone();
        for (w, priority) in raw.widgets.iter_mut().zip([
            Priority::Low,
            Priority::High,
            Priority::Critical,
            Priority::High,
            Priority::Normal,
        ]) {
            w.warning.advanced.priority = priority;
        }
        let cfg = validate(raw).unwrap();
        let mut model = AcquisitionModel::demo_configured(&cfg);
        let values = [1., 150., 95., 80., 30., 700., 13.8, 1.];
        model.ingest_batch(values, 0);
        model.ingest_batch(values, 1);
        let mut pager = WarningPager::new();
        let mut seen = Vec::new();
        for step in 0..if mode == WarningMode::CycleOne { 5 } else { 2 } {
            let now = 1 + step * 500;
            model.ingest_batch(values, now);
            let page = pager.select(&cfg, &model.snapshot(), 1, now, true).unwrap();
            seen.extend(page.lines().iter().map(|l| l.as_str().to_owned()));
        }
        assert_eq!(seen, ["OIL 2", "OIL 1", "OIL 3", "OIL 4", "OIL 0"]);
    }
}

/// New critical episodes around page boundaries interrupt immediately without starving persistent rules.
#[test]
fn repeated_new_episodes_interrupt_without_erasing_scheduled_rotation() {
    for offset in [-1i64, 0, 1] {
        let mut raw = config(WarningMode::CycleOne, WarningScope::AllViews).config().clone();
        raw.widgets.truncate(4);
        raw.warnings.page_ms = 1000;
        // Independently reactivate the first critical rule; three normal rules persist.
        raw.widgets[0].warning.trigger = 140.;
        raw.widgets[0].warning.clear = 138.;
        raw.widgets[0].warning.off_ms = 1;
        raw.widgets[0].warning.advanced.priority = Priority::Critical;
        let c = validate(raw).unwrap();
        let mut m = AcquisitionModel::demo_configured(&c);
        let mut pager = WarningPager::new();
        let sample = |m: &mut AcquisitionModel<'_>, oil, now| {
            m.ingest_batch([1., oil, 95., 80., 30., 700., 13.8, 1.], now);
        };
        sample(&mut m, 150., 0);
        sample(&mut m, 150., 1);
        pager.select(&c, &m.snapshot(), 1, 1, true).unwrap();
        let mut seen = std::collections::BTreeSet::new();
        for cycle in 1..=4 {
            let boundary = 1 + cycle * 1000;
            let event_at = (boundary as i64 + offset) as u64;
            for now in [event_at - 4, event_at - 3] {
                sample(&mut m, 135., now);
                seen.insert(
                    pager.select(&c, &m.snapshot(), 1, now, true).unwrap().lines()[0]
                        .as_str()
                        .to_owned(),
                );
            }
            sample(&mut m, 150., event_at - 1);
            seen.insert(
                pager.select(&c, &m.snapshot(), 1, event_at - 1, true).unwrap().lines()[0]
                    .as_str()
                    .to_owned(),
            );
            sample(&mut m, 150., event_at);
            assert_eq!(
                pager.select(&c, &m.snapshot(), 1, event_at, true).unwrap().lines()[0].as_str(),
                "OIL 0"
            );
            let resumed_at = boundary.max(event_at) + 1;
            let page = pager.select(&c, &m.snapshot(), 1, resumed_at, true).unwrap();
            seen.insert(page.lines()[0].as_str().to_owned());
        }
        for message in ["OIL 1", "OIL 2", "OIL 3"] {
            assert!(seen.contains(message), "{offset}: {seen:?}");
        }
    }
}

/// Maximum-length warnings invalidate both retained frames on stale, cleared and acknowledged changes.
/// Optionally writes raw RGB565 evidence to WARNING_EVIDENCE_DIR; I/O failure panics.
#[test]
fn max_message_status_changes_invalidate_both_retained_frames() {
    use dashboard_host_validation::configured_scene::Layout;
    use dashboard_host_validation::dashboard::{DashboardCache, DashboardScene};
    use dashboard_host_validation::display::St7789Renderer;
    use dashboard_host_validation::render::FpsMode;
    use dashboard_host_validation::samples::{Quality, SensorId};
    use dashboard_host_validation::vehicle_status::VehicleStatus;
    use dashboard_host_validation::warning_state::WarningCommand;
    use dashboard_host_validation::{AlignedFrame, colors};
    for mode in [WarningMode::CycleOne, WarningMode::ListThree] {
        let mut raw = config(mode, WarningScope::AllViews).config().clone();
        raw.widgets.truncate(3);
        raw.controls.screens[0].buttons[0].tap = ControlAction::AcknowledgeWarnings;
        for w in &mut raw.widgets {
            w.warning.advanced.latch = true;
            w.warning.off_ms = 1;
            w.warning.message = "ABCDEFGHIJKLMNOPQRST".try_into().unwrap();
        }
        let c = validate(raw).unwrap();
        let mut model = AcquisitionModel::demo_configured(&c);
        let mut pager = WarningPager::new();
        let mut cache = DashboardCache::new();
        let mut frames = [AlignedFrame::boxed(), AlignedFrame::boxed()];
        let mut reference = AlignedFrame::boxed();
        let mut previous = None;
        for stage in 0..4 {
            let now = 1 + stage * 10;
            match stage {
                0 => {
                    for at in [0, 1] {
                        model.ingest_batch([1., 150., 95., 80., 30., 700., 13.8, 1.], at);
                    }
                }
                1 => {
                    model.mark_fault(SensorId::Oil, Quality::TransportError, now);
                }
                2 => {
                    for at in [now - 1, now] {
                        model.ingest_batch([1., 90., 95., 80., 30., 700., 13.8, 1.], at);
                    }
                }
                _ => {
                    for at in [now - 1, now] {
                        model.ingest_batch([1., 150., 95., 80., 30., 700., 13.8, 1.], at);
                    }
                    let snap = model.snapshot();
                    model.apply_warning_command(
                        WarningCommand {
                            request_id: 1,
                            run_id: snap.warning_run_id().unwrap(),
                            observed_event_serial: snap.warning_event_serial(),
                            rules: 7,
                        },
                        now,
                    );
                }
            }
            let snapshot = model.snapshot();
            let page = pager.select(&c, &snapshot, 1, now, true).unwrap();
            assert_eq!(
                page.status(0),
                [
                    "LATCHED",
                    "LATCHED / DATA STALE",
                    "CLEARED / ACK",
                    "ACTIVE / ACKNOWLEDGED"
                ][stage as usize]
            );
            if let Some(previous) = previous {
                assert_ne!(page, previous);
            }
            previous = Some(page);
            let scene = DashboardScene {
                readings: None,
                layout: Layout::new(&c, 1, false),
                measurements: &snapshot,
                status: VehicleStatus::Demo,
                now_ms: now,
                blink_on: true,
                boost_psi: false,
                fps_mode: FpsMode::Off,
                fps_instant: 0.,
                fps_average: 0.,
                backgrounds: [colors::BLACK; 8],
                overlay: Some(Overlay::Warnings(page)),
            };
            scene.draw_full(&mut St7789Renderer::new(&mut reference.0));
            for (slot, frame) in frames.iter_mut().enumerate() {
                cache.draw(&mut St7789Renderer::new(&mut frame.0), slot, &scene);
                assert!(frame.0 == reference.0, "{mode:?} stage{stage} slot{slot}");
            }
            if let Some(dir) = std::env::var_os("WARNING_EVIDENCE_DIR") {
                std::fs::write(
                    std::path::Path::new(&dir).join(format!("{mode:?}-{stage}.rgb565")),
                    &reference.0[..],
                )
                .unwrap();
            }
        }
    }
}

/// Repeated interruptions and a long stall retain bounded rotation; a replacement acquisition run resets it.
#[test]
fn interruption_churn_and_long_stall_keep_bounded_rotation_and_run_reset() {
    let mut raw = config(WarningMode::CycleOne, WarningScope::AllViews).config().clone();
    raw.widgets.truncate(4);
    raw.controls.screens[0].buttons[0].tap = ControlAction::AcknowledgeWarnings;
    for w in &mut raw.widgets {
        w.warning.advanced.latch = true;
        w.warning.off_ms = 1;
    }
    raw.widgets[0].warning.trigger = 140.;
    raw.widgets[0].warning.clear = 138.;
    raw.widgets[0].warning.advanced.priority = Priority::Critical;
    raw.widgets[1].warning.trigger = 145.;
    raw.widgets[1].warning.clear = 143.;
    raw.widgets[1].warning.advanced.priority = Priority::High;
    let c = validate(raw).unwrap();
    let mut m = AcquisitionModel::demo_configured(&c);
    let mut p = WarningPager::new();
    let sample = |m: &mut AcquisitionModel<'_>, value, now| {
        m.ingest_batch([1., value, 95., 80., 30., 700., 13.8, 1.], now);
    };
    sample(&mut m, 150., 0);
    sample(&mut m, 150., 1);
    assert_eq!(p.select(&c, &m.snapshot(), 1, 1, true).unwrap().rules, 1);
    assert_eq!(p.select(&c, &m.snapshot(), 1, 501, true).unwrap().rules, 2);
    sample(&mut m, 135., 550);
    sample(&mut m, 135., 551);
    sample(&mut m, 142., 600);
    sample(&mut m, 142., 601);
    assert_eq!(p.select(&c, &m.snapshot(), 1, 601, true).unwrap().rules, 1);
    sample(&mut m, 150., 650);
    sample(&mut m, 150., 651);
    assert_eq!(p.select(&c, &m.snapshot(), 1, 651, true).unwrap().rules, 1);
    assert_eq!(p.select(&c, &m.snapshot(), 1, 1001, true).unwrap().rules, 4);
    m.tick(4501);
    assert_eq!(p.select(&c, &m.snapshot(), 1, 4501, true).unwrap().rules, 2);
    let mut replacement = AcquisitionModel::demo_configured(&c);
    sample(&mut replacement, 150., 4501);
    sample(&mut replacement, 150., 4502);
    assert_eq!(p.select(&c, &replacement.snapshot(), 1, 4502, true).unwrap().rules, 1);
}

/// A new warning outside the current view interrupts snooze without displaying that out-of-scope rule.
#[test]
fn out_of_view_event_interrupts_snooze_without_overriding_current_view_scope() {
    use dashboard_host_validation::warning_state::WarningCommand;
    let mut raw = config(WarningMode::CycleOne, WarningScope::CurrentView)
        .config()
        .clone();
    let other = raw.widgets[4].clone();
    raw.widgets.truncate(1);
    raw.widgets.push(other).unwrap();
    raw.widgets[1].warning.trigger = 140.;
    raw.widgets[1].warning.clear = 138.;
    raw.widgets[1].warning.advanced.priority = Priority::Critical;
    raw.widgets[0].warning.advanced.snooze_s = 5;
    raw.controls.screens[0].buttons[0].tap = ControlAction::AcknowledgeWarnings;
    let c = validate(raw).unwrap();
    let mut m = AcquisitionModel::demo_configured(&c);
    let mut p = WarningPager::new();
    for at in [0, 1] {
        m.ingest_batch([1., 135., 95., 80., 30., 700., 13.8, 1.], at);
    }
    assert_eq!(p.select(&c, &m.snapshot(), 1, 1, true).unwrap().rules, 1);
    let before = m.snapshot();
    m.apply_warning_command(
        WarningCommand {
            request_id: 1,
            run_id: before.warning_run_id().unwrap(),
            observed_event_serial: before.warning_event_serial(),
            rules: 1,
        },
        2,
    );
    assert!(p.select(&c, &m.snapshot(), 1, 2, true).is_none());
    for at in [100, 101] {
        m.ingest_batch([1., 150., 95., 80., 30., 700., 13.8, 1.], at);
    }
    let page = p.select(&c, &m.snapshot(), 1, 101, true).unwrap();
    assert_eq!(page.rules, 1);
    assert_eq!(page.lines()[0].as_str(), "OIL 0");
}
