use dashboard_host_validation::acquisition::AcquisitionModel;
use dashboard_host_validation::configured_scene::Layout;
use dashboard_host_validation::dashboard::{DashboardCache, DashboardScene};
use dashboard_host_validation::display::St7789Renderer;
use dashboard_host_validation::render::FpsMode;
use dashboard_host_validation::view_config::*;
use dashboard_host_validation::{AlignedFrame, colors};

#[test]
fn widget_background_palette_is_rendered_and_invalidated_on_both_buffers() {
    let config = validate(default_config()).unwrap();
    let model = AcquisitionModel::demo();
    let snapshot = model.snapshot();
    let mut frames = [AlignedFrame::boxed(), AlignedFrame::boxed()];
    let mut cache = DashboardCache::new();
    let mut raw = config.config().clone();
    raw.widgets[0].colors.background = Color::rgb(0, 16, 32);
    let custom = validate(raw).unwrap();
    for config in [&config, &custom] {
        let scene = scene(config, &snapshot, 0);
        let mut full = AlignedFrame::boxed();
        scene.draw_full(&mut St7789Renderer::new(&mut full.0));
        // Blank interior pixel is independent of text and of unavailable status.
        let pixel = (130 * 320 + 3) * 2;
        assert_eq!(
            &full.0[pixel..pixel + 2],
            &config.config().widgets[0].colors.background.0.to_be_bytes()
        );
        for (i, frame) in frames.iter_mut().enumerate() {
            cache.draw(&mut St7789Renderer::new(&mut frame.0), i, &scene);
            assert!(full.0 == frame.0);
        }
    }
}

#[test]
fn indicator_level_has_independent_endpoint_and_failure_oracles() {
    use dashboard_host_validation::widgets::gauge::Reading;
    for (value, expected) in [
        (-100.0, 0),
        (-1.0, 0),
        (0.0, 256),
        (1.0, 512),
        (3.0, 1024),
        (1_000_000.0, 1024),
    ] {
        assert_eq!(
            Reading {
                min: -1.0,
                max: 3.0,
                value: Some(value)
            }
            .level(),
            Some(expected)
        );
    }
    for value in [None, Some(f32::NAN), Some(f32::INFINITY)] {
        assert_eq!(
            Reading {
                min: 0.0,
                max: 1.0,
                value
            }
            .level(),
            None
        );
    }
    for (min, max) in [(0.0, 0.0), (1.0, 0.0), (f32::NAN, 1.0), (0.0, f32::INFINITY)] {
        assert_eq!(
            Reading {
                min,
                max,
                value: Some(0.0)
            }
            .level(),
            None
        );
    }
}

#[test]
fn gauge_matrix_fits_every_channel_span_and_value_and_matches_retained_frames() {
    for channel in Channel::ALL {
        for span in [Span::OneByOne, Span::TwoByOne, Span::TwoByTwo] {
            for kind in GaugeKind::ALL.into_iter().filter(|kind| !kind.is_numeric()) {
                let mut raw = default_config();
                raw.widgets.clear();
                let mut widget = default_widget(
                    channel,
                    1,
                    1,
                    Placement {
                        column: 2,
                        row: 0,
                        span,
                    },
                );
                widget.gauge.kind = kind;
                widget.gauge.expand = true;
                widget.display.average = true;
                widget.display.extrema = true;
                widget.display.trend = true;
                widget.display.decimals = 3;
                let min = widget.gauge.min;
                let max = widget.gauge.max;
                let rect = widget.placement.rect().unwrap();
                raw.widgets.push(widget).unwrap();
                let config = validate(raw).unwrap();
                let mut model = AcquisitionModel::demo();
                let mut cache = DashboardCache::new();
                let mut frames = [AlignedFrame::boxed(), AlignedFrame::boxed()];
                for (at, value) in [-1_000_000.0, min, (min + max) / 2.0, max, 1_000_000.0]
                    .into_iter()
                    .enumerate()
                {
                    model.ingest_batch([value; 8], at as u64);
                    let snapshot = model.snapshot();
                    for stale in [false, true] {
                        let mut scene = scene(&config, &snapshot, if stale { 4000 + at as u64 } else { at as u64 });
                        scene.boost_psi = at % 2 == 0;
                        let mut full = AlignedFrame::boxed();
                        scene.draw_full(&mut St7789Renderer::new(&mut full.0));
                        for (i, frame) in frames.iter_mut().enumerate() {
                            cache.draw(&mut St7789Renderer::new(&mut frame.0), i, &scene);
                            assert!(full.0 == frame.0, "{channel:?}/{span:?}/{kind:?}/{value}/{stale}");
                        }
                        let mut owned = AlignedFrame::boxed();
                        owned.0.fill(0x5A);
                        scene.draw_cell(&mut St7789Renderer::new(&mut owned.0), 0);
                        for row in 0..240usize {
                            let start = row * 640;
                            let bytes = &owned.0[start..start + 640];
                            if (usize::from(rect.y)..usize::from(rect.y + rect.height)).contains(&row) {
                                assert!(bytes[..usize::from(rect.x) * 2].iter().all(|b| *b == 0x5A));
                                assert!(bytes[usize::from(rect.x + rect.width) * 2..].iter().all(|b| *b == 0x5A));
                            } else {
                                assert!(bytes.iter().all(|b| *b == 0x5A));
                            }
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn hidden_statistics_and_trend_do_not_dirty_a_compact_gauge() {
    let mut raw = default_config();
    raw.widgets.clear();
    let mut widget = default_widget(
        Channel::Oil,
        1,
        1,
        Placement {
            column: 0,
            row: 0,
            span: Span::OneByOne,
        },
    );
    widget.gauge.kind = GaugeKind::Vertical;
    raw.widgets.push(widget).unwrap();
    let config = validate(raw).unwrap();
    let mut model = AcquisitionModel::demo();
    model.ingest_batch([80.0; 8], 0);
    model.ingest_batch([90.0; 8], 1);
    let snapshot = model.snapshot();
    let mut cache = DashboardCache::new();
    let first = scene(&config, &snapshot, 1);
    cache.prepare(0, &first);
    cache.prepare(1, &first);
    for now in [751, 2002] {
        let later = scene(&config, &snapshot, now);
        for buffer in 0..2 {
            assert_eq!(cache.prepare(buffer, &later).cells, 0);
        }
    }
}

#[test]
fn unavailable_gauge_unit_toggle_and_reset_repair_both_buffers() {
    let mut raw = default_config();
    raw.widgets.truncate(1);
    raw.widgets[0].gauge.kind = GaugeKind::Circular;
    raw.widgets[0].gauge.expand = true;
    let config = validate(raw).unwrap();
    let mut model = AcquisitionModel::demo();
    model.ingest_batch([5.0; 8], 0);
    model.ingest_batch([1.0; 8], 1);
    let mut cache = DashboardCache::new();
    let mut frames = [AlignedFrame::boxed(), AlignedFrame::boxed()];
    for (psi, reset) in [(false, false), (true, false), (true, true), (false, false)] {
        if reset {
            model.reset_statistics(1);
        }
        let snapshot = model.snapshot();
        let mut scene = scene(&config, &snapshot, 4000);
        scene.boost_psi = psi;
        let mut full = AlignedFrame::boxed();
        scene.draw_full(&mut St7789Renderer::new(&mut full.0));
        for (i, frame) in frames.iter_mut().enumerate() {
            cache.draw(&mut St7789Renderer::new(&mut frame.0), i, &scene);
            assert!(full.0 == frame.0, "stale psi={psi} reset={reset}");
        }
    }
}

fn scene<'a>(
    config: &'a ValidatedConfig,
    snapshot: &'a dashboard_host_validation::acquisition::DashboardSnapshot,
    now: u64,
) -> DashboardScene<'a> {
    DashboardScene {
        layout: Layout::new(config, 1, false),
        measurements: snapshot,
        status: dashboard_host_validation::vehicle_status::VehicleStatus::Demo,
        now_ms: now,
        blink_on: false,
        boost_psi: false,
        fps_mode: FpsMode::Off,
        fps_instant: 0.0,
        fps_average: 0.0,
        backgrounds: [colors::BLACK; 8],
        overlay: None,
    }
}

#[test]
fn every_gauge_draws_a_distinct_indicator_and_repairs_both_buffers() {
    let mut model = AcquisitionModel::demo();
    model.ingest_batch([1.1, 80.0, 90.0, 75.0, 30.0, 700.0, 13.8, 0.9], 0);
    let snapshot = model.snapshot();
    let mut pictures = Vec::new();
    let mut cache = DashboardCache::new();
    let mut buffers = [AlignedFrame::boxed(), AlignedFrame::boxed()];
    for kind in GaugeKind::ALL {
        let mut raw = default_config();
        raw.widgets.truncate(1);
        raw.widgets[0].gauge.kind = kind;
        let config = validate(raw).unwrap();
        let scene = scene(&config, &snapshot, 0);
        let mut full = AlignedFrame::boxed();
        scene.draw_full(&mut St7789Renderer::new(&mut full.0));
        for (i, frame) in buffers.iter_mut().enumerate() {
            cache.draw(&mut St7789Renderer::new(&mut frame.0), i, &scene);
            assert!(frame.0 == full.0);
        }
        assert!(
            pictures.iter().all(|p: &Vec<u8>| *p != full.0.to_vec()),
            "style {kind:?} is indistinguishable"
        );
        pictures.push(full.0.to_vec());
    }
}

#[test]
fn needle_changes_below_numeric_rounding_and_when_only_scale_changes() {
    let mut raw = default_config();
    raw.widgets.truncate(1);
    raw.widgets[0].display.decimals = 0;
    raw.widgets[0].display.extrema = false;
    raw.widgets[0].gauge = Gauge {
        kind: GaugeKind::Dial,
        min: 0.0,
        max: 1.0,
        expand: true,
    };
    let config = validate(raw).unwrap();
    let mut model = AcquisitionModel::demo();
    let mut cache = DashboardCache::new();
    let mut buffers = [AlignedFrame::boxed(), AlignedFrame::boxed()];
    for (now, value) in [(0, 0.11), (1, 0.12), (2, 3.0), (3, 0.12)] {
        model.ingest_batch([value, 80.0, 90.0, 75.0, 30.0, 700.0, 13.8, 0.9], now);
        let snapshot = model.snapshot();
        let scene = scene(&config, &snapshot, now);
        let mut full = AlignedFrame::boxed();
        scene.draw_full(&mut St7789Renderer::new(&mut full.0));
        for (i, frame) in buffers.iter_mut().enumerate() {
            cache.draw(&mut St7789Renderer::new(&mut frame.0), i, &scene);
            assert!(frame.0 == full.0);
        }
    }
}

#[test]
fn vehicle_gauges_expand_only_on_accepted_samples_and_reset_with_statistics() {
    use dashboard_host_validation::samples::{DataMode, Quality, Quantity, Sample, SensorId, Source};
    use dashboard_host_validation::widgets::gauge::Reading;
    let mut raw = default_config();
    raw.widgets.clear();
    let mut w = default_widget(
        Channel::Water,
        1,
        1,
        Placement {
            column: 0,
            row: 0,
            span: Span::TwoByTwo,
        },
    );
    w.gauge = Gauge {
        kind: GaugeKind::Circular,
        min: 60.,
        max: 110.,
        expand: true,
    };
    w.colors.background = Color::rgb(0, 16, 32);
    raw.widgets.push(w).unwrap();
    let config = validate(raw).unwrap();
    let w = &config.config().widgets[0];
    let mut model = AcquisitionModel::configured(&config, 0x7E8);
    let mut cache = DashboardCache::new();
    let mut buffers = [AlignedFrame::boxed(), AlignedFrame::boxed()];
    for (sequence, value) in [90., 120., 95., f32::NAN, 100.].into_iter().enumerate() {
        let now = sequence as u64 * 100;
        let sample = Sample {
            value: Quantity::CoolantC(value),
            sequence: sequence as u64 + 1,
            received_at_ms: now,
            source: Source::Obd { ecu: 0x7E8, pid: 5 },
        };
        assert_eq!(model.ingest_sample(sample, now).is_ok(), value.is_finite());
        let snapshot = model.snapshot();
        assert_eq!(snapshot.mode, DataMode::Vehicle);
        let reading = Reading::new(w, &snapshot, now);
        assert_eq!(
            (reading.min, reading.max),
            (60., if sequence == 0 { 110. } else { 120. })
        );
        if !value.is_finite() {
            assert!(reading.value.is_none());
        }
        let mut scene = scene(&config, &snapshot, now);
        scene.status = dashboard_host_validation::vehicle_status::VehicleStatus::Data;
        let mut full = AlignedFrame::boxed();
        scene.draw_full(&mut St7789Renderer::new(&mut full.0));
        for (i, frame) in buffers.iter_mut().enumerate() {
            cache.draw(&mut St7789Renderer::new(&mut frame.0), i, &scene);
            assert!(full.0 == frame.0);
        }
    }
    assert!(model.mark_fault(SensorId::Water, Quality::TransportError, 501));
    assert_eq!(Reading::new(w, &model.snapshot(), 501).value, None);
    assert!(model.reset_statistics(1));
    assert!(!model.reset_statistics(1));
    let reading = Reading::new(w, &model.snapshot(), 501);
    assert_eq!((reading.min, reading.max), (60., 110.));
    assert_eq!(reading.value, None);
}

#[test]
fn public_gauge_draw_caps_geometry_before_iteration_and_handles_tiny_regions() {
    use dashboard_host_validation::widgets::gauge::{Reading, draw};
    use embedded_graphics::prelude::*;
    use embedded_graphics::primitives::Rectangle;
    for kind in GaugeKind::ALL {
        for (point, size) in [
            (Point::new(i32::MAX, i32::MAX), Size::new(u32::MAX, u32::MAX)),
            (Point::zero(), Size::zero()),
            (Point::zero(), Size::new(19, 27)),
            (Point::new(10, 30), Size::new(u32::MAX, u32::MAX)),
        ] {
            let mut frame = AlignedFrame::boxed();
            draw(
                &mut St7789Renderer::new(&mut frame.0),
                kind,
                Rectangle::new(point, size),
                Reading {
                    min: -1.,
                    max: 3.,
                    value: Some(1.),
                },
                "-1000000.000",
                false,
                (colors::WHITE, colors::RED, colors::GRAY, colors::WHITE),
            );
            if size.width < 20 || point.x == i32::MAX {
                assert!(frame.0.iter().all(|b| *b == 0));
            } else {
                for y in 0..240 {
                    for x in 0..320 {
                        if !(10..166).contains(&x) || !(30..228).contains(&y) {
                            assert_eq!(&frame.0[(y * 320 + x) * 2..(y * 320 + x) * 2 + 2], &[0, 0]);
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn each_active_palette_role_has_its_own_pixels_without_leaking_from_the_cell() {
    for role in ColorRole::ALL
        .into_iter()
        .filter(|r| !matches!(r, ColorRole::WarningForeground | ColorRole::WarningBackground))
    {
        let mut raw = default_config();
        raw.widgets.clear();
        let mut w = default_widget(
            if role == ColorRole::Peak {
                Channel::Oil
            } else {
                Channel::Boost
            },
            1,
            1,
            Placement {
                column: 0,
                row: 0,
                span: Span::TwoByTwo,
            },
        );
        w.display.graph = true;
        w.display.trend = true;
        w.display.average = true;
        w.display.extrema = true;
        if matches!(role, ColorRole::Gauge | ColorRole::Track | ColorRole::Ticks) {
            w.gauge.kind = GaugeKind::Circular;
        }
        let colour = if matches!(role, ColorRole::Background | ColorRole::Track) {
            Color::rgb(0, 0, 96)
        } else {
            Color(0x07E0)
        };
        role.set(&mut w.colors, colour);
        raw.widgets.push(w).unwrap();
        let config = validate(raw).unwrap();
        let mut model = AcquisitionModel::demo();
        model.ingest_batch([0.5; 8], 0);
        model.ingest_batch([2.; 8], 2000);
        let now = if role == ColorRole::Peak {
            2001
        } else {
            model.ingest_batch([1.5; 8], 4000);
            4100
        };
        let snapshot = model.snapshot();
        let mut frame = AlignedFrame::boxed();
        scene(&config, &snapshot, now).draw_cell(&mut St7789Renderer::new(&mut frame.0), 0);
        assert!(
            frame.0.as_chunks::<2>().0.iter().any(|p| *p == colour.0.to_be_bytes()),
            "missing {role:?}"
        );
        assert!(
            frame.0[26 * 640..]
                .as_chunks::<640>()
                .0
                .iter()
                .all(|row| row[320..].iter().all(|b| *b == 0))
        );
    }
}

#[test]
fn warning_foreground_only_phase_and_overlay_palette_change_repaint_both_buffers() {
    use dashboard_host_validation::dashboard::Overlay;
    use dashboard_host_validation::view_state::WarningPager;
    let mut raw = default_config();
    raw.widgets.clear();
    raw.warnings.mode = WarningMode::ListThree;
    let mut w = default_widget(
        Channel::Oil,
        1,
        1,
        Placement {
            column: 0,
            row: 0,
            span: Span::TwoByTwo,
        },
    );
    for role in ColorRole::ALL {
        role.set(&mut w.colors, Color(65535));
    }
    w.colors.background = Color(0xF800);
    w.colors.track = Color(0);
    w.colors.warning = WarningColors {
        foreground: Color(0),
        background: Color(0xF800),
    };
    w.warning.on_ms = 1;
    raw.widgets.push(w).unwrap();
    let config = validate(raw).unwrap();
    let mut changed = config.config().clone();
    changed.warning_colors.background = Color::rgb(0, 0, 96);
    let changed = validate(changed).unwrap();
    let mut model = AcquisitionModel::demo_configured(&config);
    for at in [0, 1] {
        model.ingest_batch([150.; 8], at);
    }
    let snapshot = model.snapshot();
    let mut cache = DashboardCache::new();
    let mut frames = [AlignedFrame::boxed(), AlignedFrame::boxed()];
    let mut previous = None;
    for (config, now, blink, popup) in [
        (&config, 1, false, false),
        (&config, 2, true, false),
        (&config, 3, true, true),
        (&changed, 4, true, true),
        (&changed, 5, false, false),
        (&changed, 4000, true, false),
    ] {
        let mut scene = scene(config, &snapshot, now);
        scene.blink_on = blink;
        if popup {
            scene.overlay = WarningPager::new()
                .select(config, &snapshot, 1, now, blink)
                .map(Overlay::Warnings);
        }
        let mut full = AlignedFrame::boxed();
        scene.draw_full(&mut St7789Renderer::new(&mut full.0));
        if let Some(old) = previous {
            assert!(
                old != full.0.to_vec(),
                "visible transition did not change pixels at {now}"
            );
        }
        previous = Some(full.0.to_vec());
        for (i, frame) in frames.iter_mut().enumerate() {
            cache.draw(&mut St7789Renderer::new(&mut frame.0), i, &scene);
            assert!(frame.0 == full.0);
        }
    }
}
