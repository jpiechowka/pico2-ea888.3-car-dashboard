//! Transport progress and sample availability are separate from ECU support.
use crate::acquisition::DashboardSnapshot;
use crate::diagnostics::service::{LinkSnapshot, LinkState};
use crate::samples::{DataMode, Quality, SensorId};

/// Header status distinguishing synthetic data, link progress and qualified current samples.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VehicleStatus {
    /// Explicit synthetic acquisition mode; header label is empty.
    Demo,
    /// Vehicle transport disabled.
    Disabled,
    /// Enabled owner absent or link/adapter initialization ongoing.
    Starting,
    /// Recoverable link failure is backing off/retrying.
    Retrying,
    /// Legacy byte framing exists, but missing CAN metadata remains unqualified.
    Raw,
    /// Qualified polling is active without a currently valid or stale measurement.
    Waiting,
    /// At least one source-qualified fresh value exists; not a vehicle-health certification.
    Data,
    /// No fresh value exists and at least one channel explicitly reports Stale.
    Stale,
    /// Terminal owner/link fault requiring explicit reset/recovery.
    ResetRequired,
    /// Acquisition link is parked or draining for maintenance.
    Maintenance,
    /// BLE adapter identified; ECU request path remains disabled.
    AdapterOnly,
    /// Explicit synthetic BLE test peer, never vehicle acquisition.
    SyntheticPeer,
    /// BLE security establishment is in progress.
    Pairing,
}
impl VehicleStatus {
    /// Returns compact fixed ASCII header text; Demo intentionally returns empty text.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Demo => "",
            Self::Disabled => "NO LINK",
            Self::Starting => "STARTING",
            Self::Retrying => "RETRY",
            Self::Raw => "RAW ONLY",
            Self::Waiting => "WAIT ECU",
            Self::Data => "DATA",
            Self::Stale => "STALE",
            Self::ResetRequired => "RESET REQ",
            Self::Maintenance => "MAINT",
            Self::AdapterOnly => "BLE ONLY",
            Self::SyntheticPeer => "TEST PEER",
            Self::Pairing => "PAIRING",
        }
    }

    /// A connected prototype adapter has no qualified vehicle acquisition path.
    /// Returns status for BLE `state`; AdapterReady/SyntheticReady never map to Data or Demo.
    pub const fn from_ble(state: crate::ble::link::LinkState) -> Self {
        use crate::ble::link::LinkState as Ble;
        match state {
            Ble::Disabled => Self::Disabled,
            Ble::Starting | Ble::Connecting | Ble::Discovering | Ble::Identifying => Self::Starting,
            Ble::Securing => Self::Pairing,
            Ble::AdapterReady => Self::AdapterOnly,
            Ble::SyntheticReady => Self::SyntheticPeer,
            Ble::Backoff => Self::Retrying,
            Ble::Maintenance => Self::Maintenance,
            Ble::Fault => Self::ResetRequired,
        }
    }

    /// Returns status from transport `enabled`, optional `link`, canonical `samples`
    /// and monotonic `now` milliseconds. Demo wins first, then disabled/missing link;
    /// only Polling inspects samples, preferring any fresh source-qualified value over
    /// explicit stale quality, otherwise Waiting. Other link phases override sample history.
    pub fn derive(
        enabled: bool,
        link: Option<LinkSnapshot>,
        samples: &DashboardSnapshot,
        now: u64,
    ) -> Self {
        if samples.mode == DataMode::Demo {
            return Self::Demo;
        }
        if !enabled {
            return Self::Disabled;
        }
        let Some(link) = link else {
            return Self::Starting;
        };
        match link.state {
            LinkState::Recovering => Self::Retrying,
            LinkState::Quiet | LinkState::Handshake => Self::Starting,
            LinkState::Unqualified => Self::Raw,
            LinkState::Quarantined => Self::ResetRequired,
            LinkState::Maintenance => Self::Maintenance,
            LinkState::Polling => {
                if SensorId::ALL.iter().any(|&id| samples.value_at(id, now).is_some()) {
                    Self::Data
                } else if SensorId::ALL
                    .iter()
                    .any(|&id| samples.quality(id, now) == Quality::Stale)
                {
                    Self::Stale
                } else {
                    Self::Waiting
                }
            }
        }
    }
}
