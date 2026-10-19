#[path = "../../firmware/ble-probe/src/session.rs"]
mod session;

use core::future::{Future, pending, poll_fn, ready};
use core::pin::pin;
use core::task::{Context, Poll, Waker};
use std::cell::Cell;

#[test]
fn connection_event_failure_is_observed_while_mtu_is_still_pending() {
    let run_called = Cell::new(false);
    let mut session = pin!(session::inspect_with_events(
        pending::<Result<(), &str>>(),
        |_| {
            run_called.set(true);
            ready(Ok::<_, &str>(true))
        },
        ready("connection parameters rejected"),
    ));
    assert_eq!(
        session.as_mut().poll(&mut Context::from_waker(Waker::noop())),
        Poll::Ready(Err("connection parameters rejected"))
    );
    assert!(!run_called.get());
}

#[test]
fn event_work_progresses_during_startup_and_inventory_without_restarting_startup() {
    let stage = Cell::new(0);
    let startup_polls = Cell::new(0);
    let event_polls = Cell::new(0);
    let client_runs = Cell::new(0);
    let mut session = pin!(session::inspect_with_events(
        poll_fn(|_| {
            startup_polls.set(startup_polls.get() + 1);
            if stage.get() == 0 {
                Poll::Pending
            } else {
                Poll::Ready(Ok::<_, &str>(42))
            }
        }),
        |client| {
            assert_eq!(client, 42);
            client_runs.set(client_runs.get() + 1);
            poll_fn(|_| {
                if stage.get() < 2 {
                    Poll::Pending
                } else {
                    Poll::Ready(Ok(true))
                }
            })
        },
        poll_fn(|_| {
            event_polls.set(event_polls.get() + 1);
            Poll::<&str>::Pending
        }),
    ));
    let mut cx = Context::from_waker(Waker::noop());
    assert_eq!(session.as_mut().poll(&mut cx), Poll::Pending);
    assert_eq!((startup_polls.get(), event_polls.get(), client_runs.get()), (1, 1, 0));
    stage.set(1);
    assert_eq!(session.as_mut().poll(&mut cx), Poll::Pending);
    assert_eq!((startup_polls.get(), event_polls.get(), client_runs.get()), (2, 2, 1));
    stage.set(2);
    assert_eq!(session.as_mut().poll(&mut cx), Poll::Ready(Ok(true)));
    assert_eq!(startup_polls.get(), 2);
    assert_eq!(client_runs.get(), 1);
}

#[test]
fn immediate_mtu_failure_keeps_stage_error_and_never_starts_discovery() {
    let mut session = pin!(session::inspect_with_events(
        ready(Err::<(), _>("MTU error")),
        |_| -> core::future::Ready<Result<bool, &str>> { panic!("failed startup must not discover") },
        ready("disconnect notification"),
    ));
    assert_eq!(
        session.as_mut().poll(&mut Context::from_waker(Waker::noop())),
        Poll::Ready(Err("MTU error"))
    );
}

struct DropFlag<'a>(&'a Cell<bool>);
impl Drop for DropFlag<'_> {
    fn drop(&mut self) { self.0.set(true); }
}

#[test]
fn connection_failure_drops_the_active_work_before_returning() {
    for during_startup in [true, false] {
        let work_dropped = Cell::new(false);
        let events_dropped = Cell::new(false);
        let work_guard = DropFlag(&work_dropped);
        let events_guard = DropFlag(&events_dropped);
        let mut session = pin!(session::inspect_with_events(
            async move {
                let guard = work_guard;
                if during_startup {
                    pending::<()>().await;
                }
                Ok::<_, &str>(guard)
            },
            |guard| {
                async move {
                    let _guard = guard;
                    pending::<Result<bool, &str>>().await
                }
            },
            async move {
                let _guard = events_guard;
                "disconnect"
            },
        ));
        assert_eq!(
            session.as_mut().poll(&mut Context::from_waker(Waker::noop())),
            Poll::Ready(Err("disconnect"))
        );
        assert!(work_dropped.get(), "active work must be gone before link cleanup");
        assert!(events_dropped.get());
    }
}

#[test]
fn completed_inventory_drops_event_consumer_before_returning() {
    let events_dropped = Cell::new(false);
    let events_guard = DropFlag(&events_dropped);
    let mut session = pin!(session::inspect_with_events(
        ready(Ok::<_, &str>(42)),
        |client| ready(Ok(client)),
        async move {
            let _guard = events_guard;
            pending::<&str>().await
        },
    ));
    assert_eq!(
        session.as_mut().poll(&mut Context::from_waker(Waker::noop())),
        Poll::Ready(Ok(42))
    );
    assert!(events_dropped.get());
}

#[test]
fn outer_cancellation_drops_both_waits_in_mtu_and_discovery_phases() {
    for during_startup in [true, false] {
        let work_dropped = Cell::new(false);
        let events_dropped = Cell::new(false);
        {
            let work_guard = DropFlag(&work_dropped);
            let events_guard = DropFlag(&events_dropped);
            let mut session = pin!(session::inspect_with_events(
                async move {
                    let guard = work_guard;
                    if during_startup {
                        pending::<()>().await;
                    }
                    Ok::<_, &str>(guard)
                },
                |guard| {
                    async move {
                        let _guard = guard;
                        pending::<Result<bool, &str>>().await
                    }
                },
                async move {
                    let _guard = events_guard;
                    pending::<&str>().await
                },
            ));
            assert_eq!(
                session.as_mut().poll(&mut Context::from_waker(Waker::noop())),
                Poll::Pending
            );
        }
        assert!(work_dropped.get());
        assert!(events_dropped.get());
    }
}

#[test]
fn borrowed_startup_and_event_futures_are_dropped_before_outer_cleanup() {
    for during_startup in [true, false] {
        let work_dropped = Cell::new(false);
        let events_dropped = Cell::new(false);
        let mut inspection = pin!(async {
            let result = {
                let work_guard = DropFlag(&work_dropped);
                let events_guard = DropFlag(&events_dropped);
                let startup = pin!(async move {
                    let guard = work_guard;
                    if during_startup {
                        pending::<()>().await;
                    }
                    Ok::<_, &str>(guard)
                });
                let events = pin!(async move {
                    let _guard = events_guard;
                    "disconnect"
                });
                session::inspect_with_events(
                    startup,
                    |guard| {
                        async move {
                            let _guard = guard;
                            pending::<Result<bool, &str>>().await
                        }
                    },
                    events,
                )
                .await
            };
            // Cleanup starts outside the scope owning the pinned operations.
            assert!(work_dropped.get());
            assert!(events_dropped.get());
            result
        });
        assert_eq!(
            inspection.as_mut().poll(&mut Context::from_waker(Waker::noop())),
            Poll::Ready(Err("disconnect"))
        );
    }
}

#[test]
fn every_attempt_publishes_end_after_cleanup_ack_including_failed_connect() {
    use std::cell::RefCell;

    use dashboard_core::discovery::Event;
    for (_connected, cleanup_ok, reason) in [
        (false, true, None),
        (true, true, Some(0x13)),
        (false, false, None),
        (true, false, None),
    ] {
        let ack = Cell::new(false);
        let events = RefCell::new(Vec::new());
        let mut finish = pin!(session::finish_cleanup(
            77,
            poll_fn(|_| {
                if ack.get() {
                    Poll::Ready(cleanup_ok)
                } else {
                    Poll::Pending
                }
            }),
            || 1234,
            || reason,
            async |event| events.borrow_mut().push(event)
        ));
        let mut cx = Context::from_waker(Waker::noop());
        assert_eq!(finish.as_mut().poll(&mut cx), Poll::Pending);
        assert!(
            events.borrow().is_empty(),
            "no successful End before actual cleanup acknowledgement"
        );
        ack.set(true);
        assert_eq!(finish.as_mut().poll(&mut cx), Poll::Ready(cleanup_ok));
        assert!(
            matches!(events.borrow().as_slice(), [Event::InspectionEnd { revision: 77, at_ms: 1234, cleanup_complete, disconnect_reason }] if *cleanup_complete == cleanup_ok && *disconnect_reason == reason)
        );
    }
}
