use dashboard_helper_protocol::*;
/// Trusted native ingestion data. Process output must be produced by StreamFramer;
/// no raw pipe or control-frame bytes may bypass that single sanitizer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LogInput {
    pub stage: StageId,
    pub source: Source,
    pub text: String,
    pub progress: bool,
}
#[derive(Clone, Copy)]
enum Escape {
    Text,
    Esc,
    EscIntermediate,
    Csi,
    String,
    StringEsc,
}
/// One stream's bounded decoder/sanitizer/redactor. No Debug: secret patterns are private.
pub struct StreamFramer {
    secrets: Vec<[u8; 64]>,
    utf8: Vec<u8>,
    escape: Escape,
    lookbehind: String,
    line: String,
    last_cr: bool,
    closed: bool,
}
impl StreamFramer {
    /// Register on every associated stream before delivering a newly generated nonce.
    /// Existing decoder/redactor state is retained; patterns live through output closure.
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

    pub fn new(patterns: &[&str]) -> Result<Self, Error> {
        if patterns.len() > 3 {
            return Err(Error::Bounds);
        }
        let mut secrets = Vec::new();
        for s in patterns {
            if s.len() != 64 || !s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) {
                return Err(Error::Schema);
            }
            let mut pattern = [0; 64];
            pattern.copy_from_slice(s.as_bytes());
            if secrets.contains(&pattern) {
                return Err(Error::Schema);
            }
            secrets.push(pattern);
        }
        Ok(Self {
            secrets,
            utf8: Vec::with_capacity(4),
            escape: Escape::Text,
            lookbehind: String::with_capacity(68),
            line: String::with_capacity(256),
            last_cr: false,
            closed: false,
        })
    }

    pub fn retained_bytes(&self) -> usize { self.utf8.len() + self.lookbehind.len() + self.line.len() }

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
