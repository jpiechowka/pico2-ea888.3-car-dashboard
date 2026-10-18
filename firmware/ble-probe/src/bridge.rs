//! Cross-core messages contain owned bytes or exclusive framebuffer references.
use core::cell::Cell;
use core::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, Ordering};

use dashboard_core::discovery::{Event, Input, RadioPhase, Request};
use dashboard_core::display::FrameBuffer;
use embassy_sync::blocking_mutex::Mutex;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;

pub static REPORTS: Channel<CriticalSectionRawMutex, Event, 32> = Channel::new();
pub static EVENTS: Channel<CriticalSectionRawMutex, Event, 8> = Channel::new();
pub static INPUTS: Channel<CriticalSectionRawMutex, Input, 16> = Channel::new();
pub static FREE: Channel<CriticalSectionRawMutex, &'static mut FrameBuffer, 2> = Channel::new();
pub static READY: Channel<CriticalSectionRawMutex, &'static mut FrameBuffer, 2> = Channel::new();
static REQUEST: Mutex<CriticalSectionRawMutex, Cell<Request>> = Mutex::new(Cell::new(Request::INITIAL));
pub static RADIO_READY: AtomicBool = AtomicBool::new(false);
pub static RADIO_FATAL: AtomicBool = AtomicBool::new(false);
pub static RADIO_PROGRESS: AtomicU32 = AtomicU32::new(0);
pub static RADIO_MALFORMED: AtomicU32 = AtomicU32::new(0);
pub static DROPPED_REPORTS: AtomicU32 = AtomicU32::new(0);
pub static PHASE: AtomicU8 = AtomicU8::new(RadioPhase::Starting as u8);
pub static UI_PROGRESS: AtomicU32 = AtomicU32::new(0);
pub static DISPLAY_PROGRESS: AtomicU32 = AtomicU32::new(0);
pub static INPUT_PROGRESS: AtomicU32 = AtomicU32::new(0);
pub static ENCODER_PROGRESS: AtomicU32 = AtomicU32::new(0);
pub static ENCODER_CONNECTED: AtomicBool = AtomicBool::new(false);
pub static DISPLAY_ACTIVE: AtomicBool = AtomicBool::new(false);
pub static DISPLAY_FATAL: AtomicBool = AtomicBool::new(false);
pub static RESET_REQUESTED: AtomicBool = AtomicBool::new(false);

pub fn request() -> Request { REQUEST.lock(Cell::get) }
pub fn set_request(request: Request) { REQUEST.lock(|cell| cell.set(request)); }
pub async fn emit_event(event: Event) { EVENTS.send(event).await; }
pub fn emit_report(event: Event) {
    if REPORTS.try_send(event).is_err() {
        let _ = DROPPED_REPORTS.try_update(Ordering::Relaxed, Ordering::Relaxed, |v| Some(v.saturating_add(1)));
    }
}
