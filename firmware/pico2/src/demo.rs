//! Synthetic bench values, evaluated from acquisition time rather than frames.
pub fn values(elapsed_ms: u64) -> [f32; 8] {
    // Reduce integer micro-radian phases before converting to f32. Uptime
    // therefore never consumes the precision needed for a 10 ms step.
    // Tau is rounded to one micro-radian for synthetic values only (~0.049 ppm).
    const TAU_MICRORADIANS: u64 = 6_283_185;
    let reduced_ms = elapsed_ms % TAU_MICRORADIANS;
    let wave = |rate_milliradians_per_second: u64| {
        // Largest rate is 500: multiplication stays below 3.2 billion.
        let phase = (reduced_ms * rate_milliradians_per_second) % TAU_MICRORADIANS;
        micromath::F32(phase as f32 / 1_000_000.0).sin().0
    };
    [
        (0.3 + 2.2 * wave(500).abs()).min(2.0),
        60.0 + 55.0 * wave(300),
        88.0 + 7.0 * wave(400),
        75.0 + 40.0 * wave(350),
        30.0 + 40.0 * wave(250),
        200.0 + 1000.0 * wave(40).abs(),
        12.0 + 2.5 * wave(150),
        1.0 + 0.25 * wave(450),
    ]
}
