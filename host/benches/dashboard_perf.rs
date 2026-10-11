use std::hint::black_box;
use std::time::Duration;

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use dashboard_host_validation::acquisition::{AcquisitionModel, DashboardSnapshot};
use dashboard_host_validation::display::damage::DamagePlan;
use dashboard_host_validation::renderer::St7789Renderer;
use dashboard_host_validation::sensor_state::SensorState;
use dashboard_host_validation::{AlignedFrame, FRAME_BYTES, primitives, widgets};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::watch::Watch;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::Rectangle;

fn framebuffer(c: &mut Criterion) {
    let mut frame = AlignedFrame::boxed();
    let mut group = c.benchmark_group("framebuffer");
    group.throughput(Throughput::Bytes(FRAME_BYTES as u64));
    group.bench_function("clear_full", |b| {
        b.iter(|| {
            St7789Renderer::new(&mut frame.0).clear_buffer(black_box(Rgb565::BLUE));
            black_box(&frame.0);
        });
    });
    group.finish();

    let rect = Rectangle::new(Point::new(-8, 40), Size::new(128, 80));
    let mut group = c.benchmark_group("clipped_contiguous");
    group.throughput(Throughput::Elements(128 * 80));
    group.bench_function("left_8_px", |b| {
        b.iter(|| {
            let colors = (0..128 * 80).map(|i| {
                if black_box(i) & 1 == 0 {
                    Rgb565::RED
                } else {
                    Rgb565::BLUE
                }
            });
            St7789Renderer::new(&mut frame.0)
                .fill_contiguous(black_box(&rect), colors)
                .unwrap();
            black_box(&frame.0);
        });
    });
    group.finish();
}

fn widgets_and_history(c: &mut Criterion) {
    let mut frame = AlignedFrame::boxed();
    c.bench_function("boost_cell/psi_peak", |b| {
        b.iter(|| {
            widgets::draw_boost_cell(
                &mut St7789Renderer::new(&mut frame.0),
                0,
                26,
                80,
                107,
                black_box(1.0),
                black_box(2.0),
                true,
                false,
                true,
                0,
            );
            black_box(&frame.0);
        });
    });

    let mut samples = [0.0f32; 60];
    for (index, sample) in samples.iter_mut().enumerate() {
        *sample = (index as f32 * 0.37) % 2.5;
    }
    samples[59] = 3.0;
    c.bench_function("mini_graph/60_samples_newest_impulse", |b| {
        b.iter(|| {
            primitives::draw_mini_graph(
                &mut St7789Renderer::new(&mut frame.0),
                0,
                0,
                64,
                20,
                black_box(&samples),
                60,
                0,
                60,
                0.0,
                3.0,
                |_| Rgb565::WHITE,
            );
            black_box(&frame.0);
        });
    });

    let mut state = SensorState::new();
    let mut sample_index = 0usize;
    let mut sample_clock_ms = 0u64;
    let trace = [0.2f32, 0.8, 1.1, 2.0, 0.4, -0.3, 1.5, 0.7];
    c.bench_function("sensor_state/next_measurement_update", |b| {
        b.iter(|| {
            state.update_at(black_box(trace[sample_index % trace.len()]), false, sample_clock_ms);
            sample_index = sample_index.wrapping_add(1);
            sample_clock_ms = sample_clock_ms.wrapping_add(16);
            black_box(state.get_average());
        });
    });
}

fn acquisition(c: &mut Criterion) {
    let mut model = AcquisitionModel::new();
    let mut at = 0u64;
    c.bench_function("acquisition/batch_and_snapshot", |b| {
        b.iter(|| {
            model.ingest_batch(black_box([1.5, 90.0, 95.0, 80.0, 30.0, 750.0, 13.8, 14.7]), at);
            at += 10;
            black_box(model.snapshot());
        });
    });
    let watch = Watch::<CriticalSectionRawMutex, DashboardSnapshot, 1>::new();
    let mut receiver = watch.receiver().unwrap();
    let snapshot = model.snapshot();
    c.bench_function("acquisition/watch_copy_roundtrip", |b| {
        b.iter(|| {
            watch.sender().send(black_box(snapshot));
            black_box(receiver.try_changed().unwrap());
        });
    });
}

fn damage_planning(c: &mut Criterion) {
    let previous = AlignedFrame::boxed();
    let mut current = AlignedFrame::boxed();
    for (label, mode) in [("stationary", 0), ("one_cell", 1), ("dense", 2)] {
        current.0.fill(0);
        if mode == 1 {
            for y in 32..128 {
                for x in 160..240 {
                    current.0[(y * 320 + x) * 2] = 0xF8;
                }
            }
        } else if mode == 2 {
            current.0.fill(0xFF);
        }
        c.bench_function(&format!("damage/{label}"), |b| {
            b.iter(|| black_box(DamagePlan::between(black_box(&previous.0), black_box(&current.0))));
        });
    }
}

criterion_group! {
    name = dashboard_perf;
    config = Criterion::default()
        .sample_size(30)
        .warm_up_time(Duration::from_secs(1))
        .measurement_time(Duration::from_secs(2));
    targets = framebuffer, widgets_and_history, acquisition, damage_planning
}
criterion_main!(dashboard_perf);
