//! DWT availability requires register enablement and observed counter progress.
use dashboard_host_validation::cpu_math::cycle_counter_available;

/// Wraparound counts as progress; disabled, absent, trace-disabled or static counters do not.
#[test]
fn progress_requires_enabled_implemented_counter_and_trace() {
    assert!(cycle_counter_available(1, 1 << 24, 10, 74));
    assert!(cycle_counter_available(1, 1 << 24, u32::MAX, 0));
    assert!(!cycle_counter_available(1, 1 << 24, 10, 10));
    assert!(!cycle_counter_available(0, 1 << 24, 10, 74));
    assert!(!cycle_counter_available(1, 0, 10, 74));
    assert!(!cycle_counter_available(1 | (1 << 25), 1 << 24, 10, 74));
}
