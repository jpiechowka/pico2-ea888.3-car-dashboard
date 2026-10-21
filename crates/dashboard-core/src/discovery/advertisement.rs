//! Legacy advertising reports and bounded AD parsing without radio access.
//!
//! AD structures use length, type, then payload as specified by the
//! [Bluetooth Core Supplement, Part A](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/CSS_v14/out/en/core-supplementary-features/data-types-specification.html).
use core::fmt::Write;

use heapless::String;
/// Address plus address type; equal octets with different types are different peers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Peer {
    /// Six address octets in display order, most significant octet first.
    pub address: [u8; 6],
    /// `true` for a random address; `false` for a public address.
    pub random: bool,
}
impl Peer {
    /// Returns `self`'s address as 17 uppercase, colon-separated ASCII bytes.
    /// The address type is not included; this string alone is not a peer identity.
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

    /// Returns `self`'s address-type label, using the high two address bits for random addresses.
    /// A resolvable label does not mean that this application resolved the address.
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
/// Legacy controller report category used for catalogue admission.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReportKind {
    /// Undirected connectable advertisement; may create a catalogue entry.
    Connectable,
    /// Directed connectable advertisement; may create a catalogue entry.
    Directed,
    /// Scan response; can supplement an already admitted peer only.
    ScanResponse,
    /// Unsupported or nonconnectable report; ignored by the catalogue.
    Other,
}
/// Owned byte prefix with explicit truncation; stores at most `min(N, 255)` bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Payload<const N: usize> {
    /// Storage; only the prefix selected by `len` is meaningful.
    pub bytes: [u8; N],
    /// Recorded prefix length in bytes; readers also clamp this to `N`.
    pub len: u8,
    /// Source bytes were omitted; may also propagate upstream truncation.
    pub truncated: bool,
}
impl<const N: usize> Payload<N> {
    /// Returns zeroed storage with length zero and no truncation evidence.
    pub const fn empty() -> Self {
        Self {
            bytes: [0; N],
            len: 0,
            truncated: false,
        }
    }

    /// Copies `bytes` into a returned owned prefix of at most `min(N, 255)` bytes.
    /// Sets `truncated` exactly when the source exceeds that capacity.
    pub fn new(bytes: &[u8]) -> Self {
        let mut result = Self::empty();
        let len = bytes.len().min(N).min(255);
        result.bytes[..len].copy_from_slice(&bytes[..len]);
        result.len = len as u8;
        result.truncated = len < bytes.len();
        result
    }

    /// Returns `self`'s borrowed meaningful prefix; clamps even externally mutated `len` to storage.
    pub fn as_slice(&self) -> &[u8] { &self.bytes[..usize::from(self.len).min(N)] }
}
/// One controller report with its original identity, signal strength and receipt time.
#[derive(Clone, Copy, Debug)]
pub struct Advertisement {
    /// Address and address type observed on this report.
    pub peer: Peer,
    /// Controller report category, not inferred from its payload.
    pub kind: ReportKind,
    /// Raw RSSI in dBm; catalogue admission maps values outside -127..=20 to unavailable.
    pub rssi: i8,
    /// Legacy advertising data, truncated explicitly above 31 bytes.
    pub data: Payload<31>,
    /// Boot-relative monotonic report time in milliseconds.
    pub at_ms: u64,
}
impl Advertisement {
    /// Returns a report for `peer`, controller `kind`, raw dBm `rssi`, and boot-relative
    /// `at_ms` milliseconds. Copies `bytes`, retaining at most 31 with truncation evidence.
    /// Identity, RSSI and time are preserved without qualification here.
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
/// Borrowed payload of one well-framed AD structure.
#[derive(Debug, PartialEq, Eq)]
pub struct AdField<'a> {
    /// Assigned AD type octet, excluding the length byte.
    pub kind: u8,
    /// AD payload excluding length and type; may be empty.
    pub data: &'a [u8],
}
/// Forward-only AD iterator; malformed lengths yield one error and terminate.
pub struct AdFields<'a> {
    /// Unconsumed packet suffix; cleared on padding or malformed framing.
    remaining: &'a [u8],
}
impl<'a> AdFields<'a> {
    /// Returns an iterator borrowing `remaining` without copying or validating ahead.
    pub fn new(remaining: &'a [u8]) -> Self { Self { remaining } }
}
impl<'a> Iterator for AdFields<'a> {
    /// A framed borrowed field, or `Err(())` for a length exceeding the remaining packet.
    type Item = Result<AdField<'a>, ()>;

    /// Consumes the next field from `self`; returns `None` at EOF or zero-length padding.
    /// A malformed length returns `Some(Err(()))` once, then permanently exhausts the iterator.
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

/// Returns at most `N` display bytes from `bytes`, replacing bytes outside ASCII 32..=126
/// with `?`. Stops when the fixed-capacity string fills, without a truncation marker.
/// Bitmap fonts are ASCII; callers retain raw evidence separately as hex.
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
