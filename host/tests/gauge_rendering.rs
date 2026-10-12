use dashboard_host_validation::acquisition::AcquisitionModel;
use dashboard_host_validation::configured_scene::Layout;
use dashboard_host_validation::dashboard::{DashboardCache, DashboardScene};
use dashboard_host_validation::display::St7789Renderer;
use dashboard_host_validation::render::FpsMode;
use dashboard_host_validation::view_config::*;
use dashboard_host_validation::{AlignedFrame, colors};

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
