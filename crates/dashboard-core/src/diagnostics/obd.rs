//! Reviewed generic Mode01 subset. Wideband/MAP scaling and manufacturer DIDs
//! remain unqualified; no guessed conversion can produce a live sample here.
use crate::samples::Quantity;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Support {
    Unknown,
    Supported,
    Unsupported,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CapabilityError {
    Source,
    Page,
    Unadvertised,
    Changed,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Capabilities {
    ecu: u16,
    epoch: u64,
    pages: [Option<u32>; 3],
}
impl Capabilities {
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

    pub const fn ecu(&self) -> u16 { self.ecu }

    pub const fn epoch(&self) -> u64 { self.epoch }

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecodeError {
    Length,
    UnqualifiedPid,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Decoded {
    Quantity(Quantity),
    Supported { base: u8, bits: u32 },
}
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
