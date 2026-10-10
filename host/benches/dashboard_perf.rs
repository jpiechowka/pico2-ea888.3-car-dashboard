use std::{hint::black_box, time::Duration};

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use dashboard_host_validation::{
    AlignedFrame, FRAME_BYTES, primitives, renderer::St7789Renderer, sensor_state::SensorState,
    widgets,
};
use embedded_graphics::{pixelcolor::Rgb565, prelude::*, primitives::Rectangle};

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
    let trace = [0.2f32, 0.8, 1.1, 2.0, 0.4, -0.3, 1.5, 0.7];
    c.bench_function("sensor_state/next_render_update", |b| {
        b.iter(|| {
            state.update(black_box(trace[sample_index % trace.len()]), false);
            sample_index = sample_index.wrapping_add(1);
            black_box(state.get_average());
        });
    });
}

criterion_group! {
    name = dashboard_perf;
    config = Criterion::default()
        .sample_size(30)
        .warm_up_time(Duration::from_secs(1))
        .measurement_time(Duration::from_secs(2));
    targets = framebuffer, widgets_and_history
}
criterion_main!(dashboard_perf);
