//! Studio adapter. Ui owns the mask across preview and editor replacement.
use dashboard_core::cache_diagnostics::{CacheExhaustionCause, CacheExhaustionSeen, CacheExhaustionStatus};

pub fn report(
    seen: &mut CacheExhaustionSeen,
    status: CacheExhaustionStatus,
    mut visible_warning: impl FnMut(CacheExhaustionCause),
    mut console_warning: impl FnMut(CacheExhaustionCause),
) {
    for cause in seen.observe(status).causes() {
        visible_warning(cause);
        console_warning(cause);
    }
}
