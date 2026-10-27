//! Single core0 journal owner. Byte leases are returned even after failed I/O.
use core::pin::pin;
use core::sync::atomic::Ordering;

use dashboard_core::discovery::names::SavedIdentity;
use dashboard_core::discovery::{capture, capture_store as journal, capture_ui as ui};
use dashboard_pico2::flash_guard::{CheckedFlash, WriteConditions};
use embassy_futures::select::{Either, select};
use embassy_futures::yield_now;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use static_cell::StaticCell;

use crate::{bridge, capture_reply, maintenance, radio};

#[path = "capture_admission.rs"]
/// Full capture-format checks layered over the journal's opaque payload integrity.
mod admission;

pub use capture_reply::{Buffer, Reply as UiReply};
/// Explicit local UI journal operations; only Save/Reset can mutate persistent bytes.
pub enum UiOperation {
    /// Inspect four slot states and decode supported saved identities.
    List,
    /// Load and fully decode one selected supported capture.
    Load(
        /// Logical slot index 0..4; the journal rejects other indices.
        u8,
    ),
    /// Persist an already frozen bounded payload after rechecking both copies.
    Save {
        /// Logical slot index, checked again by journal admission.
        slot: u8,
        /// Initialized prefix of the supplied byte lease; oversized lengths reject.
        length: usize,
        /// Explicit authorization to replace known occupied data, never unknown formats.
        overwrite: bool,
    },
    /// Explicit local destructive reset of both copies in a slot.
    Reset(
        /// Logical slot selected by the UI's separate Reset confirmation.
        u8,
    ),
}
/// Owned UI request with an optional affine payload allocation.
pub struct UiRequest {
    /// Capture UI request identity echoed even on failure.
    pub id: ui::RequestId,
    /// Frozen operation selected before storage admission.
    pub operation: UiOperation,
    /// Exclusive UI buffer; returned through the response on every ordinary path.
    pub buffer: Option<&'static mut Buffer>,
}
/// Read-only USB journal requests; no erase/program operation exists here.
pub enum UsbRequest {
    /// Return opaque journal metadata for all four slots.
    List,
    /// Load an integrity-checked raw payload into the USB-owned allocation.
    Get {
        /// Logical slot index checked by the journal.
        slot: u8,
        /// Exclusive destination lease returned on both success and failure.
        buffer: &'static mut Buffer,
    },
}
/// USB result that releases storage ownership before endpoint transmission.
pub enum UsbReply {
    /// Metadata, empty state or independent error for each physical logical slot.
    Listed(
        /// Four entries in slot order; payload decoding is not implied.
        [Result<Option<journal::StoredCapture>, ui::UiError>; 4],
    ),
    /// Loaded metadata/result paired with the returned affine byte allocation.
    Loaded {
        /// Journal result; success identifies the valid prefix length and CRC.
        result: Result<journal::StoredCapture, ui::UiError>,
        /// Original USB destination lease even if the load failed.
        buffer: &'static mut Buffer,
    },
}
/// Capacity-one local UI operation queue.
pub static UI_REQUEST: Channel<CriticalSectionRawMutex, UiRequest, 1> = Channel::new();
/// Affine result envelope exchange; storage waits until core1 returns the record.
pub static UI_REPLY: capture_reply::Mailbox = capture_reply::Mailbox::new();
/// Capacity-one read-only USB request queue.
pub static USB_REQUEST: Channel<CriticalSectionRawMutex, UsbRequest, 1> = Channel::new();
/// USB response queue returning the payload lease before any host wait.
pub static USB_REPLY: Channel<CriticalSectionRawMutex, UsbReply, 1> = Channel::new();
/// Single payload allocation exclusively circulated between UI and storage.
static UI_BYTES: StaticCell<Buffer> = StaticCell::new();
/// Returns the zeroed UI payload lease once; panics on repeated initialization.
pub fn ui_buffer() -> &'static mut Buffer { UI_BYTES.init_with(|| [0; journal::PAYLOAD_MAX]) }

/// Sole journal backend; no concurrent owner may interleave inspection and mutation.
struct Storage {
    /// Capture-only checked physical flash handle.
    flash: CheckedFlash,
}
#[derive(Debug)]
/// Hardware admission failure or mutation barrier/operation failure.
enum Error {
    /// Direct read or qualification failure from the checked handle.
    Flash(
        /// Original flash guard error.
        dashboard_pico2::flash_guard::Error,
    ),
    /// One erase/page operation failed before or during its maintenance ticket.
    Maintenance(
        /// Original barrier or nested flash failure.
        maintenance::Error<dashboard_pico2::flash_guard::Error>,
    ),
}
impl journal::Storage for Storage {
    /// Physical flash or owner/deadline failure retained by the journal.
    type Error = Error;

    /// Reads physical `offset` into `out`, then yields. Returns the checked flash
    /// result; the synchronous copy ends before yielding and never spans an await.
    async fn read(
        &mut self,
        offset: u32,
        out: &mut [u8],
    ) -> Result<(), Error> {
        let result = self.flash.read(offset, out).map_err(Error::Flash);
        yield_now().await;
        result
    }

    /// Erases one sector at physical `offset` under its own maintenance ticket.
    /// Returns checked mutation/barrier failure; never cancel during restoration.
    async fn erase_sector(
        &mut self,
        offset: u32,
    ) -> Result<(), Error> {
        maintenance::run(|request| self.flash.erase(offset, conditions(request)))
            .await
            .map_err(Error::Maintenance)
    }

    /// Programs SRAM `page` at physical `offset` under its own maintenance ticket.
    /// Returns checked mutation/barrier failure and resumes owners between pages.
    async fn program_page(
        &mut self,
        offset: u32,
        page: &[u8; 256],
    ) -> Result<(), Error> {
        maintenance::run(|request| self.flash.program(offset, page, conditions(request)))
            .await
            .map_err(Error::Maintenance)
    }
}
/// Returns current write permission for `request` plus display/radio quiescence;
/// the flash handle independently rechecks physical peripheral and SRAM bounds.
fn conditions(request: dashboard_core::maintenance::Request) -> WriteConditions {
    WriteConditions {
        maintenance_valid: maintenance::writing(request),
        display_idle: !bridge::DISPLAY_ACTIVE.load(Ordering::SeqCst),
        radio_idle: radio::bus_idle(),
    }
}
/// Returns the UI classification of journal `error`, latching Storage except for
/// expected Empty. Logs detailed hardware/barrier failures without payload bytes.
fn error(error: journal::Error<Error>) -> ui::UiError {
    if !matches!(error, journal::Error::Empty) {
        bridge::STATUS.record(crate::status::Fault::Storage);
    }
    match error {
        journal::Error::Empty => ui::UiError::Empty,
        journal::Error::UnsupportedSchema(_) => ui::UiError::Unsupported,
        journal::Error::Corrupt | journal::Error::AmbiguousGeneration | journal::Error::VerifyFailed => {
            ui::UiError::Corrupt
        }
        journal::Error::Storage(Error::Flash(ref e)) => {
            defmt::warn!("probe.capture flash {:?}", e);
            ui::UiError::Storage
        }
        journal::Error::Storage(Error::Maintenance(ref e)) => {
            match e {
                maintenance::Error::Drain => defmt::warn!("probe.capture display drain deadline"),
                maintenance::Error::Gate(reason) => {
                    defmt::warn!("probe.capture maintenance gate {=str}", reason.label())
                }
                maintenance::Error::Operation(reason) => {
                    defmt::warn!("probe.capture maintenance write {:?}", reason)
                }
            }
            ui::UiError::Storage
        }
        _ => ui::UiError::Storage,
    }
}
/// Returns the UI classification of admission `e`, preserving version/corrupt/
/// changed distinctions and publishing only actual capture fault categories.
fn admission_error(e: admission::Error<Error>) -> ui::UiError {
    let error = match e {
        admission::Error::Journal(e) => error(e),
        admission::Error::Unsupported => ui::UiError::Unsupported,
        admission::Error::Corrupt => ui::UiError::Corrupt,
        admission::Error::Changed => ui::UiError::Changed,
    };
    bridge::STATUS.capture_error(error);
    error
}
/// Fills `slots` in index order using exclusive `storage`, independently retaining
/// each slot's metadata/error. Reads opaque journal integrity, not capture bodies;
/// supported old copies remain readable beside unsupported/damaged siblings.
async fn listing(
    storage: &mut Storage,
    slots: &mut [Result<Option<journal::StoredCapture>, ui::UiError>; 4],
) {
    for (slot, out) in slots.iter_mut().enumerate() {
        *out = match journal::inspect_slot(storage, slot as u8).await {
            Ok(info) => {
                // An intact old copy stays readable. Unknown or damaged copies
                // without one must be explicitly reset locally before save.
                if let Some(selected) = info.selected {
                    Ok(Some(selected))
                } else if info
                    .copies
                    .iter()
                    .any(|c| matches!(c, journal::CopyState::UnsupportedSchema(_)))
                {
                    Err(ui::UiError::Unsupported)
                } else if info.copies.iter().any(|c| *c != journal::CopyState::Empty) {
                    Err(ui::UiError::Corrupt)
                } else {
                    Ok(None)
                }
            }
            Err(e) => Err(error(e)),
        };
        if let Err(e) = *out {
            bridge::STATUS.capture_error(e);
        }
    }
}
/// Fills `slots` and `identities` using `storage`, reusable `bytes`, and `decoder`.
/// Each label requires full capture decoding; failed entries clear their identity
/// and become unavailable/corrupt/unsupported without discarding other slots.
async fn ui_listing(
    storage: &mut Storage,
    decoder: &mut capture::Decoder,
    bytes: &mut Buffer,
    slots: &mut [ui::Slot; 4],
    identities: &mut [Option<SavedIdentity>; 4],
) {
    for (slot, (out, identity)) in slots.iter_mut().zip(identities).enumerate() {
        *identity = None;
        *out = match admission::directory_entry(storage, decoder, slot as u8, bytes).await {
            Ok((state, saved_identity)) => {
                *identity = saved_identity;
                state
            }
            Err(e) => {
                match admission_error(e) {
                    ui::UiError::Corrupt => ui::Slot::Corrupt,
                    ui::UiError::Unsupported => ui::Slot::Unsupported,
                    _ => ui::Slot::Unavailable,
                }
            }
        };
        if matches!(
            *out,
            ui::Slot::Corrupt | ui::Slot::Unsupported | ui::Slot::Unavailable | ui::Slot::SavedBlocked { .. }
        ) {
            bridge::STATUS.record(crate::status::Fault::Storage);
        }
    }
}
#[embassy_executor::task]
/// Owns `flash` indefinitely and serializes UI/USB requests. Returns every byte
/// lease on ordinary errors and awaits UI envelope return before accepting work.
/// Never cancel: a request may be in a flash mutation, restoration or affine reply
/// exchange. This task cannot send BLE traffic or flash outside the capture region.
pub async fn probe_storage_task(flash: CheckedFlash) {
    crate::metrics::measure(
        0,
        pin!(async {
            let mut storage = Storage { flash };
            let mut decoder = capture::Decoder::new();
            /// Single response envelope filled in place to avoid large future copies.
            static REPLY: StaticCell<UiReply> = StaticCell::new();
            let mut reply = REPLY.init_with(UiReply::new);
            loop {
                match select(UI_REQUEST.receive(), USB_REQUEST.receive()).await {
                    Either::First(request) => {
                        handle_ui(&mut storage, &mut decoder, request, reply).await;
                        reply = UI_REPLY.publish(reply).await;
                    }
                    Either::Second(UsbRequest::List) => {
                        // Fill the final reply in place instead of returning and
                        // moving the four-slot array through an async result.
                        let mut reply = UsbReply::Listed([Ok(None); 4]);
                        if let UsbReply::Listed(slots) = &mut reply {
                            listing(&mut storage, slots).await;
                        }
                        USB_REPLY.send(reply).await;
                    }
                    Either::Second(UsbRequest::Get { slot, buffer }) => {
                        let result = journal::load_slot(&mut storage, slot, buffer).await.map_err(error);
                        USB_REPLY.send(UsbReply::Loaded { result, buffer }).await;
                    }
                }
            }
        }),
    )
    .await;
}
/// Executes owned `request` against `storage` with reusable `decoder`, filling
/// `reply` in place. The incoming lease always moves into the reply; errors set a
/// failure completion and latch status. Panics if the previous reply still owns
/// a buffer. No response publication occurs until the operation/restoration ends.
/// Save/Reset hold the exclusive operation gate across admission and restoration;
/// a live connection returns Busy with the original byte lease and no mutation.
/// List/Load only read flash and do not take the mutation gate.
async fn handle_ui(
    storage: &mut Storage,
    decoder: &mut capture::Decoder,
    request: UiRequest,
    reply: &mut UiReply,
) {
    assert!(reply.buffer.is_none());
    reply.id = request.id;
    reply.completion = ui::Completion::Failed(ui::UiError::Storage);
    reply.buffer = request.buffer;
    reply.length = 0;
    // UI eligibility is advisory: a newer Inspect can race a queued mutation.
    // Keep this lease across every page, readback and maintenance restoration.
    let _mutation = if matches!(request.operation, UiOperation::Save { .. } | UiOperation::Reset(_)) {
        let Some(lease) = bridge::OPERATION_GATE.mutation() else {
            reply.completion = ui::Completion::Failed(ui::UiError::Busy);
            return;
        };
        Some(lease)
    } else {
        None
    };
    let result = match request.operation {
        UiOperation::List => {
            match reply.buffer.as_deref_mut() {
                None => Err(ui::UiError::Storage),
                Some(bytes) => {
                    // Fill the final reply in place; the existing affine byte
                    // lease visits each slot without another payload allocation.
                    reply.completion = ui::Completion::Listed {
                        slots: [ui::Slot::Unavailable; 4],
                        identities: [None; 4],
                    };
                    if let ui::Completion::Listed { slots, identities } = &mut reply.completion {
                        ui_listing(storage, decoder, bytes, slots, identities).await;
                    }
                    Ok(())
                }
            }
        }
        UiOperation::Load(slot) => {
            match reply.buffer.as_deref_mut() {
                None => Err(ui::UiError::Storage),
                Some(bytes) => {
                    match admission::load(storage, slot, bytes).await.map_err(admission_error) {
                        Err(e) => Err(e),
                        Ok(meta) => {
                            reply.length = meta.length as usize;
                            match decoder.decode(&bytes[..reply.length]) {
                                Ok(view) => {
                                    reply.completion = ui::Completion::Loaded {
                                        pages: dashboard_core::discovery::saved_page_count(&view),
                                    };
                                    Ok(())
                                }
                                Err(capture::CaptureError::Version) => Err(ui::UiError::Unsupported),
                                Err(_) => Err(ui::UiError::Corrupt),
                            }
                        }
                    }
                }
            }
        }
        UiOperation::Save {
            slot,
            length: used,
            overwrite,
        } => {
            if let Some(bytes) = reply.buffer.as_deref().and_then(|b| b.get(..used)) {
                admission::save(storage, decoder, slot, bytes, overwrite)
                    .await
                    .map_err(admission_error)
                    .and_then(|meta| {
                        // Derive from the same admitted bytes whose journal
                        // write and readback succeeded, never the live peer.
                        let view = decoder.decode(bytes).map_err(|_| ui::UiError::Corrupt)?;
                        reply.completion = ui::Completion::Saved {
                            generation: meta.generation,
                            identity: SavedIdentity::from_capture(&view),
                        };
                        Ok(())
                    })
            } else {
                Err(ui::UiError::Storage)
            }
        }

        UiOperation::Reset(slot) => {
            journal::reset_slot(storage, slot)
                .await
                .map(|()| reply.completion = ui::Completion::Reset)
                .map_err(error)
        }
    };
    if let Err(error) = result {
        bridge::STATUS.capture_error(error);
        reply.completion = ui::Completion::Failed(error);
    }
}
