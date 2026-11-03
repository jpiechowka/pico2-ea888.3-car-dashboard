//! Stable-order bounded peer catalogue with per-packet provenance and monotonic admission.
use heapless::{String, Vec};

use super::*;
/// Metadata belongs to the packet it accompanied, not the newest peer report.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PacketObservation {
    /// Boot-relative receipt time in milliseconds.
    pub at_ms: u64,
    /// RSSI in dBm in -127..=20, or unavailable for controller sentinel/out-of-range values.
    pub rssi: Option<i8>,
    /// Report category belonging to this particular packet.
    pub kind: ReportKind,
}
/// Latest admitted packet pair and retained name for one exact address/type identity.
#[derive(Clone, Debug)]
pub struct Device {
    /// Address and address type; both participate in catalogue equality.
    pub peer: Peer,
    /// Latest connectable/directed packet prefix; never overwritten by a scan response.
    pub advertisement: Payload<31>,
    /// Latest scan-response prefix, independent of the advertising packet.
    pub response: Payload<31>,
    /// Metadata for `advertisement`; absent before that packet exists.
    pub advertisement_meta: Option<PacketObservation>,
    /// Metadata for `response`; absent before that packet exists.
    pub response_meta: Option<PacketObservation>,
    /// Latest accepted report time in boot-relative milliseconds.
    pub last_seen_ms: u64,
    /// First admitted report time in boot-relative milliseconds; never advanced.
    pub first_seen_ms: u64,
    /// Saturating count of accepted reports, including equal-time reports.
    pub reports: u32,
    /// Latest accepted report RSSI in dBm, or unavailable outside -127..=20.
    pub rssi: Option<i8>,
    /// Last eligible name observation; does not replace raw packet evidence.
    pub cached_name: Option<names::ObservedName>,
}
impl Device {
    /// Returns `self`'s selected name as at most 31 printable ASCII bytes, replacing
    /// nonprintable bytes with `?`; returns an empty string when no name is available.
    pub fn name(&self) -> String<31> { names::device_name(self).map(|n| printable(n.bytes)).unwrap_or_default() }

    /// Admits `source`/`complete`/`truncated` at boot-relative `at_ms` only when not older
    /// than the cache and not downgrading a complete untruncated name. Rejection returns
    /// None without mutation. Success updates metadata and returns resident storage;
    /// the caller must synchronously replace its byte array and length before publishing.
    fn name_destination(
        &mut self,
        source: names::NameSource,
        complete: bool,
        truncated: bool,
        at_ms: u64,
    ) -> Option<&mut names::ObservedName> {
        if self
            .cached_name
            .as_ref()
            .is_some_and(|old| at_ms < old.at_ms || (old.complete && !old.data.truncated && !(complete && !truncated)))
        {
            return None;
        }
        let name = self.cached_name.get_or_insert_with(|| {
            names::ObservedName {
                data: Payload::empty(),
                source,
                complete,
                at_ms,
            }
        });
        name.source = source;
        name.complete = complete;
        name.at_ms = at_ms;
        name.data.truncated = truncated;
        Some(name)
    }

    /// Returns milliseconds since `self`'s last report at boot-relative `now_ms`;
    /// time reversal saturates to zero and does not alter the stored timestamp.
    pub fn age_ms(
        &self,
        now_ms: u64,
    ) -> u64 {
        now_ms.saturating_sub(self.last_seen_ms)
    }
}
/// Up to 48 peers in first-admission order; overflow never evicts prior evidence.
pub struct Catalog {
    /// Stable-order entries indexed by exact address and address type.
    devices: Vec<Device, 48>,
    /// Saturating count of reports rejected because a new peer could not fit.
    pub overflow_reports: u32,
}
impl Default for Catalog {
    /// Returns an empty catalogue with zero rejected-report count.
    fn default() -> Self { Self::new() }
}
impl Catalog {
    /// Returns an empty catalogue with capacity for 48 peers and no overflow evidence.
    pub const fn new() -> Self {
        Self {
            devices: Vec::new(),
            overflow_reports: 0,
        }
    }

    /// Returns `self`'s borrowed admitted peers in first-seen order, without sorting by RSSI.
    pub fn devices(&self) -> &[Device] { &self.devices }

    /// Caches complete GAP `data` for exact `peer` at boot-relative `at_ms` milliseconds.
    /// Empty/truncated data, unknown peers and observations older than the peer's last
    /// packet are ignored. Returns unit; raw packets and report timestamps stay intact.
    /// Accepted payload storage is copied directly into the resident cache; `data` is
    /// borrowed only during the call and its bytes/length/truncation remain unchanged.
    pub(super) fn remember_gap_name(
        &mut self,
        peer: Peer,
        data: &Payload<64>,
        at_ms: u64,
    ) {
        if data.as_slice().is_empty() || data.truncated {
            return;
        }
        if let Some(device) = self
            .devices
            .iter_mut()
            .find(|d| d.peer == peer && at_ms >= d.last_seen_ms)
            && let Some(name) = device.name_destination(names::NameSource::Gap, true, false, at_ms)
        {
            name.data.bytes.copy_from_slice(&data.bytes);
            name.data.len = data.len;
        }
    }

    /// Admits owned `report` into `self`, returning unit. Unsupported reports, orphan
    /// scan responses and reports older than the peer's last timestamp are ignored.
    /// New connectable/directed peers append until capacity; further new-peer reports
    /// saturating-increment overflow. Equal-time reports may replace the corresponding
    /// packet. RSSI outside -127..=20 becomes unavailable; counters saturate.
    pub fn observe(
        &mut self,
        report: Advertisement,
    ) {
        self.observe_ref(&report);
    }

    /// Admits borrowed `report` with [`Self::observe`]'s identity, time and capacity checks.
    /// Retained packet/name bytes are copied into the catalogue; no borrow survives return.
    #[inline(always)]
    pub fn observe_ref(
        &mut self,
        report: &Advertisement,
    ) {
        if report.kind == ReportKind::Other {
            return;
        }
        let index = self.devices.iter().position(|d| d.peer == report.peer);
        let index = match index {
            Some(index) => index,
            None if report.kind == ReportKind::ScanResponse => return,
            None => {
                let index = self.devices.len();
                if self
                    .devices
                    .push(Device {
                        peer: report.peer,
                        advertisement: Payload::empty(),
                        response: Payload::empty(),
                        advertisement_meta: None,
                        response_meta: None,
                        last_seen_ms: report.at_ms,
                        first_seen_ms: report.at_ms,
                        reports: 0,
                        rssi: None,
                        cached_name: None,
                    })
                    .is_err()
                {
                    self.overflow_reports = self.overflow_reports.saturating_add(1);
                    return;
                }
                index
            }
        };
        let device = &mut self.devices[index];
        // Late queue entries never regress timestamps or overwrite newer payloads.
        if report.at_ms < device.last_seen_ms {
            return;
        }
        device.last_seen_ms = report.at_ms;
        device.reports = device.reports.saturating_add(1);
        device.rssi = (-127..=20).contains(&report.rssi).then_some(report.rssi);
        let metadata = Some(PacketObservation {
            at_ms: report.at_ms,
            rssi: device.rssi,
            kind: report.kind,
        });
        let source = if report.kind == ReportKind::ScanResponse {
            names::NameSource::ScanResponse
        } else {
            names::NameSource::Advertisement
        };
        if let Some(name) = names::packet_observation(&report.data, source, report.at_ms)
            && let Some(cached) = device.name_destination(name.source, name.complete, name.truncated, report.at_ms)
        {
            // AD names contain at most 29 bytes. Preserve Payload::new's zeroed tail
            // while copying the borrowed selected field directly into retained storage.
            cached.data.bytes.fill(0);
            cached.data.bytes[..name.bytes.len()].copy_from_slice(name.bytes);
            cached.data.len = name.bytes.len() as u8;
        }
        if report.kind == ReportKind::ScanResponse {
            device.response = report.data;
            device.response_meta = metadata;
        } else {
            device.advertisement = report.data;
            device.advertisement_meta = metadata;
        }
    }
}
