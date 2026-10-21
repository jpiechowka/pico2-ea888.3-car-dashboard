//! Cross-core messages contain owned bytes or exclusive framebuffer references.
use core::cell::Cell;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use dashboard_core::discovery::{Event, Request, ScanDiagnostics};
use dashboard_core::display::FrameBuffer;
use embassy_sync::blocking_mutex::Mutex;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;

/// Lossy bounded advertisement events; full queue increments a saturating drop counter.
pub static REPORTS: Channel<CriticalSectionRawMutex, Event, 32> = Channel::new();
/// Reliable inventory/cleanup events; senders await capacity instead of dropping evidence.
pub static EVENTS: Channel<CriticalSectionRawMutex, Event, 8> = Channel::new();
/// Semantic controls from onboard buttons or the retained rotary encoder.
pub enum Control {
    /// Signed selection movement, normalized by the physical input adapter.
    Rotate(
        /// Positive advances selection; negative moves backward.
        i32,
    ),
    /// Explicit selected-peer inspection or capture-screen confirmation.
    Click,
    /// Navigate back or request a live operation stop.
    Back,
    /// Protected two-second Y recovery request.
    Recover,
    /// Short X gesture interpreted by the capture/live UI state.
    XShort,
    /// Held X gesture for capture workflow entry/confirmation.
    XHold,
}
/// Bounded onboard/encoder controls, drained ahead of advertisements.
pub static INPUTS: Channel<CriticalSectionRawMutex, Control, 16> = Channel::new();
// Filled once before core1 starts; keeps the UI task future argument-free.
/// One immutable boot-evidence message populated before core1 starts.
pub static BOOT: Channel<CriticalSectionRawMutex, dashboard_core::health::reset::BootEvidence, 1> = Channel::new();
/// Exclusive framebuffers available to the core1 renderer; exactly two circulate.
pub static FREE: Channel<CriticalSectionRawMutex, &'static mut FrameBuffer, 2> = Channel::new();
/// Rendered exclusive framebuffers awaiting core0 SPI transfer.
pub static READY: Channel<CriticalSectionRawMutex, &'static mut FrameBuffer, 2> = Channel::new();
/// Latest UI-issued operation generation, copied under a short cross-core lock.
static REQUEST: Mutex<CriticalSectionRawMutex, Cell<Request>> = Mutex::new(Cell::new(Request::INITIAL));
/// Host initialization completed; does not prove connection or adapter compatibility.
pub static RADIO_READY: AtomicBool = AtomicBool::new(false);
/// Latched fatal host/radio condition that stops supervisor feeding.
pub static RADIO_FATAL: AtomicBool = AtomicBool::new(false);
/// Wrapping complete-radio-bus-boundary progress sequence.
pub static RADIO_PROGRESS: AtomicU32 = AtomicU32::new(0);
/// Coherent bounded malformed-report evidence from the radio owner.
pub static SCAN_DIAGNOSTICS: Mutex<CriticalSectionRawMutex, Cell<ScanDiagnostics>> =
    Mutex::new(Cell::new(ScanDiagnostics::new()));
/// Saturating advertisement queue-overflow total.
pub static DROPPED_REPORTS: AtomicU32 = AtomicU32::new(0);
/// Atomic fault-priority latch and live/saved status adapter.
pub static STATUS: crate::status::Status = crate::status::Status::new();
/// Wrapping useful UI-loop progress sequence, independent of rendered frames.
pub static UI_PROGRESS: AtomicU32 = AtomicU32::new(0);
/// Wrapping completed onboard-button polling sequence.
pub static INPUT_PROGRESS: AtomicU32 = AtomicU32::new(0);
/// Wrapping completed encoder iteration sequence, including ordinary I2C errors.
pub static ENCODER_PROGRESS: AtomicU32 = AtomicU32::new(0);
/// Last encoder initialization/iteration connectivity state.
pub static ENCODER_CONNECTED: AtomicBool = AtomicBool::new(false);
/// Display initialization/transfer owns SPI; sequentially consistent for flash admission.
pub static DISPLAY_ACTIVE: AtomicBool = AtomicBool::new(false);
/// Latched panel failure; its framebuffer stays quarantined until reset.
pub static DISPLAY_FATAL: AtomicBool = AtomicBool::new(false);
/// Explicit recovery from a terminal radio fault requests watchdog expiry.
pub static RESET_REQUESTED: AtomicBool = AtomicBool::new(false);

/// Returns the latest operation request as one coherent generation/payload copy.
pub fn request() -> Request { REQUEST.lock(Cell::get) }
/// Replaces the operation with `request`; radio observes generation changes to
/// cancel old work before admitting another operation.
pub fn set_request(request: Request) { REQUEST.lock(|cell| cell.set(request)); }
/// Reliably queues owned `event`, awaiting capacity. Cancellation before send
/// completes can lose that terminal/inventory event, so callers retain the future.
pub async fn emit_event(event: Event) { EVENTS.send(event).await; }
/// Attempts to queue owned advertisement `event` immediately. Full capacity
/// latches a capacity fault and saturating drop count; no blocking or retry.
pub fn emit_report(event: Event) {
    if REPORTS.try_send(event).is_err() {
        STATUS.record(crate::status::Fault::Capacity);
        let _ = DROPPED_REPORTS.try_update(Ordering::Relaxed, Ordering::Relaxed, |v| Some(v.saturating_add(1)));
    }
}
