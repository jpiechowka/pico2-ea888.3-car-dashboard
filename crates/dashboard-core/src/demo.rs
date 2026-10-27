//! Synthetic bench values, evaluated from acquisition time rather than frames.
/// Returns synthetic values at `elapsed_ms` milliseconds since the demo origin,
/// in sensor order: boost bar, oil/coolant/DSG/IAT/EGT degrees Celsius, battery
/// volts, lambda ratio. Nominal ranges are respectively `0.3..=2`, `5..=115`,
/// `81..=95`, `35..=115`, `-10..=70`, `200..=1200`, `9.5..=14.5`, `0.75..=1.25`.
/// Integer phase reduction preserves short-step motion at long uptimes; this
/// stateless approximate sine model supplies no vehicle qualification.
pub fn values(elapsed_ms: u64) -> [f32; 8] {
    // Reduce integer micro-radian phases before converting to f32. Uptime
    // therefore never consumes the precision needed for a 10 ms step.
    // Tau is rounded to one micro-radian for synthetic values only (~0.049 ppm).
    /// Rounded full-turn phase in microradians; also the synthetic time-reduction period in ms.
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

/// Returns four synthetic signed corrections in degrees at acquisition
/// `elapsed_ms` milliseconds, in cylinder-number order. Independent phases range
/// from -6 to +1 degrees, including zero plateaus, to exercise OK/RET rendering.
/// Integer phase reduction handles the full u64 uptime range without overflow;
/// this generator supplies no ECU measurement, decoder or vehicle qualification.
pub fn ignition_values(elapsed_ms: u64) -> [f32; 4] {
    let phase = elapsed_ms % 8_000;
    core::array::from_fn(|index| {
        let local = (phase + index as u64 * 1_700) % 8_000;
        match local {
            0..2_000 => 0.0,
            2_000..4_000 => -6.0 * (local - 2_000) as f32 / 2_000.0,
            4_000..6_000 => -6.0 + 6.0 * (local - 4_000) as f32 / 2_000.0,
            _ => (local - 6_000) as f32 / 2_000.0,
        }
    })
}
