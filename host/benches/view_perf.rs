use std::hint::black_box;

use criterion::Criterion;
use dashboard_host_validation::acquisition::AcquisitionModel;
use dashboard_host_validation::configured_scene::Layout;
use dashboard_host_validation::dashboard::{DashboardCache, DashboardScene};
use dashboard_host_validation::display::St7789Renderer;
use dashboard_host_validation::render::FpsMode;
use dashboard_host_validation::view_config::*;
use dashboard_host_validation::{AlignedFrame, colors};

pub fn benchmarks(c: &mut Criterion) {
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
            let mut model = AcquisitionModel::demo();
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
            c.bench_function(&format!("views/{name}_full"), |b| {
                b.iter(|| {
                    black_box(&scene).draw_full(&mut St7789Renderer::new(&mut frames[0].0));
                    black_box(&frames[0].0);
                })
            });
            let mut slot = 0;
            c.bench_function(&format!("views/{name}_retained_stationary"), |b| {
                b.iter(|| {
                    slot ^= 1;
                    black_box(cache.draw(&mut St7789Renderer::new(&mut frames[slot].0), slot, black_box(&scene)));
                    black_box(&frames[slot].0);
                })
            });

            if !kind.is_numeric() {
                model.ingest_batch([1.81, 88., 96., 83., 39., 801., 13.7, 0.91], 118001);
                let changed_snapshot = model.snapshot();
                let changed = DashboardScene {
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
                        black_box(cache.draw(&mut St7789Renderer::new(&mut frames[slot].0), slot, black_box(next)));
                        black_box(&frames[slot].0);
                    })
                });
            }
        }
    }
}
