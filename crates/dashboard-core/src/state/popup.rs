//! Timestamped user feedback; backlight shutdown may extend expiry until presentation completes.
/// Standard feedback lifetime in monotonic milliseconds.
pub const POPUP_DURATION_MS: u64 = 3000;
/// Shorter duration for transient feedback popups (brightness adjustment).
/// Milliseconds for non-Off brightness and view feedback.
pub const POPUP_DURATION_SHORT_MS: u64 = 1500;

/// Requested feedback and its creation time; contains no physical presentation acknowledgment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Popup {
    /// Producer-acknowledged statistics reset.
    Reset(
        /// Creation time in monotonic milliseconds.
        u64,
    ),
    /// Warning acknowledgment/snooze command status.
    Warnings(
        /// Creation time in monotonic milliseconds.
        u64,
        /// Producer command/result feedback.
        crate::warning_state::WarningFeedback,
    ),
    /// FPS display mode changed; current mode resides in control state.
    Fps(
        /// Creation time in monotonic milliseconds.
        u64,
    ),
    /// Physical boost-unit preference changed; current unit resides in control state.
    BoostUnit(
        /// Creation time in monotonic milliseconds.
        u64,
    ),
    /// Named dashboard view changed; current name resolves through configuration.
    View(
        /// Creation time in monotonic milliseconds.
        u64,
    ),
    /// Monotonic milliseconds and page-specific brightness feedback.
    Brightness(
        /// Creation time in monotonic milliseconds.
        u64,
        /// Requested preset or legacy slider value, possibly awaiting shutdown completion.
        crate::backlight::Brightness,
    ),
}

impl Popup {
    /// Returns feedback creation time in monotonic milliseconds, not first displayed time.
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

    /// Returns whether `now_ms` is before creation or at/after the nominal lifetime:
    /// 2000ms for Dashboard Off, 1500ms for other brightness/view, 3000ms otherwise.
    /// Backlight's presentation gate and pending warning commands may override this result.
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
