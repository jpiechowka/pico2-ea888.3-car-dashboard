//! Capture-v6 raw collection records; stable tags and borrowed bytes avoid a full evidence copy.
use super::{CaptureError, Reader, Writer};
use crate::ble::canyonero::CleanupOutcome;
use crate::ble::canyonero::collection::{Command, Error, EvidenceKind, ReadStatus, SendState, Submission};
use crate::ble::canyonero::reads::ReadId;
use crate::discovery::collection::{Evidence, RowRef, Summary};

/// Borrowed latest terminal prefix, not a complete command transcript or successful response.
#[derive(Clone, Copy, Debug)]
pub struct TerminalRef<'a> {
    /// Envelope or reassembled diagnostic provenance.
    pub kind: EvidenceKind,
    /// Announced prefix length, at most 245, including bytes not yet published.
    pub declared_len: u16,
    /// Exact retained prefix, never longer than the declared length.
    pub raw: &'a [u8],
}

/// Returns the closed manifest entry for canonical index `code`; arbitrary DIDs cannot be constructed.
fn read_id(code: u8) -> Result<ReadId, CaptureError> {
    ReadId::ALL
        .get(usize::from(code))
        .copied()
        .ok_or(CaptureError::InvalidData)
}
/// Returns stable fault tag and auxiliary byte for `error`; no representation casts.
fn fault_code(error: Error) -> (u8, u8) {
    match error {
        Error::Stale => (0, 0),
        Error::State => (1, 0),
        Error::Capacity => (2, 0),
        Error::Clock => (3, 0),
        Error::Timeout => (4, 0),
        Error::Cancelled => (5, 0),
        Error::Frame => (6, 0),
        Error::Remote(v) => (7, v),
        Error::Diagnostic => (8, 0),
        Error::PendingLimit => (9, 0),
    }
}
/// Decodes stable fault `tag`/`value`, rejecting noncanonical auxiliary fields.
/// Returns the recorded protocol error, or InvalidData for an unknown/noncanonical encoding.
fn fault(
    tag: u8,
    value: u8,
) -> Result<Error, CaptureError> {
    if tag != 7 && value != 0 {
        return Err(CaptureError::InvalidData);
    }
    Ok(match tag {
        0 => Error::Stale,
        1 => Error::State,
        2 => Error::Capacity,
        3 => Error::Clock,
        4 => Error::Timeout,
        5 => Error::Cancelled,
        6 => Error::Frame,
        7 => Error::Remote(value),
        8 => Error::Diagnostic,
        9 => Error::PendingLimit,
        _ => return Err(CaptureError::InvalidData),
    })
}
/// Writes `command` as stable kind plus optional compiled read identity; errors leave a prefix.
/// `w` receives or measures the encoding. Returns unit on completion or its capacity error.
fn write_command(
    w: &mut Writer<'_>,
    command: Command,
) -> Result<(), CaptureError> {
    let (tag, id) = match command {
        Command::Voltage => (0, None),
        Command::Open => (1, None),
        Command::Arbitration => (2, None),
        Command::Read(id) => (3, Some(id)),
        Command::FlowControl(id) => (4, Some(id)),
        Command::Close => (5, None),
    };
    w.u8(tag)?;
    if let Some(id) = id {
        w.u8(id.index() as u8)?;
    }
    Ok(())
}
/// Reads one closed command from `r`, rejecting unknown tags/identities.
/// Returns the decoded command or a truncation/invalid-data error after consuming its prefix.
fn read_command(r: &mut Reader<'_>) -> Result<Command, CaptureError> {
    Ok(match r.u8()? {
        0 => Command::Voltage,
        1 => Command::Open,
        2 => Command::Arbitration,
        3 => Command::Read(read_id(r.u8()?)?),
        4 => Command::FlowControl(read_id(r.u8()?)?),
        5 => Command::Close,
        _ => return Err(CaptureError::InvalidData),
    })
}
/// Writes small `s` metadata and fixed manifest count; errors leave untrusted partial output.
/// `w` receives or measures the summary. Returns unit on completion or its capacity error.
// Real encoding and worst-case admission share this body under utility LTO.
#[inline(never)]
pub(super) fn write_summary(
    w: &mut Writer<'_>,
    s: &Summary,
) -> Result<(), CaptureError> {
    w.u8(1)?;
    w.u16(0x7E0)?;
    w.u16(0x7E8)?;
    w.u8(28)?;
    w.u64(s.started_at_ms)?;
    w.u64(s.updated_at_ms)?;
    w.opt_u64(s.ended_at_ms)?;
    w.bool(s.adapter_mv.is_some())?;
    if let Some(v) = s.adapter_mv {
        w.u16(v)?;
    }
    w.opt_u8(s.handle)?;
    w.bool(s.configured)?;
    w.bool(s.completed)?;
    w.bool(s.fault.is_some())?;
    if let Some(error) = s.fault {
        let (tag, value) = fault_code(error);
        w.u8(tag)?;
        w.u8(value)?;
    }
    w.u8(match s.cleanup {
        CleanupOutcome::Pending => 0,
        CleanupOutcome::Clean => 1,
        CleanupOutcome::Failed => 2,
        CleanupOutcome::TimedOut => 3,
    })?;
    w.bool(s.last_submission.is_some())?;
    if let Some(last) = s.last_submission {
        write_command(w, last.command)?;
        w.u8(match last.state {
            SendState::NotAttempted => 0,
            SendState::InFlightUnknown => 1,
            SendState::LocallyComplete => 2,
        })?;
        w.opt_u64(last.attempted_at_ms)?;
        w.opt_u64(last.completed_at_ms)?;
    }
    w.u8(match s.terminal_kind {
        None => 0,
        Some(EvidenceKind::Envelope) => 1,
        Some(EvidenceKind::DiagnosticPayload) => 2,
    })?;
    w.u16(s.terminal_len)
}
/// Reads `r` into reusable unpublished `destination`, returning the fixed manifest count28.
/// After the version/address/count prefix passes, fields replace the slot in wire order;
/// truncation/invalid-data errors may leave partial metadata, which must never be published.
/// Full timestamp, response and cleanup relationships remain the outer decoder's responsibility.
pub(super) fn read_summary(
    r: &mut Reader<'_>,
    destination: &mut Option<Summary>,
) -> Result<u8, CaptureError> {
    if r.u8()? != 1 || r.u16()? != 0x7E0 || r.u16()? != 0x7E8 {
        return Err(CaptureError::InvalidData);
    }
    let count = r.u8()?;
    if count != 28 {
        return Err(CaptureError::InvalidData);
    }
    let summary = destination.insert(Summary::new(0));
    summary.started_at_ms = r.u64()?;
    summary.updated_at_ms = r.u64()?;
    summary.ended_at_ms = r.opt_u64()?;
    summary.adapter_mv = if r.bool()? { Some(r.u16()?) } else { None };
    summary.handle = r.opt_u8()?;
    summary.configured = r.bool()?;
    summary.completed = r.bool()?;
    summary.fault = if r.bool()? {
        Some(fault(r.u8()?, r.u8()?)?)
    } else {
        None
    };
    summary.cleanup = match r.u8()? {
        0 => CleanupOutcome::Pending,
        1 => CleanupOutcome::Clean,
        2 => CleanupOutcome::Failed,
        3 => CleanupOutcome::TimedOut,
        _ => return Err(CaptureError::InvalidData),
    };
    summary.last_submission = if r.bool()? {
        Some(Submission {
            command: read_command(r)?,
            state: match r.u8()? {
                0 => SendState::NotAttempted,
                1 => SendState::InFlightUnknown,
                2 => SendState::LocallyComplete,
                _ => return Err(CaptureError::InvalidData),
            },
            attempted_at_ms: r.opt_u64()?,
            completed_at_ms: r.opt_u64()?,
        })
    } else {
        None
    };
    summary.terminal_kind = match r.u8()? {
        0 => None,
        1 => Some(EvidenceKind::Envelope),
        2 => Some(EvidenceKind::DiagnosticPayload),
        _ => return Err(CaptureError::InvalidData),
    };
    summary.terminal_len = r.u16()?;
    Ok(count)
}
/// Writes borrowed `row` as tag5, preserving incomplete prefix state; errors leave partial bytes.
/// `w` receives or measures the record. Returns unit on completion or its capacity error.
// Keep real rows and reservation rows on the same bounded serializer call path.
#[inline(never)]
fn write_row(
    w: &mut Writer<'_>,
    row: RowRef<'_>,
) -> Result<(), CaptureError> {
    w.record(5, |w| {
        w.u8(row.id.index() as u8)?;
        w.u16(row.id.did())?;
        w.opt_u64(row.attempted_at_ms())?;
        w.opt_u64(row.submitted_at_ms())?;
        w.opt_u64(row.completed_at_ms())?;
        let (tag, nrc) = match row.status {
            None => (0, 0),
            Some(ReadStatus::Positive) => (1, 0),
            Some(ReadStatus::Negative(n)) => (2, n),
            Some(ReadStatus::SessionNotDefault) => (3, 0),
        };
        w.u8(tag)?;
        w.u8(nrc)?;
        w.u8(row.pending)?;
        w.u8(row.declared_len)?;
        w.u8(row.raw.len() as u8)?;
        w.bytes(row.raw)
    })
}
/// Reads one tag5 body from `r`, returning a borrowed row after primitive bounds validation.
// Validation-only callers discard the row; inlining avoids constructing its unused return value.
#[inline(always)]
pub(super) fn read_row<'a>(r: &mut Reader<'a>) -> Result<RowRef<'a>, CaptureError> {
    let id = read_id(r.u8()?)?;
    if r.u16()? != id.did() {
        return Err(CaptureError::InvalidData);
    }
    let attempted_at_ms = r.opt_u64()?;
    let submitted_at_ms = r.opt_u64()?;
    let completed_at_ms = r.opt_u64()?;
    let tag = r.u8()?;
    let nrc = r.u8()?;
    if tag != 2 && nrc != 0 {
        return Err(CaptureError::InvalidData);
    }
    let status = match tag {
        0 => None,
        1 => Some(ReadStatus::Positive),
        2 => Some(ReadStatus::Negative(nrc)),
        3 => Some(ReadStatus::SessionNotDefault),
        _ => return Err(CaptureError::InvalidData),
    };
    let pending = r.u8()?;
    let declared_len = r.u8()?;
    let len = usize::from(r.u8()?);
    if len > 67 || usize::from(declared_len) > 67 || len > usize::from(declared_len) || pending > 3 {
        return Err(CaptureError::InvalidData);
    }
    Ok(RowRef::new(
        id,
        [attempted_at_ms, submitted_at_ms, completed_at_ms],
        status,
        pending,
        declared_len,
        r.take(len)?,
    ))
}
/// Writes the one borrowed terminal prefix as tag6; errors preserve an untrusted prefix.
/// `w` receives or measures `v`'s provenance, declared length and exact retained bytes.
/// Returns unit on completion or the writer's capacity error; semantic admission is separate.
fn write_terminal(
    w: &mut Writer<'_>,
    v: TerminalRef<'_>,
) -> Result<(), CaptureError> {
    w.record(6, |w| {
        w.u8(match v.kind {
            EvidenceKind::Envelope => 0,
            EvidenceKind::DiagnosticPayload => 1,
        })?;
        w.u16(v.declared_len)?;
        w.u16(v.raw.len() as u16)?;
        w.bytes(v.raw)
    })
}
/// Reads a terminal prefix without allocating or claiming complete envelope semantics.
/// `r` supplies the tag6 body. Returns borrowed provenance/bytes or truncation/invalid-data errors;
/// the declared limit is245 bytes for envelopes and67 for diagnostic payloads.
pub(super) fn read_terminal<'a>(r: &mut Reader<'a>) -> Result<TerminalRef<'a>, CaptureError> {
    let kind = match r.u8()? {
        0 => EvidenceKind::Envelope,
        1 => EvidenceKind::DiagnosticPayload,
        _ => return Err(CaptureError::InvalidData),
    };
    let declared_len = r.u16()?;
    let len = r.u16()?;
    if declared_len > 245 || len > declared_len || (kind == EvidenceKind::DiagnosticPayload && declared_len > 67) {
        return Err(CaptureError::InvalidData);
    }
    Ok(TerminalRef {
        kind,
        declared_len,
        raw: r.take(usize::from(len))?,
    })
}
/// Writes all 28 manifest rows and optional terminal prefix borrowed from `e`; no evidence clone.
/// `w` receives or measures the records. Returns unit or a capacity error leaving a partial prefix.
pub(super) fn write_records(
    w: &mut Writer<'_>,
    e: &Evidence,
) -> Result<(), CaptureError> {
    for id in ReadId::ALL {
        write_row(w, e.row(id))?;
    }
    if let Some(kind) = e.summary.terminal_kind {
        write_terminal(
            w,
            TerminalRef {
                kind,
                declared_len: e.summary.terminal_len,
                raw: e.terminal(),
            },
        )?;
    }
    Ok(())
}
/// Runs the same serializers over maximal small metadata/rows to reserve bytes without large arrays.
/// `w` receives or measures the worst-case3260-byte collection extension, not valid vehicle evidence.
/// Returns unit if the extension fits or a capacity error after advancing through the admitted prefix.
pub(super) fn reserve(w: &mut Writer<'_>) -> Result<(), CaptureError> {
    /// Immutable maximal-size input, avoiding a runtime Summary construction.
    /// The deliberately inconsistent fields are sizing data, never admissible evidence.
    static SUMMARY: Summary = Summary {
        started_at_ms: 0,
        updated_at_ms: 0,
        ended_at_ms: Some(0),
        adapter_mv: Some(0),
        handle: Some(0),
        configured: true,
        completed: false,
        fault: Some(Error::Remote(0)),
        cleanup: CleanupOutcome::Clean,
        last_submission: Some(Submission {
            command: Command::FlowControl(ReadId::ActiveSession),
            state: SendState::LocallyComplete,
            attempted_at_ms: Some(0),
            completed_at_ms: Some(0),
        }),
        terminal_kind: Some(EvidenceKind::Envelope),
        terminal_len: 245,
    };
    write_summary(w, &SUMMARY)?;
    for id in ReadId::ALL {
        write_row(
            w,
            RowRef::new(id, [Some(0); 3], Some(ReadStatus::Positive), 3, 67, &[0; 67]),
        )?;
    }
    write_terminal(
        w,
        TerminalRef {
            kind: EvidenceKind::Envelope,
            declared_len: 245,
            raw: &[0; 245],
        },
    )
}

#[cfg(test)]
/// Serialization-size regressions; synthetic maximal bodies are not admissible vehicle evidence.
mod tests {
    use super::*;

    /// Compact runtime timestamps retain the v6 presence-octet/little-endian-u64 wire contract.
    #[test]
    fn row_timestamp_presence_and_extrema_keep_exact_wire_bytes() {
        for mask in 0u8..8 {
            for values in [[0, u64::MAX, 17], [u64::MAX, 17, 0], [17, 0, u64::MAX]] {
                let times = core::array::from_fn(|i| (mask & (1 << i) != 0).then_some(values[i]));
                let row = RowRef::new(ReadId::ActiveSession, times, None, 0, 0, &[]);
                let mut bytes = [0u8; 64];
                let mut writer = Writer {
                    bytes: &mut bytes,
                    pos: 0,
                    measuring: false,
                };
                write_row(&mut writer, row).unwrap();
                let len = writer.pos;
                let mut expected =
                    heapless::Vec::<u8, 64>::from_slice(&[5, (11 + 8 * mask.count_ones()) as u8, 0, 0, 0x86, 0xF1])
                        .unwrap();
                for time in times {
                    expected.push(u8::from(time.is_some())).unwrap();
                    if let Some(value) = time {
                        expected.extend_from_slice(&value.to_le_bytes()).unwrap();
                    }
                }
                expected.extend_from_slice(&[0; 5]).unwrap();
                assert_eq!(&bytes[..len], expected.as_slice(), "mask={mask}");
                let mut reader = Reader { bytes: &bytes[3..len] };
                let decoded = read_row(&mut reader).unwrap();
                assert_eq!(
                    [
                        decoded.attempted_at_ms(),
                        decoded.submitted_at_ms(),
                        decoded.completed_at_ms()
                    ],
                    times,
                );
                assert!(reader.bytes.is_empty());
            }
        }
    }

    /// Truncated summary fields stay unpublished in the caller's slot instead of returning a large value.
    #[test]
    fn summary_decode_retains_only_partial_destination_on_error() {
        let summary = Evidence::new(7).summary;
        let mut bytes = [0; 128];
        let mut writer = Writer {
            bytes: &mut bytes,
            pos: 0,
            measuring: false,
        };
        write_summary(&mut writer, &summary).unwrap();
        let length = writer.pos;
        let mut destination = None;
        assert_eq!(
            read_summary(
                &mut Reader {
                    bytes: &bytes[..length - 1]
                },
                &mut destination
            ),
            Err(CaptureError::Truncated)
        );
        let partial = destination.as_ref().unwrap();
        assert_eq!((partial.started_at_ms, partial.updated_at_ms), (7, 7));
        assert_eq!(
            read_summary(
                &mut Reader {
                    bytes: &bytes[..length]
                },
                &mut destination
            ),
            Ok(28)
        );
        assert_eq!(destination, Some(summary));
    }

    #[test]
    /// Maximal summary 67 +28 rows 105 each +terminal 253 reserves exactly 3260 bytes.
    fn maximum_reservation_uses_real_serializers_and_honors_8192_boundary() {
        let mut output = [0; 8192];
        let mut actual = Writer {
            bytes: &mut output,
            pos: 0,
            measuring: false,
        };
        reserve(&mut actual).unwrap();
        assert_eq!(actual.pos, 67 + 28 * 105 + 253);
        let mut measured = Writer {
            bytes: &mut [],
            pos: 8192 - 3260,
            measuring: true,
        };
        reserve(&mut measured).unwrap();
        assert_eq!(measured.pos, 8192);
        measured.pos = 8192 - 3260 + 1;
        assert_eq!(reserve(&mut measured), Err(CaptureError::TooLarge));
    }
}
