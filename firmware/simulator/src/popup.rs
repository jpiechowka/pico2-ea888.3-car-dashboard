//! Popup state management with time-based expiration.
//!
//! Each popup variant holds its start time for expiration checking.

use std::time::Instant;

use crate::timing::POPUP_DURATION;

/// Active popup with its start time.
///
/// Consolidates popup state into a single enum. Each variant holds the `Instant`
/// when the popup was triggered, making expiration checks straightforward.
#[derive(Clone, Copy, Debug)]
pub enum Popup {
    /// "MIN/AVG/MAX RESET" popup (larger, 180×60).
    Reset(Instant),
    /// "FPS ON/OFF" popup (smaller, 140×50).
    Fps(Instant),
    /// "BOOST: BAR/PSI" popup (same size as FPS).
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
