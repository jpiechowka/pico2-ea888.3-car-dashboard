//! Studio adapter. Ui owns the mask across preview and editor replacement.
use dashboard_core::cache_diagnostics::{CacheExhaustionCause, CacheExhaustionSeen, CacheExhaustionStatus};

/// Observe `status` through persistent `seen` and invoke `visible_warning`, then
/// `console_warning`, once for each newly observed cause. Returns no value.
/// The mask advances before callbacks run; callbacks must not panic if both sinks
/// must receive every cause. Replacing an editor must not reset `seen`.
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
