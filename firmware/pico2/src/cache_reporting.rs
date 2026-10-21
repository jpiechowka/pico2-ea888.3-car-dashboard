//! Core0 application adapter. The caller owns one seen mask for the boot.
use dashboard_core::cache_diagnostics::{CacheExhaustionCause, CacheExhaustionSeen, CacheExhaustionStatus};

/// Updates boot-scoped `seen` from `status` and calls `sink` once for each newly
/// observed exhaustion cause. Repeated causes are suppressed; no cache is reset.
pub fn report(
    seen: &mut CacheExhaustionSeen,
    status: CacheExhaustionStatus,
    mut sink: impl FnMut(CacheExhaustionCause),
) {
    for cause in seen.observe(status).causes() {
        sink(cause);
    }
}
