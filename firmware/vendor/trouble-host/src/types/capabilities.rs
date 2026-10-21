//! SMP I/O-capability octets and user-facing names (Core Vol3 PartH section2.3.2).
use crate::Error;

/// Device I/O capabilities
// ([Vol 3] Part H, Section 2.3.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IoCapabilities {
    /// Display only
    DisplayOnly,
    /// Yes/no display
    DisplayYesNo,
    /// Keyboard only
    KeyboardOnly,
    /// No input and no output
    NoInputNoOutput,
    /// Both keyboard and display
    KeyboardDisplay,
}

impl TryFrom<u8> for IoCapabilities {
    /// Unsupported octets produce the host InvalidValue error.
    type Error = Error;
    /// Returns the SMP capability for `val`0x00..=0x04; other octets return InvalidValue.
    fn try_from(val: u8) -> Result<Self, Error> {
        Ok(match val {
            0x00 => Self::DisplayOnly,
            0x01 => Self::DisplayYesNo,
            0x02 => Self::KeyboardOnly,
            0x03 => Self::NoInputNoOutput,
            0x04 => Self::KeyboardDisplay,
            _ => return Err(Error::InvalidValue),
        })
    }
}

impl From<IoCapabilities> for u8 {
    /// Returns the SMP on-wire octet for `val`.
    fn from(val: IoCapabilities) -> u8 {
        match val {
            IoCapabilities::DisplayOnly => 0x00,
            IoCapabilities::DisplayYesNo => 0x01,
            IoCapabilities::KeyboardOnly => 0x02,
            IoCapabilities::NoInputNoOutput => 0x03,
            IoCapabilities::KeyboardDisplay => 0x04,
        }
    }
}

impl AsRef<str> for IoCapabilities {
    /// Returns a static human-readable I/O-capability label.
    fn as_ref(&self) -> &str {
        match self {
            IoCapabilities::DisplayOnly => "Display Only",
            IoCapabilities::DisplayYesNo => "Display Yes/No",
            IoCapabilities::KeyboardOnly => "Keyboard Only",
            IoCapabilities::NoInputNoOutput => "No Input / No Output",
            IoCapabilities::KeyboardDisplay => "Keyboard and Display",
        }
    }
}

impl core::fmt::Display for IoCapabilities {
    /// Writes the capability label into `f` and returns its formatting result.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.as_ref())
    }
}

#[cfg(feature = "defmt")]
impl defmt::Format for IoCapabilities {
    /// Emits the capability label through `fmt`.
    fn format(&self, fmt: defmt::Formatter) {
        defmt::write!(fmt, "{}", self.as_ref())
    }
}
