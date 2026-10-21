//! Fixed inputs and acceptance checks shared by benchmarks and host tests.
/// Deterministic terminal and replay workloads for diagnostic acquisition.
pub mod diagnostics;
/// Fill primitives verified against an independent coordinate oracle.
pub mod fills;
/// Retained scenes and panel damage compared to complete pixels.
pub mod scenes;

/// Registers all verified fill/scene/diagnostic/history workloads with Criterion `c`; failed prechecks panic.
pub fn benchmarks(c: &mut criterion::Criterion) {
    fills::benchmarks(c);
    scenes::benchmarks(c);
    diagnostics::benchmarks(c);
    history(c);
}

/// Registers constant-history rendering and unchanged-tick workloads with `c` after exact-output verification.
fn history(c: &mut criterion::Criterion) {
    verify_history();
    use std::hint::black_box;

    use dashboard_host_validation::display::{FrameBuffer, St7789Renderer};
    use dashboard_host_validation::sensor_state::SensorState;
    use embedded_graphics::pixelcolor::Rgb565;
    use embedded_graphics::prelude::RgbColor;
    let mut state = SensorState::new();
    for i in 0..60 {
        assert!(state.update_at(black_box(2.0), false, i * 5000));
    }
    let before = state.snapshot();
    state.tick(295001);
    assert_eq!(state.snapshot(), before);
    c.bench_function("sensor_state/no_new_sample_tick_snapshot", |b| {
        b.iter(|| {
            black_box(&mut state).tick(black_box(295001));
            black_box(state.snapshot());
        })
    });
    let mut frame = Box::new(FrameBuffer::new());
    let samples = [2.0; 60];
    c.bench_function("mini_graph/60_samples_constant", |b| {
        b.iter(|| {
            dashboard_host_validation::primitives::draw_mini_graph(
                &mut St7789Renderer::new(&mut frame),
                0,
                0,
                64,
                20,
                black_box(&samples),
                60,
                0,
                60,
                2.0,
                2.0,
                |_| Rgb565::WHITE,
            );
            black_box(&frame);
        })
    });
}

/// Asserts repeated no-sample ticks preserve state and a constant graph draws exactly its expected white row.
pub fn verify_history() {
    use dashboard_host_validation::display::{FrameBuffer, St7789Renderer};
    use dashboard_host_validation::sensor_state::SensorState;
    use embedded_graphics::pixelcolor::Rgb565;
    use embedded_graphics::prelude::{IntoStorage, RgbColor};
    let mut state = SensorState::new();
    for i in 0..60 {
        assert!(state.update_at(2.0, false, i * 5000));
    }
    let before = state.snapshot();
    for _ in 0..8 {
        state.tick(295001);
        assert_eq!(state.snapshot(), before);
    }
    let mut frame = Box::new(FrameBuffer::new());
    dashboard_host_validation::primitives::draw_mini_graph(
        &mut St7789Renderer::new(&mut frame),
        0,
        0,
        64,
        20,
        &[2.0; 60],
        60,
        0,
        60,
        2.0,
        2.0,
        |_| Rgb565::WHITE,
    );
    for y in 0..240 {
        for x in 0..320 {
            let color = if y == 9 && (2..=61).contains(&x) {
                Rgb565::WHITE
            } else {
                Rgb565::BLACK
            };
            let at = (y * 320 + x) * 2;
            assert_eq!(&frame[at..at + 2], &color.into_storage().to_be_bytes());
        }
    }
}
