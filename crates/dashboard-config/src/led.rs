//! Approved D2 indicator settings. Firmware animation is a separate adapter.
use serde::{Deserialize, Serialize};

use crate::Color;
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LedPatternKind {
    Off,
    Solid,
    Blink,
    Breathe,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LedPattern {
    pub kind: LedPatternKind,
    pub color: Color,
    pub brightness_percent: u8,
    pub period_ms: u16,
    pub on_percent: u8,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LedSettings {
    pub normal: LedPattern,
    pub warning: LedPattern,
    pub sync_warning_blink: bool,
    pub during_feedback: bool,
    pub on_debug: bool,
    pub on_logs: bool,
    pub backlight_off: bool,
}
impl LedSettings {
    pub const DEFAULT: Self = Self {
        normal: LedPattern {
            kind: LedPatternKind::Solid,
            color: Color(0x07E0),
            brightness_percent: 100,
            period_ms: 400,
            on_percent: 50,
        },
        warning: LedPattern {
            kind: LedPatternKind::Blink,
            color: Color(0xF800),
            brightness_percent: 100,
            period_ms: 400,
            on_percent: 50,
        },
        sync_warning_blink: true,
        during_feedback: false,
        on_debug: false,
        on_logs: false,
        backlight_off: false,
    };
}
impl Default for LedSettings {
    fn default() -> Self { Self::DEFAULT }
}
