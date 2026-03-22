use embassy_time::{Duration, Instant};

pub const POPUP_DURATION: Duration = Duration::from_secs(3);
/// Shorter duration for transient feedback popups (brightness adjustment).
pub const POPUP_DURATION_SHORT: Duration = Duration::from_millis(1500);

#[derive(Clone, Copy, Debug)]
pub enum Popup {
    Reset(Instant),
    Fps(Instant),
    BoostUnit(Instant),
    /// Brightness popup stores (timestamp, brightness_percent).
    Brightness(Instant, u32),
}

impl Popup {
    #[inline]
    pub const fn start_time(&self) -> Instant {
        match self {
            Self::Reset(t) | Self::Fps(t) | Self::BoostUnit(t) | Self::Brightness(t, _) => *t,
        }
    }

    #[inline]
    pub fn is_expired(&self) -> bool {
        let duration = match self {
            Self::Brightness(..) => POPUP_DURATION_SHORT,
            _ => POPUP_DURATION,
        };
        self.start_time().elapsed() >= duration
    }

    #[inline]
    pub const fn kind(&self) -> u8 {
        match self {
            Self::Reset(_) => 0,
            Self::Fps(_) => 1,
            Self::BoostUnit(_) => 2,
            Self::Brightness(..) => 4,
        }
    }
}
