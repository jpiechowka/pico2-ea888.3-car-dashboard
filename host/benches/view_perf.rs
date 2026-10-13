use std::hint::black_box;

use criterion::{BatchSize, Criterion};
use dashboard_host_validation::acquisition::AcquisitionModel;
use dashboard_host_validation::configured_scene::Layout;
use dashboard_host_validation::dashboard::{DashboardCache, DashboardScene};
use dashboard_host_validation::display::St7789Renderer;
use dashboard_host_validation::presentation::PresentationState;
use dashboard_host_validation::render::FpsMode;
use dashboard_host_validation::view_config::*;
use dashboard_host_validation::{AlignedFrame, colors};

pub fn benchmarks(c: &mut Criterion) {
    advanced(c);
    graph_retention(c);
    for kind in GaugeKind::ALL {
        for wide in [false, true] {
            let mut config = default_config();
            if wide {
                config.widgets.clear();
                for (slot, channel) in [Channel::Lambda, Channel::Egt].into_iter().enumerate() {
                    let mut widget = default_widget(
                        channel,
                        slot as u16 + 1,
                        1,
                        Placement {
                            column: slot as u8 * 2,
                            row: 0,
                            span: Span::TwoByTwo,
                        },
                    );
                    widget.display.average = true;
                    widget.display.extrema = true;
                    config.widgets.push(widget).unwrap();
                }
            }
            for widget in &mut config.widgets {
                widget.gauge.kind = kind;
                widget.gauge.expand = !kind.is_numeric();
            }
            let config = validate(config).unwrap();
            let mut model = AcquisitionModel::demo_configured(&config);
            for n in 0..60 {
                let wave = (n % 8) as f32;
                model.ingest_batch(
                    [
                        1.0 + wave / 10.0,
                        80.0 + wave,
                        90.0,
                        75.0,
                        30.0,
                        700.0 + wave * 10.0,
                        13.8,
                        0.8,
                    ],
                    n * 2000,
                );
            }
            let snapshot = model.snapshot();
            let scene = DashboardScene {
                readings: None,
                layout: Layout::new(&config, 1, false),
                measurements: &snapshot,
                now_ms: 118000,
                status: dashboard_host_validation::vehicle_status::VehicleStatus::Demo,
                blink_on: false,
                boost_psi: false,
                fps_mode: FpsMode::Off,
                fps_instant: 0.0,
                fps_average: 0.0,
                backgrounds: [colors::BLACK; 8],
                overlay: None,
            };
            let mut reference = AlignedFrame::boxed();
            let mut frames = [AlignedFrame::boxed(), AlignedFrame::boxed()];
            let mut cache = DashboardCache::new();
            scene.draw_full(&mut St7789Renderer::new(&mut reference.0));
            for (slot, frame) in frames.iter_mut().enumerate() {
                cache.draw(&mut St7789Renderer::new(&mut frame.0), slot, &scene);
                assert!(frame.0 == reference.0);
            }
            let base = if wide { "wide" } else { "default" };
            let name = if kind.is_numeric() {
                base.to_string()
            } else {
                format!("{kind:?}_{base}")
            };
            let mut presentation = PresentationState::new();
            c.bench_function(&format!("views/{name}_full"), |b| {
                b.iter(|| {
                    let readings = presentation.prepare(&config, black_box(&snapshot), 1, 118000, false);
                    let scene = DashboardScene {
                        readings: Some(&readings),
                        ..scene
                    };
                    black_box(&scene).draw_full(&mut St7789Renderer::new(&mut frames[0].0));
                    black_box(&frames[0].0);
                })
            });
            let mut slot = 0;
            c.bench_function(&format!("views/{name}_retained_stationary"), |b| {
                b.iter(|| {
                    slot ^= 1;
                    let readings = presentation.prepare(&config, black_box(&snapshot), 1, 118000, false);
                    let scene = DashboardScene {
                        readings: Some(&readings),
                        ..scene
                    };
                    black_box(cache.draw(&mut St7789Renderer::new(&mut frames[slot].0), slot, black_box(&scene)));
                    black_box(&frames[slot].0);
                })
            });

            if !kind.is_numeric() {
                model.ingest_batch([1.81, 88., 96., 83., 39., 801., 13.7, 0.91], 118001);
                let changed_snapshot = model.snapshot();
                let changed = DashboardScene {
                    readings: None,
                    measurements: &changed_snapshot,
                    now_ms: 118001,
                    ..scene
                };
                // Steady changed input alternates on every use of each physical buffer.
                let mut iteration = 0usize;
                c.bench_function(&format!("views/{name}_retained_moving"), |b| {
                    b.iter(|| {
                        let slot = iteration & 1;
                        let next = if iteration & 2 == 0 { &scene } else { &changed };
                        iteration = iteration.wrapping_add(1);
                        let readings =
                            presentation.prepare(&config, black_box(next.measurements), 1, next.now_ms, false);
                        let next = DashboardScene {
                            readings: Some(&readings),
                            ..*next
                        };
                        black_box(cache.draw(&mut St7789Renderer::new(&mut frames[slot].0), slot, black_box(&next)));
                        black_box(&frames[slot].0);
                    })
                });
            }
        }
    }
}

// The complete timed path prepares eight filters once, uses the same readings
// for keys and drawing, then computes physical-buffer damage. Setup is excluded.
fn advanced(c: &mut Criterion) {
    use dashboard_host_validation::display::damage::DamagePlan;
    for kind in [GaugeKind::Circular, GaugeKind::Horizontal] {
        let mut raw = default_config();
        for w in &mut raw.widgets {
            w.gauge.kind = kind;
            w.gauge.expand = false;
            w.display.decimals = 0;
            w.display.extrema = false;
            w.display.average = false;
            w.display.graph = false;
            w.display.trend = false;
            w.advanced.smoothing_ms = 5000;
            w.advanced.peak_mode = PeakSelection::Off;
            let (lo, hi) = (w.gauge.min, w.gauge.max);
            let one = lo + (hi - lo) / 3.;
            let two = lo + (hi - lo) * 2. / 3.;
            w.advanced
                .set_bands(&[
                    Band::new(lo, one, Color(0x07E0), "LOW").unwrap(),
                    Band::new(one, two, Color(0x07FF), "MID").unwrap(),
                    Band::new(two, hi, Color(0xF81F), "HIGH").unwrap(),
                ])
                .unwrap();
        }
        let config = validate(raw).unwrap();
        let initial = [1., 80., 90., 75., 30., 700., 13.8, 0.9];
        let mut model = AcquisitionModel::demo_configured(&config);
        model.ingest_batch(initial, 0);
        let before = model.snapshot();
        model.ingest_batch(initial.map(|v| v + 0.001), 100);
        let after = model.snapshot();
        let make_scene = |snapshot, now_ms| {
            DashboardScene {
                readings: None,
                layout: Layout::new(&config, 1, false),
                measurements: snapshot,
                now_ms,
                status: dashboard_host_validation::vehicle_status::VehicleStatus::Demo,
                blink_on: false,
                boost_psi: false,
                fps_mode: FpsMode::Off,
                fps_instant: 0.,
                fps_average: 0.,
                backgrounds: [colors::BLACK; 8],
                overlay: None,
            }
        };
        let ready = || {
            let mut state = PresentationState::new();
            let readings = state.prepare(&config, &before, 1, 0, false);
            let scene = DashboardScene {
                readings: Some(&readings),
                ..make_scene(&before, 0)
            };
            let mut frames = [AlignedFrame::boxed(), AlignedFrame::boxed()];
            let mut cache = DashboardCache::new();
            for (slot, frame) in frames.iter_mut().enumerate() {
                cache.draw(&mut St7789Renderer::new(&mut frame.0), slot, &scene);
            }
            (state, cache, frames)
        };
        let mut checked = ready();
        let readings = checked.0.prepare(&config, &after, 1, 100, false);
        let scene = DashboardScene {
            readings: Some(&readings),
            ..make_scene(&after, 100)
        };
        let mut full = AlignedFrame::boxed();
        scene.draw_full(&mut St7789Renderer::new(&mut full.0));
        assert!(full.0 == checked.2[0].0, "subpixel fixture must preserve every pixel");
        c.bench_function(&format!("advanced/{kind:?}_eight_smoothed_three_bands_subpixel"), |b| {
            b.iter_batched_ref(
                ready,
                |(state, cache, frames)| {
                    let readings = state.prepare(&config, black_box(&after), 1, 100, false);
                    let scene = DashboardScene {
                        readings: Some(&readings),
                        ..make_scene(&after, 100)
                    };
                    for (slot, frame) in frames.iter_mut().enumerate() {
                        black_box(cache.draw(&mut St7789Renderer::new(&mut frame.0), slot, &scene));
                    }
                    black_box(DamagePlan::between(&frames[0].0, &frames[1].0));
                },
                BatchSize::PerIteration,
            )
        });
    }
}

fn graph_retention(c: &mut Criterion) {
    let config = validate(default_config()).unwrap();
    let reconstruct = || {
        let mut model = AcquisitionModel::demo_configured(&config);
        for n in 0..60 {
            assert!(model.ingest_batch([1., 80., 90., 75., 30., 700., 13.8, 0.9], n * 2000));
        }
        model.snapshot()
    };
    let snapshot = reconstruct();
    let mut frames = [AlignedFrame::boxed(), AlignedFrame::boxed()];
    let mut cache = DashboardCache::new();
    let mut state = PresentationState::new();
    let mut slot = 0;
    for rebuilt in [false, true] {
        c.bench_function(
            if rebuilt {
                "advanced/graph_reconstructed_preview"
            } else {
                "advanced/graph_retained_static"
            },
            |b| {
                b.iter(|| {
                    let rebuilt_snapshot;
                    let snapshot = if rebuilt {
                        rebuilt_snapshot = reconstruct();
                        &rebuilt_snapshot
                    } else {
                        &snapshot
                    };
                    let readings = state.prepare_appearance(&config, black_box(snapshot), 1, 118000, false);
                    let scene = DashboardScene {
                        readings: Some(&readings),
                        layout: Layout::new(&config, 1, false),
                        measurements: snapshot,
                        now_ms: 118000,
                        status: dashboard_host_validation::vehicle_status::VehicleStatus::Demo,
                        blink_on: false,
                        boost_psi: false,
                        fps_mode: FpsMode::Off,
                        fps_instant: 0.,
                        fps_average: 0.,
                        backgrounds: [colors::BLACK; 8],
                        overlay: None,
                    };
                    slot ^= 1;
                    black_box(cache.draw(&mut St7789Renderer::new(&mut frames[slot].0), slot, &scene));
                    black_box(&frames[slot].0);
                })
            },
        );
    }
}
