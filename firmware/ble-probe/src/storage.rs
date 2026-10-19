//! Single core0 journal owner. Byte leases are returned even after failed I/O.
use core::pin::pin;
use core::sync::atomic::Ordering;

use dashboard_core::discovery::{capture, capture_store as journal, capture_ui as ui};
use dashboard_pico2::flash_guard::{CheckedFlash, WriteConditions};
use embassy_futures::select::{Either, select};
use embassy_futures::yield_now;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use static_cell::StaticCell;

use crate::{bridge, maintenance, radio};

#[path = "capture_admission.rs"]
mod admission;

pub type Buffer = [u8; journal::PAYLOAD_MAX];
pub enum UiOperation {
    List,
    Load(u8),
    Save { slot: u8, length: usize, overwrite: bool },
    Reset(u8),
}
pub struct UiRequest {
    pub id: ui::RequestId,
    pub operation: UiOperation,
    pub buffer: Option<&'static mut Buffer>,
}
pub struct UiReply {
    pub id: ui::RequestId,
    pub completion: ui::Completion,
    pub buffer: Option<&'static mut Buffer>,
    pub length: usize,
}
pub enum UsbRequest {
    List,
    Get { slot: u8, buffer: &'static mut Buffer },
}
pub enum UsbReply {
    Listed([Result<Option<journal::StoredCapture>, ui::UiError>; 4]),
    Loaded {
        result: Result<journal::StoredCapture, ui::UiError>,
        buffer: &'static mut Buffer,
    },
}
pub static UI_REQUEST: Channel<CriticalSectionRawMutex, UiRequest, 1> = Channel::new();
pub static UI_REPLY: Channel<CriticalSectionRawMutex, UiReply, 1> = Channel::new();
pub static USB_REQUEST: Channel<CriticalSectionRawMutex, UsbRequest, 1> = Channel::new();
pub static USB_REPLY: Channel<CriticalSectionRawMutex, UsbReply, 1> = Channel::new();
static UI_BYTES: StaticCell<Buffer> = StaticCell::new();
pub fn ui_buffer() -> &'static mut Buffer { UI_BYTES.init_with(|| [0; journal::PAYLOAD_MAX]) }

struct Storage {
    flash: CheckedFlash,
}
#[derive(Debug)]
enum Error {
    Flash(dashboard_pico2::flash_guard::Error),
    Maintenance(maintenance::Error<dashboard_pico2::flash_guard::Error>),
}
impl journal::Storage for Storage {
    type Error = Error;

    async fn read(
        &mut self,
        offset: u32,
        out: &mut [u8],
    ) -> Result<(), Error> {
        let result = self.flash.read(offset, out).map_err(Error::Flash);
        yield_now().await;
        result
    }

    async fn erase_sector(
        &mut self,
        offset: u32,
    ) -> Result<(), Error> {
        maintenance::run(|request| self.flash.erase(offset, conditions(request)))
            .await
            .map_err(Error::Maintenance)
    }

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
fn conditions(request: dashboard_core::maintenance::Request) -> WriteConditions {
    WriteConditions {
        maintenance_valid: maintenance::writing(request),
        display_idle: !bridge::DISPLAY_ACTIVE.load(Ordering::SeqCst),
        radio_idle: radio::bus_idle(),
    }
}
fn error(error: journal::Error<Error>) -> ui::UiError {
    match error {
        journal::Error::Empty => ui::UiError::Empty,
        journal::Error::UnsupportedSchema(_) => ui::UiError::Unsupported,
        journal::Error::Corrupt | journal::Error::AmbiguousGeneration | journal::Error::VerifyFailed => {
            ui::UiError::Corrupt
        }
        journal::Error::Storage(Error::Flash(ref e)) => {
            defmt::warn!("probe.capture flash {:?}", defmt::Debug2Format(e));
            ui::UiError::Storage
        }
        journal::Error::Storage(Error::Maintenance(ref e)) => {
            match e {
                maintenance::Error::Drain => defmt::warn!("probe.capture display drain deadline"),
                maintenance::Error::Gate(reason) => {
                    defmt::warn!("probe.capture maintenance gate {:?}", defmt::Debug2Format(reason))
                }
                maintenance::Error::Operation(reason) => {
                    defmt::warn!("probe.capture maintenance write {:?}", defmt::Debug2Format(reason))
                }
            }
            ui::UiError::Storage
        }
        _ => ui::UiError::Storage,
    }
}
fn admission_error(e: admission::Error<Error>) -> ui::UiError {
    match e {
        admission::Error::Journal(e) => error(e),
        admission::Error::Unsupported => ui::UiError::Unsupported,
        admission::Error::Corrupt => ui::UiError::Corrupt,
        admission::Error::Changed => ui::UiError::Changed,
    }
}
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
    }
}
async fn ui_listing(storage: &mut Storage) -> [ui::Slot; 4] {
    let mut slots = [ui::Slot::Unavailable; 4];
    for (slot, out) in slots.iter_mut().enumerate() {
        *out = match admission::inspect(storage, slot as u8).await {
            Ok(info) => info.slot,
            Err(e) => {
                match admission_error(e) {
                    ui::UiError::Corrupt => ui::Slot::Corrupt,
                    ui::UiError::Unsupported => ui::Slot::Unsupported,
                    _ => ui::Slot::Unavailable,
                }
            }
        };
    }
    slots
}
#[embassy_executor::task]
pub async fn probe_storage_task(flash: CheckedFlash) {
    crate::metrics::measure(
        0,
        pin!(async {
            let mut storage = Storage { flash };
            let mut decoder = capture::Decoder::new();
            loop {
                match select(UI_REQUEST.receive(), USB_REQUEST.receive()).await {
                    Either::First(request) => handle_ui(&mut storage, &mut decoder, request).await,
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
async fn handle_ui(
    storage: &mut Storage,
    decoder: &mut capture::Decoder,
    mut request: UiRequest,
) {
    let mut length = 0;
    let result = match request.operation {
        UiOperation::List => Ok(ui::Completion::Listed(ui_listing(storage).await)),
        UiOperation::Load(slot) => {
            match request.buffer.as_deref_mut() {
                None => Err(ui::UiError::Storage),
                Some(bytes) => {
                    match admission::load(storage, slot, bytes).await.map_err(admission_error) {
                        Err(e) => Err(e),
                        Ok(meta) => {
                            length = meta.length as usize;
                            match decoder.decode(&bytes[..length]) {
                                Ok(view) => {
                                    Ok(ui::Completion::Loaded {
                                        pages: dashboard_core::discovery::saved_page_count(&view),
                                    })
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
            if let Some(bytes) = request.buffer.as_deref().and_then(|b| b.get(..used)) {
                admission::save(storage, decoder, slot, bytes, overwrite)
                    .await
                    .map(|meta| {
                        ui::Completion::Saved {
                            generation: meta.generation,
                        }
                    })
                    .map_err(admission_error)
            } else {
                Err(ui::UiError::Storage)
            }
        }

        UiOperation::Reset(slot) => {
            journal::reset_slot(storage, slot)
                .await
                .map(|()| ui::Completion::Reset)
                .map_err(error)
        }
    };
    UI_REPLY
        .send(UiReply {
            id: request.id,
            completion: result.unwrap_or_else(ui::Completion::Failed),
            buffer: request.buffer,
            length,
        })
        .await;
}
