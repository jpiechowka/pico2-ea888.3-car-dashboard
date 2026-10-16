use serde::{Deserialize, Serialize};

use crate::*;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceStatus {
    DescriptorOnly,
    Ambiguous,
    Unsupported,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceCapacityState {
    Unverified,
    UserConfirmed,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceCapacity {
    pub state: DeviceCapacityState,
    pub bytes: Option<u64>,
    pub provenance: Option<CapacityProvenance>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceSummary {
    pub device_id: DeviceId,
    pub vid: u16,
    pub pid: u16,
    pub serial: Option<Text<126>>,
    pub product: Option<Text<128>>,
    pub status: DeviceStatus,
    pub reason: Text<512>,
    pub capacity: DeviceCapacity,
    pub flash_ready: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceScanReport {
    pub schema_version: u16,
    pub session_id: SessionId,
    pub complete: bool,
    pub reason: Text<512>,
    pub devices: BoundedVec<DeviceSummary, 16>,
}
impl Wire for DeviceScanReport {
    const MAX_BYTES: usize = 65536;

    fn validate(&self) -> Result<(), Error> {
        if self.schema_version != 1 {
            return Err(Error::Schema);
        }
        for (i, d) in self.devices.iter().enumerate() {
            if d.vid != 0x2E8A
                || d.pid != 0x000F
                || d.flash_ready
                || self.devices[..i].iter().any(|other| other.device_id == d.device_id)
            {
                return Err(Error::Schema);
            }
            if d.serial
                .as_ref()
                .is_some_and(|s| s.is_empty() || s.chars().any(char::is_control))
                || d.product.as_ref().is_some_and(|s| s.chars().any(char::is_control))
            {
                return Err(Error::Schema);
            }
            match d.capacity.state {
                DeviceCapacityState::Unverified if d.capacity.bytes.is_none() && d.capacity.provenance.is_none() => {}
                DeviceCapacityState::UserConfirmed
                    if d.capacity.bytes == Some(4 * 1024 * 1024)
                        && d.capacity.provenance == Some(CapacityProvenance::Manufacturer4MbUserConfirmed) => {}
                _ => return Err(Error::Schema),
            }
        }
        serialized_bound(self, Self::MAX_BYTES)
    }
}
