//! Affine USB directory/payload exchanges with the single journal owner.
//! These waits have no USB deadline or session cancellation: the requester must
//! reacquire its lease before checking the bus generation or awaiting endpoints.

use dashboard_core::discovery::{capture_store as journal, capture_ui as ui};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;

use super::Buffer;

/// Four independent journal results in slot order; no capture decoding is implied.
pub type Directory = [Result<Option<journal::StoredCapture>, ui::UiError>; 4];

/// Read-only USB journal requests; no erase/program operation exists here.
pub enum UsbRequest {
    /// Fill and return this exclusively owned four-slot directory allocation.
    List(
        /// USB-owned snapshot lease; returned after every slot's result is refreshed.
        &'static mut Directory,
    ),
    /// Load an integrity-checked raw payload into the USB-owned allocation.
    Get {
        /// Logical slot index checked by the journal.
        slot: u8,
        /// Exclusive destination lease returned on both success and failure.
        buffer: &'static mut Buffer,
    },
}

/// USB result releasing storage ownership before endpoint transmission.
pub enum UsbReply {
    /// Same directory lease, containing metadata/empty/error for each logical slot.
    Listed(
        /// Original allocation, released by storage before any endpoint wait.
        &'static mut Directory,
    ),
    /// Loaded metadata/result paired with the returned affine byte allocation.
    Loaded {
        /// Journal result; success identifies the valid prefix length and CRC.
        result: Result<journal::StoredCapture, ui::UiError>,
        /// Original USB destination lease even if the load failed.
        buffer: &'static mut Buffer,
    },
}

/// Capacity-one read-only USB request queue, shared with the sole storage task.
pub static USB_REQUEST: Channel<CriticalSectionRawMutex, UsbRequest, 1> = Channel::new();
/// Capacity-one response queue returning each lease before any host wait.
pub static USB_REPLY: Channel<CriticalSectionRawMutex, UsbReply, 1> = Channel::new();

/// Refreshes `directory` through the journal owner and restores its exclusive lease.
/// Panics if the lease is absent or a mismatched reply violates the sole-requester
/// contract. Must not be cancelled, including during USB reset: no session guard
/// or deadline wraps this exchange. Once returned, storage is free to process UI
/// requests while the caller holds the snapshot through any endpoint waits.
pub async fn list(directory: &mut Option<&'static mut Directory>) {
    USB_REQUEST
        .send(UsbRequest::List(directory.take().expect("USB directory lease")))
        .await;
    let UsbReply::Listed(returned) = USB_REPLY.receive().await else {
        panic!("USB list reply")
    };
    *directory = Some(returned);
}
