//! Elapsed-time animation invariants across repaint cadence, stalls and long uptimes.
use dashboard_host_validation::animations::{ColorTransition, blink_on};
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;

/// Different repaint steps reach identical midpoint/end colours at the same 150/300ms elapsed times.
#[test]
fn fade_is_identical_at_equal_elapsed_time_for_fast_slow_and_stalled_paints() {
    for step in [1, 7, 14, 17, 25, 50, 150, 300] {
        let mut fade = ColorTransition::new();
        fade.set_target(0, Rgb565::RED, 1000);
        for elapsed in (0..150).step_by(step) {
            fade.update(1000 + elapsed);
        }
        fade.update(1150);
        assert_eq!(fade.get_current(0), Rgb565::new(16, 0, 0), "halfway step {step}");
        for elapsed in (151..300).step_by(step) {
            fade.update(1000 + elapsed);
        }
        fade.update(1300);
        assert_eq!(fade.get_current(0), Rgb565::RED, "deadline step {step}");
    }
}

/// Idle time precedes the new transition; repeating its target cannot extend the 300ms deadline.
#[test]
fn idle_time_is_not_charged_to_new_target_and_same_target_does_not_restart() {
    let mut fade = ColorTransition::new();
    fade.update(0);
    assert!(fade.set_target(0, Rgb565::WHITE, 10_000));
    fade.update(10_000);
    assert_eq!(fade.get_current(0), Rgb565::BLACK);
    for elapsed in 1..=300 {
        assert!(!fade.set_target(0, Rgb565::WHITE, 10_000 + elapsed));
        fade.update(10_000 + elapsed);
    }
    assert_eq!(fade.get_current(0), Rgb565::WHITE);
    assert_eq!(fade.update(10_300), 0);
}

/// Retargeting samples the old mathematical trajectory even without paints; other cells retain their own origin.
#[test]
fn retarget_evaluates_original_trajectory_and_cells_keep_separate_start_times() {
    let mut fade = ColorTransition::new();
    fade.set_target(0, Rgb565::RED, 0);
    fade.set_target(1, Rgb565::GREEN, 100);
    // No intermediate update: target change still starts at the old trajectory's
    // correct midpoint, rather than the colour from the last painted frame.
    fade.set_target(0, Rgb565::BLACK, 150);
    assert_eq!(fade.get_current(0), Rgb565::new(16, 0, 0));
    fade.update(300);
    assert_eq!(fade.get_current(0), Rgb565::new(8, 0, 0));
    assert_eq!(fade.get_current(1), Rgb565::new(0, 42, 0));
    fade.update(450);
    assert_eq!(fade.get_current(0), Rgb565::BLACK);
    assert_eq!(fade.get_current(1), Rgb565::GREEN);
}

/// Backward timestamps leave colour/target unchanged; large forward gaps complete without replay or wrap.
#[test]
fn old_clocks_cannot_retarget_or_rewind_and_long_stalls_finish_the_fade() {
    let mut fade = ColorTransition::new();
    fade.set_target(0, Rgb565::BLUE, 100);
    fade.update(250);
    let before = fade.get_current(0);
    assert_eq!(fade.update(249), 0);
    assert!(!fade.set_target(0, Rgb565::WHITE, 249));
    assert_eq!(fade.get_current(0), before);
    fade.update(100 + (1_u64 << 32));
    assert_eq!(fade.get_current(0), Rgb565::BLUE);
    fade.set_target(1, Rgb565::WHITE, u64::MAX - 300);
    fade.update(u64::MAX);
    assert_eq!(fade.get_current(1), Rgb565::WHITE);
}

/// Checks inclusive/exclusive 200ms blink boundaries and phase arithmetic beyond u32 uptime.
#[test]
fn blink_keeps_exact_phase_across_u32_uptime_and_skipped_frames() {
    for base in [0, 4_294_967_200] {
        assert!(blink_on(base));
        assert!(blink_on(base + 199));
        assert!(!blink_on(base + 200));
        assert!(!blink_on(base + 399));
        assert!(blink_on(base + 400));
    }
    assert!(blink_on(4_294_967_296));
    assert!(blink_on(u64::MAX));
}

/// Ten-millisecond synthetic steps remain distinct after long uptimes and all channels retain finite nominal bounds.
#[test]
fn demo_does_not_freeze_acquisition_steps_after_days_of_uptime() {
    use dashboard_host_validation::demo;
    for start in [1_u64 << 27, 1 << 30, 30 * 24 * 60 * 60 * 1000, u64::MAX - 100] {
        for offset in (0..100).step_by(10) {
            assert_ne!(
                demo::values(start + offset),
                demo::values(start + offset + 10),
                "at {}",
                start + offset
            );
        }
    }
    assert_eq!(demo::values(0), [0.3, 60.0, 88.0, 75.0, 30.0, 200.0, 12.0, 1.0]);
    for at in [0, 1, 1000, 6_283_184, 6_283_185, u64::MAX] {
        let bounds = [
            (0.3, 2.0),
            (5.0, 115.0),
            (81.0, 95.0),
            (35.0, 115.0),
            (-10.0, 70.0),
            (200.0, 1200.0),
            (9.5, 14.5),
            (0.75, 1.25),
        ];
        for (value, (low, high)) in demo::values(at).into_iter().zip(bounds) {
            assert!(value.is_finite() && value >= low && value <= high);
        }
    }
}

/// Reduced motion immediately completes decorative colour/smoothing while warning blink remains time-driven.
#[test]
fn reduced_motion_snaps_decorative_fades_without_disabling_warning_blink() {
    use dashboard_host_validation::animations::MotionPolicy;
    let mut fade = ColorTransition::new();
    fade.set_target(0, Rgb565::RED, 0);
    fade.update(100);
    assert_ne!(fade.get_current(0), Rgb565::RED);
    fade.update_with_policy(100, MotionPolicy::new(true));
    assert_eq!(fade.get_current(0), Rgb565::RED);
    assert_eq!(MotionPolicy::new(true).smoothing_ms(1000), 0);
    assert!(!MotionPolicy::new(true).decorative());
    assert!(blink_on(0));
    assert!(!blink_on(200));
}
