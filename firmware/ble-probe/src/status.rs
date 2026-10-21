//! Probe-only RGB policy. One atomic word prevents activity publication from
//! overwriting a concurrent fault. Only core1's Y recovery clears the latch.
use core::sync::atomic::{AtomicU32, Ordering};

use dashboard_core::discovery::{
    Action,
    AdFields,
    Event,
    FaultLatch,
    InfoValue,
    Input,
    Probe,
    ProbeError,
    RadioPhase,
    ReadResult,
    ReportKind,
    Request,
    capture_ui as cap,
    info_length_valid,
};

/// Activity bit for a fully clean live inspection, never a saved capture.
const CLEAN: u32 = 1 << 4;
/// Activity bit while a capture storage request awaits completion.
const CAPTURE: u32 = 1 << 5;
/// Nine fault-category bits retained independently from activity publication.
const ERRORS: u32 = 0x0001_FF00;
/// Encoded first-fault identity; later faults add bits without replacing it.
const FIRST: u32 = 0x00F0_0000;
/// Scan/advertisement faults use a slower red blink than terminal failure.
const SCAN_ERRORS: u32 = ((Fault::Scan.bit() | Fault::Advertisement.bit()) as u32) << 8;

pub use dashboard_core::discovery::Fault;

/// Atomic phase/activity/fault word read by the sole RGB supervisor.
pub struct Status {
    /// Low phase bits, clean/capture flags, cumulative fault bits and first fault.
    word: AtomicU32,
}
impl Status {
    /// Returns Starting activity with no clean flag or retained fault.
    pub const fn new() -> Self {
        Self {
            word: AtomicU32::new(RadioPhase::Starting as u32),
        }
    }

    /// Atomically latches `fault`, preserving an earlier first cause and concurrent
    /// activity/fault updates; does not clear or restart a radio operation.
    pub fn record(
        &self,
        fault: Fault,
    ) {
        let _ = self.word.try_update(Ordering::Relaxed, Ordering::Relaxed, |word| {
            let first = if word & FIRST == 0 { (fault as u32) << 20 } else { 0 };
            Some(word | ((fault.bit() as u32) << 8) | first)
        });
    }

    /// Atomic clear also removes the old green activity. A fault linearized
    /// after this operation survives every subsequent activity update.
    fn recover(&self) { self.word.swap(RadioPhase::Stopping as u32, Ordering::Relaxed); }

    /// Applies live `input` at boot-relative `now_ms` to `model`, clearing the
    /// latch only for Recover and then observing resulting faults. Returns any
    /// new radio request; the caller publishes it to the sole radio owner.
    pub fn input(
        &self,
        model: &mut Probe,
        input: Input,
        now_ms: u64,
    ) -> Option<Request> {
        if matches!(input, Input::Recover) {
            self.recover();
        }
        let request = model.input(input, now_ms);
        self.observe_probe(model);
        request
    }

    /// Applies current-generation `event` to `model`, rejecting stale generations
    /// before inspecting payloads. Latches malformed reports, capacity, incomplete
    /// inspection and cleanup faults without generating another radio request.
    pub fn event(
        &self,
        model: &mut Probe,
        event: Event,
    ) {
        let revision = match event {
            Event::Report { revision, .. }
            | Event::State { revision, .. }
            | Event::Item { revision, .. }
            | Event::Failure { revision, .. }
            | Event::FailureContext { revision, .. }
            | Event::InspectionEnd { revision, .. }
            | Event::Complete { revision, .. } => revision,
        };
        if revision != model.request.revision {
            return;
        }
        if let Event::Report { report, .. } = event
            && model.request.action == Action::Scan(true)
            && report.kind != ReportKind::Other
            && (report.data.truncated || AdFields::new(report.data.as_slice()).any(|f| f.is_err()))
        {
            self.record(Fault::Advertisement);
        }
        model.event(event);
        self.observe_probe(model);
        if let Event::InspectionEnd {
            cleanup_complete: false,
            ..
        } = event
            && matches!(model.request.action, Action::Inspect(_))
        {
            self.record(Fault::Probe);
        }
        if let Event::Complete { .. } = event
            && matches!(model.request.action, Action::Inspect(_))
            && !model.inspection.complete
        {
            self.record(Fault::Probe);
        }
    }

    /// Latches errors and bounded-capacity omissions already represented by `model`;
    /// reading old counters can retain a fault but never creates a clean success.
    fn observe_probe(
        &self,
        model: &Probe,
    ) {
        let inspection = &model.inspection;
        if model.error != ProbeError::None
            || model.failure.is_some()
            || inspection.malformed != 0
            || inspection.values.iter().any(|v| !valid_read(v))
        {
            self.record(Fault::Probe);
        }
        if model.catalog.overflow_reports != 0
            || inspection.omitted != 0
            || inspection.values.iter().any(|v| v.value.truncated)
        {
            self.record(Fault::Capacity);
        }
    }

    /// Applies capture `input` at `now_ms` with the supplied live `context` to `ui`.
    /// Returns the resulting action and latches any resulting storage fault.
    pub fn capture_input(
        &self,
        ui: &mut cap::CaptureUi,
        input: cap::Input,
        now_ms: u64,
        context: cap::Context,
    ) -> Option<cap::Action> {
        let action = ui.input(input, now_ms, context);
        self.observe_capture(ui);
        action
    }

    /// Advances capture `ui` with boot-relative `now_ms`, observes resulting
    /// faults, and returns a newly due action if any.
    pub fn capture_tick(
        &self,
        ui: &mut cap::CaptureUi,
        now_ms: u64,
    ) -> Option<cap::Action> {
        let action = ui.tick(now_ms);
        self.observe_capture(ui);
        action
    }

    /// Applies `completion` only if `id` still matches the pending request in
    /// `ui`; stale results do not latch their errors or alter the accepted screen.
    /// Byte-lease return is handled separately and must occur even for stale IDs.
    pub fn capture_complete(
        &self,
        ui: &mut cap::CaptureUi,
        id: cap::RequestId,
        completion: &cap::Completion,
    ) {
        if ui.complete_ref(id, completion) {
            if let cap::Completion::Failed(error) = completion {
                self.capture_error(*error);
            }
            self.observe_capture(ui);
        }
    }

    /// Latches storage faults from `ui` error state or blocked/corrupt slot rows;
    /// it does not load, erase, or reinterpret the underlying capture bytes.
    fn observe_capture(
        &self,
        ui: &cap::CaptureUi,
    ) {
        if let cap::Screen::Error(error) = ui.screen() {
            self.capture_error(error);
        }
        if ui.screen() == cap::Screen::Slots
            && ui.slots().iter().any(|slot| {
                matches!(
                    slot,
                    cap::Slot::Corrupt
                        | cap::Slot::Unsupported
                        | cap::Slot::Unavailable
                        | cap::Slot::SavedBlocked { .. }
                )
            })
        {
            self.record(Fault::Storage);
        }
    }

    /// Latches storage failure for `error`, excluding expected Empty/NoPeer/Busy
    /// admission rejections. Other errors remain visible until protected recovery.
    pub fn capture_error(
        &self,
        error: cap::UiError,
    ) {
        // These are expected admission rejections, not a failed I/O operation.
        if !matches!(error, cap::UiError::Empty | cap::UiError::NoPeer | cap::UiError::Busy) {
            self.record(Fault::Storage);
        }
    }

    /// Latches Encoder only when `was_connected` was true; absence at startup is
    /// allowed because onboard controls remain available.
    pub fn encoder_failed(
        &self,
        was_connected: bool,
    ) {
        if was_connected {
            self.record(Fault::Encoder);
        }
    }

    /// Latches USB only if `session_current` is true; reset/unplug invalidation
    /// is ordinary cancellation and does not create a transfer fault.
    pub fn usb_failed(
        &self,
        session_current: bool,
    ) {
        // A bus reset/unplug is a normal session cancellation. A failed or
        // timed-out transfer on the still-current session is not.
        if session_current {
            self.record(Fault::Usb);
        }
    }

    /// Publishes phase/clean/capture activity from `model` and `ui`, atomically
    /// preserving every concurrent fault bit and first cause. Only a clean live
    /// inspection can set green eligibility; saved pages never do.
    pub fn publish(
        &self,
        model: &Probe,
        ui: &cap::CaptureUi,
    ) {
        let activity = model.phase as u32
            | if ui.screen() == cap::Screen::Live && clean_probe(model) {
                CLEAN
            } else {
                0
            }
            | if ui.pending().is_some() { CAPTURE } else { 0 };
        // No associated non-atomic data is read by the consumer.
        let _ = self.word.try_update(Ordering::Relaxed, Ordering::Relaxed, |word| {
            Some((word & (ERRORS | FIRST)) | activity)
        });
    }

    /// Electrical high levels for PIM715 R/G/B (active low), sampled at 50 ms.
    /// Returns electrical R/G/B levels at boot-relative `now_ms`; false means on.
    /// `terminal_fault` overrides all activity with a 250 ms red blink. Other
    /// faults, capture activity, scanning and clean paused inspection follow in
    /// priority order; these levels certify no vehicle or radio compatibility.
    pub fn levels(
        &self,
        now_ms: u64,
        terminal_fault: bool,
    ) -> [bool; 3] {
        let word = self.word.load(Ordering::Relaxed);
        if terminal_fault || word & 15 == RadioPhase::Fault as u32 {
            return [now_ms % 250 >= 125, true, true];
        }
        if word & ERRORS & !SCAN_ERRORS != 0 {
            return [false, true, true];
        }
        if word & SCAN_ERRORS != 0 {
            return [now_ms % 1000 >= 500, true, true];
        }
        if word & CAPTURE != 0 {
            return [false, true, false];
        }
        if word & 15 == RadioPhase::Scanning as u32 {
            return [true, true, false];
        }
        if word & 15 == RadioPhase::Paused as u32 {
            return if word & CLEAN != 0 {
                [true, false, true]
            } else {
                [false, false, true]
            };
        }
        [true, true, now_ms % 1000 >= 500]
    }

    /// UI snapshot of the same latch bits; cumulative counters cannot relatch it.
    /// Returns a coherent first-cause/category snapshot; panics only if the
    /// internally maintained atomic fault encoding violates its construction rules.
    pub fn fault_latch(&self) -> FaultLatch {
        let word = self.word.load(Ordering::Relaxed);
        FaultLatch::from_parts(((word & ERRORS) >> 8) as u16, ((word & FIRST) >> 20) as u8)
            .expect("atomic fault evidence invariant")
    }

    #[cfg(test)]
    /// Returns the legacy coarse category of the same atomic fault snapshot.
    pub fn error_latch(&self) -> dashboard_core::discovery::ErrorLatch { self.fault_latch().category() }
}

/// Returns whether `model` records a selected-peer live inspection with every
/// stage complete, acknowledged normal cleanup, matching termination evidence,
/// valid bounded values and link/timing metadata. Saved data cannot satisfy this.
fn clean_probe(model: &Probe) -> bool {
    let i = &model.inspection;
    matches!(model.request.action, Action::Inspect(peer) if model.selected_device().is_some_and(|d| d.peer == peer))
        && model.error == ProbeError::None
        && model.failure.is_none()
        && i.complete
        && i.cleanup_complete
        && dashboard_core::ble::termination::normal_reason(i.disconnect_reason)
        && model.failure_context.local_error.is_none()
        && model
            .failure_context
            .disconnect_reason
            .is_none_or(|code| Some(code) == i.disconnect_reason)
        && i.stages.iter().all(|&done| done)
        && i.omitted == 0
        && i.malformed == 0
        && i.link.is_some_and(|link| link.att_mtu.is_some())
        && i.started_at_ms
            .zip(i.ended_at_ms)
            .is_some_and(|(start, end)| end >= start)
        && i.values.iter().all(|v| valid_read(v) && !v.value.truncated)
}

/// Returns whether `value` succeeded without failure and has the allowed byte
/// length for its standard UUID; truncation is checked separately by callers.
fn valid_read(value: &InfoValue) -> bool {
    value.result == ReadResult::Ok
        && value.failure.is_none()
        && info_length_valid(value.uuid, value.value.as_slice().len())
}
