use dashboard_host_validation::extrema::Extrema;

#[test]
fn first_negative_measurement_initializes_both_extrema_without_peak_flash() {
    let mut extrema = Extrema::new();
    assert!(!extrema.would_raise(-20.0));
    assert!(!extrema.would_extend(-20.0));
    extrema.record(-20.0);
    assert_eq!((extrema.min(), extrema.max()), (Some(-20.0), Some(-20.0)));
    assert!(extrema.would_raise(-19.0));
    extrema.record(-19.0);
    assert_eq!((extrema.min(), extrema.max()), (Some(-20.0), Some(-19.0)));
    assert!(!extrema.would_raise(-30.0));
    assert!(extrema.would_extend(-30.0));
    extrema.record(-30.0);
    assert_eq!((extrema.min(), extrema.max()), (Some(-30.0), Some(-19.0)));
}

#[test]
fn range_detects_both_voltage_extremes_without_repeated_peak_flash() {
    let mut extrema = Extrema::new();
    extrema.record(12.2);
    assert!(!extrema.would_extend(12.2));
    assert!(extrema.would_extend(12.6));
    extrema.record(12.6);
    assert!(extrema.would_extend(11.9));
    extrema.record(11.9);
    assert_eq!((extrema.min(), extrema.max()), (Some(11.9), Some(12.6)));
}

#[test]
fn reset_restarts_extrema_from_a_valid_current_value() {
    let mut extrema = Extrema::new();
    extrema.record(-20.0);
    extrema.record(30.0);
    extrema.reset_to(-5.0);
    assert_eq!((extrema.min(), extrema.max()), (Some(-5.0), Some(-5.0)));
    assert!(!extrema.would_raise(-5.0));
    extrema.reset_to(f32::NAN);
    assert_eq!((extrema.min(), extrema.max()), (None, None));
}

#[test]
fn invalid_measurements_cannot_replace_extrema() {
    let mut extrema = Extrema::new();
    extrema.record(1.0);
    for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 1_000_001.0] {
        assert!(!extrema.would_extend(invalid));
        extrema.record(invalid);
    }
    assert_eq!((extrema.min(), extrema.max()), (Some(1.0), Some(1.0)));
}
