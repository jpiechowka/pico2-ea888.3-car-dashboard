//! Diagnostic availability, coherent publication, register-derived rates and bounded formatting.
use dashboard_host_validation::cpu_math::{CycleWindow, checked_segment};
use dashboard_host_validation::diagnostic_values::{
    FrameWorkObservation,
    WorkObservation,
    memory_kib,
    rate_text,
    spi_clock_hz,
    work_text,
};

/// Work/cycle observations share publication and expiry; wider valid work can outlive u32 cycle display capacity.
#[test]
fn frame_work_and_cycles_share_publication_and_expiry() {
    let mut published = FrameWorkObservation::EMPTY;
    assert_eq!((published.work(0), published.cycles(0)), (None, None));
    published = FrameWorkObservation::new(1_000_000, 150_000_000, Some(15_000_000), 1_000_000);
    // The caller retains this whole observation until another iteration commits.
    assert_eq!(
        (published.work(1_200_000), published.cycles(1_200_000)),
        (Some(100), Some(15_000_000))
    );
    published = FrameWorkObservation::new(1_300_000, 150_000_000, Some(30_000_000), 1_000_000);
    assert_eq!(
        (published.work(1_300_000), published.cycles(1_300_000)),
        (Some(200), Some(30_000_000))
    );
    for now in [1_299_999, 3_300_001] {
        assert_eq!((published.work(now), published.cycles(now)), (None, None));
    }
    assert_eq!(published.cycles(3_300_000), Some(30_000_000));
    for (hz, cycles, elapsed) in [(0, Some(1), 1), (150_000_000, None, 100), (150_000_000, Some(1), 0)] {
        let invalid = FrameWorkObservation::new(0, hz, cycles, elapsed);
        assert_eq!((invalid.work(0), invalid.cycles(0)), (None, None));
    }
    let zero = FrameWorkObservation::new(0, 150_000_000, Some(0), 1);
    assert_eq!((zero.work(0), zero.cycles(0)), (Some(0), Some(0)));
    let wide = FrameWorkObservation::new(0, 150_000_000, Some(u64::from(u32::MAX) + 1), 60_000_000);
    assert_eq!(wide.work(0), Some(477));
    assert_eq!(wide.cycles(0), None);
}

/// Exhausts prescaler admission and checks integer-Hz register calculations, fractional-MHz inputs and zero outputs.
#[test]
fn spi_register_rate_rejects_unknown_dividers_and_keeps_fractional_profiles() {
    assert_eq!(spi_clock_hz(150_000_000, 2, 1), Some(37_500_000));
    assert_eq!(spi_clock_hz(250_000_000, 2, 1), Some(62_500_000));
    assert_eq!(spi_clock_hz(290_400_000, 2, 2), Some(48_400_000));
    assert_eq!(spi_clock_hz(150_000_000, 254, 255), Some(2306));
    assert_eq!(spi_clock_hz(u32::MAX, 2, 0), Some(2_147_483_647));
    for prescale in 0..=u8::MAX {
        assert_eq!(
            spi_clock_hz(150_000_000, prescale, 0).is_some(),
            prescale >= 2 && prescale % 2 == 0
        );
    }
    assert_eq!(spi_clock_hz(0, 2, 0), None);
    assert_eq!(spi_clock_hz(1, 254, 255), None);
}
use dashboard_host_validation::display::telemetry::CompletionRate;

/// Valid zero work differs from absent/future/stale data; retained stack observations do not expire with work.
#[test]
fn observations_distinguish_absent_zero_stale_and_future() {
    assert_eq!(WorkObservation::EMPTY.work(0), None);
    assert_eq!(WorkObservation::EMPTY.stack_bytes, None);
    let sample = WorkObservation {
        at_us: Some(1_000_000),
        work_tenths: Some(0),
        stack_bytes: Some(123),
    };
    assert_eq!(sample.work(999_999), None);
    assert_eq!(sample.work(1_000_000), Some(0));
    assert_eq!(sample.work(3_000_000), Some(0));
    assert_eq!(sample.work(3_000_001), None);
    assert_eq!(sample.stack_bytes, Some(123)); // lifetime observed maximum
    assert_eq!(work_text(None).as_str(), "--");
    assert_eq!(work_text(Some(0)).as_str(), "0.0%");
    assert_eq!(work_text(Some(1000)).as_str(), "100.0%");
    assert_eq!(work_text(Some(1001)).as_str(), "--");
}

/// Completion rate requires a whole first window, expires after two seconds and resets validity on clock reversal.
#[test]
fn rates_have_first_window_reset_and_freshness_validity() {
    let mut meter = CompletionRate::new(0, 0);
    assert!(meter.latest(0).is_none());
    meter.sample(999_999, 19).unwrap();
    assert!(meter.latest(999_999).is_none());
    meter.sample(1_000_000, 20).unwrap();
    assert_eq!(meter.latest(1_000_000).unwrap().current, 20.0);
    assert!(meter.latest(999_999).is_none());
    assert!(meter.latest(3_000_000).is_some());
    assert!(meter.latest(3_000_001).is_none());
    assert!(meter.sample(999_999, 20).is_err());
    assert!(meter.latest(1_000_000).is_none());
    meter.sample(2_000_000, 20).unwrap();
    assert_eq!(meter.latest(2_000_000).unwrap().current, 0.0);
    meter = CompletionRate::new(2_000_000, 20);
    assert!(meter.latest(2_000_000).is_none());
}

/// KiB conversion rounds both sides upward while rejecting used-over-total, zero and unsupported capacities.
#[test]
fn memory_rounding_preserves_order_and_rejects_invalid_bounds() {
    for (bytes, expected) in [(0, 0), (1, 1), (1023, 1), (1024, 1), (1025, 2), (66568, 66)] {
        assert_eq!(memory_kib(bytes, 66568), Some((expected, 66)));
    }
    assert_eq!(memory_kib(66569, 66568), None);
    assert_eq!(memory_kib(0, 0), None);
    assert_eq!(memory_kib(0, u32::MAX), None);
    assert_eq!(memory_kib(524288, 524288), Some((512, 512)));
}

/// Nonfinite/negative/missing rates stay unavailable; valid zero and bounded decimal rounding remain distinct.
#[test]
fn rates_reject_invalid_values_and_bound_conversion() {
    for value in [None, Some(-1.0), Some(f32::NAN), Some(f32::INFINITY)] {
        assert_eq!(rate_text(value).as_str(), "--");
    }
    assert_eq!(rate_text(Some(0.0)).as_str(), "0.0");
    assert_eq!(rate_text(Some(19.96)).as_str(), "20.0");
    assert_eq!(rate_text(Some(f32::MAX)).as_str(), ">999");
}

/// One-wrap segments are admitted within the time/cycle bound; invalid/overflow input poisons only its current window.
#[test]
fn invalid_cycles_poison_only_the_current_window_and_do_not_become_idle() {
    let hz = 150_000_000;
    assert_eq!(checked_segment(hz, u32::MAX - 9, 10, 1), Some(20));
    assert_eq!(checked_segment(hz, 0, hz * 2, 2_000_000), Some(u64::from(hz) * 2));
    assert_eq!(checked_segment(hz, 0, hz * 2 + 1, 2_000_000), None);
    assert_eq!(checked_segment(hz, 1, 2, 2_000_001), None);
    assert_eq!(checked_segment(0, 0, 0, 1), None);
    let mut window = CycleWindow::new();
    window.record(Some(15_000_000));
    assert_eq!(window.finish(hz, 1_000_000), Some(100));
    window.record(None);
    window.record(Some(15_000_000));
    assert_eq!(window.finish(hz, 1_000_000), None);
    assert_eq!(window.finish(hz, 1_000_000), Some(0));
    window.record(Some(u64::MAX));
    window.record(Some(1));
    assert_eq!(window.finish(hz, 1_000_000), None);
    assert_eq!(window.finish(hz, 0), None);
    assert_eq!(window.finish(hz, u64::MAX), None);
}
