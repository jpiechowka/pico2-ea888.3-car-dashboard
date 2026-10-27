//! Explicit stable schema-v5 identity fields; no enum discriminant casts or I/O.
use super::{CaptureError, Reader, Writer, read_reason, read_result, result_code, write_reason};
use crate::ble::canyonero::{CleanupOutcome, DecodeError, Fault, Reason, Stage, TransportDomain, TransportFault};
use crate::discovery::identity::{CANDIDATE_SPSM, ChannelParameters, Evidence, VENDOR_UUIDS};
use crate::discovery::{InfoValue, Payload, ReadResult};

/// Public source commit bytes for the explicit compatibility policy, big-endian hex order.
const SOURCE: [u8; 20] = [
    0x78, 0x09, 0x1B, 0xB5, 0x59, 0x9C, 0x4F, 0x5E, 0x30, 0x02, 0xFB, 0xD5, 0x70, 0x0B, 0x82, 0x6A, 0x62, 0x5E, 0x8B,
    0x04,
];

/// Returns stable wire stage for `stage`; independent of Rust representation.
fn stage_code(stage: Stage) -> u8 {
    match stage {
        Stage::Idle => 0,
        Stage::Connecting => 1,
        Stage::Discovering => 2,
        Stage::TransportParameters => 3,
        Stage::OpeningCoc => 4,
        Stage::SendingInfo => 5,
        Stage::AwaitingInfo => 6,
        Stage::Closing => 7,
        Stage::Finished => 8,
    }
}
/// Returns the stage decoded from stable `code`, or InvalidData for unknown values.
fn stage(code: u8) -> Result<Stage, CaptureError> {
    Ok(match code {
        0 => Stage::Idle,
        1 => Stage::Connecting,
        2 => Stage::Discovering,
        3 => Stage::TransportParameters,
        4 => Stage::OpeningCoc,
        5 => Stage::SendingInfo,
        6 => Stage::AwaitingInfo,
        7 => Stage::Closing,
        8 => Stage::Finished,
        _ => return Err(CaptureError::InvalidData),
    })
}
/// Appends optional u16 `value` to `w`; a write error can leave a presence prefix.
/// Returns unit on success or the writer's capacity error.
fn optional(
    w: &mut Writer<'_>,
    value: Option<u16>,
) -> Result<(), CaptureError> {
    w.bool(value.is_some())?;
    if let Some(value) = value {
        w.u16(value)?;
    }
    Ok(())
}
/// Returns optional u16 read from `r`, or an invalid/truncated presence/byte error.
fn read_optional(r: &mut Reader<'_>) -> Result<Option<u16>, CaptureError> {
    Ok(if r.bool()? { Some(r.u16()?) } else { None })
}
/// Returns stable protocol error tag and optional type byte for `error`.
fn decode_error_code(error: DecodeError) -> (u8, u8) {
    match error {
        DecodeError::Marker => (0, 0),
        DecodeError::UnsupportedSize => (1, 0),
        DecodeError::UnexpectedType(v) => (2, v),
        DecodeError::Remote(v) => (3, v),
        DecodeError::TrailingData => (4, 0),
        DecodeError::Truncated => (5, 0),
        DecodeError::FieldCount => (6, 0),
        DecodeError::FieldTooLong => (7, 0),
        DecodeError::InvalidUtf8 => (8, 0),
        DecodeError::ControlCharacter => (9, 0),
    }
}
/// Returns canonical protocol error read from `r`, or a format/truncation error;
/// unused bytes must be zero.
#[inline(always)]
fn read_decode_error(r: &mut Reader<'_>) -> Result<DecodeError, CaptureError> {
    let tag = r.u8()?;
    let code = r.u8()?;
    if !matches!(tag, 2 | 3) && code != 0 {
        return Err(CaptureError::InvalidData);
    }
    Ok(match tag {
        0 => DecodeError::Marker,
        1 => DecodeError::UnsupportedSize,
        2 => DecodeError::UnexpectedType(code),
        3 if matches!(code, 0xE0..=0xE5 | 0xEF) => DecodeError::Remote(code),
        4 => DecodeError::TrailingData,
        5 => DecodeError::Truncated,
        6 => DecodeError::FieldCount,
        7 => DecodeError::FieldTooLong,
        8 => DecodeError::InvalidUtf8,
        9 => DecodeError::ControlCharacter,
        _ => return Err(CaptureError::InvalidData),
    })
}
/// Appends exact first `fault` to `w`; bounds errors may leave a partial prefix.
/// Returns unit on success or the writer's capacity error.
#[inline(always)]
fn write_fault(
    w: &mut Writer<'_>,
    fault: Fault,
) -> Result<(), CaptureError> {
    w.u8(stage_code(fault.stage))?;
    w.u64(fault.at_ms)?;
    let tag = match fault.reason {
        Reason::Protocol(_) => 0,
        Reason::TimedOut => 1,
        Reason::TimeReversed => 2,
        Reason::ClockOverflow => 3,
        Reason::Cancelled => 4,
        Reason::Parked => 5,
        Reason::Transport(_) => 6,
        Reason::UnexpectedEvent => 7,
        Reason::UnexpectedEmptySdu => 8,
        Reason::Eof => 9,
        Reason::CleanupTimedOut => 10,
        Reason::CleanupFailed => 11,
    };
    w.u8(tag)?;
    match fault.reason {
        Reason::Protocol(error) => {
            let (tag, code) = decode_error_code(error);
            w.u8(tag)?;
            w.u8(code)?;
        }
        Reason::Transport(error) => {
            w.u8(match error.domain {
                TransportDomain::Hci => 0,
                TransportDomain::Att => 1,
                TransportDomain::L2cap => 2,
                TransportDomain::Local => 3,
            })?;
            optional(w, error.code)?;
            w.bool(error.partial_send)?;
        }
        _ => {}
    }
    Ok(())
}
/// Returns first fault read from `r`, preserving exact stage/time/domain, or a
/// format/truncation error for invalid tags or incomplete bytes.
#[inline(always)]
fn read_fault(r: &mut Reader<'_>) -> Result<Fault, CaptureError> {
    let stage = stage(r.u8()?)?;
    let at_ms = r.u64()?;
    let reason = match r.u8()? {
        0 => Reason::Protocol(read_decode_error(r)?),
        1 => Reason::TimedOut,
        2 => Reason::TimeReversed,
        3 => Reason::ClockOverflow,
        4 => Reason::Cancelled,
        5 => Reason::Parked,
        6 => {
            let domain = match r.u8()? {
                0 => TransportDomain::Hci,
                1 => TransportDomain::Att,
                2 => TransportDomain::L2cap,
                3 => TransportDomain::Local,
                _ => return Err(CaptureError::InvalidData),
            };
            Reason::Transport(TransportFault {
                domain,
                code: read_optional(r)?,
                partial_send: r.bool()?,
            })
        }
        7 => Reason::UnexpectedEvent,
        8 => Reason::UnexpectedEmptySdu,
        9 => Reason::Eof,
        10 => Reason::CleanupTimedOut,
        11 => Reason::CleanupFailed,
        _ => return Err(CaptureError::InvalidData),
    };
    Ok(Fault { stage, reason, at_ms })
}

/// Appends optional `evidence` to `w` with stable policy/version/source fields.
/// Returns a write/representation error; output is untrusted until full decode.
// One metadata encoder owns this path; inline to reuse its temporary stack slots.
#[inline(always)]
pub(super) fn write(
    w: &mut Writer<'_>,
    evidence: Option<&Evidence>,
) -> Result<(), CaptureError> {
    w.bool(evidence.is_some())?;
    let Some(e) = evidence else {
        return Ok(());
    };
    // Extension version1, policy1: source-configured compatibility candidate.
    w.u8(1)?;
    w.u8(1)?;
    w.u16(CANDIDATE_SPSM)?;
    w.bytes(&SOURCE)?;
    w.u64(e.started_at_ms)?;
    w.u64(e.updated_at_ms)?;
    w.opt_u64(e.ended_at_ms)?;
    w.u8(stage_code(e.stage))?;
    w.bool(e.fault.is_some())?;
    if let Some(fault) = e.fault {
        write_fault(w, fault)?;
    }
    w.u8(match e.cleanup {
        CleanupOutcome::Pending => 0,
        CleanupOutcome::Clean => 1,
        CleanupOutcome::Failed => 2,
        CleanupOutcome::TimedOut => 3,
    })?;
    for value in &e.vendor_values {
        w.bool(value.is_some())?;
        if let Some(value) = value {
            w.u16(value.service_start)?;
            w.u16(value.handle)?;
            w.u16(value.uuid)?;
            w.u8(result_code(value.result))?;
            write_reason(w, value.failure)?;
            w.u32(value.elapsed_ms)?;
            w.bool(value.value.truncated)?;
            if value.value.len > 64 {
                return Err(CaptureError::InvalidData);
            }
            w.u8(value.value.len)?;
            w.bytes(value.value.as_slice())?;
        }
    }
    w.bool(e.channel.is_some())?;
    if let Some(c) = e.channel.as_ref() {
        w.u16(c.offered_mtu)?;
        w.u16(c.offered_mps)?;
        w.u16(c.offered_credits)?;
        // Preserve wire order without materializing another channel/field array.
        optional(w, c.local_mtu)?;
        optional(w, c.local_mps)?;
        optional(w, c.peer_mtu)?;
        optional(w, c.peer_mps)?;
        optional(w, c.local_cid)?;
        optional(w, c.peer_cid)?;
        optional(w, c.local_credits)?;
        optional(w, c.peer_credits)?;
    }
    w.opt_u64(e.request_submitted_at_ms)?;
    w.u64(e.observed_bytes)?;
    w.u32(e.sdu_count)?;
    w.u16(e.max_sdu_bytes)?;
    w.u8(e.raw_len)?;
    if usize::from(e.raw_len) > e.raw.len() {
        return Err(CaptureError::InvalidData);
    }
    w.bytes(e.raw())
}

/// Decodes optional identity evidence from `r` into unpublished `destination`.
/// Clears absent data and rejects unknown policy/source/version and field bounds;
/// full metadata/relationship checks occur after decoding the entire capture.
/// Returns unit on success or a format/truncation error, retaining any unpublished prefix.
// One metadata decoder owns this path; keep field temporaries in that owner frame.
#[inline(always)]
pub(super) fn read(
    r: &mut Reader<'_>,
    destination: &mut Option<Evidence>,
) -> Result<(), CaptureError> {
    *destination = None;
    if !r.bool()? {
        return Ok(());
    }
    if r.u8()? != 1 || r.u8()? != 1 || r.u16()? != CANDIDATE_SPSM || r.take(SOURCE.len())? != SOURCE {
        return Err(CaptureError::InvalidData);
    }
    let e = destination.insert(Evidence::new(r.u64()?));
    e.updated_at_ms = r.u64()?;
    e.ended_at_ms = r.opt_u64()?;
    e.stage = stage(r.u8()?)?;
    if r.bool()? {
        e.fault = Some(read_fault(r)?);
    }
    e.cleanup = match r.u8()? {
        0 => CleanupOutcome::Pending,
        1 => CleanupOutcome::Clean,
        2 => CleanupOutcome::Failed,
        3 => CleanupOutcome::TimedOut,
        _ => return Err(CaptureError::InvalidData),
    };
    for (value, expected) in e.vendor_values.iter_mut().zip(VENDOR_UUIDS) {
        if r.bool()? {
            // The whole destination remains unpublished until capture admission.
            // Fill its final raw storage, avoiding a second 64-byte stack payload.
            let value = value.insert(InfoValue {
                service_start: 0,
                handle: 0,
                uuid: expected,
                value: Payload::empty(),
                result: ReadResult::Ok,
                failure: None,
                elapsed_ms: 0,
            });
            value.service_start = r.u16()?;
            value.handle = r.u16()?;
            value.uuid = r.u16()?;
            value.result = read_result(r.u8()?)?;
            value.failure = read_reason(r)?;
            value.elapsed_ms = r.u32()?;
            value.value.truncated = r.bool()?;
            let len = r.u8()?;
            if len > 64 || value.uuid != expected {
                return Err(CaptureError::InvalidData);
            }
            value.value.bytes[..usize::from(len)].copy_from_slice(r.take(usize::from(len))?);
            value.value.len = len;
        }
    }
    if r.bool()? {
        // Decode only into unpublished final storage, preserving the exact field order.
        let c = e.channel.insert(ChannelParameters {
            offered_mtu: 0,
            offered_mps: 0,
            offered_credits: 0,
            local_mtu: None,
            local_mps: None,
            peer_mtu: None,
            peer_mps: None,
            local_cid: None,
            peer_cid: None,
            local_credits: None,
            peer_credits: None,
        });
        c.offered_mtu = r.u16()?;
        c.offered_mps = r.u16()?;
        c.offered_credits = r.u16()?;
        c.local_mtu = read_optional(r)?;
        c.local_mps = read_optional(r)?;
        c.peer_mtu = read_optional(r)?;
        c.peer_mps = read_optional(r)?;
        c.local_cid = read_optional(r)?;
        c.peer_cid = read_optional(r)?;
        c.local_credits = read_optional(r)?;
        c.peer_credits = read_optional(r)?;
    }
    e.request_submitted_at_ms = r.opt_u64()?;
    e.observed_bytes = r.u64()?;
    e.sdu_count = r.u32()?;
    e.max_sdu_bytes = r.u16()?;
    e.raw_len = r.u8()?;
    if usize::from(e.raw_len) > e.raw.len() {
        return Err(CaptureError::InvalidData);
    }
    e.raw[..usize::from(e.raw_len)].copy_from_slice(r.take(usize::from(e.raw_len))?);
    Ok(())
}
