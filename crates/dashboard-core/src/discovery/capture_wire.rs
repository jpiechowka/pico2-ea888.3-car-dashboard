//! Read-only USB command parsing and bounded response framing; no transport I/O.
use super::capture_store::{PAYLOAD_MAX, crc32};
pub const COMMAND_MAX: usize = 32;
pub const HEADER_BYTES: usize = 32;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Request {
    List,
    Get(u8),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WireError {
    TooLong,
    InvalidCommand,
    InvalidHeader,
    Integrity,
    Length,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameKind {
    ListEntry,
    Capture,
    End,
    Error,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameStatus {
    Ok,
    Empty,
    Corrupt,
    Unsupported,
    Busy,
    InvalidRequest,
    StorageError,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameHeader {
    pub kind: FrameKind,
    pub status: FrameStatus,
    pub slot: Option<u8>,
    pub generation: u64,
    pub length: u16,
    pub payload_crc: u32,
}
pub struct RequestParser {
    bytes: [u8; COMMAND_MAX],
    len: usize,
    overflow: bool,
}
impl Default for RequestParser {
    fn default() -> Self { Self::new() }
}
impl RequestParser {
    pub const fn new() -> Self {
        Self {
            bytes: [0; COMMAND_MAX],
            len: 0,
            overflow: false,
        }
    }

    pub fn reset(&mut self) {
        self.len = 0;
        self.overflow = false;
    }

    /// An overlong line is discarded through its newline, never reparsed as a suffix.
    /// Call reset when the USB session ends so a new host cannot finish an old command.
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
pub fn parse_list_payload(bytes: &[u8]) -> Result<(u32, u32), WireError> {
    if bytes.len() != 8 {
        return Err(WireError::Length);
    }
    let length = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    let crc = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
    list_payload(length, crc)?;
    Ok((length, crc))
}
