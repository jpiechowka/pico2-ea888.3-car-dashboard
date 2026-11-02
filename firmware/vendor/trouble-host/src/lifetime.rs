//! Executor-local failure latch and connection command cancellation boundary.
//! Intrusive event waiters are independently owned; no single runner waker is reused.

use core::cell::Cell;
use core::future::{poll_fn, Future};
use core::pin::Pin;
use core::task::Poll;

use embassy_futures::select::{select, Either};
use futures_intrusive::sync::LocalManualResetEvent;

use crate::{BleHostError, Error};

/// Permanent failure state for one host; only rebuilding the host can clear it.
pub(crate) struct HostFault {
    /// True after uncertain command cancellation, identity loss, or worker failure.
    failed: Cell<bool>,
    /// Set permanently on failure, waking every independently registered waiter.
    stopped: LocalManualResetEvent,
}

impl HostFault {
    /// Returns a healthy latch with no registered waiters.
    pub(crate) fn new() -> Self {
        Self {
            failed: Cell::new(false),
            stopped: LocalManualResetEvent::new(false),
        }
    }

    /// Permanently closes this host before waking its workers and public operations.
    pub(crate) fn fail(&self) {
        self.failed.set(true);
        self.stopped.set();
    }

    /// Returns InvalidState after failure; never resets the latch.
    pub(crate) fn check(&self) -> Result<(), Error> {
        if self.failed.get() {
            Err(Error::InvalidState)
        } else {
            Ok(())
        }
    }

    /// Polls `work` only while healthy, checking again before returning its result.
    /// A failure wakes all callers. The caller owns pinned work and must drop it before
    /// starting another operation; guard cancellation only releases its borrow. Partial effects remain.
    pub(crate) async fn run<T, E>(
        &self,
        mut work: Pin<&mut impl Future<Output = Result<T, BleHostError<E>>>>,
    ) -> Result<T, BleHostError<E>> {
        let checked = poll_fn(|cx| {
            self.check()?;
            let result = work.as_mut().poll(cx);
            self.check()?;
            result
        });
        match select(checked, self.stopped.wait()).await {
            Either::First(result) => result,
            Either::Second(()) => Err(Error::InvalidState.into()),
        }
    }

    /// Runs worker `work`, returning its result and latching error or cancellation after its
    /// first poll so split siblings stop. The caller keeps the pinned owner in the immediate
    /// await scope, so completion or cancellation releases the child before subsequent work.
    pub(crate) async fn worker<T, E>(
        &self,
        work: Pin<&mut impl Future<Output = Result<T, BleHostError<E>>>>,
    ) -> Result<T, BleHostError<E>> {
        let cancel = crate::host::OnDrop::new(|| self.fail());
        let result = self.run(work).await;
        cancel.defuse();
        if result.is_err() {
            self.fail();
        }
        result
    }

    /// Guards a retained link's `command` using `live` and the manager's `changed` pulse.
    /// Stale before polling returns Disconnected without controller I/O. Identity loss
    /// or cancellation after any poll permanently fails the host, since generic HCI
    /// admission/transport futures cannot prove whether a command was submitted.
    /// The executor-local check/poll/wait registration has no intervening yield. The caller
    /// owns the pinned command through this immediate await and drops it on every exit.
    pub(crate) async fn link<T, E>(
        &self,
        changed: &LocalManualResetEvent,
        live: impl Fn() -> bool,
        mut command: Pin<&mut impl Future<Output = Result<T, BleHostError<E>>>>,
    ) -> Result<T, BleHostError<E>> {
        self.run(core::pin::pin!(async {
            let polled = Cell::new(false);
            let cancel = crate::host::OnDrop::new(|| {
                if polled.get() {
                    self.fail();
                }
            });
            let result = loop {
                let checked = poll_fn(|cx| {
                    if !live() {
                        return Poll::Ready(Err(if polled.get() {
                            self.fail();
                            Error::InvalidState
                        } else {
                            Error::Disconnected
                        }
                        .into()));
                    }
                    polled.set(true);
                    let result = command.as_mut().poll(cx);
                    if !live() {
                        self.fail();
                        return Poll::Ready(Err(Error::InvalidState.into()));
                    }
                    result
                });
                match select(checked, changed.wait()).await {
                    Either::First(result) => break result,
                    Either::Second(()) => {}
                }
            };
            cancel.defuse();
            result
        }))
        .await
    }
}
