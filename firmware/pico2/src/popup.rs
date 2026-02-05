//! Popup state management for the dashboard.
//!
//! Handles temporary overlay popups that appear when buttons are pressed
//! (FPS toggle, boost unit change, statistics reset).

use embassy_time::{Duration, Instant};

/// Duration that popups remain visible on screen.
pub const POPUP_DURATION: Duration = Duration::from_secs(3);

/// Active popup with its start time.
#[derive(Clone, Copy, Debug)]
pub enum Popup {
    /// "MIN/AVG/MAX RESET" popup.
    Reset(Instant),
    /// "FPS ON/OFF" popup.
    Fps(Instant),
    /// "BOOST: BAR/PSI" popup.
    BoostUnit(Instant),
}

impl Popup {
    /// Get the start time of this popup.
    #[inline]
    pub const fn start_time(&self) -> Instant {
        match self {
            Self::Reset(t) | Self::Fps(t) | Self::BoostUnit(t) => *t,
        }
    }

    /// Check if this popup has expired.
    #[inline]
    pub fn is_expired(&self) -> bool { self.start_time().elapsed() >= POPUP_DURATION }

    /// Get the popup kind as a u8 discriminant for RenderState tracking.
    #[inline]
    pub const fn kind(&self) -> u8 {
        match self {
            Self::Reset(_) => 0,
            Self::Fps(_) => 1,
            Self::BoostUnit(_) => 2,
        }
    }
}
