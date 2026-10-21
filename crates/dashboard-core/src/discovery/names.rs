//! Untrusted name observations. Learning a name never initiates a connection.
use super::capture::{CaptureView, Record};
use super::{AdFields, Device, Payload, Peer, ReadResult};

/// Origin of untrusted name bytes; does not certify the device's identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NameSource {
    /// Local-name AD field in an advertising packet.
    Advertisement,
    /// Local-name AD field in the corresponding peer's scan response.
    ScanResponse,
    /// Allowlisted GAP Device Name characteristic read during explicit inspection.
    Gap,
}
impl NameSource {
    /// Returns `self`'s stable capture code: advertisement 1, scan response 2, GAP 3.
    pub const fn code(self) -> u8 {
        match self {
            Self::Advertisement => 1,
            Self::ScanResponse => 2,
            Self::Gap => 3,
        }
    }

    /// Decodes capture `code`; returns `None` for zero or codes outside 1..=3.
    pub const fn from_code(code: u8) -> Option<Self> {
        match code {
            1 => Some(Self::Advertisement),
            2 => Some(Self::ScanResponse),
            3 => Some(Self::Gap),
            _ => None,
        }
    }

    /// Returns `self`'s stable ASCII provenance label for export and diagnostics.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Advertisement => "advertisement",
            Self::ScanResponse => "scan_response",
            Self::Gap => "gap_device_name",
        }
    }
}

/// Cached name observation; raw packet evidence is retained independently.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ObservedName {
    /// At most 64 original bytes; not necessarily UTF-8 or printable.
    pub data: Payload<64>,
    /// Packet/read origin of these bytes.
    pub source: NameSource,
    /// Source declared a complete name; independent of local payload truncation.
    pub complete: bool,
    /// Boot-relative packet time, or end of the explicit inspection for GAP.
    pub at_ms: u64,
}
impl ObservedName {
    /// Returns a borrowed view of `self` marked cached, retaining its observation time
    /// and truncation; no capture record index is available for cached observations.
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

/// Selected name and provenance borrowed from live or admitted saved evidence.
#[derive(Clone, Copy, Debug)]
pub struct NameRef<'a> {
    /// Original bytes; display code must sanitize rather than assume UTF-8.
    pub bytes: &'a [u8],
    /// Advertising, scan-response or GAP origin.
    pub source: NameSource,
    /// Name was declared complete by its source, independently of `truncated`.
    pub complete: bool,
    /// The retained byte prefix or containing packet lost source bytes.
    pub truncated: bool,
    /// Zero-based capture record containing a GAP read; absent for packet/cache names.
    pub record_index: Option<usize>,
    /// Boot-relative milliseconds when observed; absent for older evidence without a time.
    pub observed_at_ms: Option<u64>,
    /// Selected from retained observation cache rather than the current packet/read.
    pub cached: bool,
}

/// Finds the first nonempty AD field of `kind` in `bytes`, borrowing its payload.
/// `truncated`, `source` and optional boot-relative `at_ms` are copied as provenance;
/// kind 9 means complete. Returns `None` if absent or beyond malformed AD framing.
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

/// Returns an owned name from `data`, preferring complete AD type 9 over shortened 8.
/// `source` and boot-relative `at_ms` identify this packet. Empty/absent names return
/// `None`; packet truncation remains set even if the selected field itself fits.
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

/// Borrows the best name in `device`: complete current packet, complete untruncated
/// cache, shortened packet, then any cache. Scan response wins equal packet priority.
/// Returns `None` if no nonempty packet field or cached observation exists; no I/O occurs.
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

/// Returns a borrowed selected name from admitted `view`: complete packet, successful complete
/// GAP read, complete untruncated cache, shortened packet, then cache; otherwise `None`.
/// Scan response wins equal packet priority. GAP selection includes the zero-based
/// record index and inspection end time. Capture admission binds reads to GAP declarations.
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

/// Shared slot/export identity retaining exact peer type and a bounded display-name prefix.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SavedIdentity {
    /// Frozen address and address type from the capture header.
    pub peer: Peer,
    /// Up to 31 selected raw name bytes; empty when absent, truncation remains explicit.
    pub name: Payload<31>,
}
impl SavedIdentity {
    /// Returns identity from admitted `view`, using [`capture_name`] for the name.
    /// Copies at most 31 name bytes and propagates earlier truncation; performs no BLE I/O.
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
