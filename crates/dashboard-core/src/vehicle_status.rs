//! Transport progress and sample availability are separate from ECU support.
use crate::acquisition::DashboardSnapshot;
use crate::diagnostics::service::{LinkSnapshot, LinkState};
use crate::samples::{DataMode, Quality, SensorId};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VehicleStatus {
    Demo,
    Disabled,
    Starting,
    Retrying,
    Raw,
    Waiting,
    Data,
    Stale,
    ResetRequired,
    Maintenance,
    AdapterOnly,
    SyntheticPeer,
    Pairing,
}
impl VehicleStatus {
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
