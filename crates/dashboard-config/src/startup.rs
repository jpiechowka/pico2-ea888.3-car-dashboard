//! Standalone procedural startup profile. Legacy EAV remains dashboard-only.
use serde::{Deserialize, Serialize};

use crate::Orientation;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum StartupTheme {
    #[default]
    Aezakmi,
    Turbo,
    Sanic,
    BoostBro,
    WhereLambo,
    Custom,
}
impl StartupTheme {
    pub const ALL: [Self; 5] = [
        Self::Aezakmi,
        Self::Turbo,
        Self::Sanic,
        Self::BoostBro,
        Self::WhereLambo,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Aezakmi => "AEZAKMI",
            Self::Turbo => "SUTUTU",
            Self::Sanic => "GOTTA GO FAST",
            Self::BoostBro => "HOW MUCH BOOST BRO",
            Self::WhereLambo => "WHERE LAMBO",
            Self::Custom => "Custom artwork",
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum StartupStatus {
    Off,
    #[default]
    Brief,
    Detailed,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StartupProfile {
    pub theme: StartupTheme,
    pub status: StartupStatus,
    pub status_ms: u16,
    pub duration_ms: u16,
    pub loop_animation: bool,
    pub interrupt_on_attention: bool,
    pub warning_led: bool,
    pub status_orientation: Option<Orientation>,
    pub splash_orientation: Option<Orientation>,
}
impl Default for StartupProfile {
    fn default() -> Self { Self::DEFAULT }
}
impl StartupProfile {
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

    pub const fn valid(self) -> bool { self.status_ms <= 2000 && self.duration_ms <= 10000 }

    pub const fn status_duration(self) -> u64 {
        if matches!(self.status, StartupStatus::Off) {
            0
        } else {
            self.status_ms as u64
        }
    }

    pub const fn total_duration(self) -> u64 { self.status_duration() + self.duration_ms as u64 }

    pub fn effective_status_orientation(
        self,
        default: Orientation,
    ) -> Orientation {
        self.status_orientation.unwrap_or(default)
    }

    pub fn effective_splash_orientation(
        self,
        default: Orientation,
    ) -> Orientation {
        self.splash_orientation.unwrap_or(default)
    }

    /// Fixed 16-byte v2 profile. The last two bytes remain reserved and zero.
    pub fn encode(self) -> Option<[u8; 16]> {
        if !self.valid() {
            return None;
        }
        let mut bytes = [0; 16];
        bytes[..4].copy_from_slice(b"EASP");
        bytes[4] = 2;
        bytes[5] = self.theme as u8;
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

    pub fn decode(bytes: &[u8]) -> Option<Self> {
        if bytes.len() != 16
            || &bytes[..4] != b"EASP"
            || !matches!(bytes[4], 1 | 2)
            || bytes[7] & !7 != 0
            || bytes[14..].iter().any(|b| *b != 0)
            || (bytes[4] == 1 && bytes[12..14] != [0, 0])
        {
            return None;
        }
        let profile = Self {
            theme: if bytes[5] == 5 {
                StartupTheme::Custom
            } else {
                *StartupTheme::ALL.get(usize::from(bytes[5]))?
            },
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

fn encode_orientation(orientation: Option<Orientation>) -> u8 {
    match orientation {
        None => 0,
        Some(Orientation::Landscape) => 1,
        Some(Orientation::Portrait) => 2,
        Some(Orientation::LandscapeInverted) => 3,
        Some(Orientation::PortraitInverted) => 4,
    }
}

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
