//! Compile-time image labels and linked discriminator; binary roots fix the inspector alias.

/// Stable image discriminator; it selects application policy, never a peer or connection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum ToolKind {
    /// Standard service inventory and allowlisted reads only, even with extra features.
    StandardProbe = 0,
    /// Explicit selected-peer NGM identity trial; requires the `ngm-identity` feature.
    NgmIdentity = 1,
}

impl ToolKind {
    /// Returns a fixed ASCII display title for this image, at most12 bytes.
    pub const fn title(self) -> &'static str {
        match self {
            Self::StandardProbe => "BLE probe",
            Self::NgmIdentity => "NGM identity",
        }
    }

    /// Returns the manual inspection label for this image, at most16 ASCII bytes.
    pub const fn inspect_label(self) -> &'static str {
        match self {
            Self::StandardProbe => "Inspect GATT",
            Self::NgmIdentity => "Inspect NGM INFO",
        }
    }

    /// Returns the USB product descriptor; this label does not change export protocol.
    pub const fn usb_product(self) -> &'static str {
        match self {
            Self::StandardProbe => "Pico BLE Probe Captures",
            Self::NgmIdentity => "Pico NGM Identity Captures",
        }
    }
}

/// Returns this binary's constant policy, including before hardware initialization.
/// Package features cannot change it. No side effects, runtime input or fallback.
pub const fn tool_kind() -> ToolKind { crate::TOOL_KIND }
