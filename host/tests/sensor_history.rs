use dashboard_host_validation::sensor_state::{GRAPH_HISTORY_SIZE, SensorState};

#[test]
fn sampling_uses_measurement_timestamps_and_never_replays_render_ticks() {
    let mut state = SensorState::new();
    assert!(state.update_at(10.0, false, 0));
    for now in (0..2_000).step_by(17) {
        state.tick(now);
    }
    assert_eq!(state.get_average(), Some(10.0));
    assert_eq!(state.history().count(), 1);

    assert!(state.update_at(20.0, false, 1_999));
    assert_eq!(state.history().count(), 1);
    assert!(state.update_at(30.0, false, 2_000));
    assert_eq!(state.history().count(), 2);
    assert_eq!(state.get_average(), Some(10.0));
    assert!(state.update_at(40.0, false, 4_999));
    assert!(state.update_at(50.0, false, 5_000));
    assert_eq!(state.get_average(), Some(30.0));
}

#[test]
fn duplicate_regressed_and_nonfinite_samples_do_not_change_history_or_peak() {
    let mut state = SensorState::new();
    assert!(state.update_at(-20.0, false, 100));
    assert!(!state.update_at(90.0, true, 100));
    assert!(!state.update_at(90.0, true, 99));
    assert!(!state.update_at(f32::NAN, true, 101));
    assert!(!state.update_at(f32::INFINITY, true, 102));
    assert!(!state.update_at(f32::NEG_INFINITY, true, 103));
    assert_eq!(state.get_average(), Some(-20.0));
    let count = state.history().count();
    let (min, max) = state.history().bounds().unwrap();
    assert_eq!((count, min, max), (1, -20.0, -20.0));
    assert!(!state.is_new_peak);
    assert!(state.update_at(-21.0, false, 200));
}

#[test]
fn peak_hold_uses_elapsed_time_and_reset_clears_it() {
    let mut state = SensorState::new();
    assert!(state.update_at(1.0, true, 100));
    state.tick(849);
    assert!(state.is_new_peak);
    state.tick(850);
    assert!(!state.is_new_peak);
    assert!(state.update_at(2.0, true, 900));
    assert!(state.is_new_peak);
    state.reset_peak();
    assert!(!state.is_new_peak);
    state.tick(1_000);
    assert!(!state.is_new_peak);
}

#[test]
fn graph_wrap_recalculates_negative_minimum_after_eviction() {
    let mut state = SensorState::new();
    for index in 0..GRAPH_HISTORY_SIZE {
        let value = if index == 0 {
            -30.0
        } else if index == 59 {
            50.0
        } else {
            -10.0
        };
        assert!(state.update_at(value, false, index as u64 * 2_000));
    }
    assert_eq!(
        (state.history().count(), state.history().bounds()),
        (60, Some((-30.0, 50.0)))
    );
    assert!(state.update_at(0.0, false, 120_000));
    assert_eq!(
        (state.history().count(), state.history().bounds()),
        (60, Some((-10.0, 50.0)))
    );
    assert_eq!(state.history().iter().last().unwrap().1, Some(0.0));
}

#[test]
fn average_wrap_evicts_the_oldest_sample_without_changing_capacity() {
    let mut state = SensorState::new();
    for index in 0..60 {
        let value = if index == 0 { 100.0 } else { 1.0 };
        assert!(state.update_at(value, false, index * 5_000));
    }
    assert_eq!(state.get_average(), Some(159.0 / 60.0));
    assert!(state.update_at(1.0, false, 300_000));
    assert_eq!(state.get_average(), Some(1.0));
}

#[test]
fn long_gaps_do_not_fabricate_samples_and_resets_restart_each_window() {
    let mut state = SensorState::new();
    assert!(state.update_at(1.0, true, 1));
    assert!(state.update_at(3.0, false, 100_001));
    assert_eq!(state.get_average(), Some(2.0));
    assert_eq!(state.history().count(), 2);
    state.reset_average();
    state.reset_graph();
    state.reset_peak();
    assert_eq!(state.get_average(), None);
    assert_eq!(state.history().count(), 0);
    assert!(!state.is_new_peak);
    assert!(state.update_at(5.0, false, 100_002));
    assert_eq!(state.get_average(), Some(5.0));
    assert_eq!(state.history().count(), 1);
}

fn replay_with_render_ticks(render_ticks: &[u64]) -> (Option<f32>, Vec<f32>, Option<bool>, bool) {
    let mut state = SensorState::new();
    let mut render = 0;
    for sample_ms in (0..=10_000).step_by(100) {
        while render < render_ticks.len() && render_ticks[render] <= sample_ms {
            state.tick(render_ticks[render]);
            render += 1;
        }
        let value = (sample_ms / 1_000) as f32;
        assert!(state.update_at(value, sample_ms == 9_600, sample_ms));
    }
    state.tick(10_000);
    let graph = state.history().iter().filter_map(|(_, value)| value).collect();
    (state.get_average(), graph, state.get_trend(), state.is_new_peak)
}

#[test]
fn identical_timestamped_trace_is_independent_of_render_schedule() {
    let baseline: Vec<u64> = (0..=10_000).step_by(50).collect();
    let expected = replay_with_render_ticks(&baseline);
    for interval in [17, 25, 50] {
        let schedule: Vec<u64> = (0..=10_000).step_by(interval).collect();
        assert_eq!(replay_with_render_ticks(&schedule), expected);
    }
    let mut irregular = vec![0];
    while *irregular.last().unwrap() < 10_000 {
        let next = irregular.last().unwrap() + if irregular.len() % 3 == 0 { 137 } else { 7 };
        irregular.push(next);
    }
    assert_eq!(replay_with_render_ticks(&irregular), expected);
}

#[test]
fn trend_uses_elapsed_windows_and_channel_threshold() {
    let mut sensitive = SensorState::with_trend_threshold(0.05);
    let mut conservative = SensorState::with_trend_threshold(0.5);
    for at_ms in (0..=6_000).step_by(200) {
        let value = if at_ms > 4_000 { 1.2 } else { 1.0 };
        assert!(sensitive.update_at(value, false, at_ms));
        assert!(conservative.update_at(value, false, at_ms));
    }
    assert_eq!(sensitive.get_trend(), Some(true));
    assert_eq!(conservative.get_trend(), None);
    assert_eq!(SensorState::with_trend_threshold(f32::NAN).get_trend(), None);
}

#[test]
fn old_tick_and_late_peak_do_not_resurrect_expired_highlight() {
    let mut state = SensorState::new();
    assert!(state.update_at(1.0, false, 100));
    state.tick(1_000);
    assert!(state.update_at(2.0, true, 200));
    assert!(!state.is_new_peak);
    state.tick(300);
    assert!(!state.is_new_peak);
    assert!(state.update_at(3.0, true, 1_001));
    assert!(state.is_new_peak);
    state.tick(1_751);
    assert!(!state.is_new_peak);
}

#[test]
fn timed_bucket_keeps_last_observation_and_expires_without_samples() {
    let mut state = SensorState::new();
    state.update_at(1.0, false, 0);
    state.update_at(2.0, false, 100);
    assert_eq!(state.history().bounds().unwrap().1, 2.0, "last observation in bucket");
    state.tick(120_000);
    assert_eq!(state.history().count(), 0, "no held points after a full window");
}

use dashboard_host_validation::history::History;

#[test]
fn bucket_boundaries_sparse_gaps_fault_recovery_and_duration_reset() {
    let mut history = History::new(10);
    history.observe(1.0, 0);
    history.observe(2.0, 166);
    history.observe(3.0, 167);
    assert_eq!(
        history.view().iter().collect::<Vec<_>>(),
        [(0, Some(2.0)), (167, Some(3.0))]
    );
    history.observe(4.0, 500);
    assert_eq!(
        history.view().iter().collect::<Vec<_>>(),
        [(0, Some(2.0)), (167, Some(3.0)), (334, None), (500, Some(4.0))]
    );
    history.invalidate(501);
    history.observe(5.0, 502);
    assert_eq!(history.view().iter().last(), Some((500, Some(5.0))));
    assert_eq!(history.view().point(58).unwrap().1, None);
    history.observe(99.0, 502);
    assert_eq!(history.view().bounds(), Some((2.0, 5.0)));
    history.set_duration(900);
    assert_eq!(history.view().bounds(), None);
    assert_eq!(history.view().duration_seconds(), 900);
}

#[test]
fn fault_then_recovery_in_same_bucket_sacrifices_preceding_point_only() {
    let mut history = History::new(10);
    history.observe(1.0, 0);
    history.observe(2.0, 167);
    history.observe(3.0, 334);
    history.invalidate(335);
    history.observe(4.0, 336);
    assert_eq!(
        history.view().iter().collect::<Vec<_>>(),
        [(0, Some(1.0)), (167, None), (334, Some(4.0))]
    );
    history.invalidate(337);
    history.observe(9.0, 336); // Duplicate cannot repopulate a fault.
    assert_eq!(history.view().count(), 1);
}

#[test]
fn all_duration_presets_custom_and_u64_boundaries_match_wide_integer_oracle() {
    for seconds in [10u16, 30, 60, 120, 300, 600, 900, 137] {
        let mut history = History::new(seconds);
        let duration = u64::from(seconds) * 1000;
        for now in [
            0,
            1,
            duration / 60,
            duration / 60 + 1,
            duration - 1,
            duration,
            u64::MAX - duration,
            u64::MAX - 1,
            u64::MAX,
        ] {
            assert!(history.advance(now) <= 60);
            history.observe(3.0, now);
            let ordinal = (u128::from(now) * 60 / u128::from(duration)) as u64;
            assert_eq!(history.view().newest_ordinal(), ordinal);
            let points: Vec<_> = history.view().iter().collect();
            assert!(points.len() <= 60);
            for (i, (at, _)) in points.iter().enumerate() {
                let bucket = ordinal - (points.len() - 1 - i) as u64;
                assert_eq!(*at, (u128::from(bucket) * u128::from(duration)).div_ceil(60) as u64);
                assert!(*at <= now);
            }
        }
        assert!(history.advance(0) <= 60);
        assert_eq!(history.view().bounds(), None, "time regression clears the window");
        history.observe(4.0, 0);
        assert_eq!(history.view().bounds(), Some((4.0, 4.0)));
        assert_eq!(history.advance(86_400_000), 60);
        assert_eq!(history.view().bounds(), None);
    }
}

#[test]
fn history_duration_change_does_not_reset_statistics() {
    let mut state = SensorState::new();
    state.ingest(1.0, 0, dashboard_host_validation::sensor_state::PeakMode::Maximum);
    state.ingest(3.0, 5_000, dashboard_host_validation::sensor_state::PeakMode::Maximum);
    state.set_history_duration(10);
    assert_eq!(state.history().bounds(), None);
    assert_eq!(state.get_average(), Some(2.0));
    assert_eq!((state.minimum(), state.maximum()), (Some(1.0), Some(3.0)));
}

#[test]
fn bucket_retains_bit_exact_last_observation_including_signed_zero() {
    let mut history = History::new(10);
    history.observe(0.0, 0);
    history.observe(-0.0, 1);
    let last = history.view().iter().last().unwrap().1.unwrap();
    assert_eq!(last.to_bits(), (-0.0_f32).to_bits());
}
