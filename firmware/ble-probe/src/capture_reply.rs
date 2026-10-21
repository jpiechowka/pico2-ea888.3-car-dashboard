//! One storage-owned reply visits core1 by exclusive reference, like byte leases.
use dashboard_core::discovery::{capture_store, capture_ui as ui};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;

/// Fixed 8192-byte capture payload lease, distinct from framebuffer ownership.
pub type Buffer = [u8; capture_store::PAYLOAD_MAX];

/// Single storage-owned response envelope; core1 borrows it until returning it.
pub struct Reply {
    /// UI request generation; stale responses still return their byte lease.
    pub id: ui::RequestId,
    /// Operation-specific result; failed I/O does not erase the returned buffer field.
    pub completion: ui::Completion,
    /// Exclusive payload bytes transferred back from storage, when an operation used them.
    pub buffer: Option<&'static mut Buffer>,
    /// Valid encoded prefix length for a load; zero until that operation succeeds.
    pub length: usize,
}
impl Reply {
    /// Returns an empty failure-initialized envelope with no byte lease and ID zero.
    pub const fn new() -> Self {
        Self {
            id: 0,
            completion: ui::Completion::Failed(ui::UiError::Storage),
            buffer: None,
            length: 0,
        }
    }
}

/// Bidirectional capacity-one exchange for exactly one affine response envelope.
pub struct Mailbox {
    /// Storage-to-core1 publication; the envelope cannot be reused while queued.
    ready: Channel<CriticalSectionRawMutex, &'static mut Reply, 1>,
    /// Core1-to-storage acknowledgement returning exclusive envelope ownership.
    returned: Channel<CriticalSectionRawMutex, &'static mut Reply, 1>,
}
impl Mailbox {
    /// Returns an empty exchange; the sole storage owner supplies the reply object.
    pub const fn new() -> Self {
        Self {
            ready: Channel::new(),
            returned: Channel::new(),
        }
    }

    /// Single permanent storage task; never cancel a publication or reuse its
    /// record before core1 returns it. No lock is held across either wait.
    /// Moves `reply` to core1 and returns that exclusive envelope after consumption.
    pub async fn publish(
        &self,
        reply: &'static mut Reply,
    ) -> &'static mut Reply {
        self.ready.send(reply).await;
        self.returned.receive().await
    }

    /// The callback takes any returned byte lease even for a stale completion.
    /// Return the record synchronously on every normal path, before rendering.
    /// Invokes `consume` with a mutable envelope when available, then returns it
    /// to storage. Returns false for no reply, true after consumption. Callback
    /// panic prevents return; a full return queue panics as an ownership violation.
    pub fn consume(
        &self,
        consume: impl FnOnce(&mut Reply),
    ) -> bool {
        let Ok(reply) = self.ready.try_receive() else {
            return false;
        };
        consume(reply);
        // One affine record is in flight, so its return queue must be empty.
        assert!(self.returned.try_send(reply).is_ok());
        true
    }
}
