use dashboard_host_validation::sensor_state::{PeakMode, SensorState};

#[test]
fn accepted_measurement_updates_current_history_and_extrema_together() {
    let mut state = SensorState::new();
    assert!(state.ingest(-20.0, 100, PeakMode::Maximum));
    assert_eq!(
        (state.value(), state.minimum(), state.maximum()),
        (Some(-20.0), Some(-20.0), Some(-20.0))
    );
    assert!(!state.is_new_peak);
    for (value, time) in [(50.0, 100), (60.0, 99), (f32::NAN, 101), (1_000_001.0, 102)] {
        assert!(!state.ingest(value, time, PeakMode::Maximum));
        assert_eq!(
            (state.value(), state.minimum(), state.maximum()),
            (Some(-20.0), Some(-20.0), Some(-20.0))
        );
        assert!(!state.is_new_peak);
    }
    assert!(state.ingest(-19.0, 103, PeakMode::Maximum));
    assert!(state.is_new_peak);
    assert_eq!(state.maximum(), Some(-19.0));
}

#[test]
fn peak_modes_distinguish_upper_extremum_range_and_disabled_highlight() {
    for (mode, should_flash) in [
        (PeakMode::Maximum, false),
        (PeakMode::Range, true),
        (PeakMode::None, false),
    ] {
        let mut state = SensorState::new();
        assert!(state.ingest(12.6, 0, mode));
        assert!(!state.is_new_peak);
        assert!(state.ingest(12.2, 1, mode));
        assert_eq!(state.is_new_peak, should_flash);
        assert_eq!((state.minimum(), state.maximum()), (Some(12.2), Some(12.6)));
    }
}

#[test]
fn statistic_reset_preserves_current_and_rejects_duplicate_repopulation() {
    let mut state = SensorState::new();
    for at in (0..=6_000).step_by(200) {
        assert!(state.ingest(at as f32 / 1_000.0 - 20.0, at, PeakMode::Maximum));
    }
    assert_eq!(state.get_trend(), Some(true));
    state.reset_statistics();
    assert_eq!(
        (state.value(), state.minimum(), state.maximum()),
        (Some(-14.0), Some(-14.0), Some(-14.0))
    );
    assert_eq!(state.get_average(), None);
    assert_eq!(state.get_graph_data().2, 0);
    assert_eq!(state.get_trend(), None);
    assert!(!state.is_new_peak);
    assert!(!state.ingest(-10.0, 6_000, PeakMode::Maximum));
    assert!(state.ingest(-14.0, 6_001, PeakMode::Maximum));
    assert_eq!(state.get_average(), Some(-14.0));
    assert!(!state.is_new_peak);
}
