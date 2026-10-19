use heapless::{String, Vec};

use super::*;
/// Metadata belongs to the packet it accompanied, not the newest peer report.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PacketObservation {
    pub at_ms: u64,
    pub rssi: Option<i8>,
    pub kind: ReportKind,
}
#[derive(Clone, Debug)]
pub struct Device {
    pub peer: Peer,
    pub advertisement: Payload<31>,
    pub response: Payload<31>,
    pub advertisement_meta: Option<PacketObservation>,
    pub response_meta: Option<PacketObservation>,
    pub last_seen_ms: u64,
    pub first_seen_ms: u64,
    pub reports: u32,
    pub rssi: Option<i8>,
}
impl Device {
    pub fn name(&self) -> String<31> {
        let mut short = None;
        // A complete name wins over a shortened name, regardless of packet order.
        for packet in [&self.response, &self.advertisement] {
            for field in AdFields::new(packet.as_slice()).flatten() {
                if field.kind == 9 && !field.data.is_empty() {
                    return printable(field.data);
                }
                if field.kind == 8 && short.is_none() {
                    short = Some(field.data);
                }
            }
        }
        short.map(printable).unwrap_or_default()
    }

    pub fn age_ms(
        &self,
        now_ms: u64,
    ) -> u64 {
        now_ms.saturating_sub(self.last_seen_ms)
    }
}
pub struct Catalog {
    devices: Vec<Device, 48>,
    pub overflow_reports: u32,
}
impl Default for Catalog {
    fn default() -> Self { Self::new() }
}
impl Catalog {
    pub const fn new() -> Self {
        Self {
            devices: Vec::new(),
            overflow_reports: 0,
        }
    }

    pub fn devices(&self) -> &[Device] { &self.devices }

    pub fn observe(
        &mut self,
        report: Advertisement,
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
        if report.kind == ReportKind::ScanResponse {
            device.response = report.data;
            device.response_meta = metadata;
        } else {
            device.advertisement = report.data;
            device.advertisement_meta = metadata;
        }
    }
}
