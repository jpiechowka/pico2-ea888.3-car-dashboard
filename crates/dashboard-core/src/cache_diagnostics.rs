//! Bounded exhaustion metadata. Application owners choose where to report it.
use dashboard_config::{CacheIdentityStatus, Channel};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CacheExhaustionCause {
    ConfigIssuer,
    AcquisitionIssuer,
    GraphRevision(Channel),
    WarningEventSerial,
    WarningRequestId,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CacheExhaustionStatus(u16);

impl CacheExhaustionStatus {
    pub const EMPTY: Self = Self(0);

    pub const fn from_cause(cause: CacheExhaustionCause) -> Self {
        Self(
            1 << match cause {
                CacheExhaustionCause::ConfigIssuer => 0,
                CacheExhaustionCause::AcquisitionIssuer => 1,
                CacheExhaustionCause::GraphRevision(channel) => 2 + channel.index(),
                CacheExhaustionCause::WarningEventSerial => 10,
                CacheExhaustionCause::WarningRequestId => 11,
            },
        )
    }

    pub const fn from_config(status: CacheIdentityStatus) -> Self {
        match status {
            CacheIdentityStatus::Exhausted => Self::from_cause(CacheExhaustionCause::ConfigIssuer),
            _ => Self::EMPTY,
        }
    }

    pub const fn union(
        self,
        other: Self,
    ) -> Self {
        Self(self.0 | other.0)
    }

    pub fn causes(self) -> impl Iterator<Item = CacheExhaustionCause> {
        (0..12).filter(move |bit| self.0 & (1 << bit) != 0).map(|bit| {
            match bit {
                0 => CacheExhaustionCause::ConfigIssuer,
                1 => CacheExhaustionCause::AcquisitionIssuer,
                10 => CacheExhaustionCause::WarningEventSerial,
                11 => CacheExhaustionCause::WarningRequestId,
                _ => CacheExhaustionCause::GraphRevision(Channel::ALL[bit - 2]),
            }
        })
    }
}

#[derive(Default)]
pub struct CacheExhaustionSeen(u16);
impl CacheExhaustionSeen {
    pub const fn new() -> Self { Self(0) }

    pub fn observe(
        &mut self,
        status: CacheExhaustionStatus,
    ) -> CacheExhaustionStatus {
        let fresh = CacheExhaustionStatus(status.0 & !self.0);
        self.0 |= status.0;
        fresh
    }
}

impl core::fmt::Display for CacheExhaustionCause {
    fn fmt(
        &self,
        f: &mut core::fmt::Formatter<'_>,
    ) -> core::fmt::Result {
        match self {
            Self::ConfigIssuer => f.write_str("Cache config IDs exhausted"),
            Self::AcquisitionIssuer => f.write_str("Cache origin IDs exhausted"),
            Self::WarningEventSerial => f.write_str("Warning event IDs exhausted; acknowledgment disabled"),
            Self::WarningRequestId => f.write_str("Warning request IDs exhausted"),
            Self::GraphRevision(channel) => write!(f, "Cache graph {} exhausted", channel.label()),
        }
    }
}

const _: () = assert!(core::mem::size_of::<CacheExhaustionStatus>() == 2);
const _: () = assert!(core::mem::size_of::<CacheExhaustionSeen>() == 2);
