//! USB generation fencing for one capture-task waiter.
//!
//! Handler transitions and endpoint polls run cooperatively on core0. Neither a
//! Handler callback nor the RP endpoint's DPRAM copy yields. A generation check
//! immediately before each endpoint poll therefore excludes old-session I/O.
use core::future::{Future, poll_fn};
use core::pin::pin;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use core::task::Poll;

use embassy_sync::waitqueue::AtomicWaker;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Non-wrapping USB configuration generation, usable only while its session matches.
pub struct Epoch(
    /// Checked-increment generation; zero is the unconfigured initial state.
    u32,
);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Guard rejection after reset, reconfiguration, disconnect, or generation exhaustion.
pub struct Changed;

/// Core0 cooperative USB session fence with one registered capture-task waiter.
pub struct Session {
    /// Monotonically incremented generation; never wraps after u32::MAX.
    epoch: AtomicU32,
    /// Whether the current bus configuration admits application endpoint I/O.
    configured: AtomicBool,
    /// Permanent fail-closed latch when the generation can no longer increment.
    exhausted: AtomicBool,
    /// Single waiter notified after every reset/configuration transition.
    waker: AtomicWaker,
}
impl Session {
    /// Returns an unconfigured generation-zero session with no exhaustion latch.
    pub const fn new() -> Self {
        Self {
            epoch: AtomicU32::new(0),
            configured: AtomicBool::new(false),
            exhausted: AtomicBool::new(false),
            waker: AtomicWaker::new(),
        }
    }

    /// Call on every bus reset/configuration change, including configured=false.
    /// Exhaustion fails closed until reboot; an old token can never become valid.
    /// `configured` becomes the new availability only after a successful increment;
    /// transitions always wake the waiter, including failed/exhausted transitions.
    pub fn change(
        &self,
        configured: bool,
    ) {
        self.configured.store(false, Ordering::SeqCst);
        if self
            .epoch
            .try_update(Ordering::SeqCst, Ordering::SeqCst, |epoch| epoch.checked_add(1))
            .is_err()
        {
            self.exhausted.store(true, Ordering::SeqCst);
        }
        if !self.exhausted.load(Ordering::SeqCst) {
            self.configured.store(configured, Ordering::SeqCst);
        }
        self.waker.wake();
    }

    /// Returns the current generation only while configured and unexhausted;
    /// otherwise returns None. Endpoint poll callers run cooperatively on core0.
    pub fn current(&self) -> Option<Epoch> {
        if self.configured.load(Ordering::SeqCst) && !self.exhausted.load(Ordering::SeqCst) {
            Some(Epoch(self.epoch.load(Ordering::SeqCst)))
        } else {
            None
        }
    }

    /// Returns whether `epoch` is the currently configured, unexhausted generation.
    pub fn matches(
        &self,
        epoch: Epoch,
    ) -> bool {
        self.current() == Some(epoch)
    }

    /// Waits until `epoch` is invalid, registering before checking to avoid lost
    /// wakeups. Only one task may own this session's waiter at a time.
    pub async fn changed(
        &self,
        epoch: Epoch,
    ) {
        poll_fn(|cx| {
            self.waker.register(cx.waker());
            if self.matches(epoch) {
                Poll::Pending
            } else {
                Poll::Ready(())
            }
        })
        .await
    }

    /// Polls `future` only while `epoch` matches, checking again after Ready.
    /// Returns its output or Changed; mismatch drops the future without another
    /// poll, so its caller must provide a cancellation-safe operation. With core0
    /// cooperative endpoint copies this prevents I/O from crossing USB sessions.
    pub async fn guard<F: Future>(
        &self,
        epoch: Epoch,
        future: F,
    ) -> Result<F::Output, Changed> {
        let mut future = pin!(future);
        poll_fn(|cx| {
            // Register first, then check, so a transition cannot be lost between
            // checking the token and installing this task's wakeup.
            self.waker.register(cx.waker());
            if !self.matches(epoch) {
                return Poll::Ready(Err(Changed));
            }
            match future.as_mut().poll(cx) {
                Poll::Ready(value) if self.matches(epoch) => Poll::Ready(Ok(value)),
                Poll::Ready(_) => Poll::Ready(Err(Changed)),
                Poll::Pending => Poll::Pending,
            }
        })
        .await
    }
}

#[cfg(test)]
/// Regression for the permanent generation-exhaustion fence.
mod tests {
    use super::*;
    #[test]
    /// Verifies the final generation remains distinct and later transitions never
    /// wrap or reenable a session, even if configuration becomes true again.
    fn exhausted_generation_never_wraps_or_reenables() {
        let session = Session::new();
        session.epoch.store(u32::MAX - 1, Ordering::SeqCst);
        session.change(true);
        let last = session.current().unwrap();
        assert_eq!(last, Epoch(u32::MAX));
        session.change(false);
        assert_eq!(session.current(), None);
        assert!(!session.matches(last));
        session.change(true);
        assert_eq!(session.current(), None);
        assert_eq!(session.epoch.load(Ordering::SeqCst), u32::MAX);
    }
}
