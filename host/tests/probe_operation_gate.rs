//! Actual probe operation admission prevents flash maintenance from parking a live connection.
#[allow(dead_code)]
#[path = "../../firmware/ble-probe/src/bridge.rs"]
mod bridge;
#[allow(dead_code)]
#[path = "../../firmware/ble-probe/src/session.rs"]
mod session;
#[allow(dead_code)]
#[path = "../../firmware/ble-probe/src/status.rs"]
mod status;

use core::future::{Future, pending, poll_fn};
use core::pin::pin;
use core::sync::atomic::Ordering;
use core::task::{Context, Poll, Waker};
use std::cell::Cell;

/// A full real event queue cannot hide HCI death or admit mutation after canceled work releases its lease.
#[test]
fn stopped_host_latches_before_operation_drop_and_blocked_terminal_publication() {
    let stopped = Cell::new(false);
    let dropped = Cell::new(false);
    let revision = 1;
    let terminal = dashboard_core::discovery::Event::InspectionEnd {
        revision,
        at_ms: 1,
        cleanup_complete: false,
        disconnect_reason: None,
    };
    bridge::RADIO_READY.store(true, Ordering::Release);
    for _ in 0..8 {
        bridge::EVENTS.try_send(terminal).unwrap();
    }
    let hci = poll_fn(|_| if stopped.get() { Poll::Ready(()) } else { Poll::Pending });
    let work = async {
        let lease = bridge::OPERATION_GATE.connection().unwrap();
        let guard = Released(&dropped);
        pending::<()>().await;
        drop((lease, guard));
    };
    let mut host = pin!(async {
        embassy_futures::select::select(session::latch_completion(hci, bridge::latch_radio_fault), work).await;
        bridge::emit_event(terminal).await;
    });
    let mut cx = Context::from_waker(Waker::noop());
    assert!(host.as_mut().poll(&mut cx).is_pending());
    assert!(bridge::OPERATION_GATE.mutation().is_none());
    stopped.set(true);
    assert!(host.as_mut().poll(&mut cx).is_pending());
    assert!(dropped.get());
    assert!(
        bridge::RADIO_FATAL.load(Ordering::Acquire),
        "fatal visibility waits behind full EVENTS"
    );
    assert!(!bridge::RADIO_READY.load(Ordering::Acquire));
    assert!(
        bridge::OPERATION_GATE.mutation().is_none(),
        "cancellation released unconfirmed link admission"
    );
    assert!(bridge::OPERATION_GATE.connection().is_none());
    while bridge::EVENTS.try_receive().is_ok() {}
    assert!(host.as_mut().poll(&mut cx).is_ready());
    assert!(bridge::OPERATION_GATE.mutation().is_none());
}

/// Test observation of operation future destruction.
struct Released<'a>(
    /// Set on destruction before any terminal publication can complete.
    &'a Cell<bool>,
);
impl Drop for Released<'_> {
    /// Verifies fatal visibility already exists while the losing operation is being destroyed.
    fn drop(&mut self) {
        assert!(bridge::RADIO_FATAL.load(Ordering::Acquire));
        assert!(!bridge::RADIO_READY.load(Ordering::Acquire));
        self.0.set(true);
    }
}

/// Fatal closure is irreversible for idle gates and both kinds of outstanding lease.
#[test]
fn terminal_closure_survives_every_owner_release() {
    for owner in 0..3 {
        let gate = bridge::OperationGate::new();
        let lease = match owner {
            0 => None,
            1 => gate.connection(),
            _ => gate.mutation(),
        };
        gate.close();
        assert!(gate.connection().is_none());
        assert!(gate.mutation().is_none());
        drop(lease);
        gate.close();
        assert!(gate.connection().is_none());
        assert!(gate.mutation().is_none());
    }
}

/// Cross-core fatal closure racing lease release cannot restore idle admission.
#[test]
fn terminal_closure_wins_concurrent_release() {
    for _ in 0..64 {
        let gate = bridge::OperationGate::new();
        let lease = gate.connection().unwrap();
        let barrier = std::sync::Barrier::new(2);
        std::thread::scope(|scope| {
            scope.spawn(|| {
                barrier.wait();
                gate.close();
            });
            scope.spawn(|| {
                barrier.wait();
                drop(lease);
            });
        });
        assert!(gate.connection().is_none());
        assert!(gate.mutation().is_none());
    }
}

/// Either admission order excludes the other owner until its unique lease is dropped.
#[test]
fn connection_and_mutation_are_mutually_exclusive() {
    let gate = bridge::OperationGate::new();
    let connection = gate.connection().expect("idle connection admission");
    assert!(gate.mutation().is_none());
    assert!(gate.connection().is_none());
    drop(connection);
    let mutation = gate.mutation().expect("cleanup released connection");
    assert!(gate.connection().is_none());
    assert!(gate.mutation().is_none());
    drop(mutation);
    assert!(gate.connection().is_some());
}

/// Simultaneous cores cannot both acquire the idle gate; losing admission performs no work.
#[test]
fn simultaneous_admission_has_one_owner() {
    let gate = bridge::OperationGate::new();
    let barrier = std::sync::Barrier::new(2);
    std::thread::scope(|scope| {
        let first = scope.spawn(|| {
            let lease = gate.connection();
            barrier.wait();
            let admitted = lease.is_some();
            barrier.wait();
            admitted
        });
        let second = scope.spawn(|| {
            let lease = gate.mutation();
            barrier.wait();
            let admitted = lease.is_some();
            barrier.wait();
            admitted
        });
        assert_ne!(first.join().unwrap(), second.join().unwrap());
    });
    assert!(gate.mutation().is_some());
}
