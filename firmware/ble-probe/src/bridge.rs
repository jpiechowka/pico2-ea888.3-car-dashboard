//! Cross-core messages contain owned bytes or exclusive framebuffer references.
use core::cell::Cell;
use core::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, Ordering};

use dashboard_core::discovery::{Event, Request, ScanDiagnostics};
use dashboard_core::display::FrameBuffer;
use embassy_sync::blocking_mutex::Mutex;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
#[cfg(feature = "ngm-collector")]
use embassy_sync::signal::Signal;

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
    /// Collector-only B hold opens a local confirmation, never directly sends CAN.
    Collect,
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
pub static INPUTS: Channel<CriticalSectionRawMutex, InputEvent, 16> = Channel::new();
/// Input edge and its observation time, retained through queue backpressure.
pub struct InputEvent {
    /// Original debounced gesture, never reinterpreted from elapsed queue time.
    pub control: Control,
    /// Boot-relative gesture observation milliseconds, before awaiting queue capacity.
    pub at_ms: u64,
}
/// Queues `control` with its original `at_ms` observation; cancellation may drop an unqueued edge.
/// Returns a lazy future completing on queue acceptance, not on model application.
pub fn emit_control(
    control: Control,
    at_ms: u64,
) -> impl core::future::Future<Output = ()> {
    INPUTS.send(InputEvent { control, at_ms })
}
/// Latest collector revision requesting worst-case capture capacity admission on core1.
/// Signals carry no payload/command bytes and never themselves authorize a radio write.
#[cfg(feature = "ngm-collector")]
pub static COLLECTION_ADMISSION: Signal<CriticalSectionRawMutex, u32> = Signal::new();
/// Core1's matching revision/capacity decision. Stale replies cannot authorize another run.
#[cfg(feature = "ngm-collector")]
pub static COLLECTION_ADMITTED: Signal<CriticalSectionRawMutex, (u32, bool)> = Signal::new();
// Filled once before core1 starts; keeps the UI task future argument-free.
/// One immutable boot-evidence message populated before core1 starts.
pub static BOOT: Channel<CriticalSectionRawMutex, dashboard_core::health::reset::BootEvidence, 1> = Channel::new();
/// Exclusive framebuffers available to the core1 renderer; exactly two circulate.
pub static FREE: Channel<CriticalSectionRawMutex, &'static mut FrameBuffer, 2> = Channel::new();
/// Rendered exclusive framebuffers awaiting core0 SPI transfer.
pub static READY: Channel<CriticalSectionRawMutex, ReadyFrame, 2> = Channel::new();
/// Exclusive pixels plus immutable confirmation identity; no third framebuffer is allocated.
pub struct ReadyFrame {
    /// Sole owned pixels, retained by the display until valid completion.
    pub pixels: &'static mut FrameBuffer,
    /// Prompt instance drawn into these pixels, or None for other screens.
    pub prompt_id: Option<u32>,
}
/// Latest completed frame's prompt/time; short lock avoids torn 64-bit timestamps across cores.
static PRESENTED_PROMPT: Mutex<CriticalSectionRawMutex, Cell<Option<(u32, u64)>>> = Mutex::new(Cell::new(None));
/// Records `prompt_id` at actual HAL completion `at_ms`; None invalidates prior displayed consent.
/// This records software transfer completion, not LCD scanout or operator comprehension.
pub fn frame_presented(
    prompt_id: Option<u32>,
    at_ms: u64,
) {
    PRESENTED_PROMPT.lock(|c| c.set(prompt_id.map(|id| (id, at_ms))));
}
/// Returns the latest exact completed prompt instance and boot-relative milliseconds.
pub fn presented_prompt() -> Option<(u32, u64)> { PRESENTED_PROMPT.lock(Cell::get) }
/// Latest UI-issued operation generation, copied under a short cross-core lock.
static REQUEST: Mutex<CriticalSectionRawMutex, Cell<Request>> = Mutex::new(Cell::new(Request::INITIAL));
/// Explicit stop for one nonzero collector generation, retained without replacing its request.
/// Generations never wrap; a later request cannot inherit this stop marker.
static COLLECTION_CANCELLED: AtomicU32 = AtomicU32::new(0);
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

/// Excludes connection lifetimes from journal mutation, including each restoration
/// between pages. Scanning is permitted: its permanent bus owner parks separately.
/// This guard is independent of UI state and closes queued-request admission races.
pub struct OperationGate {
    /// Zero is idle, one owns a connection, two owns a mutation, three is terminal.
    owner: AtomicU8,
}
impl OperationGate {
    /// Returns an idle gate. It owns no radio, storage or peripheral resources.
    pub const fn new() -> Self {
        Self {
            owner: AtomicU8::new(0),
        }
    }

    /// Returns an affine connection lease if idle, otherwise None without effects.
    /// Retain through connect cancellation and acknowledged link cleanup; never
    /// drop merely because an application timeout or close submission occurred.
    pub fn connection(&self) -> Option<OperationLease<'_>> { self.acquire(1) }

    /// Returns an affine mutation lease if no connection/mutation owns this gate.
    /// Retain across admission, every journal page and owner restoration. None is
    /// an ordinary Busy result; callers must not erase/program before admission.
    pub fn mutation(&self) -> Option<OperationLease<'_>> { self.acquire(2) }

    /// Permanently rejects new connection/mutation admission until reset, even when
    /// an existing lease is later dropped. Existing owners still perform restoration;
    /// this does not cancel an already admitted synchronous flash operation.
    pub fn close(&self) { self.owner.store(3, Ordering::Release); }

    /// Claims idle state for private nonzero `owner`, returning its unique lease.
    /// Acquire/Release ordering makes admission/release coherent across both cores.
    fn acquire(
        &self,
        owner: u8,
    ) -> Option<OperationLease<'_>> {
        self.owner
            .compare_exchange(0, owner, Ordering::AcqRel, Ordering::Acquire)
            .ok()
            .map(|_| OperationLease { gate: self, owner })
    }
}
/// Unique operation permission; no Clone/Copy or public constructor is provided.
#[must_use = "retain until acknowledged cleanup or full mutation restoration"]
pub struct OperationLease<'a> {
    /// Gate whose unique nonzero owner is released by Drop.
    gate: &'a OperationGate,
    /// Exact live owner state; Drop may clear only this state, never terminal closure.
    owner: u8,
}
impl Drop for OperationLease<'_> {
    /// Releases the sole owner's live gate after caller-established terminal conditions.
    /// A fatal closure remains terminal after release, including cancellation drops.
    /// Does not perform cleanup itself; abandoning a live owner is a caller bug.
    fn drop(&mut self) {
        let _ = self
            .gate
            .owner
            .compare_exchange(self.owner, 0, Ordering::AcqRel, Ordering::Acquire);
    }
}
/// Core0 radio/storage admission, also coherent with core1 intent publication.
pub static OPERATION_GATE: OperationGate = OperationGate::new();

/// Closes future operation admission and latches unhealthy radio state synchronously.
/// Call before dropping unconfirmed radio work or awaiting terminal event capacity.
/// A held lease cannot reopen admission; supervisor observes fatal without queue progress.
pub fn latch_radio_fault() {
    OPERATION_GATE.close();
    RADIO_FATAL.store(true, Ordering::Release);
    RADIO_READY.store(false, Ordering::Release);
}

/// Returns the latest operation request as one coherent generation/payload copy.
pub fn request() -> Request { REQUEST.lock(Cell::get) }
/// Replaces the operation with `request`; radio observes generation changes to
/// cancel old work before admitting another operation.
pub fn set_request(request: Request) { REQUEST.lock(|cell| cell.set(request)); }
/// Requests collection stop for the current nonzero `revision`, preserving its action/evidence.
/// Foreign generations and ordinary Inspect/Scan are ignored. Repeated calls are idempotent;
/// this is cancellation intent, never an acknowledgment of channel/link cleanup.
pub fn cancel_collection(revision: u32) {
    let current = request();
    if revision != 0
        && current.revision == revision
        && matches!(current.action, dashboard_core::discovery::Action::Collect(_))
        && !collection_cancelled(revision)
    {
        COLLECTION_CANCELLED.store(revision, Ordering::Release);
    }
}
/// Returns whether explicit cancellation targets nonzero `revision`; no request is changed.
/// Retaining the marker through End prevents reconnection under the same generation.
pub fn collection_cancelled(revision: u32) -> bool {
    revision != 0 && COLLECTION_CANCELLED.load(Ordering::Acquire) == revision
}
/// Returns a lazy send future owning `event`, without a second async relay.
/// Polling awaits capacity; completion means queued, not yet consumed. Dropping
/// before completion discards this event, so required terminal publications retain it.
/// One shared poll boundary avoids duplicating the queue state machine at each publisher.
pub fn emit_event(event: Event) -> impl core::future::Future<Output = ()> {
    let mut send = EVENTS.send(event);
    core::future::poll_fn(move |cx| poll_event_send(&mut send, cx))
}

/// Polls the original queue `send` with `cx`, preserving its acceptance and wakeup result.
/// Locked Embassy SendFuture is Unpin and retains the event while full; no extra event
/// owner, queue, allocation or cancellation action is introduced by this boundary.
/// Returns Pending while capacity is unavailable or Ready after the event is queued.
/// As with the original future, polling again after Ready panics.
#[inline(never)]
fn poll_event_send(
    send: &mut embassy_sync::channel::SendFuture<'_, CriticalSectionRawMutex, Event, 8>,
    cx: &mut core::task::Context<'_>,
) -> core::task::Poll<()> {
    core::future::Future::poll(core::pin::Pin::new(send), cx)
}
/// Attempts to queue owned advertisement `event` immediately. Full capacity
/// latches a capacity fault and saturating drop count; no blocking or retry.
pub fn emit_report(event: Event) {
    if REPORTS.try_send(event).is_err() {
        STATUS.record(crate::status::Fault::Capacity);
        let _ = DROPPED_REPORTS.try_update(Ordering::Relaxed, Ordering::Relaxed, |v| Some(v.saturating_add(1)));
    }
}
