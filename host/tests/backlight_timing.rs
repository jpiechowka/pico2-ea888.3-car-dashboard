//! Backlight divider/duty arithmetic at supported clocks; host calculations are not measured PWM.
use dashboard_host_validation::config::backlight::{PWM_TOP, brightness_compare, pwm_divider, pwm_frequency_hz};

/// Checks actual integer divider and achieved frequency, preserving fractional-MHz profile differences.
#[test]
fn actual_clock_profiles_preserve_divider_policy_and_truthful_frequency() {
    for (clock_hz, expected_divider, expected_hz) in [
        (132_000_000, 13, 1015),
        (140_000_000, 14, 1000),
        (150_000_000, 15, 1000),
        (250_000_000, 25, 1000),
        (280_000_000, 28, 1000),
        (290_000_000, 29, 1000),
        (290_400_000, 29, 1001),
        (300_000_000, 30, 1000),
    ] {
        let divider = pwm_divider(clock_hz).expect("supported clock");
        assert_eq!(divider, expected_divider);
        assert_eq!(pwm_frequency_hz(clock_hz, divider), expected_hz);
    }
}

/// Zero/full brightness map to zero/always-high compare, all percent steps increase duty and excess values clamp.
#[test]
fn backlight_is_truly_off_and_remains_monotonic() {
    assert_eq!(brightness_compare(0), 0);
    assert_eq!(brightness_compare(100), PWM_TOP + 1);
    let mut previous = 0;
    for percent in 1..=100 {
        let compare = brightness_compare(percent);
        assert!(compare > previous, "brightness {percent} must increase duty");
        previous = compare;
    }
    assert_eq!(brightness_compare(255), PWM_TOP + 1);
}

/// Calculated one-percent pulses remain at least 1000ns and frequency at most 150kHz for the enumerated clocks.
#[test]
fn supported_pwm_pulses_meet_driver_minimum_on_time() {
    for clock_hz in [
        132_000_000,
        140_000_000,
        150_000_000,
        250_000_000,
        280_000_000,
        290_000_000,
        290_400_000,
        300_000_000,
    ] {
        let divider = pwm_divider(clock_hz).unwrap();
        let frequency_hz = pwm_frequency_hz(clock_hz, divider);
        assert!(frequency_hz <= 150_000);
        let min_high_ns =
            1_000_000_000_u64 * u64::from(brightness_compare(1)) / (u64::from(frequency_hz) * u64::from(PWM_TOP + 1));
        assert!(min_high_ns >= 1_000, "{clock_hz} Hz: {min_high_ns} ns");
    }
}
