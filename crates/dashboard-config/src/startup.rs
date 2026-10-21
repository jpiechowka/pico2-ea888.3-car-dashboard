//! Standalone procedural startup profile. Legacy EAV remains dashboard-only.
use serde::{Deserialize, Serialize};

use crate::Orientation;

/// Splash selection; wire tags are stable and distinct from procedural picker indices.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum StartupTheme {
    /// Default AEZAKMI procedural artwork, wire tag zero.
    #[default]
    Aezakmi,
    /// SUTUTU turbo artwork.
    Turbo,
    /// GOTTA GO FAST artwork.
    Sanic,
    /// HOW MUCH BOOST BRO artwork.
    BoostBro,
    /// WHERE LAMBO artwork.
    WhereLambo,
    /// Separately admitted EASA artwork; not included in the procedural picker.
    Custom,
    /// BLESS THIS MACHINE SPIRIT artwork.
    MachineSpirit,
    /// ERROR 404: APPLY INCENSE artwork.
    RitualDiagnostics,
    /// RED GOES FASTA artwork.
    RedGoesFasta,
    /// MORE BOOST, YOU NEED artwork.
    BoostPadawan,
    /// OOGA BOOGA: SMOL SNAIL artwork.
    TurboBrainlet,
}
impl StartupTheme {
    /// Ten procedural choices in UI order, excluding separately supplied custom artwork.
    pub const ALL: [Self; 10] = [
        Self::Aezakmi,
        Self::Turbo,
        Self::Sanic,
        Self::BoostBro,
        Self::WhereLambo,
        Self::MachineSpirit,
        Self::RitualDiagnostics,
        Self::RedGoesFasta,
        Self::BoostPadawan,
        Self::TurboBrainlet,
    ];

    /// Return the stable EASP identity, independent of the procedural picker order.
    pub const fn wire_tag(self) -> u8 {
        match self {
            Self::Aezakmi => 0,
            Self::Turbo => 1,
            Self::Sanic => 2,
            Self::BoostBro => 3,
            Self::WhereLambo => 4,
            Self::Custom => 5,
            Self::MachineSpirit => 6,
            Self::RitualDiagnostics => 7,
            Self::RedGoesFasta => 8,
            Self::BoostPadawan => 9,
            Self::TurboBrainlet => 10,
        }
    }

    /// Decode `tag` in the current zero-through-ten vocabulary; returns `None` for every other byte.
    /// Version-one admission separately rejects tags above five.
    pub const fn from_wire_tag(tag: u8) -> Option<Self> {
        Some(match tag {
            0 => Self::Aezakmi,
            1 => Self::Turbo,
            2 => Self::Sanic,
            3 => Self::BoostBro,
            4 => Self::WhereLambo,
            5 => Self::Custom,
            6 => Self::MachineSpirit,
            7 => Self::RitualDiagnostics,
            8 => Self::RedGoesFasta,
            9 => Self::BoostPadawan,
            10 => Self::TurboBrainlet,
            _ => return None,
        })
    }

    /// Return the static picker label; it is descriptive text, not a serialization key.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Aezakmi => "AEZAKMI",
            Self::Turbo => "SUTUTU",
            Self::Sanic => "GOTTA GO FAST",
            Self::BoostBro => "HOW MUCH BOOST BRO",
            Self::WhereLambo => "WHERE LAMBO",
            Self::Custom => "Custom artwork",
            Self::MachineSpirit => "BLESS THIS MACHINE SPIRIT",
            Self::RitualDiagnostics => "ERROR 404: APPLY INCENSE",
            Self::RedGoesFasta => "RED GOES FASTA",
            Self::BoostPadawan => "MORE BOOST, YOU NEED",
            Self::TurboBrainlet => "OOGA BOOGA: SMOL SNAIL",
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
/// Boot-information presentation before the splash; declaration order is its EASP wire tag.
pub enum StartupStatus {
    /// Skip status even when a nonzero status duration is stored.
    Off,
    /// Compact status presentation.
    #[default]
    Brief,
    /// Detailed status presentation using the same duration bound.
    Detailed,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// Independent startup policy; construction/serde does not validate durations or custom assets.
pub struct StartupProfile {
    /// Procedural choice or request for independently validated custom artwork.
    pub theme: StartupTheme,
    /// Boot-information density; Off suppresses its effective duration.
    pub status: StartupStatus,
    /// Stored status dwell in milliseconds, zero through 2000 even when status is Off.
    pub status_ms: u16,
    /// Splash dwell in milliseconds, zero through 10000; zero skips the splash.
    pub duration_ms: u16,
    /// Repeat animation during the splash; false uses the renderer's non-looping presentation.
    pub loop_animation: bool,
    /// Permit qualified attention to exit startup early; recovery/fault exits are independent.
    pub interrupt_on_attention: bool,
    /// Permit dashboard warning LED signaling while startup is presented.
    pub warning_led: bool,
    /// Status mounting override; None inherits the device orientation.
    pub status_orientation: Option<Orientation>,
    /// Splash mounting override; None inherits the device orientation independently of Status.
    pub splash_orientation: Option<Orientation>,
}
impl Default for StartupProfile {
    /// Return the same deterministic policy as [`Self::DEFAULT`].
    fn default() -> Self { Self::DEFAULT }
}
impl StartupProfile {
    /// 600 ms brief status then 2500 ms looping AEZAKMI; attention does not interrupt by default.
    /// Warning LED signaling remains enabled and both screens inherit mounting orientation.
    pub const DEFAULT: Self = Self {
        theme: StartupTheme::Aezakmi,
        status: StartupStatus::Brief,
        status_ms: 600,
        duration_ms: 2500,
        loop_animation: true,
        interrupt_on_attention: false,
        warning_led: true,
        status_orientation: None,
        splash_orientation: None,
    };

    /// Return whether both stored durations meet their limits, including disabled status.
    /// This does not establish custom-asset presence, geometry or hardware readiness.
    pub const fn valid(self) -> bool { self.status_ms <= 2000 && self.duration_ms <= 10000 }

    /// Return effective status milliseconds, zero for Off; does not perform validation.
    pub const fn status_duration(self) -> u64 {
        if matches!(self.status, StartupStatus::Off) {
            0
        } else {
            self.status_ms as u64
        }
    }

    /// Return nominal status-plus-splash milliseconds, at most 12000 after admission.
    /// Actual presentation also depends on completed frames and early-exit policy.
    pub const fn total_duration(self) -> u64 { self.status_duration() + self.duration_ms as u64 }

    /// Return the status override or `default`; splash overrides do not participate.
    pub fn effective_status_orientation(
        self,
        default: Orientation,
    ) -> Orientation {
        self.status_orientation.unwrap_or(default)
    }

    /// Return the splash override or `default`; status overrides do not participate.
    pub fn effective_splash_orientation(
        self,
        default: Orientation,
    ) -> Orientation {
        self.splash_orientation.unwrap_or(default)
    }

    /// Fixed 16-byte v2 profile. Tags 6..10 extend v2; old readers reject them.
    /// Legacy v1 keeps its frozen theme vocabulary 0..5. The last two bytes remain reserved and zero.
    /// Returns `None` for invalid durations; this format has no checksum or authenticity guarantee.
    pub fn encode(self) -> Option<[u8; 16]> {
        if !self.valid() {
            return None;
        }
        let mut bytes = [0; 16];
        bytes[..4].copy_from_slice(b"EASP");
        bytes[4] = 2;
        bytes[5] = self.theme.wire_tag();
        bytes[6] = self.status as u8;
        bytes[7] = u8::from(self.loop_animation)
            | (u8::from(self.warning_led) << 1)
            | (u8::from(self.interrupt_on_attention) << 2);
        bytes[8..10].copy_from_slice(&self.status_ms.to_le_bytes());
        bytes[10..12].copy_from_slice(&self.duration_ms.to_le_bytes());
        bytes[12] = encode_orientation(self.status_orientation);
        bytes[13] = encode_orientation(self.splash_orientation);
        Some(bytes)
    }

    /// Admit exactly sixteen `bytes` of EASP v1/v2 and return a valid profile.
    /// Rejects trailing/truncated data, unknown tags/flags, reserved bytes and out-of-range timing.
    /// V1 permits only themes zero through five and inherited orientations; no asset is inspected.
    pub fn decode(bytes: &[u8]) -> Option<Self> {
        if bytes.len() != 16
            || &bytes[..4] != b"EASP"
            || !matches!(bytes[4], 1 | 2)
            || bytes[7] & !7 != 0
            || bytes[14..].iter().any(|b| *b != 0)
            || (bytes[4] == 1 && (bytes[12..14] != [0, 0] || bytes[5] > 5))
        {
            return None;
        }
        let profile = Self {
            theme: StartupTheme::from_wire_tag(bytes[5])?,
            status: match bytes[6] {
                0 => StartupStatus::Off,
                1 => StartupStatus::Brief,
                2 => StartupStatus::Detailed,
                _ => return None,
            },
            status_ms: u16::from_le_bytes([bytes[8], bytes[9]]),
            duration_ms: u16::from_le_bytes([bytes[10], bytes[11]]),
            loop_animation: bytes[7] & 1 != 0,
            warning_led: bytes[7] & 2 != 0,
            interrupt_on_attention: bytes[7] & 4 != 0,
            status_orientation: decode_orientation(bytes[12])?,
            splash_orientation: decode_orientation(bytes[13])?,
        };
        profile.valid().then_some(profile)
    }
}

/// Return the explicit EASP byte for `orientation`; zero means inheritance, one through four mounting.
fn encode_orientation(orientation: Option<Orientation>) -> u8 {
    match orientation {
        None => 0,
        Some(Orientation::Landscape) => 1,
        Some(Orientation::Portrait) => 2,
        Some(Orientation::LandscapeInverted) => 3,
        Some(Orientation::PortraitInverted) => 4,
    }
}

/// Decode `value`: returns outer None for an invalid byte, inner None for inherited orientation.
fn decode_orientation(value: u8) -> Option<Option<Orientation>> {
    Some(match value {
        0 => None,
        1 => Some(Orientation::Landscape),
        2 => Some(Orientation::Portrait),
        3 => Some(Orientation::LandscapeInverted),
        4 => Some(Orientation::PortraitInverted),
        _ => return None,
    })
}
