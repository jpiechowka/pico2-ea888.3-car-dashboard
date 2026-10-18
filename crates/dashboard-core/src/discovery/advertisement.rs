use core::fmt::Write;

use heapless::String;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Peer {
    pub address: [u8; 6],
    pub random: bool,
}
impl Peer {
    pub fn to_string(self) -> String<17> {
        let mut text = String::new();
        for (i, byte) in self.address.iter().enumerate() {
            if i != 0 {
                let _ = text.push(':');
            }
            let _ = write!(text, "{byte:02X}");
        }
        text
    }

    pub fn kind_label(self) -> &'static str {
        if !self.random {
            return "Public";
        }
        match self.address[0] >> 6 {
            3 => "Random static",
            1 => "Random resolvable",
            0 => "Random private",
            _ => "Random reserved",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReportKind {
    Connectable,
    Directed,
    ScanResponse,
    Other,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Payload<const N: usize> {
    pub bytes: [u8; N],
    pub len: u8,
    pub truncated: bool,
}
impl<const N: usize> Payload<N> {
    pub const fn empty() -> Self {
        Self {
            bytes: [0; N],
            len: 0,
            truncated: false,
        }
    }

    pub fn new(bytes: &[u8]) -> Self {
        let mut result = Self::empty();
        let len = bytes.len().min(N).min(255);
        result.bytes[..len].copy_from_slice(&bytes[..len]);
        result.len = len as u8;
        result.truncated = len < bytes.len();
        result
    }

    pub fn as_slice(&self) -> &[u8] { &self.bytes[..usize::from(self.len).min(N)] }
}
#[derive(Clone, Copy, Debug)]
pub struct Advertisement {
    pub peer: Peer,
    pub kind: ReportKind,
    pub rssi: i8,
    pub data: Payload<31>,
    pub at_ms: u64,
}
impl Advertisement {
    pub fn new(
        peer: Peer,
        kind: ReportKind,
        rssi: i8,
        bytes: &[u8],
        at_ms: u64,
    ) -> Self {
        Self {
            peer,
            kind,
            rssi,
            data: Payload::new(bytes),
            at_ms,
        }
    }
}
#[derive(Debug, PartialEq, Eq)]
pub struct AdField<'a> {
    pub kind: u8,
    pub data: &'a [u8],
}
pub struct AdFields<'a> {
    remaining: &'a [u8],
}
impl<'a> AdFields<'a> {
    pub fn new(remaining: &'a [u8]) -> Self { Self { remaining } }
}
impl<'a> Iterator for AdFields<'a> {
    type Item = Result<AdField<'a>, ()>;

    fn next(&mut self) -> Option<Self::Item> {
        let (&length, tail) = self.remaining.split_first()?;
        if length == 0 {
            self.remaining = &[];
            return None;
        }
        if usize::from(length) > tail.len() {
            self.remaining = &[];
            return Some(Err(()));
        }
        let (field, remaining) = tail.split_at(usize::from(length));
        self.remaining = remaining;
        Some(Ok(AdField {
            kind: field[0],
            data: &field[1..],
        }))
    }
}

/// Bitmap fonts are ASCII; raw bytes remain separately available as hex.
pub fn printable<const N: usize>(bytes: &[u8]) -> String<N> {
    let mut out = String::new();
    for &byte in bytes {
        if out
            .push(if (32..=126).contains(&byte) {
                char::from(byte)
            } else {
                '?'
            })
            .is_err()
        {
            break;
        }
    }
    out
}
