//! Exercise the actual firmware reply handoff, including hidden late completion.
use core::future::Future;
use core::pin::pin;
use core::task::{Context, Poll, Waker};

use dashboard_core::discovery::capture_ui as ui;

#[path = "../../firmware/ble-probe/src/capture_reply.rs"]
mod mailbox;

#[test]
fn reply_is_borrowed_until_consumed_and_the_same_record_returns_to_its_owner() {
    let channel = mailbox::Mailbox::new();
    let reply = Box::leak(Box::new(mailbox::Reply::new()));
    let address = core::ptr::from_mut(reply);
    reply.id = 9;
    reply.completion = ui::Completion::Listed {
        slots: [ui::Slot::Saved { generation: 7 }; 4],
        identities: [None; 4],
    };
    let mut send = pin!(channel.publish(reply));
    let mut cx = Context::from_waker(Waker::noop());
    assert!(send.as_mut().poll(&mut cx).is_pending());
    // Merely enqueueing a reply cannot let the storage owner reuse it.
    assert!(send.as_mut().poll(&mut cx).is_pending());
    assert!(channel.consume(|received| {
        assert_eq!(core::ptr::from_mut(received), address);
        assert_eq!(received.id, 9);
        assert!(matches!(received.completion, ui::Completion::Listed { .. }));
        received.length = 123;
    }));
    assert!(!channel.consume(|_| panic!("no duplicate reply")));
    let Poll::Ready(returned) = send.as_mut().poll(&mut cx) else {
        panic!("reply not returned")
    };
    assert_eq!(core::ptr::from_mut(returned), address);
    assert_eq!(returned.length, 123);
}

#[test]
fn failed_or_stale_reply_returns_byte_lease_after_back_recovery_and_timeout() {
    for hide in [ui::Input::Back, ui::Input::Recover, ui::Input::Click] {
        for stale in [false, true] {
            let channel = mailbox::Mailbox::new();
            let reply = Box::leak(Box::new(mailbox::Reply::new()));
            let bytes = Box::leak(Box::new([0xA5; dashboard_core::discovery::capture_store::PAYLOAD_MAX]));
            let address = core::ptr::from_mut(bytes);
            let mut captures = ui::CaptureUi::new();
            let context = ui::Context {
                save: ui::SaveAvailability::NoPeer,
                revision: 1,
            };
            let Some(ui::Action::Refresh { id }) = captures.input(ui::Input::XHold, 1, context) else {
                panic!("refresh missing")
            };
            if hide == ui::Input::Click {
                captures.tick(1 + ui::OPERATION_TIMEOUT_MS);
            } else {
                captures.input(hide, 2, context);
            }
            let screen = captures.screen();
            reply.id = if stale { id + 1 } else { id };
            reply.completion = ui::Completion::Failed(ui::UiError::Storage);
            reply.buffer = Some(bytes);
            let mut send = pin!(channel.publish(reply));
            let mut cx = Context::from_waker(Waker::noop());
            assert!(send.as_mut().poll(&mut cx).is_pending());
            let mut restored = None;
            assert!(channel.consume(|received| {
                restored = received.buffer.take();
                assert_eq!(captures.complete_ref(received.id, &received.completion), !stale);
            }));
            let restored = restored.unwrap();
            assert_eq!(core::ptr::from_mut(restored), address);
            assert!(restored.iter().all(|b| *b == 0xA5));
            assert_eq!(captures.screen(), screen);
            assert_eq!(captures.pending().is_some(), stale);
            let Poll::Ready(returned) = send.as_mut().poll(&mut cx) else {
                panic!("reply not returned")
            };
            assert!(returned.buffer.is_none());
        }
    }
}

#[test]
fn repeated_handoffs_do_not_replay_old_replies_or_lose_a_lease() {
    let channel = mailbox::Mailbox::new();
    let mut reply = Box::leak(Box::new(mailbox::Reply::new()));
    for id in 1..=32 {
        reply.id = id;
        reply.completion = ui::Completion::Reset;
        let mut send = pin!(channel.publish(reply));
        let mut cx = Context::from_waker(Waker::noop());
        assert!(send.as_mut().poll(&mut cx).is_pending());
        assert!(channel.consume(|received| assert_eq!(received.id, id)));
        let Poll::Ready(returned) = send.as_mut().poll(&mut cx) else {
            panic!("reply not returned")
        };
        reply = returned;
    }
    assert!(!channel.consume(|_| panic!("old reply replayed")));
}
