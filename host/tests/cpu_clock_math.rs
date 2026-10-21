//! Instrumented work arithmetic uses actual clock Hz, including fractional-MHz profiles.
use dashboard_host_validation::cpu_math::{sanity_cycles, utilization};

/// Verifies tenths-percent at three actual clocks, saturation, zero metadata and overflow-safe legacy fallback.
#[test]
fn actual_low_and_legacy_clocks_control_utilization() {
    for (hz, cycles, wanted) in [
        (50_000_000, 25_000_000, 500),
        (290_400_000, 145_200_000, 500),
        (300_000_000, 300_000_000, 1000),
    ] {
        assert_eq!(utilization(hz, cycles, 1_000_000), wanted);
        assert_eq!(sanity_cycles(hz), Some(hz * 2));
    }
    assert_eq!(sanity_cycles(0), None);
    assert_eq!(sanity_cycles(u32::MAX), Some(u32::MAX));
    assert_eq!(utilization(0, u32::MAX, 1), 0);
    assert_eq!(utilization(u32::MAX, u32::MAX, u32::MAX), 0);
    assert_eq!(utilization(50_000_000, u32::MAX, 1), 1000);
    assert_eq!(utilization(50_000_000, 1, 0), 0);
}
