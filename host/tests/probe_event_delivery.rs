//! Actual cross-core event forwarding: lazy ownership, FIFO backpressure and cancellation.
#[allow(dead_code)]
#[path = "../../firmware/ble-probe/src/bridge.rs"]
mod bridge;
#[allow(dead_code)]
#[path = "../../firmware/ble-probe/src/status.rs"]
mod status;

use core::future::Future;
use core::pin::pin;
use core::task::{Context, Poll, Waker};

use dashboard_core::discovery::Event;

/// Returns distinguishable terminal evidence for `revision`; no live cleanup is implied.
fn event(revision: u32) -> Event {
    Event::InspectionEnd {
        revision,
        at_ms: u64::from(revision),
        cleanup_complete: false,
        disconnect_reason: Some(0x16),
    }
}

/// Drains one event and verifies all fields against `revision`, including the valid false state.
fn receive(revision: u32) {
    let Event::InspectionEnd {
        revision: actual,
        at_ms,
        cleanup_complete,
        disconnect_reason,
    } = bridge::EVENTS.try_receive().expect("queued event")
    else {
        panic!("unexpected event variant");
    };
    assert_eq!(actual, revision);
    assert_eq!(at_ms, u64::from(revision));
    assert!(!cleanup_complete);
    assert_eq!(disconnect_reason, Some(0x16));
}

/// Creation is lazy; full queues preserve order, and cancellation cannot publish a pending event later.
#[test]
fn forwarding_preserves_lazy_send_fifo_and_cancellation() {
    let mut cx = Context::from_waker(Waker::noop());
    let unpolled = bridge::emit_event(event(99));
    assert!(bridge::EVENTS.try_receive().is_err());
    drop(unpolled);
    for revision in 1..=8 {
        let mut send = pin!(bridge::emit_event(event(revision)));
        assert_eq!(send.as_mut().poll(&mut cx), Poll::Ready(()));
    }
    {
        let mut canceled = pin!(bridge::emit_event(event(100)));
        assert!(canceled.as_mut().poll(&mut cx).is_pending());
    }
    let mut delayed = pin!(bridge::emit_event(event(9)));
    assert!(delayed.as_mut().poll(&mut cx).is_pending());
    receive(1);
    assert_eq!(delayed.as_mut().poll(&mut cx), Poll::Ready(()));
    for revision in 2..=9 {
        receive(revision);
    }
    assert!(bridge::EVENTS.try_receive().is_err());
    let mut next = pin!(bridge::emit_event(event(10)));
    assert_eq!(next.as_mut().poll(&mut cx), Poll::Ready(()));
    receive(10);
}
