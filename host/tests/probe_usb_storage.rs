//! Actual USB/storage queues preserve affine directory ownership across bus changes.

use core::future::{Future, poll_fn};
use core::pin::pin;
use core::task::{Context, Poll, Waker};
use std::cell::Cell;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::task::Wake;

use dashboard_core::discovery::capture_store as journal;
use dashboard_core::discovery::capture_ui::UiError;

/// Same bounded payload allocation used by the production storage adapter.
type Buffer = [u8; journal::PAYLOAD_MAX];
/// Production USB generation guard, including its own bounded exhaustion regression.
#[path = "../../firmware/ble-probe/src/usb_session.rs"]
mod usb_session;
/// Production exchange without hardware or physical flash access.
#[path = "../../firmware/ble-probe/src/usb_storage.rs"]
mod usb_storage;

use usb_storage::{Directory, USB_REPLY, USB_REQUEST, UsbReply, UsbRequest};

/// Tests serialize the same capacity-one queues; no task may consume another test's lease.
static OWNER: Mutex<()> = Mutex::new(());

/// A reset during storage work cannot cancel the exchange or leak stale bytes to a new host.
#[test]
fn directory_returns_before_changed_session_rejects_output() {
    let _owner = OWNER.lock().unwrap();
    let directory = Box::leak(Box::new([Ok(None); 4]));
    let address = directory.as_ptr();
    let mut owned = Some(directory);
    let session = usb_session::Session::new();
    session.change(true);
    let epoch = session.current().unwrap();
    let writes = Cell::new(0);
    let mut cx = Context::from_waker(Waker::noop());
    {
        let mut response = pin!(async {
            usb_storage::list(&mut owned).await;
            let slots = owned.as_deref().expect("returned before endpoint wait");
            assert_eq!(slots.as_ptr(), address);
            assert_eq!(slots[0], Err(UiError::Unsupported));
            session
                .guard(
                    epoch,
                    poll_fn(|_| {
                        writes.set(writes.get() + 1);
                        Poll::Ready(())
                    }),
                )
                .await
        });
        assert!(response.as_mut().poll(&mut cx).is_pending());
        let UsbRequest::List(slots) = USB_REQUEST.try_receive().unwrap() else {
            panic!("directory request expected")
        };
        assert_eq!(slots.as_ptr(), address);
        slots[0] = Err(UiError::Unsupported);
        session.change(false);
        session.change(true);
        assert!(pin!(session.changed(epoch)).as_mut().poll(&mut cx).is_ready());
        assert!(
            response.as_mut().poll(&mut cx).is_pending(),
            "storage must finish despite reset"
        );
        USB_REPLY
            .try_send(UsbReply::Listed(slots))
            .unwrap_or_else(|_| panic!("reply queue full"));
        assert_eq!(response.as_mut().poll(&mut cx), Poll::Ready(Err(usb_session::Changed)));
    }
    assert_eq!(writes.get(), 0);
    assert_eq!(owned.unwrap().as_ptr(), address);
    assert!(USB_REQUEST.try_receive().is_err());
    assert!(USB_REPLY.try_receive().is_err());
}

/// A pending endpoint holds the returned snapshot without retaining the storage owner or queues.
#[test]
fn stalled_host_keeps_snapshot_and_storage_can_process_another_request() {
    let _owner = OWNER.lock().unwrap();
    let mut owned = Some(Box::leak(Box::new([Ok(None); 4])));
    let snapshot: Directory = [
        Ok(Some(journal::StoredCapture {
            slot: 0,
            copy: 1,
            generation: u64::MAX,
            length: 8192,
            payload_crc: u32::MAX,
        })),
        Ok(None),
        Err(UiError::Corrupt),
        Err(UiError::Unsupported),
    ];
    let endpoint_ready = Cell::new(false);
    let mut cx = Context::from_waker(Waker::noop());
    {
        let mut response = pin!(async {
            usb_storage::list(&mut owned).await;
            let slots = owned.as_deref().unwrap();
            assert_eq!(*slots, snapshot);
            poll_fn(|_| {
                if endpoint_ready.get() {
                    Poll::Ready(Err::<(), ()>(()))
                } else {
                    Poll::Pending
                }
            })
            .await
        });
        assert!(response.as_mut().poll(&mut cx).is_pending());
        let UsbRequest::List(slots) = USB_REQUEST.try_receive().unwrap() else {
            panic!("list")
        };
        *slots = snapshot;
        USB_REPLY
            .try_send(UsbReply::Listed(slots))
            .unwrap_or_else(|_| panic!("reply queue full"));
        assert!(response.as_mut().poll(&mut cx).is_pending());
        assert!(USB_REQUEST.try_receive().is_err());
        assert!(
            USB_REPLY.try_receive().is_err(),
            "storage reply consumed before endpoint wait"
        );
        // The storage side can accept and return another independently owned byte lease.
        let buffer = Box::leak(Box::new([0; journal::PAYLOAD_MAX]));
        let address = buffer.as_ptr();
        USB_REQUEST
            .try_send(UsbRequest::Get { slot: 3, buffer })
            .unwrap_or_else(|_| panic!("request queue full"));
        let UsbRequest::Get { slot, buffer } = USB_REQUEST.try_receive().unwrap() else {
            panic!("get")
        };
        assert_eq!(slot, 3);
        USB_REPLY
            .try_send(UsbReply::Loaded {
                result: Err(UiError::Corrupt),
                buffer,
            })
            .unwrap_or_else(|_| panic!("reply queue full"));
        let UsbReply::Loaded { result, buffer } = USB_REPLY.try_receive().unwrap() else {
            panic!("loaded")
        };
        assert_eq!(result, Err(UiError::Corrupt));
        assert_eq!(buffer.as_ptr(), address);
        endpoint_ready.set(true);
        assert_eq!(response.as_mut().poll(&mut cx), Poll::Ready(Err(())));
    }
    assert_eq!(
        *owned.unwrap(),
        snapshot,
        "endpoint failure preserves the directory lease and contents"
    );
}

/// Counts real reply-queue wakeups without executing or cancelling the exchange.
#[derive(Default)]
struct WakeCount(
    /// Number of wake requests observed by this test, incremented atomically.
    AtomicUsize,
);
impl Wake for WakeCount {
    /// Records a wake request; the test explicitly polls the retained owner afterward.
    fn wake(self: Arc<Self>) { self.0.fetch_add(1, Ordering::SeqCst); }
}

/// Construction is lazy, a returned reply wakes the waiter, and the same lease can be reused.
#[test]
fn directory_exchange_is_lazy_woken_and_reusable() {
    let _owner = OWNER.lock().unwrap();
    let mut owned = Some(Box::leak(Box::new([Err(UiError::Storage); 4])));
    let address = owned.as_deref().unwrap().as_ptr();
    let wake = Arc::new(WakeCount::default());
    let waker = Waker::from(wake.clone());
    let mut cx = Context::from_waker(&waker);
    for value in [Err(UiError::Unsupported), Ok(None)] {
        {
            let mut request = pin!(usb_storage::list(&mut owned));
            assert!(
                USB_REQUEST.try_receive().is_err(),
                "unpolled future must not transfer ownership"
            );
            assert!(request.as_mut().poll(&mut cx).is_pending());
            let UsbRequest::List(slots) = USB_REQUEST.try_receive().unwrap() else {
                panic!("list")
            };
            assert_eq!(slots.as_ptr(), address);
            slots.fill(value);
            let before = wake.0.load(Ordering::SeqCst);
            USB_REPLY
                .try_send(UsbReply::Listed(slots))
                .unwrap_or_else(|_| panic!("reply queue full"));
            assert!(wake.0.load(Ordering::SeqCst) > before);
            assert!(request.as_mut().poll(&mut cx).is_ready());
        }
        assert_eq!(*owned.as_deref().unwrap(), [value; 4]);
    }
}
