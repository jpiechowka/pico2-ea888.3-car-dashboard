//! One storage-owned reply visits core1 by exclusive reference, like byte leases.
use dashboard_core::discovery::{capture_store, capture_ui as ui};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;

pub type Buffer = [u8; capture_store::PAYLOAD_MAX];

pub struct Reply {
    pub id: ui::RequestId,
    pub completion: ui::Completion,
    pub buffer: Option<&'static mut Buffer>,
    pub length: usize,
}
impl Reply {
    pub const fn new() -> Self {
        Self {
            id: 0,
            completion: ui::Completion::Failed(ui::UiError::Storage),
            buffer: None,
            length: 0,
        }
    }
}

pub struct Mailbox {
    ready: Channel<CriticalSectionRawMutex, &'static mut Reply, 1>,
    returned: Channel<CriticalSectionRawMutex, &'static mut Reply, 1>,
}
impl Mailbox {
    pub const fn new() -> Self {
        Self {
            ready: Channel::new(),
            returned: Channel::new(),
        }
    }

    /// Single permanent storage task; never cancel a publication or reuse its
    /// record before core1 returns it. No lock is held across either wait.
    pub async fn publish(
        &self,
        reply: &'static mut Reply,
    ) -> &'static mut Reply {
        self.ready.send(reply).await;
        self.returned.receive().await
    }

    /// The callback takes any returned byte lease even for a stale completion.
    /// Return the record synchronously on every normal path, before rendering.
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
