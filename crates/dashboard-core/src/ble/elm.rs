//! Incremental ELM ASCII framing and explicitly configured raw CAN replies.
use heapless::Vec;

use crate::diagnostics::CanFrame;

/// Maximum printable bytes in one ELM line, excluding CR/LF and prompt.
pub const LINE_CAPACITY: usize = 64;
/// Maximum reassembled diagnostic payload bytes, including positive/negative reply prefix.
pub const PAYLOAD_CAPACITY: usize = 64;
/// Owned bounded printable line without its terminator.
pub type Line = Vec<u8, LINE_CAPACITY>;
/// Owned bounded ISO-TP diagnostic payload, without CAN/PCI framing.
pub type Payload = Vec<u8, PAYLOAD_CAPACITY>;

/// Bounded framing/reassembly classification shared by the parser and session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// Line, notification or reassembly capacity was exceeded.
    Overflow,
    /// Stream contained a byte outside permitted printable ASCII/control delimiters.
    Ascii,
    /// Prompt placement, CAN grammar or terminal parser state is invalid.
    Frame,
    /// ISO-TP consecutive-frame ordering disagrees with the expected nibble.
    Sequence,
    /// A CAN reply came from an identifier other than the expected engine source.
    Foreign,
    /// Prompt arrived before a complete acceptable reply.
    Incomplete,
}

/// Result of consuming one ASCII stream byte.
#[derive(Debug, PartialEq, Eq)]
pub enum Event {
    /// No complete line or prompt yet, including empty CR/LF separators.
    None,
    /// Nonempty line completed by CR or LF.
    Line(
        /// Owned line bytes with no terminator.
        Line,
    ),
    /// Prompt at an empty line boundary; session still validates reply completeness.
    Prompt,
}

/// Incremental ASCII line/prompt parser that permanently poisons itself after an error.
#[derive(Default)]
pub struct Parser {
    /// Current printable line prefix, at most 64 bytes.
    line: Line,
    /// Any previous parse/capacity error; only replacing the parser clears it.
    failed: bool,
}
impl Parser {
    /// Returns an empty unfailed parser without stream state from an earlier transaction.
    pub const fn new() -> Self {
        Self {
            line: Line::new(),
            failed: false,
        }
    }

    /// Consumes `byte` into `self`, returning a complete Line, boundary Prompt or None.
    /// Only ASCII 0x20..=0x7E plus CR/LF delimiters is accepted; `>` requires an empty
    /// line. Overflow, misplaced prompt or non-ASCII clears the line and poisons the
    /// parser; subsequent calls return Frame until a fresh parser replaces it.
    pub fn push(
        &mut self,
        byte: u8,
    ) -> Result<Event, Error> {
        if self.failed {
            return Err(Error::Frame);
        }
        let result = match byte {
            b'\r' | b'\n' if self.line.is_empty() => Ok(Event::None),
            b'\r' | b'\n' => Ok(Event::Line(core::mem::take(&mut self.line))),
            b'>' if self.line.is_empty() => Ok(Event::Prompt),
            b'>' => Err(Error::Frame),
            0x20..=0x7E => self.line.push(byte).map(|()| Event::None).map_err(|_| Error::Overflow),
            _ => Err(Error::Ascii),
        };
        if result.is_err() {
            self.failed = true;
            self.line.clear();
        }
        result
    }
}

/// Returns a validated 11-bit classic CAN frame parsed from bounded `line`: three hex
/// identifier digits, one hex DLC digit 0..8, and exactly DLC two-digit octets separated
/// by ASCII spaces. Repeated/leading/trailing spaces are allowed; hex case is ignored.
/// Length above 64 returns Overflow; missing/extra tokens, invalid hex, DLC or identifier
/// returns Frame. This requires the session's explicitly configured headers/DLC/raw mode.
pub fn frame(line: &[u8]) -> Result<CanFrame, Error> {
    if line.len() > LINE_CAPACITY {
        return Err(Error::Overflow);
    }
    let mut parts = line.split(|b| *b == b' ').filter(|part| !part.is_empty());
    let header = parts.next().ok_or(Error::Frame)?;
    if header.len() != 3 {
        return Err(Error::Frame);
    }
    let id = (u16::from(hex(header[0])?) << 8) | (u16::from(hex(header[1])?) << 4) | u16::from(hex(header[2])?);
    let dlc = parts.next().ok_or(Error::Frame)?;
    if dlc.len() != 1 {
        return Err(Error::Frame);
    }
    let dlc = usize::from(hex(dlc[0])?);
    if dlc > 8 {
        return Err(Error::Frame);
    }
    let mut data = [0; 8];
    for byte in data.iter_mut().take(dlc) {
        let part = parts.next().ok_or(Error::Frame)?;
        if part.len() != 2 {
            return Err(Error::Frame);
        }
        *byte = (hex(part[0])? << 4) | hex(part[1])?;
    }
    if parts.next().is_some() {
        return Err(Error::Frame);
    }
    CanFrame::new(id, &data[..dlc]).map_err(|_| Error::Frame)
}

/// Returns nibble 0..15 for ASCII hex `byte`, accepting either letter case;
/// all other bytes return Frame without fallback conversion.
fn hex(byte: u8) -> Result<u8, Error> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        _ => Err(Error::Frame),
    }
}

#[cfg(test)]
/// Stream poison, prompt-boundary and strict raw-CAN grammar regressions.
mod tests {
    use super::*;

    #[test]
    /// Returns unit after checking line buffering, CRLF handling and separate prompt
    /// publication; panics if incomplete lines publish or framing bytes enter CAN data.
    fn stream_waits_for_line_boundaries_and_preserves_prompt() {
        let mut parser = Parser::new();
        for byte in b"7E8 8 03 41 05 82 00 00 00 00" {
            assert_eq!(parser.push(*byte), Ok(Event::None));
        }
        let Ok(Event::Line(line)) = parser.push(b'\r') else {
            panic!("missing line")
        };
        assert_eq!(frame(&line).unwrap().data(), &[3, 0x41, 5, 130, 0, 0, 0, 0]);
        assert_eq!(parser.push(b'\n'), Ok(Event::None));
        assert_eq!(parser.push(b'>'), Ok(Event::Prompt));
    }

    #[test]
    /// Returns unit after checking exact line capacity and non-ASCII poison retention;
    /// panics if a delimiter can recover a failed transaction parser.
    fn overflow_and_non_ascii_poison_parser() {
        let mut parser = Parser::new();
        for _ in 0..64 {
            assert_eq!(parser.push(b'A'), Ok(Event::None));
        }
        assert_eq!(parser.push(b'A'), Err(Error::Overflow));
        assert!(parser.push(b'\r').is_err());
        let mut parser = Parser::new();
        assert_eq!(parser.push(0xFF), Err(Error::Ascii));
        assert!(parser.push(b'>').is_err());
    }

    #[test]
    /// Returns unit after rejecting absent headers/DLC, extended identifiers, width/count
    /// errors and bad hex while admitting lowercase; panics on grammar drift.
    fn headers_dlc_and_exact_octets_are_mandatory() {
        for line in [
            b"41 05 82".as_slice(),
            b"7E8 8 03 41 05 82",
            b"18DAF110 4 03 41 05 82",
            b"7E8 4 03 41 5 82",
            b"7E8 9 03 41 05 82",
            b"7E8 4 03 41 05 GG",
        ] {
            assert!(frame(line).is_err(), "{:?}", line);
        }
        assert_eq!(frame(b"7e8 4 03 41 05 82").unwrap().id(), 0x7E8);
    }

    #[test]
    /// Returns unit after confirming the standalone CAN parser rejects oversized input
    /// before scanning its whitespace; panics if stream-level bounds are required externally.
    fn standalone_frame_decoder_also_bounds_input_work() {
        let mut line = [b' '; 1024];
        line[..17].copy_from_slice(b"7E8 4 03 41 05 82");
        assert_eq!(frame(&line), Err(Error::Overflow));
    }
}
