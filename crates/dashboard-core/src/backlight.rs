//! Core-0-owned backlight policy, independent of GPIO and the render cadence.
use core::fmt::Write;

const PWM_PERIOD: u16 = PWM_TOP + 1;

use crate::config::backlight::{PWM_TOP, brightness_compare};
use crate::display::pipeline::FrameToken;
use crate::pages::Page;
use crate::popup::Popup;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Brightness {
    #[default]
    Full,
    Dim,
    Off,
    /// Legacy Debug control scale, mapped to the existing 45%-100% duty range.
    Percent(u8),
}

impl Brightness {
    pub fn compare(self) -> u16 {
        match self {
            Self::Full => PWM_PERIOD,
            Self::Dim => PWM_PERIOD / 2,
            Self::Off => 0,
            Self::Percent(percent) => brightness_compare(percent),
        }
    }

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

    fn next_dashboard(self) -> Self {
        match self.compare() {
            0 => Self::Full,
            duty if duty > PWM_PERIOD / 2 => Self::Dim,
            _ => Self::Off,
        }
    }
}

#[derive(Default)]
pub struct EncoderEffect {
    pub popup: Option<Popup>,
    pub motion_applied: bool,
    pub press_applied: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum OffPhase {
    Feedback,
    Clear,
    Transfer(FrameToken),
}

#[derive(Clone, Copy)]
struct PendingOff {
    popup: Popup,
    phase: OffPhase,
    popup_frame: Option<FrameToken>,
    visible_at_ms: Option<u64>,
}

pub struct Backlight {
    current: Brightness,
    restore: Brightness,
    compare: u16,
    pending: Option<PendingOff>,
}

impl Default for Backlight {
    fn default() -> Self { Self::new() }
}

impl Backlight {
    /// Brightness is deliberately runtime-only: every boot starts illuminated.
    pub const fn new() -> Self {
        Self {
            current: Brightness::Full,
            restore: Brightness::Full,
            compare: PWM_TOP + 1,
            pending: None,
        }
    }

    pub const fn compare(&self) -> u16 { self.compare }

    pub const fn brightness(&self) -> Brightness { self.current }

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
                // Rotation is reserved for dashboard views; one currently exists.
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

    /// The completed panel reference already matches the popup-free scene.
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
