use dashboard_configurator::timing::Playback;

#[test]
fn elapsed_time_dropped_frames_pause_and_restart_do_not_depend_on_fps() {
    for interval in [1, 14, 25, 50, 1000] {
        let mut clock = Playback::new(118_000);
        clock.start(10.0);
        for time in (10..3010).step_by(interval) {
            clock.sample(f64::from(time));
        }
        assert_eq!(clock.pause(3010.75), 121_000);
        assert_eq!(clock.sample(99_000.0), 121_000);
        clock.start(100_000.0);
        assert_eq!(clock.sample(130_123.5), 151_123);
    }
}

#[test]
fn invalid_clock_rollback_and_overflow_never_rewind_or_wrap() {
    let mut clock = Playback::new(1);
    clock.start(f64::NAN);
    assert_eq!(clock.sample(200.0), 1);
    clock.start(1000.0);
    assert_eq!(clock.sample(1005.0), 6);
    for value in [f64::NAN, f64::INFINITY, -1.0, 999.0] {
        assert_eq!(clock.sample(value), 6);
    }
    assert_eq!(clock.sample(1006.0), 7);
    let mut end = Playback::new(u64::MAX - 1);
    end.start(1.0);
    assert_eq!(end.sample(999.0), u64::MAX);
}

#[test]
fn jitter_repeated_start_and_invalid_pause_keep_deterministic_elapsed_time() {
    for step in [1, 3, 16, 17, 33, 50, 250, 1000] {
        let mut clock = Playback::new(42);
        clock.start(100.25);
        for time in (101..10_000).step_by(step) {
            clock.start(time as f64); // repeated Play cannot re-anchor an active run
            clock.sample(time as f64);
            clock.sample(f64::NEG_INFINITY);
            clock.sample(f64::NAN);
            clock.sample(time as f64 - 50.0); // clock rewind never rewinds the sample
        }
        assert_eq!(clock.pause(10_100.25), 10_042);
        assert_eq!(clock.pause(f64::NAN), 10_042);
        clock.start(f64::INFINITY);
        clock.start(-0.1);
        assert_eq!(clock.sample(20_000.0), 10_042);
        clock.start(20_000.5);
        assert_eq!(clock.pause(20_010.5), 10_052);
    }
}
