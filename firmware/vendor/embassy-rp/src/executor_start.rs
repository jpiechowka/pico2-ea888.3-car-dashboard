//! Initialization/publication ordering shared by the interrupt executor and host regressions.

use core::cell::Cell;

/// Runs `initialize` once and publishes `started`; the caller must exclude concurrent starts/readers.
/// Panics before invoking the callback when already started. The callback must fully initialize storage
/// before returning; this helper does not itself provide cross-core exclusion or memory barriers.
/// If the callback panics, readiness remains false, but its own partially modified state is not rolled back.
pub(crate) fn initialize_once(
    started: &Cell<bool>,
    initialize: impl FnOnce(),
) {
    if started.get() {
        panic!("InterruptExecutor::start() called multiple times on the same executor.");
    }
    initialize();
    started.set(true);
}

#[cfg(test)]
/// Publication must not admit readers while the initializer still owns uninitialized storage.
mod tests {
    extern crate std;
    use super::*;

    /// A reader observing the flag during initialization must not be told storage is ready.
    #[test]
    fn readiness_is_published_only_after_storage_initialization() {
        let started = Cell::new(false);
        let storage = Cell::new(None);
        initialize_once(&started, || {
            assert!(!started.get(), "reader could access uninitialized executor storage");
            storage.set(Some(42));
        });
        assert!(started.get());
        assert_eq!(storage.get(), Some(42));
    }

    /// A second start must fail before replacing the first executor's live state.
    #[test]
    fn duplicate_start_cannot_run_initializer() {
        let started = Cell::new(false);
        let initialized = Cell::new(0);
        initialize_once(&started, || initialized.set(1));
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                initialize_once(&started, || initialized.set(2));
            }))
            .is_err()
        );
        assert_eq!(initialized.get(), 1);
        assert!(started.get());
    }

    /// Failed initialization does not publish readiness or pretend to undo callback effects.
    #[test]
    fn initializer_panic_retains_unpublished_partial_state() {
        let started = Cell::new(false);
        let storage = Cell::new(None);
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                initialize_once(&started, || {
                    storage.set(Some(42));
                    panic!("injected initialization failure");
                });
            }))
            .is_err()
        );
        assert!(!started.get());
        assert_eq!(storage.get(), Some(42));
    }
}
