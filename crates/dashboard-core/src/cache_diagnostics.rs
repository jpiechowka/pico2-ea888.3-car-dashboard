//! Bounded render-cache exhaustion metadata. Application owners choose where to report it.
use dashboard_config::{CacheIdentityStatus, Channel};

/// Bounded identity domain that exhausted; reporting never restores/reuses IDs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CacheExhaustionCause {
    /// Validated-configuration origin issuer cannot mint another proof.
    ConfigIssuer,
    /// Acquisition run-origin issuer cannot mint another proof.
    AcquisitionIssuer,
    /// One channel's graph-content revision overflowed.
    GraphRevision(
        /// Canonical channel whose graph must be conservatively redrawn.
        Channel,
    ),
    /// Warning episode identity exhausted; acknowledgments cannot be safely matched.
    WarningEventSerial,
    /// Warning-command request identity exhausted.
    WarningRequestId,
}

/// Two-byte set of exhaustion causes, suitable for coherent snapshot publication.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CacheExhaustionStatus(
    /// Bits 0/1 issuer, 2..9 graph channels, 10/11 warning domains.
    u16,
);

impl CacheExhaustionStatus {
    /// No observed exhaustion; absence of data is not itself an exhaustion report.
    pub const EMPTY: Self = Self(0);

    /// Returns a set containing exactly `cause` in the stable domain-bit mapping.
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

    /// Returns configuration-issuer exhaustion only for exhausted `status`;
    /// other identity states produce an empty set.
    pub const fn from_config(status: CacheIdentityStatus) -> Self {
        match status {
            CacheIdentityStatus::Exhausted => Self::from_cause(CacheExhaustionCause::ConfigIssuer),
            _ => Self::EMPTY,
        }
    }

    /// Returns the union with `other`, retaining every cause from either set.
    pub const fn union(
        self,
        other: Self,
    ) -> Self {
        Self(self.0 | other.0)
    }

    /// Returns each cause once in issuer, channel-index, then warning-domain order.
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

/// Reporter-local deduplication; a dropped report is still seen and is not retried.
#[derive(Default)]
pub struct CacheExhaustionSeen(
    /// Union of causes already observed by this reporter.
    u16,
);
impl CacheExhaustionSeen {
    /// Returns a reporter with no observed causes.
    pub const fn new() -> Self { Self(0) }

    /// Returns only newly seen causes from `status` and permanently records all
    /// its bits. Reporting success/failure does not affect drawing or this history.
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
    /// Writes a bounded human-readable cause to `f`; returns its formatting result,
    /// including sink errors. Does not mutate the exhausted owner or retry IDs.
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

/// Snapshot footprint invariant for cause metadata.
const _: () = assert!(core::mem::size_of::<CacheExhaustionStatus>() == 2);
/// Reporter footprint invariant for deduplication state.
const _: () = assert!(core::mem::size_of::<CacheExhaustionSeen>() == 2);
