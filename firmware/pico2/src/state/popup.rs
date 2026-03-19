use embassy_time::{Duration, Instant};

pub const POPUP_DURATION: Duration = Duration::from_secs(3);

#[derive(Clone, Copy, Debug)]
pub enum Popup {
    Reset(Instant),
    Fps(Instant),
    BoostUnit(Instant),
}

impl Popup {
    #[inline]
    pub const fn start_time(&self) -> Instant {
        match self {
            Self::Reset(t) | Self::Fps(t) | Self::BoostUnit(t) => *t,
        }
    }

    #[inline]
    pub fn is_expired(&self) -> bool { self.start_time().elapsed() >= POPUP_DURATION }

    #[inline]
    pub const fn kind(&self) -> u8 {
        match self {
            Self::Reset(_) => 0,
            Self::Fps(_) => 1,
            Self::BoostUnit(_) => 2,
        }
    }
}
