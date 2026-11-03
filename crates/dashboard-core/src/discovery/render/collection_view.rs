//! Bounded raw ECU evidence rows shared by live and saved views; no decoding or I/O.
use super::{EvidenceLines, hex};
use crate::ble::canyonero::CleanupOutcome;
use crate::ble::canyonero::collection::{Command, Error, EvidenceKind, ReadStatus, SendState};
use crate::discovery::capture::TerminalRef;
use crate::discovery::collection::{RowRef, Summary};

/// Appends small collection `s` metadata to `emit`; all times are boot-relative milliseconds.
/// Completion describes the finite exchange, never qualified vehicle measurements.
/// Explicit labels retain the prior Debug spelling without derived formatter builders;
/// Remote payloads remain decimal and every row still uses the shared 50-byte lane.
pub(super) fn summary(
    s: &Summary,
    emit: &mut EvidenceLines<'_>,
) {
    emit.text("ECU READ COLLECTION / raw evidence");
    emit.text("Engine 7E0 -> 7E8 / 500000 bit/s / raw CAN");
    emit.text("28 fixed UDS22 reads; no writes or unlock");
    emit.text("Reads are active CAN traffic, not passive");
    emit.formatted(format_args!("Started at: {} ms", s.started_at_ms));
    emit.formatted(format_args!("Updated at: {} ms", s.updated_at_ms));
    if let Some(at) = s.ended_at_ms {
        emit.formatted(format_args!("Ended at: {at} ms"));
    }
    let cleanup = match s.cleanup {
        CleanupOutcome::Pending => "Pending",
        CleanupOutcome::Clean => "Clean",
        CleanupOutcome::Failed => "Failed",
        CleanupOutcome::TimedOut => "TimedOut",
    };
    emit.formatted(format_args!("Closed: {} / cleanup: {cleanup}", s.completed));
    if let Some(fault) = s.fault {
        match fault {
            Error::Stale => emit.text("Failure: Stale"),
            Error::State => emit.text("Failure: State"),
            Error::Capacity => emit.text("Failure: Capacity"),
            Error::Clock => emit.text("Failure: Clock"),
            Error::Timeout => emit.text("Failure: Timeout"),
            Error::Cancelled => emit.text("Failure: Cancelled"),
            Error::Frame => emit.text("Failure: Frame"),
            Error::Remote(code) => emit.formatted(format_args!("Failure: Remote({code})")),
            Error::Diagnostic => emit.text("Failure: Diagnostic"),
            Error::PendingLimit => emit.text("Failure: PendingLimit"),
        }
    }
    if let Some(mv) = s.adapter_mv {
        emit.formatted(format_args!("Adapter supply: {mv} mV / not ECU voltage"));
    } else {
        emit.text("Adapter supply: unavailable");
    }
    if let Some(last) = s.last_submission {
        match last.command {
            Command::Read(id) => emit.formatted(format_args!("Last command: UDS22 {:04X}", id.did())),
            Command::FlowControl(id) => emit.formatted(format_args!("Last command: CTS for {:04X}", id.did())),
            Command::Voltage => emit.text("Last command: Voltage"),
            Command::Open => emit.text("Last command: Open"),
            Command::Arbitration => emit.text("Last command: Arbitration"),
            Command::Close => emit.text("Last command: Close"),
        }
        emit.text(match last.state {
            SendState::NotAttempted => "Local write: NotAttempted",
            SendState::InFlightUnknown => "Local write: InFlightUnknown",
            SendState::LocallyComplete => "Local write: LocallyComplete",
        });
        emit.text("Local completion is not CAN acknowledgment");
    }
    emit.text("Raw results only; not dashboard measurements");
}

/// Appends one compiled read `r` with exact raw prefix to `emit`, preserving unavailable outcomes.
/// A partial write is not displayed as unsent, and NRCs never become successful measurements.
pub(super) fn row(
    r: RowRef<'_>,
    emit: &mut EvidenceLines<'_>,
) {
    emit.formatted(format_args!(
        "ECU READ {:02}/28 / 22 {:04X}",
        r.id.index() + 1,
        r.id.did()
    ));
    emit.text(r.id.label());
    if let Some(at) = r.attempted_at_ms() {
        emit.formatted(format_args!("Attempt: {at} ms"));
    } else {
        emit.text("Not attempted");
    }
    if let Some(at) = r.submitted_at_ms() {
        emit.formatted(format_args!("Local completion: {at} ms"));
    } else if r.attempted_at_ms().is_some() {
        emit.text("Submission unknown / may have sent bytes");
    }
    match r.status {
        Some(ReadStatus::Positive) => emit.text("Positive reply / ECU schema unqualified"),
        Some(ReadStatus::Negative(nrc)) => emit.formatted(format_args!("Negative reply: NRC 0x{nrc:02X}")),
        Some(ReadStatus::SessionNotDefault) => emit.text("Default session not established; stopped"),
        None => emit.text("Complete reply unavailable"),
    }
    if let Some(at) = r.completed_at_ms() {
        emit.formatted(format_args!("Reply: {at} ms"));
    }
    emit.formatted(format_args!(
        "Pending: {} / raw {}/{} bytes",
        r.pending,
        r.raw.len(),
        r.declared_len
    ));
    if r.raw.len() != usize::from(r.declared_len) {
        emit.text("Incomplete retained prefix");
    }
    hex(r.raw, emit);
}

/// Appends terminal `r` prefix to `emit`; it is not a full transport or CAN transcript.
/// Provenance labels retain their prior Debug spelling, followed by exact raw hex.
pub(super) fn terminal(
    r: TerminalRef<'_>,
    emit: &mut EvidenceLines<'_>,
) {
    emit.text(match r.kind {
        EvidenceKind::Envelope => "Last raw prefix: Envelope",
        EvidenceKind::DiagnosticPayload => "Last raw prefix: DiagnosticPayload",
    });
    emit.formatted(format_args!("Retained {}/{} bytes", r.raw.len(), r.declared_len));
    emit.text("Latest prefix only / not a full wire transcript");
    hex(r.raw, emit);
}

#[cfg(test)]
/// Frozen Debug-equivalent output coverage for all collection-only labels and numeric payloads.
mod tests {
    use super::*;
    use crate::ble::canyonero::CleanupOutcome;
    use crate::ble::canyonero::collection::{Error, EvidenceKind, SendState, Submission};
    use crate::ble::canyonero::reads::ReadId;
    extern crate std;

    /// Returns rendered summary `value` rows with each ephemeral scratch row copied by the sink.
    fn rows(value: &Summary) -> std::vec::Vec<std::string::String> {
        let mut output = std::vec::Vec::new();
        summary(
            value,
            &mut EvidenceLines::new(Some(&mut |row| output.push(std::string::String::from(row)))),
        );
        output
    }

    /// Every error, including all decimal Remote values, retains the previous derived-Debug row bytes.
    #[test]
    fn error_rows_match_prior_debug_for_every_variant_and_remote_byte() {
        let mut value = crate::discovery::collection::Evidence::new(u64::MAX).summary;
        let errors = [
            Error::Stale,
            Error::State,
            Error::Capacity,
            Error::Clock,
            Error::Timeout,
            Error::Cancelled,
            Error::Frame,
            Error::Diagnostic,
            Error::PendingLimit,
        ];
        for fault in errors.into_iter().chain((0..=u8::MAX).map(Error::Remote)) {
            value.fault = Some(fault);
            let output = rows(&value);
            assert!(output.iter().any(|row| row == &std::format!("Failure: {fault:?}")));
            assert!(output.iter().all(|row| row.len() <= 50 && row.is_ascii()));
        }
        value.fault = None;
        assert!(!rows(&value).iter().any(|row| row.starts_with("Failure:")));
    }

    /// Every command/state/cleanup combination keeps its original label and fixed DID formatting.
    #[test]
    fn submission_and_cleanup_rows_match_prior_debug_labels() {
        let mut value = crate::discovery::collection::Evidence::new(0).summary;
        let commands = [Command::Voltage, Command::Open, Command::Arbitration, Command::Close];
        for command in commands
            .into_iter()
            .chain(ReadId::ALL.into_iter().map(Command::Read))
            .chain(ReadId::ALL.into_iter().map(Command::FlowControl))
        {
            for state in [
                SendState::NotAttempted,
                SendState::InFlightUnknown,
                SendState::LocallyComplete,
            ] {
                value.last_submission = Some(Submission {
                    command,
                    state,
                    attempted_at_ms: None,
                    completed_at_ms: None,
                });
                let output = rows(&value);
                let expected = match command {
                    Command::Read(id) => std::format!("Last command: UDS22 {:04X}", id.did()),
                    Command::FlowControl(id) => std::format!("Last command: CTS for {:04X}", id.did()),
                    other => std::format!("Last command: {other:?}"),
                };
                assert!(output.contains(&expected));
                assert!(output.contains(&std::format!("Local write: {state:?}")));
                assert!(output.iter().all(|row| row.len() <= 50));
            }
        }
        for completed in [false, true] {
            for cleanup in [
                CleanupOutcome::Pending,
                CleanupOutcome::Clean,
                CleanupOutcome::Failed,
                CleanupOutcome::TimedOut,
            ] {
                value.completed = completed;
                value.cleanup = cleanup;
                let mut count = EvidenceLines::new(None);
                summary(&value, &mut count);
                let output = rows(&value);
                assert_eq!(count.count, output.len());
                assert!(output.contains(&std::format!("Closed: {completed} / cleanup: {cleanup:?}")));
            }
        }
    }

    /// Both prefix provenances preserve Debug labels and byte evidence at empty/maximum bounds.
    #[test]
    fn terminal_kind_rows_match_prior_debug_and_raw_boundaries() {
        let raw = [0xAB; 245];
        for kind in [EvidenceKind::Envelope, EvidenceKind::DiagnosticPayload] {
            for length in [0, 1, 67, 245] {
                let value = TerminalRef {
                    kind,
                    declared_len: u16::MAX,
                    raw: &raw[..length],
                };
                let mut output = std::vec::Vec::new();
                terminal(
                    value,
                    &mut EvidenceLines::new(Some(&mut |row| output.push(std::string::String::from(row)))),
                );
                let mut count = EvidenceLines::new(None);
                terminal(value, &mut count);
                assert_eq!(count.count, output.len());
                assert_eq!(output[0], std::format!("Last raw prefix: {kind:?}"));
                assert_eq!(output[1], std::format!("Retained {length}/65535 bytes"));
                assert_eq!(output.len(), 3 + length.div_ceil(16));
                assert!(output.iter().all(|row| row.len() <= 50 && row.is_ascii()));
            }
        }
    }
}
