use core::cell::RefCell;
use core::future::poll_fn;
use core::task::{Context, Poll};

use embassy_sync::waitqueue::WakerRegistration;

pub enum State<CTX> {
    Active(CTX),
    Cancel(CTX),
    CancelPending(CTX),
    Failed(bt_hci::param::Error),
    Idle,
}

pub struct Inner<CTX> {
    state: State<CTX>,
    host: WakerRegistration,
    controller: WakerRegistration,
}

/// A helper type for keeping track of the state of a controller command.
pub struct CommandState<CTX> {
    inner: RefCell<Inner<CTX>>,
}

impl<CTX: Clone + Copy> CommandState<CTX> {
    pub fn new() -> Self {
        Self {
            inner: RefCell::new(Inner {
                state: State::Idle,
                host: WakerRegistration::new(),
                controller: WakerRegistration::new(),
            }),
        }
    }

    fn with_inner<F: FnMut(&mut Inner<CTX>) -> R, R>(&self, mut f: F) -> R {
        let mut inner = self.inner.borrow_mut();
        f(&mut inner)
    }

    /// Request a new command
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

    /// Request a new command.
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
    /// requests cannot overwrite the status before that caller runs again.
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

    /// Poll if the command should be canceled
    pub fn poll_cancelled(&self, cx: &mut Context<'_>) -> Poll<CTX> {
        self.with_inner(|inner| {
            inner.controller.register(cx.waker());
            match inner.state {
                State::Cancel(ctx) => Poll::Ready(ctx),
                _ => Poll::Pending,
            }
        })
    }

    /// Request that any pending command be canceled
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

    /// A central connection event is admissible only for a live caller.
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

    /// Check if the command state is idle.
    pub fn is_idle(&self) -> bool {
        self.with_inner(|inner| matches!(inner.state, State::Idle))
    }

    /// Check whether an active or canceling command's context matches a predicate.
    #[cfg(all(feature = "security", feature = "central"))]
    pub fn is_active_with(&self, predicate: impl Fn(CTX) -> bool) -> bool {
        self.with_inner(|inner| match inner.state {
            State::Active(ctx) | State::Cancel(ctx) | State::CancelPending(ctx) => predicate(ctx),
            State::Idle | State::Failed(_) => false,
        })
    }

    pub fn done(&self) {
        self.with_inner(|inner| {
            inner.state = State::Idle;
            inner.host.wake();
            inner.controller.wake();
        })
    }
}

#[cfg(test)]
mod tests {
    use core::future::Future;
    use core::pin::pin;
    use core::task::Waker;

    use embassy_futures::block_on;

    use super::*;

    #[test]
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
