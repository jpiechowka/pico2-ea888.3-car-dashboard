//! Runtime-only exact identities. Never serialized or derived from an address.
use core::num::NonZeroU32;
use core::sync::atomic::{AtomicU32, Ordering};

#[cfg(not(target_has_atomic = "32"))]
compile_error!("configuration cache identities require 32-bit atomics");

/// Process-local immutable-configuration cache key; equality is exact, not a content hash.
/// Clones retain identity. Never serialize, reuse across processes or infer publication ordering.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConfigCacheIdentity(
    /// Nonzero issued ordinal; `u32::MAX` remains the exhaustion sentinel.
    NonZeroU32,
);

/// Whether caching may use an exact issued configuration identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CacheIdentityStatus {
    /// A valid process-local key was issued.
    Tracked,
    /// No issuance has been attempted for this storage state.
    Untracked,
    /// Issuer exhausted permanently; configuration remains valid but caches must bypass.
    Exhausted,
}

/// Monotonic issuer; relaxed atomics ensure uniqueness only, not data publication.
pub(crate) struct Issuer(
    /// Next ordinal; starts at one and stops permanently at `u32::MAX`.
    AtomicU32,
);
/// Failure to allocate a fresh cache key; never a configuration-validation error.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum IssueError {
    /// No further ordinals can be issued without risking identity reuse.
    Exhausted,
}
impl Issuer {
    /// Construct an issuer with next ordinal `next`; returns private counter state.
    /// Caller must supply nonzero `next`; zero would panic on the first successful issue.
    const fn new(next: u32) -> Self { Self(AtomicU32::new(next)) }

    /// Return a unique nonzero key or permanent exhaustion; never wraps or reuses keys.
    /// The final issued key is `u32::MAX - 1`. Does not publish associated configuration bytes.
    pub(crate) fn issue(&self) -> Result<ConfigCacheIdentity, IssueError> {
        self.0
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |next| next.checked_add(1))
            .map(|next| ConfigCacheIdentity(NonZeroU32::new(next).expect("issuer starts at one")))
            .map_err(|_| IssueError::Exhausted)
    }

    #[cfg(test)]
    /// Return isolated test state beginning at nonzero `next`, without exhausting the global issuer.
    pub(crate) fn with_next_for_test(next: u32) -> Self { Self::new(next) }
}
/// Global process-lifetime identity source; zero is never an initial state.
static ISSUER: Issuer = Issuer::new(1);

/// Return a fresh global cache key or exhaustion; no configuration data is synchronized here.
pub(crate) fn issue() -> Result<ConfigCacheIdentity, IssueError> { ISSUER.issue() }

#[cfg(test)]
/// Counter-terminal-state regression independent of the global issuer.
mod tests {
    use super::*;
    #[test]
    /// The final valid ordinal succeeds exactly once; repeated later attempts remain errors.
    fn last_success_is_not_exhaustion_and_failure_is_permanent() {
        let issuer = Issuer::with_next_for_test(u32::MAX - 1);
        assert!(issuer.issue().is_ok());
        assert_eq!(issuer.issue(), Err(IssueError::Exhausted));
        assert_eq!(issuer.issue(), Err(IssueError::Exhausted));
    }
}
