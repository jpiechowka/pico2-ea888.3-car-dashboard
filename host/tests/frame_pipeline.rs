use std::cell::Cell;
use std::future::{Future, poll_fn};
use std::pin::pin;
use std::task::{Context, Poll, Waker};

use dashboard_host_validation::display::pipeline::{BufferId, BufferState, FrameLifecycle, after_first_poll};

#[test]
fn two_buffers_overlap_but_inflight_storage_cannot_be_reacquired() {
    let mut lifecycle = FrameLifecycle::new();
    let a = lifecycle.begin(BufferId::A).unwrap();
    assert!(lifecycle.begin(BufferId::A).is_err());
    lifecycle.ready(a).unwrap();
    lifecycle.queue(a).unwrap();
    assert!(lifecycle.complete(a).is_err());
    lifecycle.started(a).unwrap();
    let b = lifecycle.begin(BufferId::B).unwrap();
    lifecycle.ready(b).unwrap();
    assert!(lifecycle.queue(b).is_err());
    assert_eq!(lifecycle.state(BufferId::A), BufferState::InFlight(a));
    assert!(lifecycle.begin(BufferId::A).is_err());
    lifecycle.complete(a).unwrap();
    lifecycle.queue(b).unwrap();
    lifecycle.started(b).unwrap();
    lifecycle.complete(b).unwrap();
    assert!(lifecycle.begin(BufferId::A).is_ok());
}

#[test]
fn old_wrong_duplicate_and_out_of_order_events_leave_ownership_unchanged() {
    let mut lifecycle = FrameLifecycle::new();
    let a = lifecycle.begin(BufferId::A).unwrap();
    let b = lifecycle.begin(BufferId::B).unwrap();
    assert!(lifecycle.queue(a).is_err());
    assert!(lifecycle.started(a).is_err());
    lifecycle.ready(a).unwrap();
    assert!(lifecycle.ready(a).is_err());
    lifecycle.queue(a).unwrap();
    assert!(lifecycle.started(b).is_err());
    assert!(lifecycle.complete(b).is_err());
    lifecycle.started(a).unwrap();
    assert!(lifecycle.started(a).is_err());
    lifecycle.complete(a).unwrap();
    assert!(lifecycle.complete(a).is_err());
    let next_a = lifecycle.begin(BufferId::A).unwrap();
    lifecycle.ready(next_a).unwrap();
    lifecycle.queue(next_a).unwrap();
    lifecycle.started(next_a).unwrap();
    assert!(lifecycle.complete(a).is_err());
    assert_eq!(lifecycle.state(BufferId::A), BufferState::InFlight(next_a));
}

#[test]
fn timeout_quarantines_the_exact_buffer_without_releasing_it() {
    for started in [false, true] {
        let mut lifecycle = FrameLifecycle::new();
        let a = lifecycle.begin(BufferId::A).unwrap();
        lifecycle.ready(a).unwrap();
        lifecycle.queue(a).unwrap();
        if started {
            lifecycle.started(a).unwrap();
        }
        lifecycle.quarantine(a).unwrap();
        assert!(lifecycle.complete(a).is_err());
        assert!(lifecycle.begin(BufferId::A).is_err());
        assert_eq!(lifecycle.state(BufferId::A), BufferState::Quarantined(a));
        let b = lifecycle.begin(BufferId::B).unwrap();
        lifecycle.ready(b).unwrap();
        // A fault requires resetting the peripheral; no new transfers may start.
        assert!(lifecycle.queue(b).is_err());
    }
}

#[test]
fn started_callback_runs_after_the_first_payload_poll_once() {
    for immediate in [false, true] {
        let polls = Cell::new(0);
        let callbacks = Cell::new(0);
        let complete = Cell::new(immediate);
        let payload = poll_fn(|_| {
            polls.set(polls.get() + 1);
            if complete.get() { Poll::Ready(17) } else { Poll::Pending }
        });
        let mut transfer = pin!(after_first_poll(payload, || {
            assert_eq!(polls.get(), 1);
            callbacks.set(callbacks.get() + 1);
        }));
        let mut context = Context::from_waker(Waker::noop());
        assert_eq!(polls.get(), 0);
        assert_eq!(callbacks.get(), 0);
        assert_eq!(
            transfer.as_mut().poll(&mut context),
            if immediate { Poll::Ready(17) } else { Poll::Pending }
        );
        assert_eq!(callbacks.get(), 1);
        if !immediate {
            assert_eq!(transfer.as_mut().poll(&mut context), Poll::Pending);
            complete.set(true);
            assert_eq!(transfer.as_mut().poll(&mut context), Poll::Ready(17));
            assert_eq!(callbacks.get(), 1);
        }
    }
}

#[test]
fn bounded_channel_moves_the_lease_and_retains_delayed_completion() {
    use dashboard_host_validation::display::FrameBuffer;
    use dashboard_host_validation::display::pipeline::FrameLease;
    use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
    use embassy_sync::channel::Channel;
    let mut a = Box::new(FrameBuffer::new());
    let mut b = Box::new(FrameBuffer::new());
    let [mut a, mut b] = FrameLease::pair(&mut a, &mut b);
    a.buffer()[0] = 123;
    let mut lifecycle = FrameLifecycle::new();
    let token = lifecycle.begin(a.id()).unwrap();
    lifecycle.ready(token).unwrap();
    lifecycle.queue(token).unwrap();
    let requests = Channel::<CriticalSectionRawMutex, _, 1>::new();
    let completions = Channel::<CriticalSectionRawMutex, _, 1>::new();
    assert!(
        requests
            .try_send(a.submit(
                token,
                dashboard_host_validation::display::telemetry::RenderTrace::instant(10)
            ))
            .is_ok()
    );
    let request = requests.try_receive().unwrap();
    lifecycle.started(request.token()).unwrap();
    for _ in 0..100 {
        assert!(completions.try_receive().is_err());
        assert!(lifecycle.begin(BufferId::A).is_err());
        b.buffer()[0] = b.buffer()[0].wrapping_add(1);
        assert_eq!(request.pixels()[0], 123);
    }
    assert!(completions.try_send(request).is_ok());
    // Completion remains available until consumed; there is no latest-wins slot.
    let completed = completions.try_receive().unwrap();
    lifecycle.complete(completed.token()).unwrap();
    let mut a = completed.into_lease();
    a.buffer()[0] = 55;
    assert!(completions.try_receive().is_err());
    assert_eq!(a.buffer()[0], 55);
}
