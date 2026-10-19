//! Legacy report decoding shared with native tests of the exact HCI dependency.
use bt_hci::param::{AddrKind, LeAdvEventKind, LeAdvReport, LeAdvReports};
use bt_hci::{FromHciBytes, FromHciBytesError};
use dashboard_core::discovery::{Advertisement, Event, Peer, ReportKind, ScanErrorKind, ScanFailure};

pub fn parse_reports(
    revision: Option<u32>,
    reports: LeAdvReports<'_>,
    at_ms: u64,
    mut on_report: impl FnMut(Event),
    mut on_error: impl FnMut(ScanFailure),
) {
    let Some(revision) = revision else { return };
    // Use the iterator's exact record decoder/count/stop semantics while retaining
    // the borrowed cursor for numeric diagnostics. No payload bytes are retained.
    let total_reports = reports.num_reports;
    let mut remaining = reports.bytes.as_ref();
    // HCI event parameters cap this at 253 bytes; saturate synthetic oversized input.
    let report_bytes = u16::try_from(remaining.len()).unwrap_or(u16::MAX);
    for index in 0..total_reports {
        let report = match LeAdvReport::from_hci_bytes(remaining) {
            Ok((report, rest)) => {
                remaining = rest;
                report
            }
            Err(error) => {
                on_error(ScanFailure {
                    kind: match error {
                        FromHciBytesError::InvalidSize => ScanErrorKind::InvalidSize,
                        FromHciBytesError::InvalidValue => ScanErrorKind::InvalidValue,
                    },
                    revision,
                    index,
                    total_reports,
                    report_bytes,
                    remaining_bytes: u16::try_from(remaining.len()).unwrap_or(u16::MAX),
                    declared_data_len: remaining.get(8).copied(),
                    at_ms,
                });
                break;
            }
        };
        if report.addr_kind != AddrKind::PUBLIC && report.addr_kind != AddrKind::RANDOM {
            continue;
        }
        let kind = match report.event_kind {
            LeAdvEventKind::AdvInd => ReportKind::Connectable,
            LeAdvEventKind::AdvDirectInd => ReportKind::Directed,
            LeAdvEventKind::ScanRsp => ReportKind::ScanResponse,
            _ => continue,
        };
        let mut address = [0; 6];
        address.copy_from_slice(report.addr.raw());
        address.reverse();
        let peer = Peer {
            address,
            random: report.addr_kind == AddrKind::RANDOM,
        };
        on_report(Event::Report {
            revision,
            report: Advertisement::new(peer, kind, report.rssi, report.data, at_ms),
        });
    }
}

#[cfg(not(test))]
pub struct Reports {
    revision: core::cell::Cell<Option<u32>>,
}

#[cfg(not(test))]
impl Reports {
    pub fn new() -> Self {
        Self {
            revision: core::cell::Cell::new(None),
        }
    }

    pub fn set_revision(
        &self,
        revision: Option<u32>,
    ) {
        self.revision.set(revision);
    }
}

#[cfg(not(test))]
impl trouble_host::prelude::EventHandler for Reports {
    fn on_adv_report_data(
        &self,
        reports: LeAdvReports<'_>,
    ) {
        parse_reports(
            self.revision.get(),
            reports,
            embassy_time::Instant::now().as_millis(),
            crate::bridge::emit_report,
            |failure| {
                // Only this bounded Copy value crosses the critical section.
                crate::bridge::SCAN_DIAGNOSTICS.lock(|cell| {
                    let mut diagnostics = cell.get();
                    diagnostics.record(failure);
                    cell.set(diagnostics);
                });
            },
        );
    }
}
