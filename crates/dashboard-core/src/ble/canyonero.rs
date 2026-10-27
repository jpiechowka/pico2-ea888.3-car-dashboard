//! Bounded INFO-only CANyonero codec and diagnostic session, with no vehicle commands.
//!
//! Wire framing and field order follow the public [CANyonero encoder](https://github.com/Automotive-Swift/Swift-CANyonero/blob/78091bb5599c4f5e3002fbd5700b826a625e8b04/Sources/libCANyonero/Protocol.cpp).
//! The 245-byte frame ceiling, 64-byte fields and strict text validation are local
//! admission policy, not measured adapter limits. Device adapters own all I/O.

mod session;
pub use session::*;

/// Maximum retained received bytes, including the four-byte header.
pub const MAX_FRAME_BYTES: usize = 245;
/// Maximum admitted INFO body bytes, including separators and optional final LF.
pub const MAX_BODY_BYTES: usize = MAX_FRAME_BYTES - 4;
/// Maximum UTF-8 bytes in each identity field, before any separator.
pub const MAX_FIELD_BYTES: usize = 64;

/// Explicit local framing, text or remote-message failure; never a vehicle diagnosis.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecodeError {
    /// First byte was not 0x1F; scanning for a later marker is forbidden.
    Marker,
    /// Declared body exceeds the local 241-byte ceiling.
    UnsupportedSize,
    /// Message type cannot satisfy an INFO request; includes generic OK 0x80.
    UnexpectedType(
        /// Exact received type byte.
        u8,
    ),
    /// Explicit upstream negative response; opaque body remains raw evidence.
    Remote(
        /// Exact E0/E1/E2/E3/E4/E5/EF type byte.
        u8,
    ),
    /// Additional bytes followed the one allowed frame, including duplicate INFO.
    TrailingData,
    /// End of input arrived before a complete header or declared body.
    Truncated,
    /// Payload does not contain exactly five fields, allowing one final LF.
    FieldCount,
    /// A field exceeds 64 bytes even if the complete frame fits.
    FieldTooLong,
    /// A field is not valid UTF-8.
    InvalidUtf8,
    /// A field contains a Unicode control character, including NUL and DEL.
    ControlCharacter,
}

/// Incremental framing result, before session cleanup establishes success.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecodeStatus {
    /// A header or body still needs bytes.
    NeedMore,
    /// One complete, locally validated INFO frame is available.
    Complete,
}

/// Borrowed validated INFO fields; an empty field is explicitly unavailable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Identity<'a> {
    /// Vendor text, or unavailable when the transmitted field was empty.
    pub vendor: Option<&'a str>,
    /// Model text, or unavailable when empty.
    pub model: Option<&'a str>,
    /// Hardware revision text, or unavailable when empty.
    pub hardware: Option<&'a str>,
    /// Serial text, or unavailable when empty; callers must treat it as private evidence.
    pub serial: Option<&'a str>,
    /// Firmware version text, or unavailable when empty.
    pub firmware: Option<&'a str>,
    /// True only for the accepted extra terminal-LF variant, not an empty fifth field.
    pub trailing_lf: bool,
}

/// Validates exactly one complete INFO `bytes` frame, returning fields borrowed
/// from that slice without allocating or copying a receive buffer. Missing bytes
/// return Truncated; malformed headers, text and any suffix retain decoder errors.
pub fn parse_frame(bytes: &[u8]) -> Result<Identity<'_>, DecodeError> {
    let layout = parse_layout(bytes, bytes.len() as u64)?.ok_or(DecodeError::Truncated)?;
    Ok(identity_view(bytes, &layout.fields, layout.trailing_lf))
}

/// Validates exactly one complete INFO `bytes` frame without constructing field
/// views or copying payload storage. Returns unit on success; incomplete input
/// returns Truncated and every malformed header, text or suffix preserves the
/// same error as [`parse_frame`]. At most241 body bytes are examined.
pub fn validate_frame(bytes: &[u8]) -> Result<(), DecodeError> {
    parse_layout(bytes, bytes.len() as u64)?
        .ok_or(DecodeError::Truncated)
        .map(|_| ())
}

/// Validated offsets only; no payload buffer is allocated for complete-frame callers.
struct FieldLayout {
    /// Five absolute UTF-8 field ranges within the admitted at-most245-byte frame.
    fields: [(u8, u8); 5],
    /// True for exactly one additional terminal LF beyond the five fields.
    trailing_lf: bool,
}

/// Validates retained `raw` with total `observed` stream length and returns bounded
/// field offsets, None for incomplete input, or the same ordered protocol error for
/// both incremental and complete parsing. Only the declared at-most241-byte body
/// is scanned; omitted suffix bytes still fail using the full observed length.
fn parse_layout(
    raw: &[u8],
    observed: u64,
) -> Result<Option<FieldLayout>, DecodeError> {
    if raw.is_empty() {
        return Ok(None);
    }
    if raw[0] != 0x1F {
        return Err(DecodeError::Marker);
    }
    if raw.len() < 4 {
        return Ok(None);
    }
    let body_len = usize::from(u16::from_be_bytes([raw[2], raw[3]]));
    if body_len > MAX_BODY_BYTES {
        return Err(DecodeError::UnsupportedSize);
    }
    match raw[1] {
        0x91 => {}
        kind @ (0xE0..=0xE5 | 0xEF) => return Err(DecodeError::Remote(kind)),
        kind => return Err(DecodeError::UnexpectedType(kind)),
    }
    let end = body_len + 4;
    if observed > end as u64 {
        return Err(DecodeError::TrailingData);
    }
    if raw.len() < end {
        return Ok(None);
    }
    let body = &raw[4..end];
    let separators = body.iter().filter(|&&byte| byte == b'\n').count();
    let trailing_lf = separators == 5 && body.last() == Some(&b'\n');
    if separators != 4 && !trailing_lf {
        return Err(DecodeError::FieldCount);
    }
    let text_end = end - usize::from(trailing_lf);
    let mut fields = [(0, 0); 5];
    let mut start = 4;
    for (index, field) in raw[4..text_end].split(|&byte| byte == b'\n').enumerate() {
        if field.len() > MAX_FIELD_BYTES {
            return Err(DecodeError::FieldTooLong);
        }
        let text = core::str::from_utf8(field).map_err(|_| DecodeError::InvalidUtf8)?;
        if text.chars().any(char::is_control) {
            return Err(DecodeError::ControlCharacter);
        }
        fields[index] = (start as u8, (start + field.len()) as u8);
        start += field.len() + 1;
    }
    Ok(Some(FieldLayout { fields, trailing_lf }))
}

/// Returns views of validated `fields` within `raw`, preserving `trailing_lf`.
/// Empty fields remain unavailable. Private callers supply only parser-produced
/// in-bounds ranges; invalid internal ranges would panic rather than read unchecked.
fn identity_view<'a>(
    raw: &'a [u8],
    fields: &[(u8, u8); 5],
    trailing_lf: bool,
) -> Identity<'a> {
    let field = |index: usize| {
        let (start, end) = fields[index];
        if start == end {
            None
        } else {
            core::str::from_utf8(&raw[usize::from(start)..usize::from(end)]).ok()
        }
    };
    Identity {
        vendor: field(0),
        model: field(1),
        hardware: field(2),
        serial: field(3),
        firmware: field(4),
        trailing_lf,
    }
}

/// Incremental one-frame decoder with a single raw buffer and borrowed text views.
/// Each push examines at most 245 retained bytes; suffixes fail without scanning.
pub struct FrameDecoder {
    /// Received stream prefix, retained without normalization for later explicit export.
    raw: [u8; MAX_FRAME_BYTES],
    /// Initialized prefix length, bounded by MAX_FRAME_BYTES.
    len: u8,
    /// Total bytes supplied before the first fault, saturating at u64::MAX.
    observed: u64,
    /// First error is sticky; further pushes do not alter evidence.
    error: Option<DecodeError>,
    /// Validated field ranges as absolute byte offsets into raw.
    fields: [(u8, u8); 5],
    /// True after all five fields were validated and no suffix was observed.
    complete: bool,
    /// Whether the successful body used one additional terminal LF.
    trailing_lf: bool,
}

impl Default for FrameDecoder {
    /// Returns a new empty decoder with no received bytes or fault.
    fn default() -> Self { Self::new() }
}

impl FrameDecoder {
    /// Returns an empty decoder without reading any source.
    pub const fn new() -> Self {
        Self {
            raw: [0; MAX_FRAME_BYTES],
            len: 0,
            observed: 0,
            error: None,
            fields: [(0, 0); 5],
            complete: false,
            trailing_lf: false,
        }
    }

    /// Appends `bytes` in stream order and returns framing progress or the first fault.
    /// Empty input is a no-op, not EOF. A complete frame plus any suffix fails in
    /// this same call; later nonempty input also revokes completion. At most 245
    /// prefix bytes are copied, with loss reported by `evidence_truncated`.
    pub fn push(
        &mut self,
        bytes: &[u8],
    ) -> Result<DecodeStatus, DecodeError> {
        if let Some(error) = self.error {
            return Err(error);
        }
        let was_complete = self.complete;
        self.observed = self.observed.saturating_add(bytes.len() as u64);
        let old_len = usize::from(self.len);
        let copied = bytes.len().min(MAX_FRAME_BYTES - old_len);
        self.raw[old_len..old_len + copied].copy_from_slice(&bytes[..copied]);
        self.len = (old_len + copied) as u8;
        let result = if was_complete && !bytes.is_empty() {
            Err(DecodeError::TrailingData)
        } else {
            self.parse()
        };
        if let Err(error) = result {
            self.error = Some(error);
            self.complete = false;
        }
        result
    }

    /// Ends the stream: returns Complete only for one valid whole frame, otherwise
    /// latches Truncated or preserves the existing fault. Does not discard evidence.
    pub fn finish(&mut self) -> Result<DecodeStatus, DecodeError> {
        if let Some(error) = self.error {
            return Err(error);
        }
        if self.complete {
            return Ok(DecodeStatus::Complete);
        }
        self.error = Some(DecodeError::Truncated);
        Err(DecodeError::Truncated)
    }

    /// Returns the retained received prefix; no bytes are synthesized or normalized.
    pub fn raw(&self) -> &[u8] { &self.raw[..usize::from(self.len)] }

    /// Returns bytes supplied before failure, saturating at u64::MAX.
    pub const fn observed_bytes(&self) -> u64 { self.observed }

    /// Returns true if supplied bytes exceeded the retained prefix capacity.
    pub const fn evidence_truncated(&self) -> bool { self.observed > MAX_FRAME_BYTES as u64 }

    /// Returns the big-endian declared body length once all four header bytes exist.
    /// This is evidence only and may exceed the accepted local limit.
    pub fn declared_body_bytes(&self) -> Option<u16> {
        (self.len >= 4).then(|| u16::from_be_bytes([self.raw[2], self.raw[3]]))
    }

    /// Returns the first decoder fault, or None while incomplete or valid.
    pub const fn error(&self) -> Option<DecodeError> { self.error }

    /// Returns cached whole-frame admission without constructing text views. Only
    /// successful parsing sets completion; a suffix or sticky fault revokes it.
    /// Session predicates use this exact [`Self::identity`] availability condition.
    const fn has_identity(&self) -> bool { self.complete && self.error.is_none() }

    /// Returns borrowed validated field text only while one whole frame remains valid.
    /// A later duplicate/suffix or any fault makes the identity unavailable.
    pub fn identity(&self) -> Option<Identity<'_>> {
        if !self.complete || self.error.is_some() {
            return None;
        }
        Some(identity_view(self.raw(), &self.fields, self.trailing_lf))
    }

    /// Validates the retained header/body, returning progress or a local protocol error.
    /// Publishes field offsets/completion only after all fields pass shared borrowed
    /// validation. No caller-controlled loop exceeds the 245-byte raw buffer.
    fn parse(&mut self) -> Result<DecodeStatus, DecodeError> {
        let Some(layout) = parse_layout(self.raw(), self.observed)? else {
            return Ok(DecodeStatus::NeedMore);
        };
        self.fields = layout.fields;
        self.trailing_lf = layout.trailing_lf;
        self.complete = true;
        Ok(DecodeStatus::Complete)
    }
}

#[cfg(test)]
/// Cached completion must agree with borrowed identity admission after every mutation.
mod completion_tests {
    use super::FrameDecoder;

    /// Prefixes, all byte mutations, EOF and sticky suffix failures retain identical admission.
    #[test]
    fn cached_completion_matches_identity_after_every_transition() {
        let valid = *b"\x1f\x91\0\x09A\nB\nC\nD\nE";
        for offset in 0..valid.len() {
            for byte in 0..=u8::MAX {
                let mut bytes = valid;
                bytes[offset] = byte;
                let mut decoder = FrameDecoder::new();
                assert!(!decoder.has_identity());
                for chunk in bytes.chunks(1) {
                    let _ = decoder.push(chunk);
                    assert_eq!(decoder.has_identity(), decoder.identity().is_some());
                }
                let _ = decoder.finish();
                assert_eq!(decoder.has_identity(), decoder.identity().is_some());
                let _ = decoder.push(b"suffix");
                assert!(!decoder.has_identity());
                assert!(decoder.identity().is_none());
                let _ = decoder.push(&valid);
                assert!(!decoder.has_identity());
                assert!(decoder.identity().is_none());
            }
        }
        for end in 0..valid.len() {
            let mut decoder = FrameDecoder::new();
            decoder.push(&valid[..end]).unwrap();
            assert!(!decoder.has_identity());
            assert!(decoder.finish().is_err());
            assert!(!decoder.has_identity());
            assert!(decoder.identity().is_none());
        }
    }
}
