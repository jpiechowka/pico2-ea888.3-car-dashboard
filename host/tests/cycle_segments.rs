//! Per-poll active cycle segments exclude yield gaps and preserve state after invalid transitions.
use dashboard_host_validation::cycle_segments::{CycleError, CycleSegments};

/// Only running intervals contribute cycles; duplicate pause/resume requests report their exact state error.
#[test]
fn cycle_segments_exclude_other_tasks_between_yields() {
    let mut meter = CycleSegments::new(100, 1000);
    meter.pause(150).unwrap();
    assert_eq!(meter.total(), 50);
    assert_eq!(meter.pause(160), Err(CycleError::Paused));
    meter.resume(900).unwrap();
    assert_eq!(meter.resume(910), Err(CycleError::Running));
    meter.pause(950).unwrap();
    assert_eq!(meter.total(), 100);
}

/// One u32 counter wrap contributes the correct delta; an overlong segment fails without losing the retryable interval.
#[test]
fn cycle_wrap_is_supported_and_excessive_segments_fail_without_mutation() {
    let mut meter = CycleSegments::new(u32::MAX - 9, 500);
    meter.pause(10).unwrap();
    assert_eq!(meter.total(), 20);
    meter.resume(100).unwrap();
    assert_eq!(meter.pause(601), Err(CycleError::TooLong));
    assert_eq!(meter.total(), 20);
    meter.pause(600).unwrap();
    assert_eq!(meter.total(), 520);
}
