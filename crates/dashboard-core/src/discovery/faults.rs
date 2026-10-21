//! Stable, bounded LED evidence. Codes are also used by capture schema v3.
/// Stable fault bit assignments shared by live LED state and capture schema v3.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Fault {
    /// Explicit connection or GATT inspection failed.
    Probe = 1,
    /// HCI scan report processing failed; uses the scan-only blink category.
    Scan = 2,
    /// Bounded storage or event capacity discarded evidence.
    Capacity = 3,
    /// Persistent capture storage operation failed.
    Storage = 4,
    /// USB capture export failed.
    Usb = 5,
    /// Encoder I2C communication failed.
    Encoder = 6,
    /// Display completion timing could not be trusted.
    Timing = 7,
    /// Retained boot evidence indicates a watchdog timer reset, not necessarily a missed feed.
    Watchdog = 8,
    /// Advertising data was malformed; uses the scan-only blink category.
    Advertisement = 9,
}
impl Fault {
    /// Every defined fault in ascending stable code order; reserved bits are excluded.
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

    /// Returns the single mask bit for `self`, occupying bits 0..=8.
    pub const fn bit(self) -> u16 { 1 << (self as u8 - 1) }

    /// Returns `self`'s diagnostic category label, without asserting a root cause.
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

    /// Returns whether `self` belongs to the slow-red scan-error category.
    pub const fn scan_only(self) -> bool { matches!(self, Self::Scan | Self::Advertisement) }

    /// Returns the display header describing `self`'s LED fault category.
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

/// LED severity class derived from all latched faults, independent of their first code.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorLatch {
    /// No fault bits are present.
    None,
    /// Only scan-report and/or advertising-data faults are present.
    ScanOnly,
    /// At least one fault outside the scan-only category is present.
    Other,
}

/// First means atomic recording order, not a timestamp or a root-cause claim.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FaultLatch {
    /// Defined fault mask; bits 9..=15 must be zero.
    bits: u16,
    /// First recorded fault, necessarily present in `bits`; absent only for an empty mask.
    first: Option<Fault>,
}
impl FaultLatch {
    /// Valid empty evidence, with no first fault and no set bits.
    pub const EMPTY: Self = Self { bits: 0, first: None };

    /// Returns a latch containing only `fault`, also recorded as the first fault.
    pub const fn single(fault: Fault) -> Self {
        Self {
            bits: fault.bit(),
            first: Some(fault),
        }
    }

    /// Validates persisted `bits` and one-based `first_code`, returning a coherent latch.
    /// Returns `None` for reserved bits, unknown codes, a first fault absent from the mask,
    /// or an empty mask with a nonzero first code. Only `(0, 0)` represents no fault.
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

    /// Returns `self`'s defined fault bitmask, suitable for capture serialization.
    pub const fn bits(self) -> u16 { self.bits }

    /// Returns `self`'s first recorded fault, or `None` for an empty latch.
    pub const fn first(self) -> Option<Fault> { self.first }

    /// Returns whether `self` contains `fault`, independently of recording order.
    pub const fn contains(
        self,
        fault: Fault,
    ) -> bool {
        self.bits & fault.bit() != 0
    }

    /// Returns the LED severity of all bits in `self`; any non-scan fault takes precedence.
    pub const fn category(self) -> ErrorLatch {
        if self.bits == 0 {
            ErrorLatch::None
        } else if self.bits & !(Fault::Scan.bit() | Fault::Advertisement.bit()) == 0 {
            ErrorLatch::ScanOnly
        } else {
            ErrorLatch::Other
        }
    }

    /// Returns `self`'s fault header, preferring a non-scan fault over scan-only evidence.
    /// Uses the first fault when eligible, otherwise ascending stable code order;
    /// an empty latch returns `Error:None`.
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
/// Exhaustive small-mask admission checks for serialized latch coherence.
mod tests {
    use super::*;
    #[test]
    /// Returns unit after checking every 10-bit mask and adjacent code boundary;
    /// panics if reserved bits or a first code absent from the mask are admitted.
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
