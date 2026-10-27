//! Bounded GATT inventory, declaration relationships and standard-read allowlist.
//!
//! Declaration layout follows [Bluetooth GATT, section 3.3](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/Core-61/out/en/host/generic-attribute-profile--gatt-.html).
use heapless::Vec;

use super::{FailureReason, Payload};

/// UUID bytes use Bluetooth wire order (least significant byte first).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Uuid {
    /// Little-endian UUID prefix, zero-padded by `from_wire`; not display-string order.
    pub bytes: [u8; 16],
    /// Encoded width in bytes: 2, 4 or 16 for an admitted UUID.
    pub len: u8,
}
impl Uuid {
    /// Returns an owned UUID copied from little-endian `bytes`, accepting exactly
    /// 2, 4 or 16 bytes; returns `None` for every other encoded width.
    pub fn from_wire(bytes: &[u8]) -> Option<Self> {
        if !matches!(bytes.len(), 2 | 4 | 16) {
            return None;
        }
        let mut uuid = Self {
            bytes: [0; 16],
            len: bytes.len() as u8,
        };
        uuid.bytes[..bytes.len()].copy_from_slice(bytes);
        Some(uuid)
    }

    /// Returns `self` as a 16-bit assigned UUID when encoded directly, zero-extended
    /// to 32 bits, or embedded in the exact Bluetooth base UUID; otherwise returns `None`.
    pub const fn short(self) -> Option<u16> {
        if self.len == 2 || (self.len == 4 && self.bytes[2] == 0 && self.bytes[3] == 0) {
            Some(u16::from_le_bytes([self.bytes[0], self.bytes[1]]))
        } else if self.len == 16 && self.bytes[14] == 0 && self.bytes[15] == 0 {
            let base = [0xFB, 0x34, 0x9B, 0x5F, 0x80, 0, 0, 0x80, 0, 0x10, 0, 0];
            let mut i = 0;
            while i < base.len() {
                if self.bytes[i] != base[i] {
                    return None;
                }
                i += 1;
            }
            Some(u16::from_le_bytes([self.bytes[12], self.bytes[13]]))
        } else {
            None
        }
    }
}
/// Primary-service declaration and inclusive attribute range.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Service {
    /// Nonzero service declaration handle after admission.
    pub start: u16,
    /// Inclusive last attribute handle, at least `start` after admission.
    pub end: u16,
    /// Service UUID in Bluetooth wire order.
    pub uuid: Uuid,
}
/// Characteristic declaration associated with an admitted primary service.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Characteristic {
    /// Owning service's declaration handle, used as its inventory identity.
    pub service_start: u16,
    /// Characteristic declaration handle; strictly before its value handle.
    pub declaration: u16,
    /// Characteristic value handle, bounded by the owning service.
    pub handle: u16,
    /// Raw GATT property bitmask; bit 1 permits the read allowlist check.
    pub properties: u8,
    /// Characteristic UUID in Bluetooth wire order.
    pub uuid: Uuid,
}
/// Outcome of one standard-information read, independently of retained payload length.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReadResult {
    /// Read succeeded; `InfoValue::failure` must be absent.
    Ok,
    /// Read requires authentication, authorization or encryption unavailable to the probe.
    Protected,
    /// Standard value was unavailable; raw reason remains in failure evidence.
    Unavailable,
    /// Other failed read, with a retained failure classification.
    Error,
}
/// Information-read observation; standard inventory and explicit identity trials
/// separately admit only their matching allowlisted declarations.
#[derive(Clone, Copy, Debug)]
pub struct InfoValue {
    /// Owning primary-service declaration handle.
    pub service_start: u16,
    /// Read characteristic value handle.
    pub handle: u16,
    /// Normalized 16-bit characteristic UUID, scoped by the admitting operation.
    pub uuid: u16,
    /// Up to 64 returned bytes; truncation remains explicit and bytes need not be text.
    pub value: Payload<64>,
    /// Read outcome, required to agree with presence/absence of `failure`.
    pub result: ReadResult,
    /// Absent exactly for successful reads; otherwise the read's failure classification.
    pub failure: Option<FailureReason>,
    /// Read duration in milliseconds as measured by the producer.
    pub elapsed_ms: u32,
}
/// Descriptor declaration bounded by its owning characteristic and service.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Descriptor {
    /// Owning primary-service declaration handle.
    pub service_start: u16,
    /// Owning characteristic's value handle, strictly before this descriptor.
    pub characteristic_handle: u16,
    /// Descriptor attribute handle; precedes the next characteristic declaration.
    pub handle: u16,
    /// Descriptor UUID in Bluetooth wire order; descriptor values are not read here.
    pub uuid: Uuid,
}
/// Observed HCI connection parameters. These are not requested limits or CoC MTU.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LinkInfo {
    /// Boot-relative milliseconds when connection completion was observed.
    pub connected_at_ms: u64,
    /// Elapsed milliseconds from connect request to connection completion.
    pub connect_elapsed_ms: u32,
    /// Observed connection-event interval in microseconds.
    pub interval_us: u32,
    /// Observed peripheral latency in skipped connection events.
    pub latency: u16,
    /// Observed link supervision timeout in microseconds.
    pub supervision_timeout_us: u32,
    /// Negotiated ATT MTU in bytes, or unavailable before successful exchange.
    pub att_mtu: Option<u16>,
}
/// Independent enumeration acknowledgments indexed into `Inspection::stages`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InventoryStage {
    /// All primary services have been enumerated.
    Services,
    /// All characteristic declarations have been enumerated.
    Characteristics,
    /// All eligible descriptor ranges have been enumerated.
    Descriptors,
    /// All eligible standard reads have reached outcomes.
    Reads,
}
/// Owned event payload admitted to the current inspection by its outer request generation.
#[derive(Clone, Copy, Debug)]
pub enum InspectionItem {
    /// Small update to a separately requested identity trial, never a standard read.
    Identity(
        /// Revision fencing occurs in the outer model; relationships are checked here.
        super::identity::Update,
    ),
    /// Primary-service declaration candidate.
    Service(
        /// Service and inclusive attribute range.
        Service,
    ),
    /// Characteristic declaration candidate.
    Characteristic(
        /// Declaration and value handle association.
        Characteristic,
    ),
    /// Allowlisted information read candidate.
    Value(
        /// Read bytes, outcome and timing.
        InfoValue,
    ),
    /// Descriptor declaration candidate.
    Descriptor(
        /// Descriptor and characteristic association.
        Descriptor,
    ),
    /// Connection metadata, replacing an earlier link observation if present.
    Link(
        /// Observed link parameters and optional negotiated ATT MTU.
        LinkInfo,
    ),
    /// Producer acknowledgment that an inventory stage ended.
    StageComplete(
        /// Stage whose completion bit is set.
        InventoryStage,
    ),
}

/// Returns the display label for allowlisted standard characteristic `uuid`, or
/// `None` for other UUIDs. A label alone does not authorize a read; use [`may_read`].
pub const fn info_label(uuid: u16) -> Option<&'static str> {
    match uuid {
        0x2A29 => Some("Manufacturer"),
        0x2A24 => Some("Model"),
        0x2A25 => Some("Serial"),
        0x2A27 => Some("Hardware"),
        0x2A26 => Some("Firmware"),
        0x2A28 => Some("Software"),
        0x2A23 => Some("System ID"),
        0x2A50 => Some("PnP ID"),
        0x2A00 => Some("Device Name"),
        0x2A01 => Some("Appearance"),
        0x2A04 => Some("Preferred Parameters"),
        0x2B2A => Some("Database Hash"),
        _ => None,
    }
}
/// Returns whether `service`/`characteristic` form an allowlisted DIS, GAP or GATT
/// pair and raw `properties` includes the Read bit (0x02). Bluetooth-base aliases
/// are accepted; arbitrary vendor UUIDs are rejected. This grants no write/pairing access.
pub const fn may_read(
    service: Uuid,
    characteristic: Uuid,
    properties: u8,
) -> bool {
    properties & 2 != 0
        && matches!(
            (service.short(), characteristic.short()),
            (
                Some(0x180A),
                Some(0x2A29 | 0x2A24 | 0x2A25 | 0x2A27 | 0x2A26 | 0x2A28 | 0x2A23 | 0x2A50)
            ) | (Some(0x1800), Some(0x2A00 | 0x2A01 | 0x2A04))
                | (Some(0x1801), Some(0x2B2A))
        )
}

/// Returns whether byte `length` matches fixed-width standard characteristic `uuid`:
/// Appearance 2, Preferred Parameters/System ID 8, PnP ID 7, Database Hash 16.
/// Other UUIDs return true; this helper does not establish allowlist membership.
pub const fn info_length_valid(
    uuid: u16,
    length: usize,
) -> bool {
    match uuid {
        0x2A01 => length == 2,
        0x2A04 | 0x2A23 => length == 8,
        0x2A50 => length == 7,
        0x2B2A => length == 16,
        _ => true,
    }
}

/// A partial characteristic list cannot define its final characteristic's end.
/// Returns the inclusive descriptor range after `rows[index]` within `service`.
/// `index` selects the owning characteristic in `rows`.
/// `rows` must preserve declaration order; `complete` permits using the service end
/// for the final row. Returns `None` for missing/mismatched rows, arithmetic limits,
/// empty/reversed bounds, or an incomplete final range. Performs no discovery itself.
pub fn descriptor_range(
    service: Service,
    rows: &[Characteristic],
    index: usize,
    complete: bool,
) -> Option<(u16, u16)> {
    let row = rows.get(index)?;
    if row.service_start != service.start {
        return None;
    }
    let end = match rows.get(index + 1) {
        Some(next) if next.service_start == service.start => next.declaration.checked_sub(1)?,
        Some(_) => return None,
        None if complete => service.end,
        None => return None,
    };
    let start = row.handle.checked_add(1)?;
    (start <= end && end <= service.end).then_some((start, end))
}

/// Bounded, partial inventory retained even when enumeration fails or is cancelled.
pub struct Inspection {
    /// Explicit identity-trial evidence, absent for ordinary standard-only inspections.
    pub identity: Option<super::identity::Evidence>,
    /// Admitted, nonoverlapping primary services; at most 24.
    pub services: Vec<Service, 24>,
    /// Admitted nonoverlapping declaration/value pairs; at most 64.
    pub characteristics: Vec<Characteristic, 64>,
    /// Admitted descriptor handles with ownership relationships; at most 128.
    pub descriptors: Vec<Descriptor, 128>,
    /// Latest read outcome for each admitted service/value pair; at most 16.
    pub values: Vec<InfoValue, 16>,
    /// Most recent observed connection metadata; absent before a link exists.
    pub link: Option<LinkInfo>,
    /// Boot-relative inspection start time in milliseconds, absent before an explicit request.
    pub started_at_ms: Option<u64>,
    /// Boot-relative lifecycle end time in milliseconds, absent while unfinished.
    pub ended_at_ms: Option<u64>,
    /// Controller-confirmed HCI disconnect reason, absent without terminal evidence.
    pub disconnect_reason: Option<u8>,
    /// Radio owner acknowledged termination; independent of inventory completeness.
    pub cleanup_complete: bool,
    /// Service, characteristic, descriptor and read completion acknowledgments in enum order.
    pub stages: [bool; 4],
    /// Saturating number of items/producer limits omitted due to capacity.
    pub omitted: u32,
    /// Saturating count of declarations or reads rejected by admission guards.
    pub malformed: u32,
    /// Outer model accepted a clean completion; cleanup status is tracked separately.
    pub complete: bool,
}
impl Default for Inspection {
    /// Returns empty incomplete inventory with no timing or link evidence.
    fn default() -> Self { Self::new() }
}
impl Inspection {
    /// Returns empty incomplete inventory; all stage/cleanup flags and counters start clear.
    pub const fn new() -> Self {
        Self {
            identity: None,
            services: Vec::new(),
            characteristics: Vec::new(),
            descriptors: Vec::new(),
            values: Vec::new(),
            link: None,
            started_at_ms: None,
            ended_at_ms: None,
            disconnect_reason: None,
            cleanup_complete: false,
            stages: [false; 4],
            omitted: 0,
            malformed: 0,
            complete: false,
        }
    }

    /// Applies `item` to `self`, returning unit. Validates UUID widths, handle ownership,
    /// range overlap, read allowlisting and read-result/failure consistency before insertion.
    /// Rejected structure increments `malformed`; full vectors increment `omitted`;
    /// both saturate. Repeated valid reads replace their prior outcome. Link and stage
    /// events update metadata directly; this method never declares the whole inspection complete.
    /// Fixed-width standard values with unexpected lengths remain evidence; consumers can
    /// mark them using [`info_length_valid`]. The outer model checks request generation.
    pub fn accept(
        &mut self,
        item: InspectionItem,
    ) {
        let accepted = match item {
            InspectionItem::Identity(update) => {
                if let super::identity::Update::Start { at_ms } = update {
                    if self.identity.is_some() || self.started_at_ms.is_none_or(|start| at_ms < start) {
                        self.malformed = self.malformed.saturating_add(1);
                    } else {
                        // Initialize the final optional evidence slot.
                        let _ = self.identity.insert(super::identity::Evidence::new(at_ms));
                    }
                    return;
                }
                let relationship = match &update {
                    super::identity::Update::Vendor(value) => {
                        self.stages.iter().all(|&done| done)
                            && super::identity::vendor_relationship(value, &self.services, &self.characteristics)
                    }
                    _ => true,
                };
                if !relationship || self.identity.as_mut().is_none_or(|e| !e.apply(update)) {
                    self.malformed = self.malformed.saturating_add(1);
                }
                return;
            }
            InspectionItem::Service(service) => {
                if !matches!(service.uuid.len, 2 | 4 | 16)
                    || service.start == 0
                    || service.end < service.start
                    || self
                        .services
                        .iter()
                        .any(|s| service.start <= s.end && service.end >= s.start)
                {
                    self.malformed = self.malformed.saturating_add(1);
                    return;
                }
                self.services.push(service).is_ok()
            }
            InspectionItem::Characteristic(characteristic) => {
                let valid = self.services.iter().any(|service| {
                    service.start == characteristic.service_start
                        && characteristic.declaration > service.start
                        && characteristic.handle > characteristic.declaration
                        && characteristic.handle <= service.end
                });
                if !valid
                    || !matches!(characteristic.uuid.len, 2 | 4 | 16)
                    || self.characteristics.iter().any(|c| {
                        c.handle == characteristic.handle
                            || c.declaration == characteristic.declaration
                            || c.handle == characteristic.declaration
                            || c.declaration == characteristic.handle
                            || (c.service_start == characteristic.service_start
                                && characteristic.declaration <= c.handle
                                && c.declaration <= characteristic.handle)
                    })
                {
                    self.malformed = self.malformed.saturating_add(1);
                    return;
                }
                self.characteristics.push(characteristic).is_ok()
            }
            InspectionItem::Value(value) => {
                let valid = self.characteristics.iter().any(|c| {
                    c.handle == value.handle
                        && c.service_start == value.service_start
                        && c.uuid.short() == Some(value.uuid)
                        && self
                            .services
                            .iter()
                            .any(|s| s.start == c.service_start && may_read(s.uuid, c.uuid, c.properties))
                });
                if !valid
                    || usize::from(value.value.len) > 64
                    || (value.result == ReadResult::Ok) != value.failure.is_none()
                {
                    self.malformed = self.malformed.saturating_add(1);
                    return;
                }
                if let Some(previous) = self
                    .values
                    .iter_mut()
                    .find(|v| v.handle == value.handle && v.service_start == value.service_start)
                {
                    *previous = value;
                    true
                } else {
                    self.values.push(value).is_ok()
                }
            }
            InspectionItem::Descriptor(descriptor) => {
                let valid = matches!(descriptor.uuid.len, 2 | 4 | 16)
                    && self.characteristics.iter().any(|c| {
                        c.handle == descriptor.characteristic_handle
                            && c.service_start == descriptor.service_start
                            && descriptor.handle > c.handle
                            && self
                                .services
                                .iter()
                                .any(|s| s.start == c.service_start && descriptor.handle <= s.end)
                            && !self.characteristics.iter().any(|next| {
                                next.service_start == c.service_start
                                    && next.declaration > c.handle
                                    && next.declaration <= descriptor.handle
                            })
                    });
                if !valid || self.descriptors.iter().any(|d| d.handle == descriptor.handle) {
                    self.malformed = self.malformed.saturating_add(1);
                    return;
                }
                self.descriptors.push(descriptor).is_ok()
            }
            InspectionItem::Link(link) => {
                self.link = Some(link);
                true
            }
            InspectionItem::StageComplete(stage) => {
                self.stages[stage as usize] = true;
                true
            }
        };
        if !accepted {
            self.omitted = self.omitted.saturating_add(1);
        }
    }
}
