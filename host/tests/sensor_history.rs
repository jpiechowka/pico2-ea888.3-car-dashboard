use dashboard_host_validation::sensor_state::{GRAPH_HISTORY_SIZE, SensorState};

#[test]
fn sampling_uses_measurement_timestamps_and_never_replays_render_ticks() {
    let mut state = SensorState::new();
    assert!(state.update_at(10.0, false, 0));
    for now in (0..2_000).step_by(17) {
        state.tick(now);
    }
    assert_eq!(state.get_average(), Some(10.0));
    assert_eq!(state.get_graph_data().2, 1);

    assert!(state.update_at(20.0, false, 1_999));
    assert_eq!(state.get_graph_data().2, 1);
    assert!(state.update_at(30.0, false, 2_000));
    assert_eq!(state.get_graph_data().2, 2);
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
    let (_, _, count, min, max) = state.get_graph_data();
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
    let (_, start, count, min, max) = state.get_graph_data();
    assert_eq!((start, count, min, max), (0, 60, -30.0, 50.0));
    assert!(state.update_at(0.0, false, 120_000));
    let (data, start, count, min, max) = state.get_graph_data();
    assert_eq!((start, count, min, max), (1, 60, -10.0, 50.0));
    assert_eq!(data[(start + count - 1) % GRAPH_HISTORY_SIZE], 0.0);
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
    assert_eq!(state.get_graph_data().2, 2);
    state.reset_average();
    state.reset_graph();
    state.reset_peak();
    assert_eq!(state.get_average(), None);
    assert_eq!(state.get_graph_data().2, 0);
    assert!(!state.is_new_peak);
    assert!(state.update_at(5.0, false, 100_002));
    assert_eq!(state.get_average(), Some(5.0));
    assert_eq!(state.get_graph_data().2, 1);
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
    let (data, start, count, ..) = state.get_graph_data();
    let graph = (0..count).map(|i| data[(start + i) % GRAPH_HISTORY_SIZE]).collect();
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
