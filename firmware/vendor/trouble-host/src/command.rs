//! Single-operation admission and terminal-event cleanup state, shared by host and controller tasks.
//! `RefCell` requires serialized executor access; waiter registration stores one waker per side.
use core::cell::RefCell;
use core::future::poll_fn;
use core::task::{Context, Poll};

use embassy_sync::waitqueue::WakerRegistration;

/// Controller-operation lifecycle; `CTX` identifies the command variant to cancel.
pub enum State<CTX> {
    /// Live caller owns admission with its cancellation context.
    Active(
        /// Context supplied when the operation was admitted.
        CTX,
    ),
    /// Caller dropped; runner still needs to issue cancellation.
    Cancel(
        /// Context identifying the cancellation command.
        CTX,
    ),
    /// Cancellation was issued; terminal controller event still owns admission.
    CancelPending(
        /// Original cancellation context retained until terminal completion.
        CTX,
    ),
    /// Terminal failure retained until the original caller consumes or drops it.
    Failed(
        /// First controller status for the live operation.
        bt_hci::param::Error,
    ),
    /// No active operation or unconsumed terminal failure.
    Idle,
}

/// Mutable lifecycle and one waiter registration for each side of the command.
pub struct Inner<CTX> {
    /// Admission/cleanup phase; only idle admits a new request.
    state: State<CTX>,
    /// Caller waiting for admission, idle or terminal result.
    host: WakerRegistration,
    /// Runner waiting for a cancellation or state transition.
    controller: WakerRegistration,
}

/// A helper type for keeping track of the state of a controller command.
pub struct CommandState<CTX> {
    /// Executor-local interior mutability; reentrant access panics.
    inner: RefCell<Inner<CTX>>,
}

impl<CTX: Clone + Copy> CommandState<CTX> {
    /// Returns idle state with no registered waiters.
    pub fn new() -> Self {
        Self {
            inner: RefCell::new(Inner {
                state: State::Idle,
                host: WakerRegistration::new(),
                controller: WakerRegistration::new(),
            }),
        }
    }

    /// Applies `f` under the mutable state borrow and returns its result.
    /// Reentrant access panics; `f` must not retain the reference or poll other state users.
    fn with_inner<F: FnMut(&mut Inner<CTX>) -> R, R>(&self, mut f: F) -> R {
        let mut inner = self.inner.borrow_mut();
        f(&mut inner)
    }

    /// Waits for idle, then admits a command with cancellation context `ctx`.
    /// Canceling while pending leaves the existing command untouched. Once admitted, the caller
    /// must eventually call done/cancel or admission remains occupied.
    pub async fn request(&self, ctx: CTX) {
        poll_fn(|cx| {
            self.with_inner(|inner| {
                inner.host.register(cx.waker());
                match inner.state {
                    State::Idle => {
                        inner.state = State::Active(ctx);
                        Poll::Ready(())
                    }
                    _ => Poll::Pending,
                }
            })
        })
        .await
    }

    /// Waits until admission is idle; does not initiate cancellation or consume failed status.
    pub async fn wait_idle(&self) {
        poll_fn(|cx| {
            self.with_inner(|inner| {
                inner.host.register(cx.waker());
                match inner.state {
                    State::Idle => Poll::Ready(()),
                    _ => Poll::Pending,
                }
            })
        })
        .await
    }

    /// Retain a failed completion until its caller consumes or drops it. New
    /// requests cannot overwrite the status before that caller runs again. Returns success for idle
    /// or the retained controller error; observing it does not clear the failed state.
    pub async fn wait_result(&self) -> Result<(), bt_hci::param::Error> {
        poll_fn(|cx| {
            self.with_inner(|inner| {
                inner.host.register(cx.waker());
                match inner.state {
                    State::Idle => Poll::Ready(Ok(())),
                    State::Failed(error) => Poll::Ready(Err(error)),
                    _ => Poll::Pending,
                }
            })
        })
        .await
    }

    /// Records `error` for a live caller, preserving the first failure. A canceled/absent caller
    /// instead releases admission. Wakes both sides so terminal cleanup and waiters can progress.
    pub fn failed(&self, error: bt_hci::param::Error) {
        self.with_inner(|inner| {
            inner.state = match inner.state {
                State::Active(_) => State::Failed(error),
                State::Failed(first) => State::Failed(first),
                // A dropped caller has no result to consume; its terminal event
                // releases cancellation admission as before.
                _ => State::Idle,
            };
            inner.host.wake();
            inner.controller.wake();
        });
    }

    /// Registers `cx` and returns the cancellation context only while cancellation is unsent.
    /// Pending cancellation stays pending until a terminal event; polling does not advance state.
    pub fn poll_cancelled(&self, cx: &mut Context<'_>) -> Poll<CTX> {
        self.with_inner(|inner| {
            inner.controller.register(cx.waker());
            match inner.state {
                State::Cancel(ctx) => Poll::Ready(ctx),
                _ => Poll::Pending,
            }
        })
    }

    /// Requests cancellation using `ctx` only for an active command; clears a retained failure.
    /// Idle/already-canceling states are unchanged, so a second dropped waiter cannot reopen admission.
    pub fn cancel(&self, ctx: CTX) {
        self.with_inner(|inner| {
            if matches!(inner.state, State::Active(_)) {
                inner.state = State::Cancel(ctx);
                inner.controller.wake();
            } else if matches!(inner.state, State::Failed(_)) {
                inner.state = State::Idle;
                inner.host.wake();
                inner.controller.wake();
            }
        })
    }

    /// Keep admission closed after sending cancel until its terminal event.
    pub fn cancel_started(&self) {
        self.with_inner(|inner| {
            if let State::Cancel(ctx) = inner.state {
                inner.state = State::CancelPending(ctx);
            }
        });
    }

    /// Returns whether a live caller owns an active command; canceling/failed/idle return false.
    pub fn is_active(&self) -> bool {
        self.with_inner(|inner| matches!(inner.state, State::Active(_)))
    }

    /// Signal that a command has been canceled.
    pub fn canceled(&self) {
        self.with_inner(|inner| {
            inner.state = State::Idle;
            inner.host.wake();
            inner.controller.wake();
        })
    }

    /// Returns whether no operation, pending cancellation or retained failure owns admission.
    pub fn is_idle(&self) -> bool {
        self.with_inner(|inner| matches!(inner.state, State::Idle))
    }

    /// Returns `predicate` applied to the active/canceling context, or false for idle/failed.
    /// `predicate` runs under the state borrow and must not reenter this command.
    #[cfg(all(feature = "security", feature = "central"))]
    pub fn is_active_with(&self, predicate: impl Fn(CTX) -> bool) -> bool {
        self.with_inner(|inner| match inner.state {
            State::Active(ctx) | State::Cancel(ctx) | State::CancelPending(ctx) => predicate(ctx),
            State::Idle | State::Failed(_) => false,
        })
    }

    /// Releases admission and wakes both sides after the caller consumes its terminal result.
    pub fn done(&self) {
        self.with_inner(|inner| {
            inner.state = State::Idle;
            inner.host.wake();
            inner.controller.wake();
        })
    }
}

#[cfg(test)]
/// Regression coverage for cancellation admission and retained terminal failures.
mod tests {
    use core::future::Future;
    use core::pin::pin;
    use core::task::Waker;

    use embassy_futures::block_on;

    use super::*;

    #[test]
    /// Proves cancel acknowledgment alone cannot admit a second attempt before the terminal event.
    fn cancel_command_response_does_not_admit_a_new_attempt() {
        let state = CommandState::new();
        block_on(state.request(false));
        state.cancel(false);
        let mut cx = Context::from_waker(Waker::noop());
        assert!(state.poll_cancelled(&mut cx).is_ready());
        state.cancel_started();
        assert!(state.poll_cancelled(&mut cx).is_pending());
        assert!(!state.is_active());
        let mut next = pin!(state.request(true));
        assert!(next.as_mut().poll(&mut cx).is_pending());
        state.cancel(true); // Dropping a second waiter cannot reopen the old cancel.
        assert!(state.poll_cancelled(&mut cx).is_pending());
        state.canceled(); // Terminal LE Connection Complete.
        assert!(next.as_mut().poll(&mut cx).is_ready());
        assert!(state.is_active());
    }

    #[test]
    /// Proves a queued caller cannot overwrite the first failure before finish/drop releases it.
    fn failed_completion_retains_first_status_until_caller_finishes_or_drops() {
        for drop_caller in [false, true] {
            let state = CommandState::new();
            block_on(state.request(false));
            state.failed(bt_hci::param::Status::new(0x3e).to_result().unwrap_err());
            state.failed(bt_hci::param::Error::UNKNOWN_CONN_IDENTIFIER);
            let mut next = pin!(state.request(true));
            let mut cx = Context::from_waker(Waker::noop());
            assert!(next.as_mut().poll(&mut cx).is_pending());
            assert!(!state.is_idle());
            assert_eq!(
                block_on(state.wait_result()),
                Err(bt_hci::param::Status::new(0x3e).to_result().unwrap_err())
            );
            if drop_caller {
                state.cancel(false);
            } else {
                state.done();
            }
            assert!(next.as_mut().poll(&mut cx).is_ready());
            assert!(
                pin!(state.wait_result()).poll(&mut cx).is_pending(),
                "no previous failure in next attempt"
            );
        }
    }

    #[test]
    /// Proves terminal failure releases both unsent and already-sent cancellation states.
    fn canceled_caller_failure_releases_admission_at_terminal_event() {
        for cancel_sent in [false, true] {
            let state = CommandState::new();
            block_on(state.request(false));
            state.cancel(false);
            if cancel_sent {
                state.cancel_started();
            }
            assert!(!state.is_idle());
            state.failed(bt_hci::param::Error::UNKNOWN_CONN_IDENTIFIER);
            assert!(state.is_idle());
            assert_eq!(block_on(state.wait_result()), Ok(()));
        }
    }
}
