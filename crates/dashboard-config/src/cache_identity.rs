//! Runtime-only exact identities. Never serialized or derived from an address.
use core::num::NonZeroU32;
use core::sync::atomic::{AtomicU32, Ordering};

#[cfg(not(target_has_atomic = "32"))]
compile_error!("configuration cache identities require 32-bit atomics");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConfigCacheIdentity(NonZeroU32);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CacheIdentityStatus {
    Tracked,
    Untracked,
    Exhausted,
}

pub(crate) struct Issuer(AtomicU32);
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum IssueError {
    Exhausted,
}
impl Issuer {
    const fn new(next: u32) -> Self { Self(AtomicU32::new(next)) }

    pub(crate) fn issue(&self) -> Result<ConfigCacheIdentity, IssueError> {
        self.0
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |next| next.checked_add(1))
            .map(|next| ConfigCacheIdentity(NonZeroU32::new(next).expect("issuer starts at one")))
            .map_err(|_| IssueError::Exhausted)
    }

    #[cfg(test)]
    pub(crate) fn with_next_for_test(next: u32) -> Self { Self::new(next) }
}
static ISSUER: Issuer = Issuer::new(1);

pub(crate) fn issue() -> Result<ConfigCacheIdentity, IssueError> { ISSUER.issue() }

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn last_success_is_not_exhaustion_and_failure_is_permanent() {
        let issuer = Issuer::with_next_for_test(u32::MAX - 1);
        assert!(issuer.issue().is_ok());
        assert_eq!(issuer.issue(), Err(IssueError::Exhausted));
        assert_eq!(issuer.issue(), Err(IssueError::Exhausted));
    }
}
