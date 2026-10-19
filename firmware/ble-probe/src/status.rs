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

const CLEAN: u32 = 1 << 4;
const CAPTURE: u32 = 1 << 5;
const ERRORS: u32 = 0x0001_FF00;
const FIRST: u32 = 0x00F0_0000;
const SCAN_ERRORS: u32 = ((Fault::Scan.bit() | Fault::Advertisement.bit()) as u32) << 8;

pub use dashboard_core::discovery::Fault;

pub struct Status {
    word: AtomicU32,
}
impl Status {
    pub const fn new() -> Self {
        Self {
            word: AtomicU32::new(RadioPhase::Starting as u32),
        }
    }

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

    pub fn capture_tick(
        &self,
        ui: &mut cap::CaptureUi,
        now_ms: u64,
    ) -> Option<cap::Action> {
        let action = ui.tick(now_ms);
        self.observe_capture(ui);
        action
    }

    pub fn capture_complete(
        &self,
        ui: &mut cap::CaptureUi,
        id: cap::RequestId,
        completion: cap::Completion,
    ) {
        if ui.complete(id, completion) {
            if let cap::Completion::Failed(error) = completion {
                self.capture_error(error);
            }
            self.observe_capture(ui);
        }
    }

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

    pub fn capture_error(
        &self,
        error: cap::UiError,
    ) {
        // These are expected admission rejections, not a failed I/O operation.
        if !matches!(error, cap::UiError::Empty | cap::UiError::NoPeer | cap::UiError::Busy) {
            self.record(Fault::Storage);
        }
    }

    pub fn encoder_failed(
        &self,
        was_connected: bool,
    ) {
        if was_connected {
            self.record(Fault::Encoder);
        }
    }

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
    pub fn fault_latch(&self) -> FaultLatch {
        let word = self.word.load(Ordering::Relaxed);
        FaultLatch::from_parts(((word & ERRORS) >> 8) as u16, ((word & FIRST) >> 20) as u8)
            .expect("atomic fault evidence invariant")
    }

    #[cfg(test)]
    pub fn error_latch(&self) -> dashboard_core::discovery::ErrorLatch { self.fault_latch().category() }
}

fn clean_probe(model: &Probe) -> bool {
    let i = &model.inspection;
    matches!(model.request.action, Action::Inspect(peer) if model.selected_device().is_some_and(|d| d.peer == peer))
        && model.error == ProbeError::None
        && model.failure.is_none()
        && i.complete
        && i.cleanup_complete
        && i.stages.iter().all(|&done| done)
        && i.omitted == 0
        && i.malformed == 0
        && i.link.is_some_and(|link| link.att_mtu.is_some())
        && i.started_at_ms
            .zip(i.ended_at_ms)
            .is_some_and(|(start, end)| end >= start)
        && i.values.iter().all(|v| valid_read(v) && !v.value.truncated)
}

fn valid_read(value: &InfoValue) -> bool {
    value.result == ReadResult::Ok
        && value.failure.is_none()
        && info_length_valid(value.uuid, value.value.as_slice().len())
}
