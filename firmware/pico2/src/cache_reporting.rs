//! Core0 application adapter. The caller owns one seen mask for the boot.
use dashboard_core::cache_diagnostics::{CacheExhaustionCause, CacheExhaustionSeen, CacheExhaustionStatus};

pub fn report(
    seen: &mut CacheExhaustionSeen,
    status: CacheExhaustionStatus,
    mut sink: impl FnMut(CacheExhaustionCause),
) {
    for cause in seen.observe(status).causes() {
        sink(cause);
    }
}
