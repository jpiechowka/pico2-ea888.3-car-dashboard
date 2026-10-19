//! Read-only CDC transport. USB never sends radio requests or holds a flash lease
//! while waiting for the host. Endpoint SRAM copies finish before each await.
use core::fmt::Write;
use core::pin::pin;

use dashboard_core::discovery::capture_ui::UiError;
use dashboard_core::discovery::capture_wire::*;
use embassy_executor::Spawner;
use embassy_rp::Peri;
use embassy_rp::peripherals::USB;
use embassy_rp::usb::Driver;
use embassy_time::{Duration, Timer, with_timeout};
use embassy_usb::class::cdc_acm::{CdcAcmClass, State};
use embassy_usb::{Builder, Config, Handler, UsbDevice};
use static_cell::StaticCell;

use crate::storage;
use crate::usb_session::{Epoch, Session};
type UsbDriver = Driver<'static, USB>;
type Cdc = CdcAcmClass<'static, UsbDriver>;
static SESSION: Session = Session::new();
struct Events;
impl Handler for Events {
    fn reset(&mut self) { SESSION.change(false); }

    fn configured(
        &mut self,
        configured: bool,
    ) {
        SESSION.change(configured);
    }

    fn enabled(
        &mut self,
        enabled: bool,
    ) {
        if !enabled {
            SESSION.change(false);
        }
    }
}
pub fn start(
    spawner: Spawner,
    peripheral: Peri<'static, USB>,
) {
    static STATE: StaticCell<State<'static>> = StaticCell::new();
    static CONFIG: StaticCell<[u8; 256]> = StaticCell::new();
    static BOS: StaticCell<[u8; 256]> = StaticCell::new();
    static MSOS: StaticCell<[u8; 0]> = StaticCell::new();
    static CONTROL: StaticCell<[u8; 64]> = StaticCell::new();
    static EVENTS: StaticCell<Events> = StaticCell::new();
    static SERIAL: StaticCell<heapless::String<16>> = StaticCell::new();
    static DEVICE: StaticCell<UsbDevice<'static, UsbDriver>> = StaticCell::new();
    static CDC: StaticCell<Cdc> = StaticCell::new();
    let serial = SERIAL.init_with(heapless::String::new);
    let serial = embassy_rp::otp::get_chipid().ok().and_then(|id| {
        write!(serial, "{id:016X}").ok()?;
        Some(serial.as_str())
    });
    let mut config = Config::new(0x2E8A, 0x0009);
    config.manufacturer = Some("Pico dashboard project");
    config.product = Some("Pico BLE Probe Captures");
    config.serial_number = serial;
    // Request the USB 2 full-speed configuration budget; not a measurement.
    config.max_power = 500;
    config.max_packet_size_0 = 64;
    let mut builder = Builder::new(
        Driver::new(peripheral, crate::Irqs),
        config,
        CONFIG.init_with(|| [0; 256]),
        BOS.init_with(|| [0; 256]),
        MSOS.init([]),
        CONTROL.init_with(|| [0; 64]),
    );
    builder.handler(EVENTS.init(Events));
    // Transfer exclusive references to the task owners. Moving these large
    // structs through task constructors duplicates them on the boot stack.
    let cdc = CDC.init_with(|| CdcAcmClass::new(&mut builder, STATE.init(State::new()), 64));
    let device = DEVICE.init_with(|| builder.build());
    spawner.spawn(probe_usb_device_task(device).unwrap());
    spawner.spawn(probe_usb_capture_task(cdc).unwrap());
}
#[embassy_executor::task]
async fn probe_usb_device_task(device: &'static mut UsbDevice<'static, UsbDriver>) {
    crate::metrics::measure(0, pin!(device.run())).await;
}
#[embassy_executor::task]
async fn probe_usb_capture_task(cdc: &'static mut Cdc) {
    static BYTES: StaticCell<storage::Buffer> = StaticCell::new();
    let mut bytes = Some(BYTES.init_with(|| [0; 8192]));
    crate::metrics::measure(
        0,
        pin!(async {
            let mut parser = RequestParser::new();
            let mut packet = [0; 64];
            loop {
                cdc.wait_connection().await;
                let Some(epoch) = SESSION.current() else {
                    Timer::after_millis(20).await;
                    continue;
                };
                parser.reset();
                while same_session(epoch) {
                    let n = match with_timeout(
                        Duration::from_secs(5),
                        SESSION.guard(epoch, cdc.read_packet(&mut packet)),
                    )
                    .await
                    {
                        Ok(Ok(Ok(n))) => n,
                        Ok(Ok(Err(embassy_usb::driver::EndpointError::BufferOverflow))) => {
                            crate::bridge::STATUS.usb_failed(same_session(epoch));
                            break;
                        }
                        Ok(_) => break,
                        Err(_) => {
                            parser.reset();
                            continue;
                        }
                    };
                    if !same_session(epoch) {
                        break;
                    }
                    let mut failed = false;
                    for &byte in &packet[..n] {
                        if let Some(request) = parser.feed(byte)
                            && respond(cdc, epoch, request, &mut bytes).await.is_err()
                        {
                            crate::bridge::STATUS.usb_failed(same_session(epoch));
                            failed = true;
                            break;
                        }
                    }
                    if failed {
                        break;
                    }
                }
                // A timed-out IN packet may remain in USB DPRAM. A fresh bus reset /
                // configuration clears it; never append a new response to old bytes.
                SESSION.changed(epoch).await;
            }
        }),
    )
    .await;
}
fn same_session(epoch: Epoch) -> bool { SESSION.matches(epoch) }
fn status(error: UiError) -> FrameStatus {
    match error {
        UiError::Empty => FrameStatus::Empty,
        UiError::Corrupt => FrameStatus::Corrupt,
        UiError::Unsupported => FrameStatus::Unsupported,
        UiError::Busy => FrameStatus::Busy,
        _ => FrameStatus::StorageError,
    }
}
async fn respond(
    cdc: &mut Cdc,
    epoch: Epoch,
    request: Result<Request, WireError>,
    bytes: &mut Option<&'static mut storage::Buffer>,
) -> Result<(), ()> {
    match request {
        Err(_) => {
            crate::bridge::STATUS.usb_failed(same_session(epoch));
            frame(cdc, epoch, FrameKind::Error, FrameStatus::InvalidRequest, None, 0, &[]).await?;
        }
        Ok(Request::List) => {
            storage::USB_REQUEST.send(storage::UsbRequest::List).await;
            let storage::UsbReply::Listed(slots) = storage::USB_REPLY.receive().await else {
                panic!("USB list reply")
            };
            for (slot, entry) in slots.into_iter().enumerate() {
                match entry {
                    Ok(Some(meta)) => {
                        frame(
                            cdc,
                            epoch,
                            FrameKind::ListEntry,
                            FrameStatus::Ok,
                            Some(slot as u8),
                            meta.generation,
                            &list_payload(meta.length, meta.payload_crc).map_err(|_| ())?,
                        )
                        .await?
                    }
                    Ok(None) => {
                        frame(
                            cdc,
                            epoch,
                            FrameKind::ListEntry,
                            FrameStatus::Empty,
                            Some(slot as u8),
                            0,
                            &[],
                        )
                        .await?
                    }
                    Err(e) => frame(cdc, epoch, FrameKind::ListEntry, status(e), Some(slot as u8), 0, &[]).await?,
                }
            }
        }
        Ok(Request::Get(slot)) => {
            storage::USB_REQUEST
                .send(storage::UsbRequest::Get {
                    slot,
                    buffer: bytes.take().expect("USB byte lease"),
                })
                .await;
            let storage::UsbReply::Loaded { result, buffer } = storage::USB_REPLY.receive().await else {
                panic!("USB get reply")
            };
            *bytes = Some(buffer);
            // The storage owner has already released the buffer and journal.
            match result {
                Ok(meta) => {
                    frame(
                        cdc,
                        epoch,
                        FrameKind::Capture,
                        FrameStatus::Ok,
                        Some(slot),
                        meta.generation,
                        &bytes.as_deref().expect("returned USB lease")[..meta.length as usize],
                    )
                    .await?
                }
                Err(e) => frame(cdc, epoch, FrameKind::Error, status(e), Some(slot), 0, &[]).await?,
            }
        }
    }
    frame(cdc, epoch, FrameKind::End, FrameStatus::Ok, None, 0, &[]).await
}
async fn frame(
    cdc: &mut Cdc,
    epoch: Epoch,
    kind: FrameKind,
    status: FrameStatus,
    slot: Option<u8>,
    generation: u64,
    payload: &[u8],
) -> Result<(), ()> {
    let header = FrameHeader::new(kind, status, slot, generation, payload)
        .map_err(|_| ())?
        .encode()
        .map_err(|_| ())?;
    packet(cdc, epoch, &header).await?;
    for part in payload.chunks(64) {
        packet(cdc, epoch, part).await?;
    }
    if !payload.is_empty() && payload.len() % 64 == 0 {
        packet(cdc, epoch, &[]).await?;
    }
    Ok(())
}
async fn packet(
    cdc: &mut Cdc,
    epoch: Epoch,
    bytes: &[u8],
) -> Result<(), ()> {
    if !same_session(epoch) {
        return Err(());
    }
    // Locked RP driver awaits only before its synchronous SRAM copy. Cancelling
    // this wait cannot retain a borrowed application buffer or DMA descriptor.
    match with_timeout(Duration::from_secs(2), SESSION.guard(epoch, cdc.write_packet(bytes))).await {
        Ok(Ok(Ok(()))) => Ok(()),
        _ => Err(()),
    }
}
