//! Untrusted name observations. Learning a name never initiates a connection.
use super::capture::{CaptureView, Record};
use super::{AdFields, Device, Payload, Peer, ReadResult};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NameSource {
    Advertisement,
    ScanResponse,
    Gap,
}
impl NameSource {
    pub const fn code(self) -> u8 {
        match self {
            Self::Advertisement => 1,
            Self::ScanResponse => 2,
            Self::Gap => 3,
        }
    }

    pub const fn from_code(code: u8) -> Option<Self> {
        match code {
            1 => Some(Self::Advertisement),
            2 => Some(Self::ScanResponse),
            3 => Some(Self::Gap),
            _ => None,
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Advertisement => "advertisement",
            Self::ScanResponse => "scan_response",
            Self::Gap => "gap_device_name",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ObservedName {
    pub data: Payload<64>,
    pub source: NameSource,
    pub complete: bool,
    /// Boot-relative packet time, or end of the explicit inspection for GAP.
    pub at_ms: u64,
}
impl ObservedName {
    pub fn as_ref(&self) -> NameRef<'_> {
        NameRef {
            bytes: self.data.as_slice(),
            source: self.source,
            complete: self.complete,
            truncated: self.data.truncated,
            record_index: None,
            observed_at_ms: Some(self.at_ms),
            cached: true,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct NameRef<'a> {
    pub bytes: &'a [u8],
    pub source: NameSource,
    pub complete: bool,
    pub truncated: bool,
    pub record_index: Option<usize>,
    pub observed_at_ms: Option<u64>,
    pub cached: bool,
}

fn packet_name(
    bytes: &[u8],
    truncated: bool,
    source: NameSource,
    at_ms: Option<u64>,
    kind: u8,
) -> Option<NameRef<'_>> {
    // Invalid lengths terminate iteration; only a valid prefix can supply a name.
    AdFields::new(bytes)
        .flatten()
        .find(|f| f.kind == kind && !f.data.is_empty())
        .map(|f| {
            NameRef {
                bytes: f.data,
                source,
                complete: kind == 9,
                truncated,
                record_index: None,
                observed_at_ms: at_ms,
                cached: false,
            }
        })
}

pub(super) fn packet_observation(
    data: &Payload<31>,
    source: NameSource,
    at_ms: u64,
) -> Option<ObservedName> {
    let name = packet_name(data.as_slice(), data.truncated, source, Some(at_ms), 9)
        .or_else(|| packet_name(data.as_slice(), data.truncated, source, Some(at_ms), 8))?;
    let mut data = Payload::new(name.bytes);
    data.truncated |= name.truncated;
    Some(ObservedName {
        data,
        source,
        complete: name.complete,
        at_ms,
    })
}

pub fn device_name(device: &Device) -> Option<NameRef<'_>> {
    let packets = [
        (&device.response, device.response_meta, NameSource::ScanResponse),
        (
            &device.advertisement,
            device.advertisement_meta,
            NameSource::Advertisement,
        ),
    ];
    let packet = |kind| {
        packets
            .iter()
            .find_map(|(p, m, s)| packet_name(p.as_slice(), p.truncated, *s, m.map(|m| m.at_ms), kind))
    };
    packet(9)
        .or_else(|| {
            device
                .cached_name
                .as_ref()
                .filter(|n| n.complete && !n.data.truncated)
                .map(ObservedName::as_ref)
        })
        .or_else(|| packet(8))
        .or_else(|| device.cached_name.as_ref().map(ObservedName::as_ref))
}

/// Capture admission binds each standard read to its actual GAP declaration.
pub fn capture_name<'a>(view: &'a CaptureView<'_>) -> Option<NameRef<'a>> {
    let m = view.metadata();
    let packets = [
        (&m.response, NameSource::ScanResponse),
        (&m.advertisement, NameSource::Advertisement),
    ];
    let packet = |kind| {
        packets
            .iter()
            .find_map(|(p, s)| packet_name(p.bytes(), p.truncated, *s, p.observation.map(|m| m.at_ms), kind))
    };
    packet(9)
        .or_else(|| {
            view.records().enumerate().find_map(|(index, record)| {
                match record {
                    Record::Value(v)
                        if v.uuid == 0x2A00 && v.result == ReadResult::Ok && !v.truncated && !v.bytes.is_empty() =>
                    {
                        Some(NameRef {
                            bytes: v.bytes,
                            source: NameSource::Gap,
                            complete: true,
                            truncated: false,
                            record_index: Some(index),
                            observed_at_ms: m.ended_at_ms,
                            cached: false,
                        })
                    }
                    _ => None,
                }
            })
        })
        .or_else(|| {
            m.cached_name
                .as_ref()
                .filter(|n| n.complete && !n.data.truncated)
                .map(ObservedName::as_ref)
        })
        .or_else(|| packet(8))
        .or_else(|| m.cached_name.as_ref().map(ObservedName::as_ref))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SavedIdentity {
    pub peer: Peer,
    pub name: Payload<31>,
}
impl SavedIdentity {
    pub fn from_capture(view: &CaptureView<'_>) -> Self {
        Self {
            peer: view.metadata().peer,
            name: capture_name(view).map_or_else(Payload::empty, |n| {
                let mut name = Payload::new(n.bytes);
                name.truncated |= n.truncated;
                name
            }),
        }
    }
}
