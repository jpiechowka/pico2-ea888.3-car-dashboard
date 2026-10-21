//! Core-0-owned backlight policy, independent of GPIO and the render cadence.
use core::fmt::Write;

/// PWM counter length in ticks; compare equal to this value is full duty.
const PWM_PERIOD: u16 = PWM_TOP + 1;

use crate::config::backlight::{PWM_TOP, brightness_compare};
use crate::display::pipeline::FrameToken;
use crate::pages::Page;
use crate::popup::Popup;

/// Requested brightness policy; actual compare may remain lit during deferred Off feedback.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Brightness {
    /// Exact 100% PWM duty; every boot/recovery selects this preset.
    #[default]
    Full,
    /// Exact 50% PWM duty, independent of legacy slider mapping.
    Dim,
    /// Dashboard shutdown preset, requiring presented feedback and clear completion.
    Off,
    /// Legacy Debug control scale, mapped to the existing 45%-100% duty range.
    Percent(
        /// Legacy slider percentage, clamped to100; zero is off, nonzero maps4555..10000 ticks.
        u8,
    ),
}

impl Brightness {
    /// Returns requested PWM compare ticks in0..=10000; this does not apply deferred shutdown gates.
    pub fn compare(self) -> u16 {
        match self {
            Self::Full => PWM_PERIOD,
            Self::Dim => PWM_PERIOD / 2,
            Self::Off => 0,
            Self::Percent(percent) => brightness_compare(percent),
        }
    }

    /// Returns bounded ASCII feedback, clamping legacy slider labels to100%; fixed text fits storage.
    pub fn label(self) -> heapless::String<20> {
        let mut text = heapless::String::new();
        match self {
            Self::Full => text.push_str("BL: FULL").unwrap(),
            Self::Dim => text.push_str("BL: DIM").unwrap(),
            Self::Off | Self::Percent(0) => text.push_str("BL: OFF").unwrap(),
            Self::Percent(value) => write!(text, "BL: {}%", value.min(100)).unwrap(),
        }
        text
    }

    /// Returns the legacy slider position in0..=100; exact Dim maps to nearest position9
    /// (49.95% duty), used only when deliberate Debug rotation leaves the preset.
    fn legacy_percent(self) -> u8 {
        match self {
            Self::Full => 100,
            // Nearest old slider position to 50% duty (49.95%). Only used
            // when Debug rotation deliberately leaves the exact Dim preset.
            Self::Dim => 9,
            Self::Off => 0,
            Self::Percent(value) => value.min(100),
        }
    }

    /// Returns the next preset based on requested compare: off->Full, above50%->Dim,
    /// otherwise->Off. Uses requested state even during a deferred physical shutdown.
    fn next_dashboard(self) -> Self {
        match self.compare() {
            0 => Self::Full,
            duty if duty > PWM_PERIOD / 2 => Self::Dim,
            _ => Self::Off,
        }
    }
}

/// Feedback and consumed-effect flags from a page-specific encoder batch.
#[derive(Default)]
pub struct EncoderEffect {
    /// Last feedback in the batch, or None when no applicable action changed state.
    pub popup: Option<Popup>,
    /// Rotation changed brightness (or visible view motion when populated by ViewSelection).
    pub motion_applied: bool,
    /// At least one Dashboard click or odd Debug toggle was applied.
    pub press_applied: bool,
}

/// Dashboard shutdown gates; Debug legacy off uses only Feedback expiry.
#[derive(Clone, Copy, PartialEq, Eq)]
enum OffPhase {
    /// Waiting for feedback presentation and its required dwell.
    Feedback,
    /// Feedback expired; next frame must present the popup-free scene.
    Clear,
    /// Clear frame submitted, awaiting exact successful HAL completion.
    Transfer(
        /// Buffer and generation of the clear frame.
        FrameToken,
    ),
}

/// Deferred shutdown identity; replacing feedback invalidates all its frame tokens.
#[derive(Clone, Copy)]
struct PendingOff {
    /// Exact timestamped popup whose expiry may advance shutdown.
    popup: Popup,
    /// Feedback/clear/transfer state of the pending shutdown.
    phase: OffPhase,
    /// First submitted Dashboard Off popup frame, not any later refresh.
    popup_frame: Option<FrameToken>,
    /// First valid completion/unchanged-panel observation in milliseconds; no scanout claim.
    visible_at_ms: Option<u64>,
}

/// Core0 brightness policy with separate requested state and physically applicable PWM compare.
pub struct Backlight {
    /// Requested brightness, which can already be Off while compare is still illuminated.
    current: Brightness,
    /// Last nonzero brightness restored by Debug toggle or cancelled Dashboard shutdown.
    restore: Brightness,
    /// Current hardware-applicable compare ticks; caller owns PWM writes.
    compare: u16,
    /// Pending feedback/clear completion gates, absent after apply/recovery/completed shutdown.
    pending: Option<PendingOff>,
}

impl Default for Backlight {
    /// Returns the full-brightness boot policy, as new.
    fn default() -> Self { Self::new() }
}

impl Backlight {
    /// Brightness is deliberately runtime-only: every boot starts illuminated.
    /// Returns Full requested/applied brightness, Full restore and no pending shutdown.
    pub const fn new() -> Self {
        Self {
            current: Brightness::Full,
            restore: Brightness::Full,
            compare: PWM_TOP + 1,
            pending: None,
        }
    }

    /// Returns hardware-applicable compare ticks in0..=10000, including shutdown deferral.
    pub const fn compare(&self) -> u16 { self.compare }

    /// Returns requested policy; Off does not imply compare is already zero.
    pub const fn brightness(&self) -> Brightness { self.current }

    /// Protected recovery cancels shutdown tokens and restores full brightness.
    pub fn wake_full(&mut self) { self.apply(Brightness::Full); }

    /// Applies requested `value` immediately, cancelling shutdown tokens and retaining
    /// it as restore policy when its compare is nonzero. No hardware I/O occurs here.
    fn apply(
        &mut self,
        value: Brightness,
    ) {
        self.current = value;
        self.compare = value.compare();
        self.pending = None;
        if self.compare != 0 {
            self.restore = value;
        }
    }

    /// Requests off `value` while preserving applied compare and binds exact `popup`.
    /// Internal callers supply Off/Percent(0); the prior nonzero request becomes restore policy.
    fn defer_off(
        &mut self,
        popup: Popup,
        value: Brightness,
    ) {
        if self.current.compare() != 0 {
            self.restore = self.current;
        }
        self.current = value;
        self.pending = Some(PendingOff {
            popup,
            phase: OffPhase::Feedback,
            popup_frame: None,
            visible_at_ms: None,
        });
    }

    /// Applies `page` policy to `delta` detents and `presses` at monotonic `now_ms`.
    /// Returns last feedback and applied flags. Dashboard ignores rotation and cycles each
    /// click Full/Dim/Off (normal batches <=8); Debug clamps five-point slider steps then
    /// toggles on odd click parity; Logs ignores both. Does not independently reject clock reversal.
    pub fn encoder(
        &mut self,
        page: Page,
        delta: i32,
        presses: u8,
        now_ms: u64,
    ) -> EncoderEffect {
        let mut result = EncoderEffect::default();
        match page {
            Page::Dashboard => {
                // A batch contains at most eight debounced presses. Apply each
                // press, including intermediate Dim before deferred Off.
                for _ in 0..presses {
                    let next = self.current.next_dashboard();
                    let popup = Popup::Brightness(now_ms, next);
                    if next == Brightness::Off {
                        self.defer_off(popup, next);
                    } else {
                        self.apply(next);
                    }
                    result.popup = Some(popup);
                    result.press_applied = true;
                }
                // The separate view owner handles Dashboard rotation first.
            }
            Page::Debug => {
                let current = u32::from(self.current.legacy_percent());
                let next = crate::input_model::brightness_after(current, delta);
                if delta != 0 && next != current {
                    self.apply(Brightness::Percent(next as u8));
                    result.motion_applied = true;
                    result.popup = Some(Popup::Brightness(now_ms, self.current));
                }
                if presses % 2 == 1 {
                    if self.current.compare() == 0 {
                        self.apply(self.restore);
                    } else {
                        let off = Brightness::Percent(0);
                        self.defer_off(Popup::Brightness(now_ms, off), off);
                    }
                    result.press_applied = true;
                    result.popup = Some(Popup::Brightness(
                        now_ms,
                        Brightness::Percent(self.current.legacy_percent()),
                    ));
                }
            }
            Page::Logs => {}
        }
        result
    }

    /// Page changes/other feedback cancel a dashboard shutdown. Preserve the
    /// existing Debug toggle's immediate-off behavior when its popup is replaced.
    pub fn interrupt_feedback(&mut self) {
        if let Some(pending) = self.pending.take() {
            if matches!(pending.popup, Popup::Brightness(_, Brightness::Off)) {
                self.apply(self.restore);
            } else {
                self.compare = 0;
            }
        }
    }

    /// Checks exact `popup` at `now_ms` milliseconds and returns whether it may be
    /// removed. Dashboard Off waits2000ms from first successful presentation, then arms
    /// Clear; Debug Off uses nominal1500ms and applies zero directly. Reversal before
    /// feedback/presentation cancels Dashboard shutdown; unrelated popups use nominal expiry.
    pub fn expire_popup(
        &mut self,
        popup: Popup,
        now_ms: u64,
    ) -> bool {
        let Some(pending) = self.pending else {
            return popup.is_expired_at(now_ms);
        };
        if pending.popup != popup || pending.phase != OffPhase::Feedback {
            return popup.is_expired_at(now_ms);
        }
        if now_ms < popup.start_time() {
            self.interrupt_feedback();
        } else if matches!(popup, Popup::Brightness(_, Brightness::Off)) {
            let Some(visible_at_ms) = pending.visible_at_ms else {
                return false;
            };
            let Some(age) = now_ms.checked_sub(visible_at_ms) else {
                self.interrupt_feedback();
                return true;
            };
            if age < 2000 {
                return false;
            }
            self.pending.as_mut().unwrap().phase = OffPhase::Clear;
        } else {
            if !popup.is_expired_at(now_ms) {
                return false;
            }
            self.compare = 0;
            self.pending = None;
        }
        true
    }

    /// Record the first popup frame, or the frame clearing an expired popup.
    /// `token` must identify the successfully submitted current scene; the caller
    /// ensures the scene contains the expected popup/clear state. No completion is inferred.
    pub fn frame_submitted(
        &mut self,
        token: FrameToken,
    ) {
        if let Some(pending) = &mut self.pending {
            if pending.phase == OffPhase::Clear {
                pending.phase = OffPhase::Transfer(token);
            } else if pending.phase == OffPhase::Feedback
                && pending.popup_frame.is_none()
                && matches!(pending.popup, Popup::Brightness(_, Brightness::Off))
            {
                pending.popup_frame = Some(token);
            }
        }
    }

    /// A matching successful HAL completion releases the shutdown, never an
    /// older frame, wrong buffer, failure, duplicate completion or queued write.
    /// `token` is the completed lease and `now_ms` monotonic milliseconds. First popup
    /// completion starts the dwell; exact clear completion applies zero. Early popup
    /// timestamps cancel shutdown; caller ensures global completion ordering and successful HAL status.
    pub fn frame_completed(
        &mut self,
        token: FrameToken,
        now_ms: u64,
    ) {
        if let Some(pending) = &mut self.pending
            && pending.phase == OffPhase::Feedback
            && pending.popup_frame == Some(token)
            && pending.visible_at_ms.is_none()
        {
            if now_ms < pending.popup.start_time() {
                self.interrupt_feedback();
                return;
            }
            pending.visible_at_ms = Some(now_ms);
        }
        if self
            .pending
            .is_some_and(|pending| pending.phase == OffPhase::Transfer(token))
        {
            self.compare = 0;
            self.pending = None;
        }
    }

    /// The completed panel reference already matches the current scene.
    /// An unchanged Off popup starts its feedback timer; an unchanged popup-free
    /// scene finishes Clear and turns the backlight off.
    /// `now_ms` is monotonic milliseconds for first identical-popup presentation.
    /// Caller must prove matching completed panel identity/orientation; a local pixel
    /// comparison without that base is insufficient. Clear completion itself does not inspect time.
    pub fn frame_unchanged(
        &mut self,
        now_ms: u64,
    ) {
        if let Some(pending) = &mut self.pending
            && pending.phase == OffPhase::Feedback
            && matches!(pending.popup, Popup::Brightness(_, Brightness::Off))
            && pending.visible_at_ms.is_none()
        {
            if now_ms < pending.popup.start_time() {
                self.interrupt_feedback();
                return;
            }
            pending.visible_at_ms = Some(now_ms);
        }
        if self.pending.is_some_and(|pending| pending.phase == OffPhase::Clear) {
            self.compare = 0;
            self.pending = None;
        }
    }
}
