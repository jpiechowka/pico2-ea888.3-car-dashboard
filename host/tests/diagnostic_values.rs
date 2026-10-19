use dashboard_host_validation::cpu_math::{CycleWindow, checked_segment};
use dashboard_host_validation::diagnostic_values::{WorkObservation, memory_kib, rate_text, work_text};
use dashboard_host_validation::display::telemetry::CompletionRate;

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

#[test]
fn rates_reject_invalid_values_and_bound_conversion() {
    for value in [None, Some(-1.0), Some(f32::NAN), Some(f32::INFINITY)] {
        assert_eq!(rate_text(value).as_str(), "--");
    }
    assert_eq!(rate_text(Some(0.0)).as_str(), "0.0");
    assert_eq!(rate_text(Some(19.96)).as_str(), "20.0");
    assert_eq!(rate_text(Some(f32::MAX)).as_str(), ">999");
}

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
