//! Read-only USB command parsing and bounded response framing; no transport I/O.
use super::capture_store::{PAYLOAD_MAX, crc32};
/// Maximum bytes before LF in one command line, including optional CR.
pub const COMMAND_MAX: usize = 32;
/// Fixed binary response-header size in bytes, including header CRC.
pub const HEADER_BYTES: usize = 32;
/// Read-only commands admitted from newline-delimited ASCII; no mutation commands exist.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Request {
    /// List all four logical slots followed by an End frame.
    List,
    /// Retrieve the newest journal-admitted capture in one slot.
    Get(
        /// Zero-based slot 0..4, parsed from wire digits 1..4.
        u8,
    ),
}
/// Command/framing failures; errors require transport-session policy at the caller boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WireError {
    /// Command exceeded the bounded line buffer before its terminating LF.
    TooLong,
    /// Complete command line did not exactly match the supported grammar.
    InvalidCommand,
    /// Header magic/version/codes/reserved bytes or cross-field semantics were invalid.
    InvalidHeader,
    /// Header or payload CRC disagreed with its received bytes.
    Integrity,
    /// Header/payload/list-entry byte count violated its bound or declared length.
    Length,
}
/// Binary response frame role.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameKind {
    /// Slot status, with an eight-byte payload descriptor on success.
    ListEntry,
    /// Nonempty raw capture payload from a nonzero generation.
    Capture,
    /// Successful transaction terminator without slot or payload.
    End,
    /// Failed request outcome without payload or generation.
    Error,
}
/// Wire status code, separate from application capture-schema admission.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameStatus {
    /// Requested journal/frame operation succeeded.
    Ok,
    /// Slot has no committed payload and is erased.
    Empty,
    /// Journal admission found corrupt data without a readable selection.
    Corrupt,
    /// Journal format is unsupported by this firmware.
    Unsupported,
    /// Storage owner could not accept the read request.
    Busy,
    /// Command was malformed or unsupported.
    InvalidRequest,
    /// Storage operation failed.
    StorageError,
}
/// Validatable wire header; integer fields encode little-endian and slot encodes one-based.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameHeader {
    /// Response role determining allowed status/generation/payload combinations.
    pub kind: FrameKind,
    /// Outcome classification; only successful capture frames can carry capture bytes.
    pub status: FrameStatus,
    /// Zero-based slot 0..4; absent for End and optionally for Error.
    pub slot: Option<u8>,
    /// Nonzero committed generation for successful ListEntry/Capture, otherwise zero.
    pub generation: u64,
    /// Following payload byte count, at most 8192; successful ListEntry uses exactly eight.
    pub length: u16,
    /// CRC-32/ISO-HDLC of following payload, zero for empty payload.
    pub payload_crc: u32,
}
/// Incremental command-line parser that drains overflow through LF before resynchronizing.
pub struct RequestParser {
    /// Bounded current line prefix; stale tail bytes are never parsed.
    bytes: [u8; COMMAND_MAX],
    /// Occupied prefix length, at most [`COMMAND_MAX`].
    len: usize,
    /// Line exceeded capacity; ignore further bytes until LF then return TooLong.
    overflow: bool,
}
impl Default for RequestParser {
    /// Returns an empty parser without a partial command or overflow state.
    fn default() -> Self { Self::new() }
}
impl RequestParser {
    /// Returns an empty parser; bytes are collected only by explicit [`Self::feed`] calls.
    pub const fn new() -> Self {
        Self {
            bytes: [0; COMMAND_MAX],
            len: 0,
            overflow: false,
        }
    }

    /// Discards `self`'s partial line/overflow state, returning unit; storage need not be
    /// zeroed because length fences stale bytes. Call when the transport session changes.
    pub fn reset(&mut self) {
        self.len = 0;
        self.overflow = false;
    }

    /// An overlong line is discarded through its newline, never reparsed as a suffix.
    /// Call reset when the USB session ends so a new host cannot finish an old command.
    /// Consumes `byte` into `self`, returning `None` until LF. At LF returns one parsed
    /// request or TooLong/InvalidCommand, then resets. Accepts exact `BLECAP LIST` or
    /// `BLECAP GET 1` through `4`, with optional single CR before LF; no whitespace folding.
    pub fn feed(
        &mut self,
        byte: u8,
    ) -> Option<Result<Request, WireError>> {
        if byte != b'\n' {
            if self.len == COMMAND_MAX {
                self.overflow = true;
            } else if !self.overflow {
                self.bytes[self.len] = byte;
                self.len += 1;
            }
            return None;
        }
        let result = if self.overflow {
            Err(WireError::TooLong)
        } else {
            let line = &self.bytes[..self.len];
            let line = line.strip_suffix(b"\r").unwrap_or(line);
            if line == b"BLECAP LIST" {
                Ok(Request::List)
            } else if line.len() == 12 && line.starts_with(b"BLECAP GET ") && (b'1'..=b'4').contains(&line[11]) {
                Ok(Request::Get(line[11] - b'1'))
            } else {
                Err(WireError::InvalidCommand)
            }
        };
        self.reset();
        Some(result)
    }
}
impl FrameHeader {
    /// Returns a validated header for `kind`, `status`, zero-based optional `slot`,
    /// `generation` and `payload`, computing payload length/CRC. Oversized payload returns
    /// Length; incompatible role/status/slot/generation combinations return InvalidHeader.
    /// Does not parse the contents of a successful ListEntry payload; validation does that later.
    pub fn new(
        kind: FrameKind,
        status: FrameStatus,
        slot: Option<u8>,
        generation: u64,
        payload: &[u8],
    ) -> Result<Self, WireError> {
        if payload.len() > PAYLOAD_MAX {
            return Err(WireError::Length);
        }
        let header = Self {
            kind,
            status,
            slot,
            generation,
            length: payload.len() as u16,
            payload_crc: crc32(payload),
        };
        header.validate()?;
        Ok(header)
    }

    /// Returns unit if `self`'s cross-field invariants hold; otherwise InvalidHeader.
    /// Checks slot/payload bounds, empty CRC and role-specific status/generation/length;
    /// it cannot check payload bytes that are not part of the header.
    fn validate(self) -> Result<(), WireError> {
        if self.slot.is_some_and(|v| v >= 4)
            || usize::from(self.length) > PAYLOAD_MAX
            || (self.length == 0 && self.payload_crc != 0)
        {
            return Err(WireError::InvalidHeader);
        }
        let valid = match self.kind {
            FrameKind::Capture => {
                self.status == FrameStatus::Ok && self.slot.is_some() && self.generation != 0 && self.length != 0
            }
            FrameKind::ListEntry => {
                self.slot.is_some()
                    && if self.status == FrameStatus::Ok {
                        self.generation != 0 && self.length == 8
                    } else {
                        self.generation == 0 && self.length == 0 && self.status != FrameStatus::InvalidRequest
                    }
            }
            FrameKind::End => {
                self.status == FrameStatus::Ok && self.slot.is_none() && self.generation == 0 && self.length == 0
            }
            FrameKind::Error => self.status != FrameStatus::Ok && self.generation == 0 && self.length == 0,
        };
        if valid { Ok(()) } else { Err(WireError::InvalidHeader) }
    }

    /// Returns `self` as a 32-byte version-one header with zero reserved bytes and
    /// header CRC over its first 28 bytes; invalid metadata returns InvalidHeader.
    pub fn encode(self) -> Result<[u8; HEADER_BYTES], WireError> {
        self.validate()?;
        let mut b = [0; HEADER_BYTES];
        b[..8].copy_from_slice(b"BLECUSB1");
        b[8] = 1;
        b[9] = match self.status {
            FrameStatus::Ok => 0,
            FrameStatus::Empty => 1,
            FrameStatus::Corrupt => 2,
            FrameStatus::Unsupported => 3,
            FrameStatus::Busy => 4,
            FrameStatus::InvalidRequest => 5,
            FrameStatus::StorageError => 6,
        };
        b[10] = self.slot.map_or(0, |s| s + 1);
        b[11] = match self.kind {
            FrameKind::ListEntry => 0,
            FrameKind::Capture => 1,
            FrameKind::End => 2,
            FrameKind::Error => 3,
        };
        b[12..20].copy_from_slice(&self.generation.to_le_bytes());
        b[20..22].copy_from_slice(&self.length.to_le_bytes());
        b[24..28].copy_from_slice(&self.payload_crc.to_le_bytes());
        let crc = crc32(&b[..28]);
        b[28..32].copy_from_slice(&crc.to_le_bytes());
        Ok(b)
    }

    /// Returns a validated header from exact 32-byte `b`. Rejects wrong length,
    /// magic/version/reserved bytes/codes, header CRC and cross-field inconsistencies;
    /// payload integrity remains the caller's separate [`Self::validate_payload`] step.
    pub fn decode(b: &[u8]) -> Result<Self, WireError> {
        if b.len() != HEADER_BYTES {
            return Err(WireError::Length);
        }
        if &b[..8] != b"BLECUSB1" || b[8] != 1 || b[22..24] != [0, 0] {
            return Err(WireError::InvalidHeader);
        }
        let u32_at = |at: usize| u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]]);
        if crc32(&b[..28]) != u32_at(28) {
            return Err(WireError::Integrity);
        }
        let status = match b[9] {
            0 => FrameStatus::Ok,
            1 => FrameStatus::Empty,
            2 => FrameStatus::Corrupt,
            3 => FrameStatus::Unsupported,
            4 => FrameStatus::Busy,
            5 => FrameStatus::InvalidRequest,
            6 => FrameStatus::StorageError,
            _ => return Err(WireError::InvalidHeader),
        };
        let slot = match b[10] {
            0 => None,
            1..=4 => Some(b[10] - 1),
            _ => return Err(WireError::InvalidHeader),
        };
        let kind = match b[11] {
            0 => FrameKind::ListEntry,
            1 => FrameKind::Capture,
            2 => FrameKind::End,
            3 => FrameKind::Error,
            _ => return Err(WireError::InvalidHeader),
        };
        let header = Self {
            kind,
            status,
            slot,
            generation: u64::from_le_bytes([b[12], b[13], b[14], b[15], b[16], b[17], b[18], b[19]]),
            length: u16::from_le_bytes([b[20], b[21]]),
            payload_crc: u32_at(24),
        };
        header.validate()?;
        Ok(header)
    }

    /// Returns unit if `self` is valid and following `bytes` match exact length and CRC.
    /// Successful ListEntry also validates its inner eight-byte descriptor. Returns
    /// InvalidHeader, Length or Integrity without modifying bytes or decoding capture schema.
    pub fn validate_payload(
        self,
        bytes: &[u8],
    ) -> Result<(), WireError> {
        self.validate()?;
        if bytes.len() != usize::from(self.length) {
            return Err(WireError::Length);
        }
        if crc32(bytes) != self.payload_crc {
            return Err(WireError::Integrity);
        }
        if self.kind == FrameKind::ListEntry && self.status == FrameStatus::Ok {
            parse_list_payload(bytes)?;
        }
        Ok(())
    }
}

/// Valid LIST entries describe the journal payload without transferring it.
/// Returns eight little-endian bytes containing capture `length` and `payload_crc`;
/// rejects lengths outside 1..=8192. This CRC describes the capture, not this descriptor.
pub fn list_payload(
    length: u32,
    payload_crc: u32,
) -> Result<[u8; 8], WireError> {
    if length == 0 || length > PAYLOAD_MAX as u32 {
        return Err(WireError::Length);
    }
    let mut bytes = [0; 8];
    bytes[..4].copy_from_slice(&length.to_le_bytes());
    bytes[4..].copy_from_slice(&payload_crc.to_le_bytes());
    Ok(bytes)
}
/// Returns `(capture_length, capture_crc)` from exact eight-byte `bytes`, decoding
/// little-endian fields and rejecting lengths outside 1..=8192. Returns Length for
/// wrong descriptor size or bounds; outer frame CRC is verified separately.
pub fn parse_list_payload(bytes: &[u8]) -> Result<(u32, u32), WireError> {
    if bytes.len() != 8 {
        return Err(WireError::Length);
    }
    let length = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    let crc = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
    list_payload(length, crc)?;
    Ok((length, crc))
}
