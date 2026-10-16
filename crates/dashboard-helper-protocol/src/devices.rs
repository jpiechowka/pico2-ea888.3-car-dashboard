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
    pub descriptor_sha256: Sha256,
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BootselFacts {
    pub revision: Text<2>,
    pub chip_id: Text<18>,
    pub boot_random: Text<35>,
    pub available_cpus: Text<16>,
    pub secure_boot: bool,
    pub debug_enabled: bool,
    pub secure_debug_enabled: bool,
    pub unpartitioned_families: Text<128>,
    pub flash_size_estimate: Option<u64>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceInspection {
    pub session_id: SessionId,
    pub device_id: DeviceId,
    pub descriptor_sha256: Sha256,
    pub complete: bool,
    pub facts: Option<BootselFacts>,
    pub information_sha256: Option<Sha256>,
    pub reason: Text<512>,
    pub capacity: DeviceCapacity,
    pub flash_ready: bool,
}
impl Wire for DeviceInspection {
    const MAX_BYTES: usize = 4096;
    fn validate(&self)->Result<(),Error>{
        if self.flash_ready||(self.complete!=self.facts.is_some()||self.complete!=self.information_sha256.is_some())||self.capacity.state!=DeviceCapacityState::Unverified||self.capacity.bytes.is_some()||self.capacity.provenance.is_some(){return Err(Error::Schema)}
        if self.reason.chars().any(char::is_control){return Err(Error::Schema)}
        if let Some(facts)=&self.facts {
            let lower_hex=|s:&str|s.bytes().all(|b|b.is_ascii_digit()||(b'a'..=b'f').contains(&b));
            if facts.chip_id.len()!=18||!facts.chip_id.starts_with("0x")||!lower_hex(&facts.chip_id[2..])||facts.boot_random.len()!=35||facts.boot_random.split(':').count()!=4||!facts.boot_random.split(':').all(|part|part.len()==8&&lower_hex(part))||!matches!(facts.available_cpus.as_str(),"ARM"|"ARM, RISC-V")||facts.unpartitioned_families.chars().any(char::is_control)||facts.flash_size_estimate.is_some_and(|n|n==0||n>16*1024*1024){return Err(Error::Schema)}
            if !matches!(facts.revision.as_str(),"A2"|"A3"|"A4")||facts.secure_boot||!facts.debug_enabled||!facts.secure_debug_enabled{return Err(Error::Schema)}}
        serialized_bound(self,Self::MAX_BYTES)
    }
}
