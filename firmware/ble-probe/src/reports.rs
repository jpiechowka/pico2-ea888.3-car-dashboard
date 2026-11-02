//! Legacy report framing admission shared with native tests of the locked HCI decoder.
use bt_hci::param::{AddrKind, LeAdvEventKind, LeAdvReport, LeAdvReports};
use bt_hci::{FromHciBytes, FromHciBytesError};
use dashboard_core::discovery::{Advertisement, Event, Peer, ReportKind, ScanErrorKind, ScanFailure};

/// Decodes counted legacy HCI `reports` for optional scan `revision` at
/// boot-relative `at_ms`. None ignores the batch. Calls `on_report` for admitted
/// public/random connectable/directed/scan-response records, converting address
/// byte order and copying bounded raw payload evidence. Calls `on_error` once at
/// the first malformed batch header, record or trailing bytes, preserving the
/// valid prefix and numeric cursor evidence. Legacy counts must be 1..=25 and
/// payload lengths 0..=31. RSSI remains raw evidence, including reserved encodings.
/// It never guesses past corrupt framing or retains borrowed bytes.
pub fn parse_reports(
    revision: Option<u32>,
    reports: LeAdvReports<'_>,
    at_ms: u64,
    mut on_report: impl FnMut(Event),
    mut on_error: impl FnMut(ScanFailure),
) {
    let Some(revision) = revision else { return };
    // Core Vol 4 Part E §§5.4.4, 7.7.65.2 define exact event parameters and legacy
    // bounds. The locked decoder is lazy and does not enforce these batch bounds.
    let total_reports = reports.num_reports;
    let mut remaining = reports.bytes.as_ref();
    // HCI event parameters cap this at 253 bytes; saturate synthetic oversized input.
    let report_bytes = u16::try_from(remaining.len()).unwrap_or(u16::MAX);
    let mut fail = |kind, index, remaining: &[u8], declared_data_len| {
        on_error(ScanFailure {
            kind,
            revision,
            index,
            total_reports,
            report_bytes,
            remaining_bytes: u16::try_from(remaining.len()).unwrap_or(u16::MAX),
            declared_data_len,
            at_ms,
        });
    };
    if !(1..=25).contains(&total_reports) {
        fail(ScanErrorKind::InvalidValue, 0, remaining, None);
        return;
    }
    for index in 0..total_reports {
        if remaining.get(8).is_some_and(|&length| length > 31) {
            fail(ScanErrorKind::InvalidValue, index, remaining, remaining.get(8).copied());
            return;
        }
        let report = match LeAdvReport::from_hci_bytes(remaining) {
            Ok((report, rest)) => {
                remaining = rest;
                report
            }
            Err(error) => {
                fail(
                    match error {
                        FromHciBytesError::InvalidSize => ScanErrorKind::InvalidSize,
                        FromHciBytesError::InvalidValue => ScanErrorKind::InvalidValue,
                    },
                    index,
                    remaining,
                    remaining.get(8).copied(),
                );
                return;
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
    if !remaining.is_empty() {
        fail(ScanErrorKind::InvalidSize, total_reports, remaining, None);
    }
}

#[cfg(not(test))]
/// Core0 HCI callback adapter whose current generation enables report admission.
pub struct Reports {
    /// Active scan revision; None suppresses reports outside an acknowledged scan.
    revision: core::cell::Cell<Option<u32>>,
}

#[cfg(not(test))]
impl Reports {
    /// Returns a report sink with scanning disabled until set_revision supplies a token.
    pub fn new() -> Self {
        Self {
            revision: core::cell::Cell::new(None),
        }
    }

    /// Sets scan `revision`, or None before stopping/cleaning up the old scan.
    /// The field is core0-only; owned events carry the token across cores.
    pub fn set_revision(
        &self,
        revision: Option<u32>,
    ) {
        self.revision.set(revision);
    }
}

#[cfg(not(test))]
impl trouble_host::prelude::EventHandler for Reports {
    /// Parses borrowed `reports` under the active revision, queues bounded owned
    /// events, and records malformed-batch counters/faults without blocking on UI.
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
                crate::bridge::STATUS.record(crate::status::Fault::Scan);
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
