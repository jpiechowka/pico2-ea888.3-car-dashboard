//! Descriptor and explicit BOOTSEL inspection snapshots; neither authorizes a flash or measures capacity.
use serde::{Deserialize, Serialize};

use crate::*;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Selectability of a USB descriptor observation; not board authenticity or application health.
pub enum DeviceStatus {
    /// A unique supported serial was observed; chip/security/partition checks are still separate.
    DescriptorOnly,
    /// Duplicate serials prevent unique selection in this scan.
    Ambiguous,
    /// Missing or unsupported serial syntax prevents tool selection.
    Unsupported,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Evidence available for flash capacity; USB descriptors cannot establish it.
pub enum DeviceCapacityState {
    /// No accepted size/provenance pair is present.
    Unverified,
    /// The user confirmed the manufacturer's 4 MiB board specification.
    UserConfirmed,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// Capacity claim carried with its source; cross-field admission is enforced by its enclosing message.
pub struct DeviceCapacity {
    /// Whether a user-confirmed claim is present.
    pub state: DeviceCapacityState,
    /// Bytes: absent when unverified, exactly 4 MiB when user-confirmed.
    pub bytes: Option<u64>,
    /// Explicit manufacturer/user provenance; never a descriptor or size-estimate inference.
    pub provenance: Option<CapacityProvenance>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// One bounded BOOTSEL descriptor observation, not a continuously held device handle.
pub struct DeviceSummary {
    /// Ephemeral scan selection token; not a serial number or stable chip identity.
    pub device_id: DeviceId,
    /// Digest of canonical descriptor/topology inputs; reattachment may change it.
    pub descriptor_sha256: Sha256,
    /// USB vendor ID; validated reports require Raspberry Pi `0x2E8A`.
    pub vid: u16,
    /// USB product ID; validated reports require RP2350 BOOTSEL `0x000F`.
    pub pid: u16,
    /// Optional nonempty control-free serial, bounded in UTF-8 bytes.
    pub serial: Option<Text<126>>,
    /// Optional control-free product text; an untrusted label, not board-model evidence.
    pub product: Option<Text<128>>,
    /// Native enumerator's selection qualification; wire validation does not reconstruct USB state.
    pub status: DeviceStatus,
    /// Bounded explanation of descriptor qualification or failure.
    pub reason: Text<512>,
    /// Separately sourced capacity claim; enumeration normally leaves it unverified.
    pub capacity: DeviceCapacity,
    /// Always false in admitted descriptor reports; a scan cannot authorize programming.
    pub flash_ready: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// At most 16 descriptor observations tied to one helper session.
pub struct DeviceScanReport {
    /// Wire revision; only 1 is admitted.
    pub schema_version: u16,
    /// Helper session that issued this observation.
    pub session_id: SessionId,
    /// Enumeration completed within its policy; false cannot establish device absence.
    pub complete: bool,
    /// Bounded scan-level explanation, including incompleteness.
    pub reason: Text<512>,
    /// Observations with unique selection IDs; USB serial duplication remains explicit status.
    pub devices: BoundedVec<DeviceSummary, 16>,
}
impl Wire for DeviceScanReport {
    /// Maximum encoded/decoded JSON size in bytes.
    const MAX_BYTES: usize = 65536;

    /// Return success for revision 1, unique IDs, supported VID/PID, bounded text and consistent capacity.
    /// Reject flash readiness, descriptor controls and inconsistent claims; this performs no USB I/O.
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
/// Admitted picotool information fields; an observation, not measured flash size or board authentication.
/// See the pinned [picotool parser source](https://github.com/raspberrypi/picotool/blob/2.3.1/main.cpp).
pub struct BootselFacts {
    /// Admitted RP2350 revision spelling: A2, A3 or A4.
    pub revision: Text<2>,
    /// Exact `0x` plus 16 lowercase hexadecimal digits.
    pub chip_id: Text<18>,
    /// Four colon-separated groups of eight lowercase hex digits identifying this boot observation.
    pub boot_random: Text<35>,
    /// Exact supported CPU spelling: `ARM` or `ARM, RISC-V`.
    pub available_cpus: Text<16>,
    /// Must be false under the current explicit flashing policy.
    pub secure_boot: bool,
    /// Must be true under the current explicit flashing policy.
    pub debug_enabled: bool,
    /// Must be true under the current explicit flashing policy.
    pub secure_debug_enabled: bool,
    /// Bounded control-free family list; native parsing additionally checks order, permissions and secure ARM.
    pub unpartitioned_families: Text<128>,
    /// Optional tool-reported byte estimate in 1..=16 MiB; never accepted as a capacity measurement.
    pub flash_size_estimate: Option<u64>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// Explicit selected-device inspection, bracketed by native descriptor checks and still unqualified for flashing.
pub struct DeviceInspection {
    /// Helper session that ran the inspection.
    pub session_id: SessionId,
    /// Selected ephemeral scan token.
    pub device_id: DeviceId,
    /// Descriptor identity expected before and after the information reads.
    pub descriptor_sha256: Sha256,
    /// True exactly when both admitted facts and their digest are present.
    pub complete: bool,
    /// Parsed information on success; absent on incomplete/failed inspection.
    pub facts: Option<BootselFacts>,
    /// SHA-256 of native canonical facts JSON; wire validation checks presence, not recomputation.
    pub information_sha256: Option<Sha256>,
    /// Bounded control-free success limitation or failure explanation.
    pub reason: Text<512>,
    /// Always unverified in an inspection response; user confirmation is a distinct action.
    pub capacity: DeviceCapacity,
    /// Always false; successful information reads do not authorize programming.
    pub flash_ready: bool,
}
impl Wire for DeviceInspection {
    /// Maximum encoded/decoded JSON bytes.
    const MAX_BYTES: usize = 4096;

    /// Return success for coherent completion, unverified capacity and supported fact grammar/security policy.
    /// Reject malformed identity, controls, unsupported revisions and size estimates; no physical claim is verified.
    fn validate(&self) -> Result<(), Error> {
        if self.flash_ready
            || (self.complete != self.facts.is_some() || self.complete != self.information_sha256.is_some())
            || self.capacity.state != DeviceCapacityState::Unverified
            || self.capacity.bytes.is_some()
            || self.capacity.provenance.is_some()
        {
            return Err(Error::Schema);
        }
        if self.reason.chars().any(char::is_control) {
            return Err(Error::Schema);
        }
        if let Some(facts) = &self.facts {
            let lower_hex = |s: &str| s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
            if facts.chip_id.len() != 18
                || !facts.chip_id.starts_with("0x")
                || !lower_hex(&facts.chip_id[2..])
                || facts.boot_random.len() != 35
                || facts.boot_random.split(':').count() != 4
                || !facts
                    .boot_random
                    .split(':')
                    .all(|part| part.len() == 8 && lower_hex(part))
                || !matches!(facts.available_cpus.as_str(), "ARM" | "ARM, RISC-V")
                || facts.unpartitioned_families.chars().any(char::is_control)
                || facts
                    .flash_size_estimate
                    .is_some_and(|n| n == 0 || n > 16 * 1024 * 1024)
            {
                return Err(Error::Schema);
            }
            if !matches!(facts.revision.as_str(), "A2" | "A3" | "A4")
                || facts.secure_boot
                || !facts.debug_enabled
                || !facts.secure_debug_enabled
            {
                return Err(Error::Schema);
            }
        }
        serialized_bound(self, Self::MAX_BYTES)
    }
}
