#[path = "../../firmware/ble-probe/src/usb_session.rs"]
mod usb_session;
use core::future::{Future, poll_fn, ready};
use core::pin::pin;
use core::task::{Context, Poll, Waker};
use std::cell::{Cell, RefCell};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::task::Wake;

use usb_session::{Changed, Session};

#[test]
fn reset_while_read_pending_preserves_the_next_hosts_first_packet() {
    let session = Session::new();
    session.change(true);
    let old_epoch = session.current().unwrap();
    let packet = RefCell::new(None);
    let polls = Cell::new(0);
    let read = poll_fn(|_| {
        polls.set(polls.get() + 1);
        packet.borrow_mut().take().map_or(Poll::Pending, Poll::Ready)
    });
    let mut old = pin!(session.guard(old_epoch, read));
    let mut cx = Context::from_waker(Waker::noop());
    assert_eq!(old.as_mut().poll(&mut cx), Poll::Pending);
    session.change(false);
    session.change(true);
    *packet.borrow_mut() = Some(b'N');
    assert_eq!(old.as_mut().poll(&mut cx), Poll::Ready(Err(Changed)));
    assert_eq!(polls.get(), 1, "old read must not consume the new host's command");
    let mut new = pin!(session.guard(
        session.current().unwrap(),
        poll_fn(|_| packet.borrow_mut().take().map_or(Poll::Pending, Poll::Ready))
    ));
    assert_eq!(new.as_mut().poll(&mut cx), Poll::Ready(Ok(b'N')));
}
#[test]
fn pending_old_write_never_enqueues_after_reconfiguration() {
    let session = Session::new();
    session.change(true);
    let epoch = session.current().unwrap();
    let writable = Cell::new(false);
    let queued = RefCell::new(Vec::new());
    let write = poll_fn(|_| {
        if writable.get() {
            queued.borrow_mut().push(b'O');
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    });
    let mut old = pin!(session.guard(epoch, write));
    let mut cx = Context::from_waker(Waker::noop());
    assert_eq!(old.as_mut().poll(&mut cx), Poll::Pending);
    session.change(false);
    session.change(true);
    writable.set(true);
    assert_eq!(old.as_mut().poll(&mut cx), Poll::Ready(Err(Changed)));
    assert!(queued.borrow().is_empty(), "no old byte may reach newly enabled DPRAM");
}
struct Counter(AtomicUsize);
impl Wake for Counter {
    fn wake(self: Arc<Self>) { self.0.fetch_add(1, Ordering::SeqCst); }

    fn wake_by_ref(self: &Arc<Self>) { self.0.fetch_add(1, Ordering::SeqCst); }
}
#[test]
fn configuration_false_wakes_a_pending_guard_without_endpoint_progress() {
    let session = Session::new();
    assert!(session.current().is_none());
    session.change(true);
    let epoch = session.current().unwrap();
    let count = Arc::new(Counter(AtomicUsize::new(0)));
    let waker = Waker::from(count.clone());
    let mut cx = Context::from_waker(&waker);
    let polls = Cell::new(0);
    let mut guarded = pin!(session.guard(
        epoch,
        poll_fn(|_| {
            polls.set(polls.get() + 1);
            Poll::<()>::Pending
        })
    ));
    assert_eq!(guarded.as_mut().poll(&mut cx), Poll::Pending);
    session.change(false);
    assert!(session.current().is_none());
    assert_eq!(count.0.load(Ordering::SeqCst), 1);
    assert_eq!(guarded.as_mut().poll(&mut cx), Poll::Ready(Err(Changed)));
    assert_eq!(polls.get(), 1);
}
#[test]
fn stale_tokens_never_poll_even_immediately_ready_io_and_current_error_is_preserved() {
    let session = Session::new();
    session.change(true);
    let old = session.current().unwrap();
    session.change(true);
    let mut stale = pin!(session.guard(old, poll_fn(|_| -> Poll<()> { panic!("stale operation polled") })));
    let mut cx = Context::from_waker(Waker::noop());
    assert_eq!(stale.as_mut().poll(&mut cx), Poll::Ready(Err(Changed)));
    let mut current = pin!(session.guard(session.current().unwrap(), ready(Err::<(), _>("endpoint error"))));
    assert_eq!(current.as_mut().poll(&mut cx), Poll::Ready(Ok(Err("endpoint error"))));
}
#[test]
fn session_change_wait_is_woken_and_resolves_for_reset() {
    let session = Session::new();
    session.change(true);
    let epoch = session.current().unwrap();
    let mut changed = pin!(session.changed(epoch));
    let mut cx = Context::from_waker(Waker::noop());
    assert_eq!(changed.as_mut().poll(&mut cx), Poll::Pending);
    session.change(false);
    assert_eq!(changed.as_mut().poll(&mut cx), Poll::Ready(()));
}
