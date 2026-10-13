pub const POPUP_DURATION_MS: u64 = 3000;
/// Shorter duration for transient feedback popups (brightness adjustment).
pub const POPUP_DURATION_SHORT_MS: u64 = 1500;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Popup {
    Reset(u64),
    Warnings(u64, crate::warning_state::WarningFeedback),
    Fps(u64),
    BoostUnit(u64),
    View(u64),
    /// Monotonic milliseconds and page-specific brightness feedback.
    Brightness(u64, crate::backlight::Brightness),
}

impl Popup {
    #[inline]
    pub const fn start_time(&self) -> u64 {
        match self {
            Self::Warnings(t, _)
            | Self::Reset(t)
            | Self::Fps(t)
            | Self::BoostUnit(t)
            | Self::View(t)
            | Self::Brightness(t, _) => *t,
        }
    }

    #[inline]
    pub fn is_expired_at(
        &self,
        now_ms: u64,
    ) -> bool {
        let duration = match self {
            Self::Brightness(_, crate::backlight::Brightness::Off) => 2000,
            Self::Brightness(..) | Self::View(_) => POPUP_DURATION_SHORT_MS,
            _ => POPUP_DURATION_MS,
        };
        // Invalid time cannot keep feedback covering a warning indefinitely.
        now_ms.checked_sub(self.start_time()).is_none_or(|age| age >= duration)
    }
}
