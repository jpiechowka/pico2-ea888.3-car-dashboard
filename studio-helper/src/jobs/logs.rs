//! Bounded incremental UTF-8 decoding, terminal-control removal, secret redaction and log framing.
use dashboard_helper_protocol::*;
/// Trusted native ingestion data. Process output must be produced by StreamFramer;
/// no raw pipe or control-frame bytes may bypass that single sanitizer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LogInput {
    /// Job stage owning this already-sanitized message.
    pub stage: StageId,
    /// stdout, stderr or helper-produced source classification.
    pub source: Source,
    /// Sanitized bounded text; native tool output must pass through StreamFramer first.
    pub text: String,
    /// Carriage-return progress update rather than an ordinary newline record.
    pub progress: bool,
}
#[derive(Clone, Copy)]
/// Incremental terminal escape parser, retained across raw chunks so split controls cannot bypass sanitization.
enum Escape {
    /// Ordinary printable content or a new escape introducer.
    Text,
    /// ESC received, awaiting its sequence discriminator.
    Esc,
    /// ESC intermediate-byte sequence, consumed through its final byte.
    EscIntermediate,
    /// CSI control sequence, consumed through its final byte.
    Csi,
    /// OSC/DCS/SOS/PM/APC string, consumed through BEL or string terminator.
    String,
    /// ESC inside a control string, awaiting backslash termination.
    StringEsc,
}
/// One stream's bounded decoder/sanitizer/redactor. No Debug: secret patterns are private.
pub struct StreamFramer {
    /// At most three exact 64-byte lowercase hexadecimal secret patterns, excluded from Debug output.
    secrets: Vec<[u8; 64]>,
    /// Pending incomplete UTF-8 scalar bytes, at most the decoder's four-byte scalar bound.
    utf8: Vec<u8>,
    /// Current escape-removal state.
    escape: Escape,
    /// Sanitized characters withheld to detect secrets split across chunks/lines, normally below 64 bytes.
    lookbehind: String,
    /// Current sanitized line fragment, at most 256 UTF-8 bytes.
    line: String,
    /// Previous framed character was CR, so a following LF is suppressed as the same delimiter.
    last_cr: bool,
    /// finish completed; further pushes, registrations and finish calls are rejected.
    closed: bool,
}
impl StreamFramer {
    /// Register on every associated stream before delivering a newly generated nonce.
    /// Existing decoder/redactor state is retained; patterns live through output closure.
    /// Returns success for unique lowercase 64-hex-byte `s`; closed, fourth-pattern and malformed/duplicate inputs fail
    /// without changing registered patterns. Already-emitted text cannot be retroactively redacted.
    pub fn register_secret(
        &mut self,
        s: &str,
    ) -> Result<(), Error> {
        if self.closed {
            return Err(Error::Closed);
        }
        if self.secrets.len() == 3 {
            return Err(Error::Bounds);
        }
        if s.len() != 64 || !s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) {
            return Err(Error::Schema);
        }
        let mut pattern = [0; 64];
        pattern.copy_from_slice(s.as_bytes());
        if self.secrets.contains(&pattern) {
            return Err(Error::Schema);
        }
        self.secrets.push(pattern);
        Ok(())
    }

    /// Returns an open framer with at most three validated unique `patterns`; malformed/duplicate/bounds errors reject
    /// construction.
    pub fn new(patterns: &[&str]) -> Result<Self, Error> {
        if patterns.len() > 3 {
            return Err(Error::Bounds);
        }
        let mut frame = Self {
            secrets: Vec::new(),
            utf8: Vec::with_capacity(4),
            escape: Escape::Text,
            lookbehind: String::with_capacity(68),
            line: String::with_capacity(256),
            last_cr: false,
            closed: false,
        };
        for secret in patterns {
            frame.register_secret(secret)?;
        }
        Ok(frame)
    }

    /// Returns pending decoder/lookbehind/line byte lengths, excluding allocated capacity and secret patterns.
    pub fn retained_bytes(&self) -> usize { self.utf8.len() + self.lookbehind.len() + self.line.len() }

    /// Consumes up to 4096 raw `bytes`, delivering bounded owned `(text, progress)` fragments through `emit`.
    /// Returns closed/bounds errors before consuming input; malformed UTF-8 becomes replacement characters,
    /// incomplete scalars/escapes remain pending. Callback panics propagate after partial state/output effects.
    pub fn push(
        &mut self,
        bytes: &[u8],
        mut emit: impl FnMut(String, bool),
    ) -> Result<(), Error> {
        if self.closed {
            return Err(Error::Closed);
        }
        if bytes.len() > 4096 {
            return Err(Error::Bounds);
        }
        for &byte in bytes {
            self.utf8.push(byte);
            loop {
                match std::str::from_utf8(&self.utf8) {
                    Ok(s) => {
                        let chars = s.to_owned();
                        self.utf8.clear();
                        for c in chars.chars() {
                            self.sanitize(c, &mut emit);
                        }
                        break;
                    }
                    Err(e) => {
                        match e.error_len() {
                            None => break,
                            Some(n) => {
                                self.utf8.drain(..n);
                                self.sanitize('\u{fffd}', &mut emit);
                                if self.utf8.is_empty() {
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// Removes terminal escapes/control characters from decoded `c`, forwarding printable text and CR/LF/tab to `emit`
    /// through redaction/framing. Escape state survives chunk boundaries and unclosed strings suppress following
    /// payload.
    fn sanitize(
        &mut self,
        c: char,
        emit: &mut impl FnMut(String, bool),
    ) {
        use Escape::*;
        match self.escape {
            Esc => {
                self.escape = match c {
                    '[' => Csi,
                    ']' | 'P' | 'X' | '^' | '_' => String,
                    '\u{20}'..='\u{2f}' => EscIntermediate,
                    _ => Text,
                };
                return;
            }
            EscIntermediate => {
                if ('\u{30}'..='\u{7e}').contains(&c) {
                    self.escape = Text;
                }
                return;
            }
            Csi => {
                if ('\u{40}'..='\u{7e}').contains(&c) {
                    self.escape = Text;
                }
                return;
            }
            String => {
                if c == '\u{7}' || c == '\u{9c}' {
                    self.escape = Text;
                } else if c == '\u{1b}' {
                    self.escape = StringEsc;
                }
                return;
            }
            StringEsc => {
                self.escape = if c == '\\' { Text } else { String };
                return;
            }
            Text => {}
        }
        match c {
            '\u{1b}' => self.escape = Esc,
            '\u{9b}' => self.escape = Csi,
            '\u{90}' | '\u{98}' | '\u{9d}' | '\u{9e}' | '\u{9f}' => self.escape = String,
            '\r' | '\n' | '\t' => self.redact(c, emit),
            _ if c.is_control() => {}
            _ => self.redact(c, emit),
        }
    }

    /// Checks sanitized `c` against registered secret prefixes before framing output through `emit`.
    /// Keeps a bounded suffix for cross-chunk matches and substitutes `[redacted]` for each complete exact pattern.
    fn redact(
        &mut self,
        c: char,
        emit: &mut impl FnMut(String, bool),
    ) {
        if self.secrets.is_empty() {
            self.frame(c, emit);
            return;
        }
        self.lookbehind.push(c);
        while self.lookbehind.len() >= 64 {
            if self.secrets.iter().any(|p| self.lookbehind.as_bytes().starts_with(p)) {
                self.lookbehind.drain(..64);
                for c in "[redacted]".chars() {
                    self.frame(c, emit);
                }
            } else {
                let c = self.lookbehind.chars().next().expect("nonempty bounded lookbehind");
                self.lookbehind.drain(..c.len_utf8());
                self.frame(c, emit);
            }
        }
    }

    /// Adds sanitized/redacted `c` to a <=256-byte UTF-8 fragment, delivering completed fragments through `emit`.
    /// CR emits progress, LF emits ordinary text, CRLF emits once; empty delimiter-only lines are omitted.
    fn frame(
        &mut self,
        c: char,
        emit: &mut impl FnMut(String, bool),
    ) {
        if c == '\n' && self.last_cr {
            self.last_cr = false;
            return;
        }
        self.last_cr = c == '\r';
        if c == '\r' || c == '\n' {
            if !self.line.is_empty() {
                emit(std::mem::take(&mut self.line), c == '\r');
            }
            return;
        }
        if self.line.len() + c.len_utf8() > 256 {
            emit(std::mem::take(&mut self.line), false);
        }
        self.line.push(c);
    }

    /// Flushes pending decoder/redaction/line state through `emit`, then closes the stream; returns Closed if already
    /// finished. A partial UTF-8 scalar becomes replacement text unless inside a discarded escape sequence; a
    /// partial secret prefix is emitted because it cannot form a complete registered secret. Callback panics can
    /// leave partial output/state.
    pub fn finish(
        &mut self,
        mut emit: impl FnMut(String, bool),
    ) -> Result<(), Error> {
        if self.closed {
            return Err(Error::Closed);
        }
        if !self.utf8.is_empty() {
            self.utf8.clear();
            self.sanitize('\u{fffd}', &mut emit);
        }
        let pending = std::mem::take(&mut self.lookbehind);
        for c in pending.chars() {
            self.frame(c, &mut emit);
        }
        if !self.line.is_empty() {
            emit(std::mem::take(&mut self.line), false);
        }
        self.closed = true;
        Ok(())
    }
}
