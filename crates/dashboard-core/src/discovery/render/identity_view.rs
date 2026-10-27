//! INFO trial rows shared by live and frozen capture rendering; never performs I/O.
use super::{EvidenceLines, hex, printable, reason_line};
use crate::ble::canyonero::{CleanupOutcome, DecodeError, Reason, Stage, TransportDomain, parse_frame};
use crate::discovery::ReadResult;
use crate::discovery::identity::Evidence;

/// Appends optional bounded `evidence` to `emit`. Absent fields stay unavailable;
/// strings are sanitized for display and exact raw bytes follow separately.
/// Returns unit. A decoded identity does not certify adapter access or ECU data.
// Live/saved rendering and pagination share this traversal; keep one bounded
// implementation instead of expanding it into each utility task/count path.
#[inline(never)]
pub(super) fn details(
    evidence: Option<&Evidence>,
    emit: &mut EvidenceLines<'_>,
) {
    emit.text("NGM INFO / explicit selected-peer trial");
    emit.text("Candidate SPSM 129 / source configured");
    emit.text("Discovered SPSM: unavailable");
    emit.text("One INFO request; no ECU requests or polling");
    let Some(e) = evidence else {
        emit.text("Identity attempt evidence unavailable");
        return;
    };
    emit.formatted(format_args!(
        "Stage {} / cleanup {}",
        stage_label(e.stage),
        cleanup_label(e.cleanup)
    ));
    emit.formatted(format_args!("Started at: {} ms", e.started_at_ms));
    emit.formatted(format_args!("Last stage at: {} ms", e.updated_at_ms));
    if let Some(at) = e.ended_at_ms {
        emit.formatted(format_args!("Ended at: {at} ms"));
    }
    if let Some(fault) = e.fault.as_ref() {
        emit.formatted(format_args!("First failure: {}", stage_label(fault.stage)));
        emit.formatted(format_args!("Failure observed: {} ms", fault.at_ms));
        emit.formatted(format_args!("Failure: {}", reason_label(fault.reason)));
        match fault.reason {
            Reason::Protocol(error) => {
                emit.formatted(format_args!("Protocol error: {}", decode_label(error)));
                if let DecodeError::UnexpectedType(kind) | DecodeError::Remote(kind) = error {
                    emit.formatted(format_args!("Reply type: 0x{kind:02X}"));
                }
            }
            Reason::Transport(f) => {
                emit.formatted(format_args!("Transport domain: {}", domain_label(f.domain)));
                if let Some(code) = f.code {
                    emit.formatted(format_args!("Transport code: 0x{code:04X}"));
                } else {
                    emit.text("Transport code: unavailable");
                }
                emit.text(if f.partial_send {
                    "Partial send possible: true"
                } else {
                    "Partial send possible: false"
                });
            }
            _ => {}
        }
    }
    for (index, uuid) in crate::discovery::identity::VENDOR_UUIDS.iter().enumerate() {
        emit.formatted(format_args!("VENDOR READ {uuid:04X} / raw, uninterpreted"));
        if let Some(v) = e.vendor_values[index].as_ref() {
            emit.formatted(format_args!(
                "Handle {:04X} / service {:04X}",
                v.handle, v.service_start
            ));
            emit.formatted(format_args!("{} / {} ms", read_label(v.result), v.elapsed_ms));
            if let Some(failure) = v.failure {
                reason_line("Read error", failure, emit);
            }
            if v.value.truncated {
                emit.text("Read prefix truncated");
            }
            hex(v.value.as_slice(), emit);
        } else {
            emit.text("Not observed");
        }
    }
    if let Some(c) = e.channel.as_ref() {
        emit.formatted(format_args!("Offered RX MTU/MPS: {}/{}", c.offered_mtu, c.offered_mps));
        scalar("Offered credits", Some(c.offered_credits), emit);
        scalar("Local RX MTU bytes", c.local_mtu, emit);
        scalar("Local RX MPS bytes", c.local_mps, emit);
        scalar("Peer RX MTU bytes", c.peer_mtu, emit);
        scalar("Peer RX MPS bytes", c.peer_mps, emit);
        scalar("Local CID", c.local_cid, emit);
        scalar("Peer CID", c.peer_cid, emit);
        scalar("Local credits", c.local_credits, emit);
        scalar("Peer credits", c.peer_credits, emit);
    }
    if let Some(at) = e.request_submitted_at_ms {
        emit.formatted(format_args!("INFO submitted: {at} ms"));
        emit.text("TX: 1F 11 00 00 / not peer acknowledgment");
    } else {
        emit.text("INFO submission: unconfirmed");
    }
    emit.formatted(format_args!("RX {} bytes / {} SDUs", e.observed_bytes, e.sdu_count));
    if e.sdu_count == 0 {
        emit.text("Largest SDU: unavailable");
    } else {
        emit.formatted(format_args!("Largest SDU: {} bytes", e.max_sdu_bytes));
    }
    emit.text(if e.raw_incomplete() {
        "RX HEX / retained prefix only"
    } else {
        "RX HEX / exact observed bytes"
    });
    hex(e.raw(), emit);
    // A retained prefix can itself look complete despite omitted trailing bytes.
    // Derive fields only from the entire observed stream, matching PC exports.
    if !e.raw_incomplete()
        && let Ok(id) = parse_frame(e.raw())
    {
        field("Vendor", id.vendor, emit);
        field("Model", id.model, emit);
        field("Hardware", id.hardware, emit);
        field("Serial", id.serial, emit);
        field("Firmware", id.firmware, emit);
    }
    emit.text(if e.successful() {
        "INFO + cleanup complete / identity only"
    } else {
        "Partial or failed identity trial"
    });
    emit.text("Device-supplied text; not authentication");
}

/// Appends `label` and borrowed `value` to `emit`, sanitizing each at-most50-byte
/// chunk exactly like raw text rows. None emits the explicit empty-field label.
/// Returns unit without retaining the borrowed field or constructing a field array.
fn field(
    label: &str,
    value: Option<&str>,
    emit: &mut EvidenceLines<'_>,
) {
    emit.text(label);
    match value {
        Some(value) => {
            for chunk in value.as_bytes().chunks(50) {
                emit.text(&printable::<50>(chunk));
            }
        }
        None => emit.text("Unavailable (empty field)"),
    }
}

/// Returns an exhaustive technical label for lifecycle `stage`, at most19 ASCII bytes.
fn stage_label(stage: Stage) -> &'static str {
    match stage {
        Stage::Idle => "Idle",
        Stage::Connecting => "Connecting",
        Stage::Discovering => "Discovering",
        Stage::TransportParameters => "TransportParameters",
        Stage::OpeningCoc => "OpeningCoc",
        Stage::SendingInfo => "SendingInfo",
        Stage::AwaitingInfo => "AwaitingInfo",
        Stage::Closing => "Closing",
        Stage::Finished => "Finished",
    }
}

/// Returns the exact technical `cleanup` state without a nested formatter.
fn cleanup_label(cleanup: CleanupOutcome) -> &'static str {
    match cleanup {
        CleanupOutcome::Pending => "Pending",
        CleanupOutcome::Clean => "Clean",
        CleanupOutcome::Failed => "Failed",
        CleanupOutcome::TimedOut => "TimedOut",
    }
}

/// Returns the technical `reason` category; payloads receive separate exact rows.
fn reason_label(reason: Reason) -> &'static str {
    match reason {
        Reason::Protocol(_) => "Protocol",
        Reason::TimedOut => "TimedOut",
        Reason::TimeReversed => "TimeReversed",
        Reason::ClockOverflow => "ClockOverflow",
        Reason::Cancelled => "Cancelled",
        Reason::Parked => "Parked",
        Reason::Transport(_) => "Transport",
        Reason::UnexpectedEvent => "UnexpectedEvent",
        Reason::UnexpectedEmptySdu => "UnexpectedEmptySdu",
        Reason::Eof => "Eof",
        Reason::CleanupTimedOut => "CleanupTimedOut",
        Reason::CleanupFailed => "CleanupFailed",
    }
}

/// Returns the exact protocol `error` category; message-type bytes are rendered separately.
fn decode_label(error: DecodeError) -> &'static str {
    match error {
        DecodeError::Marker => "Marker",
        DecodeError::UnsupportedSize => "UnsupportedSize",
        DecodeError::UnexpectedType(_) => "UnexpectedType",
        DecodeError::Remote(_) => "Remote",
        DecodeError::TrailingData => "TrailingData",
        DecodeError::Truncated => "Truncated",
        DecodeError::FieldCount => "FieldCount",
        DecodeError::FieldTooLong => "FieldTooLong",
        DecodeError::InvalidUtf8 => "InvalidUtf8",
        DecodeError::ControlCharacter => "ControlCharacter",
    }
}

/// Returns the technical transport `domain` name; absent codes are never inferred as zero.
fn domain_label(domain: TransportDomain) -> &'static str {
    match domain {
        TransportDomain::Hci => "HCI",
        TransportDomain::Att => "ATT",
        TransportDomain::L2cap => "L2CAP",
        TransportDomain::Local => "Local",
    }
}

/// Returns the bounded read `result` label; raw failure codes remain in their own row.
fn read_label(result: ReadResult) -> &'static str {
    match result {
        ReadResult::Ok => "Ok",
        ReadResult::Protected => "Protected",
        ReadResult::Unavailable => "Unavailable",
        ReadResult::Error => "Error",
    }
}

/// Appends `label` and decimal `value` to `emit`, keeping None unavailable and
/// Some(0) a valid observed zero. Returns unit; labels are fixed local text.
fn scalar(
    label: &str,
    value: Option<u16>,
    emit: &mut EvidenceLines<'_>,
) {
    match value {
        Some(value) => emit.formatted(format_args!("{label}: {value}")),
        None => emit.formatted(format_args!("{label}: unavailable")),
    }
}

#[cfg(test)]
/// Rendered identity values retain full bounded counters and do not promote omitted bytes.
mod tests {
    extern crate std;
    use super::*;
    use crate::ble::canyonero::{CleanupOutcome, DecodeError, Fault, Stage, TransportDomain, TransportFault};

    /// Fault payloads remain explicit and bounded, including exact remote bytes and unavailable transport codes.
    #[test]
    fn fault_payloads_use_separate_exact_technical_rows() {
        let mut e = Evidence::new(0);
        e.fault = Some(Fault {
            stage: Stage::AwaitingInfo,
            at_ms: u64::MAX,
            reason: Reason::Protocol(DecodeError::Remote(0xEF)),
        });
        let rendered = rows(&e);
        assert!(rendered.iter().any(|row| row == "Failure: Protocol"));
        assert!(rendered.iter().any(|row| row == "Protocol error: Remote"));
        assert!(rendered.iter().any(|row| row == "Reply type: 0xEF"));
        assert!(
            rendered
                .iter()
                .any(|row| row == "Failure observed: 18446744073709551615 ms")
        );
        e.fault.as_mut().unwrap().reason = Reason::Transport(TransportFault {
            domain: TransportDomain::L2cap,
            code: None,
            partial_send: true,
        });
        let rendered = rows(&e);
        assert!(rendered.iter().any(|row| row == "Failure: Transport"));
        assert!(rendered.iter().any(|row| row == "Transport domain: L2CAP"));
        assert!(rendered.iter().any(|row| row == "Transport code: unavailable"));
        assert!(rendered.iter().any(|row| row == "Partial send possible: true"));
        assert!(rendered.iter().all(|row| row.is_ascii() && row.len() <= 50));
    }

    /// Every lifecycle combination retains its complete labels within one50-byte row.
    #[test]
    fn lifecycle_labels_are_exhaustive_and_unclipped() {
        for (stage, name) in [
            (Stage::Idle, "Idle"),
            (Stage::Connecting, "Connecting"),
            (Stage::Discovering, "Discovering"),
            (Stage::TransportParameters, "TransportParameters"),
            (Stage::OpeningCoc, "OpeningCoc"),
            (Stage::SendingInfo, "SendingInfo"),
            (Stage::AwaitingInfo, "AwaitingInfo"),
            (Stage::Closing, "Closing"),
            (Stage::Finished, "Finished"),
        ] {
            for (cleanup, label) in [
                (CleanupOutcome::Pending, "Pending"),
                (CleanupOutcome::Clean, "Clean"),
                (CleanupOutcome::Failed, "Failed"),
                (CleanupOutcome::TimedOut, "TimedOut"),
            ] {
                let mut e = Evidence::new(u64::MAX);
                e.stage = stage;
                e.cleanup = cleanup;
                e.fault = Some(Fault {
                    stage,
                    reason: Reason::TimedOut,
                    at_ms: u64::MAX,
                });
                let rendered = rows(&e);
                assert!(
                    rendered
                        .iter()
                        .any(|r| r == &std::format!("Stage {name} / cleanup {label}"))
                );
                assert!(rendered.iter().any(|r| r == &std::format!("First failure: {name}")));
                assert!(rendered.iter().all(|r| r.is_ascii() && r.len() <= 50));
            }
        }
    }

    /// All simple reasons and protocol error classes remain distinguishable without Debug formatting.
    #[test]
    fn every_failure_category_has_an_exact_visible_label() {
        let mut e = Evidence::new(0);
        for (reason, label) in [
            (Reason::TimedOut, "TimedOut"),
            (Reason::TimeReversed, "TimeReversed"),
            (Reason::ClockOverflow, "ClockOverflow"),
            (Reason::Cancelled, "Cancelled"),
            (Reason::Parked, "Parked"),
            (Reason::UnexpectedEvent, "UnexpectedEvent"),
            (Reason::UnexpectedEmptySdu, "UnexpectedEmptySdu"),
            (Reason::Eof, "Eof"),
            (Reason::CleanupTimedOut, "CleanupTimedOut"),
            (Reason::CleanupFailed, "CleanupFailed"),
        ] {
            e.fault = Some(Fault {
                stage: Stage::Closing,
                reason,
                at_ms: 0,
            });
            let rendered = rows(&e);
            assert!(rendered.iter().any(|r| r == &std::format!("Failure: {label}")));
            assert!(rendered.iter().all(|r| r.is_ascii() && r.len() <= 50));
        }
        for (error, label) in [
            (DecodeError::Marker, "Marker"),
            (DecodeError::UnsupportedSize, "UnsupportedSize"),
            (DecodeError::UnexpectedType(0x80), "UnexpectedType"),
            (DecodeError::Remote(0xEF), "Remote"),
            (DecodeError::TrailingData, "TrailingData"),
            (DecodeError::Truncated, "Truncated"),
            (DecodeError::FieldCount, "FieldCount"),
            (DecodeError::FieldTooLong, "FieldTooLong"),
            (DecodeError::InvalidUtf8, "InvalidUtf8"),
            (DecodeError::ControlCharacter, "ControlCharacter"),
        ] {
            e.fault = Some(Fault {
                stage: Stage::AwaitingInfo,
                reason: Reason::Protocol(error),
                at_ms: 0,
            });
            let rendered = rows(&e);
            assert!(rendered.iter().any(|r| r == "Failure: Protocol"));
            assert!(rendered.iter().any(|r| r == &std::format!("Protocol error: {label}")));
            assert!(rendered.iter().all(|r| r.is_ascii() && r.len() <= 50));
        }
    }

    /// Every reply byte and transport-domain/code boundary remains exact; unavailable never becomes zero.
    #[test]
    fn exact_error_codes_domains_and_partial_send_survive_rendering() {
        let mut e = Evidence::new(0);
        for byte in 0..=u8::MAX {
            for error in [DecodeError::Remote(byte), DecodeError::UnexpectedType(byte)] {
                e.fault = Some(Fault {
                    stage: Stage::AwaitingInfo,
                    reason: Reason::Protocol(error),
                    at_ms: 0,
                });
                let rendered = rows(&e);
                assert!(rendered.iter().any(|r| r == &std::format!("Reply type: 0x{byte:02X}")));
            }
        }
        for (domain, name) in [
            (TransportDomain::Hci, "HCI"),
            (TransportDomain::Att, "ATT"),
            (TransportDomain::L2cap, "L2CAP"),
            (TransportDomain::Local, "Local"),
        ] {
            for code in [None, Some(0), Some(1), Some(0xFF), Some(0x100), Some(u16::MAX)] {
                for partial_send in [false, true] {
                    e.fault = Some(Fault {
                        stage: Stage::SendingInfo,
                        at_ms: 0,
                        reason: Reason::Transport(TransportFault {
                            domain,
                            code,
                            partial_send,
                        }),
                    });
                    let rendered = rows(&e);
                    assert!(rendered.iter().any(|r| r == "Failure: Transport"));
                    assert!(rendered.iter().any(|r| r == &std::format!("Transport domain: {name}")));
                    let expected = code.map_or_else(
                        || std::string::String::from("Transport code: unavailable"),
                        |code| std::format!("Transport code: 0x{code:04X}"),
                    );
                    assert!(rendered.iter().any(|r| r == &expected));
                    assert!(
                        rendered
                            .iter()
                            .any(|r| r == &std::format!("Partial send possible: {partial_send}"))
                    );
                    assert!(rendered.iter().all(|r| r.is_ascii() && r.len() <= 50));
                }
            }
        }
    }

    /// Borrowed vendor records retain full read status, duration, handles and all64 raw bytes.
    #[test]
    fn vendor_status_and_maximum_scalars_remain_complete() {
        use crate::discovery::{FailureReason, InfoValue, Payload};
        let mut e = Evidence::new(u64::MAX);
        for (result, label) in [
            (ReadResult::Ok, "Ok"),
            (ReadResult::Protected, "Protected"),
            (ReadResult::Unavailable, "Unavailable"),
            (ReadResult::Error, "Error"),
        ] {
            e.vendor_values[0] = Some(InfoValue {
                service_start: u16::MAX,
                handle: u16::MAX,
                uuid: 0x81,
                value: Payload::new(&[0xFF; 64]),
                result,
                elapsed_ms: u32::MAX,
                failure: if result == ReadResult::Ok {
                    None
                } else {
                    Some(FailureReason::Att(0xFF))
                },
            });
            let rendered = rows(&e);
            assert!(rendered.iter().any(|r| r == "Handle FFFF / service FFFF"));
            assert!(rendered.iter().any(|r| r == &std::format!("{label} / 4294967295 ms")));
            assert_eq!(
                rendered
                    .iter()
                    .filter(|r| r.as_str() == "FF FF FF FF FF FF FF FF FF FF FF FF FF FF FF FF")
                    .count(),
                4
            );
            assert!(rendered.iter().all(|r| r.is_ascii() && r.len() <= 50));
        }
    }

    /// Returns all displayed rows for synthetic `evidence`; never performs I/O.
    fn rows(evidence: &Evidence) -> std::vec::Vec<std::string::String> {
        let mut rows = std::vec::Vec::new();
        details(
            Some(evidence),
            &mut EvidenceLines::new(Some(&mut |row| rows.push(row.into()))),
        );
        rows
    }

    /// Full-width counters must remain inspectable rather than disappear at the 50-byte row bound.
    #[test]
    fn maximum_receive_counters_are_complete() {
        let mut e = Evidence::new(u64::MAX);
        e.observed_bytes = u64::MAX;
        e.sdu_count = u32::MAX;
        let rows = rows(&e);
        assert!(
            rows.iter()
                .any(|r| r == "RX 18446744073709551615 bytes / 4294967295 SDUs")
        );
        assert!(rows.iter().all(|r| r.is_ascii() && r.len() <= 50));
    }

    /// No delivered SDU has no maximum; one delivered empty SDU has an observed zero.
    #[test]
    fn absent_sdu_is_distinct_from_observed_empty_sdu() {
        let mut e = Evidence::new(0);
        assert!(rows(&e).iter().any(|r| r == "Largest SDU: unavailable"));
        e.sdu_count = 1;
        assert!(rows(&e).iter().any(|r| r == "Largest SDU: 0 bytes"));
    }

    /// A complete-looking retained prefix with omitted trailing bytes is not a complete reply.
    #[test]
    fn omitted_bytes_suppress_derived_identity() {
        let raw = b"\x1f\x91\x00\x09v\nm\nh\ns\nf";
        let mut e = Evidence::new(0);
        e.raw[..raw.len()].copy_from_slice(raw);
        e.raw_len = raw.len() as u8;
        e.observed_bytes = raw.len() as u64;
        assert!(rows(&e).iter().any(|r| r == "Vendor"));
        e.observed_bytes += 1;
        let rows = rows(&e);
        assert!(!rows.iter().any(|r| r == "Vendor"));
        assert!(rows.iter().any(|r| r == "RX HEX / retained prefix only"));
    }
}
