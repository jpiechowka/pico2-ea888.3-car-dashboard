//! Reviewed generic Mode01 subset. Wideband/MAP scaling and manufacturer DIDs
//! remain unqualified; no guessed conversion can produce a live sample here.
//! Conversion cross-checks: [python-OBD temperature decoder](https://github.com/brendan-w/python-OBD/blob/a378bdd81d58c67d08050e4244173a9a7dbda73d/obd/decoders.py)
//! and [ELMduino Mode01 voltage implementation](https://github.com/PowerBroker2/ELMduino/blob/e9d6b55fd63f4afd7789d0981861e9990cdfcd8e/src/ELMduino.cpp).
//! These implementation references do not qualify any particular ECU.
use crate::samples::Quantity;

/// Capability evidence, keeping absent responses distinct from an explicit cleared bit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Support {
    /// Required page has not been received or lies beyond the retained pages.
    Unknown,
    /// The admitted responder advertised the PID.
    Supported,
    /// An admitted bitmap clears the PID or a prerequisite continuation bit.
    Unsupported,
}
/// Rejected capability evidence; existing pages remain unchanged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CapabilityError {
    /// Invalid OBD responder identifier, or evidence from another responder/epoch.
    Source,
    /// Bitmap base is not 0x00, 0x20 or 0x40.
    Page,
    /// Previous pages have not positively advertised this continuation page.
    Unadvertised,
    /// A previously recorded page changed within the same epoch.
    Changed,
}
/// Three immutable-once-recorded Mode 01 capability pages for one responder/epoch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Capabilities {
    /// Standard OBD response identifier in 0x7E8..=0x7EF.
    ecu: u16,
    /// Caller-owned transaction/session generation; no numeric value is reserved here.
    epoch: u64,
    /// Big-endian decoded bitmaps for bases 0x00, 0x20 and 0x40; None means absent.
    pages: [Option<u32>; 3],
}
impl Capabilities {
    /// Returns empty evidence bound to `ecu` in 0x7E8..=0x7EF and opaque `epoch`,
    /// or Source for an identifier outside that responder range.
    pub fn new(
        ecu: u16,
        epoch: u64,
    ) -> Result<Self, CapabilityError> {
        if !(0x7E8..=0x7EF).contains(&ecu) {
            return Err(CapabilityError::Source);
        }
        Ok(Self {
            ecu,
            epoch,
            pages: [None; 3],
        })
    }

    /// Returns the fixed eleven-bit responder identifier.
    pub const fn ecu(&self) -> u16 { self.ecu }

    /// Returns the caller generation to which all recorded pages belong.
    pub const fn epoch(&self) -> u64 { self.epoch }

    /// Records `bits` for `base` (0x00/0x20/0x40) from exact `ecu`/`epoch`.
    /// Bit31 advertises base+1 and bit0 base+32. Returns success for first admission
    /// or an identical repeat; wrong source, missing continuation or changed evidence
    /// returns an error without mutation. This does not authorize vehicle requests.
    pub fn record_page(
        &mut self,
        ecu: u16,
        epoch: u64,
        base: u8,
        bits: u32,
    ) -> Result<(), CapabilityError> {
        if ecu != self.ecu || epoch != self.epoch {
            return Err(CapabilityError::Source);
        }
        let index = match base {
            0 => 0,
            0x20 => 1,
            0x40 => 2,
            _ => return Err(CapabilityError::Page),
        };
        if index > 0 && self.supported(base) != Support::Supported {
            return Err(CapabilityError::Unadvertised);
        }
        if self.pages[index].is_some_and(|previous| previous != bits) {
            return Err(CapabilityError::Changed);
        }
        self.pages[index] = Some(bits);
        Ok(())
    }

    /// Returns evidence for `pid`: explicit bits/cleared continuation imply Supported/
    /// Unsupported, missing pages imply Unknown. PID zero is Supported only once page
    /// zero exists; silence is never interpreted as Unsupported.
    pub fn supported(
        &self,
        pid: u8,
    ) -> Support {
        if pid == 0 {
            return if self.pages[0].is_some() {
                Support::Supported
            } else {
                Support::Unknown
            };
        }
        // Page boundaries describe the previous page's bit0, not shift32 in
        // the next page. A missing continuation bit rules out later pages.
        let page = usize::from((pid - 1) / 32);
        for previous in self.pages.iter().take(page) {
            match previous {
                Some(bits) if bits & 1 == 0 => return Support::Unsupported,
                None => return Support::Unknown,
                _ => {}
            }
        }
        let Some(Some(bits)) = self.pages.get(page) else {
            return Support::Unknown;
        };
        let offset = (pid - 1) % 32;
        if bits & (1 << (31 - offset)) != 0 {
            Support::Supported
        } else {
            Support::Unsupported
        }
    }
}

/// Generic decoder rejection; no quantity is produced on error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecodeError {
    /// Data width differs from the selected PID's exact width.
    Length,
    /// PID has no reviewed conversion in this restricted decoder.
    UnqualifiedPid,
}
/// Canonical quantity or unbound bitmap; source/freshness admission remains with the caller.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Decoded {
    /// Temperature or module voltage decoded into canonical units.
    Quantity(
        /// Celsius or volts, as identified by the quantity variant.
        Quantity,
    ),
    /// Capability page requiring source/epoch admission before use.
    Supported {
        /// Page PID: 0x00, 0x20 or 0x40.
        base: u8,
        /// Big-endian bitmap: bit31 is base+1 and bit0 base+32.
        bits: u32,
    },
}
/// Decodes Mode 01 `pid` from application data `bytes`, excluding service/PID headers.
/// Returns Celsius for 0x05/0x0F/0x5C (one byte minus40, -40..=215), volts for
/// 0x42 (two-byte big-endian value /1000, 0..=65.535), or a four-byte capability
/// bitmap for 0x00/0x20/0x40. Unknown PIDs fail before width checking; known PIDs
/// reject extra or missing bytes. Decoding alone proves neither ECU support nor freshness.
pub fn decode(
    pid: u8,
    bytes: &[u8],
) -> Result<Decoded, DecodeError> {
    let width = match pid {
        5 | 0x0F | 0x5C => 1,
        0x42 => 2,
        0 | 0x20 | 0x40 => 4,
        _ => return Err(DecodeError::UnqualifiedPid),
    };
    if bytes.len() != width {
        return Err(DecodeError::Length);
    }
    Ok(match pid {
        5 => Decoded::Quantity(Quantity::CoolantC(f32::from(bytes[0]) - 40.0)),
        0x0F => Decoded::Quantity(Quantity::IntakeC(f32::from(bytes[0]) - 40.0)),
        0x5C => Decoded::Quantity(Quantity::EngineOilC(f32::from(bytes[0]) - 40.0)),
        0x42 => {
            Decoded::Quantity(Quantity::ModuleVolts(
                f32::from(u16::from_be_bytes([bytes[0], bytes[1]])) / 1000.0,
            ))
        }
        _ => {
            Decoded::Supported {
                base: pid,
                bits: u32::from_be_bytes(bytes.try_into().unwrap()),
            }
        }
    })
}
