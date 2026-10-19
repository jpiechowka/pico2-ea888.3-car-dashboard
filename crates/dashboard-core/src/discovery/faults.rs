//! Stable, bounded LED evidence. Codes are also used by capture schema v3.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Fault {
    Probe = 1,
    Scan = 2,
    Capacity = 3,
    Storage = 4,
    Usb = 5,
    Encoder = 6,
    Timing = 7,
    Watchdog = 8,
    Advertisement = 9,
}
impl Fault {
    pub const ALL: [Self; 9] = [
        Self::Probe,
        Self::Scan,
        Self::Capacity,
        Self::Storage,
        Self::Usb,
        Self::Encoder,
        Self::Timing,
        Self::Watchdog,
        Self::Advertisement,
    ];

    pub const fn bit(self) -> u16 { 1 << (self as u8 - 1) }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Probe => "probe / GATT",
            Self::Scan => "HCI scan report",
            Self::Capacity => "capacity / dropped evidence",
            Self::Storage => "capture storage",
            Self::Usb => "USB export",
            Self::Encoder => "encoder I2C",
            Self::Timing => "display completion timeline",
            Self::Watchdog => "watchdog timer reset",
            Self::Advertisement => "advertisement data",
        }
    }

    pub const fn scan_only(self) -> bool { matches!(self, Self::Scan | Self::Advertisement) }

    pub const fn header(self) -> &'static str {
        match self {
            Self::Probe => "Red: probe error",
            Self::Capacity => "Red: capacity",
            Self::Storage => "Red: storage",
            Self::Usb => "Red: USB export",
            Self::Encoder => "Red: encoder I2C",
            Self::Timing => "Red: timing",
            Self::Watchdog => "Red: watchdog reset",
            Self::Scan | Self::Advertisement => "Slow red: scan errors",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorLatch {
    None,
    ScanOnly,
    Other,
}

/// First means atomic recording order, not a timestamp or a root-cause claim.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FaultLatch {
    bits: u16,
    first: Option<Fault>,
}
impl FaultLatch {
    pub const EMPTY: Self = Self { bits: 0, first: None };

    pub const fn single(fault: Fault) -> Self {
        Self {
            bits: fault.bit(),
            first: Some(fault),
        }
    }

    pub fn from_parts(
        bits: u16,
        first_code: u8,
    ) -> Option<Self> {
        if bits == 0 {
            return (first_code == 0).then_some(Self::EMPTY);
        }
        let first = *Fault::ALL.get(usize::from(first_code.checked_sub(1)?))?;
        (bits & !0x1FF == 0 && bits & first.bit() != 0).then_some(Self {
            bits,
            first: Some(first),
        })
    }

    pub const fn bits(self) -> u16 { self.bits }

    pub const fn first(self) -> Option<Fault> { self.first }

    pub const fn contains(
        self,
        fault: Fault,
    ) -> bool {
        self.bits & fault.bit() != 0
    }

    pub const fn category(self) -> ErrorLatch {
        if self.bits == 0 {
            ErrorLatch::None
        } else if self.bits & !(Fault::Scan.bit() | Fault::Advertisement.bit()) == 0 {
            ErrorLatch::ScanOnly
        } else {
            ErrorLatch::Other
        }
    }

    pub fn header(self) -> &'static str {
        // A steady-red source takes precedence over scan-only blinking.
        self.first
            .filter(|f| !f.scan_only())
            .or_else(|| Fault::ALL.into_iter().find(|f| self.contains(*f) && !f.scan_only()))
            .or(self.first)
            .map_or("Error:None", Fault::header)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_masks_validate_first_membership_and_reserved_bits() {
        for bits in 0..=0x3FF {
            for code in 0..=10 {
                let expected = if bits == 0 {
                    code == 0
                } else {
                    bits <= 0x1FF && (1..=9).contains(&code) && bits & (1 << (code - 1)) != 0
                };
                assert_eq!(FaultLatch::from_parts(bits, code).is_some(), expected);
            }
        }
        assert!(FaultLatch::from_parts(u16::MAX, u8::MAX).is_none());
    }
}
