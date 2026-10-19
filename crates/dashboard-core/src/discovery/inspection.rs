use heapless::Vec;

use super::{FailureReason, Payload};

/// UUID bytes use Bluetooth wire order (least significant byte first).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Uuid {
    pub bytes: [u8; 16],
    pub len: u8,
}
impl Uuid {
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Service {
    pub start: u16,
    pub end: u16,
    pub uuid: Uuid,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Characteristic {
    pub service_start: u16,
    pub declaration: u16,
    pub handle: u16,
    pub properties: u8,
    pub uuid: Uuid,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReadResult {
    Ok,
    Protected,
    Unavailable,
    Error,
}
#[derive(Clone, Copy, Debug)]
pub struct InfoValue {
    pub service_start: u16,
    pub handle: u16,
    pub uuid: u16,
    pub value: Payload<64>,
    pub result: ReadResult,
    pub failure: Option<FailureReason>,
    pub elapsed_ms: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Descriptor {
    pub service_start: u16,
    pub characteristic_handle: u16,
    pub handle: u16,
    pub uuid: Uuid,
}
/// Observed HCI connection parameters. These are not requested limits or CoC MTU.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LinkInfo {
    pub connected_at_ms: u64,
    pub connect_elapsed_ms: u32,
    pub interval_us: u32,
    pub latency: u16,
    pub supervision_timeout_us: u32,
    pub att_mtu: Option<u16>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InventoryStage {
    Services,
    Characteristics,
    Descriptors,
    Reads,
}
#[derive(Clone, Copy, Debug)]
pub enum InspectionItem {
    Service(Service),
    Characteristic(Characteristic),
    Value(InfoValue),
    Descriptor(Descriptor),
    Link(LinkInfo),
    StageComplete(InventoryStage),
}

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

pub struct Inspection {
    pub services: Vec<Service, 24>,
    pub characteristics: Vec<Characteristic, 64>,
    pub descriptors: Vec<Descriptor, 128>,
    pub values: Vec<InfoValue, 16>,
    pub link: Option<LinkInfo>,
    pub started_at_ms: Option<u64>,
    pub ended_at_ms: Option<u64>,
    pub disconnect_reason: Option<u8>,
    pub cleanup_complete: bool,
    pub stages: [bool; 4],
    pub omitted: u32,
    pub malformed: u32,
    pub complete: bool,
}
impl Default for Inspection {
    fn default() -> Self { Self::new() }
}
impl Inspection {
    pub const fn new() -> Self {
        Self {
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

    pub fn accept(
        &mut self,
        item: InspectionItem,
    ) {
        let accepted = match item {
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
