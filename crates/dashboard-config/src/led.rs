//! Dashboard RGB indicator settings; standalone BLE-probe status uses a separate policy.
//! Runtime selection follows completed presentation, with PWM/hardware ownership at the boundary.
use serde::{Deserialize, Serialize};

use crate::Color;
/// Time-based intensity pattern; reduced-motion policy changes breathing to steady output.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LedPatternKind {
    /// Zero emitted duty.
    Off,
    /// Constant configured color/brightness.
    Solid,
    /// Periodic on/off output; warning mode may synchronize with completed popup phase.
    Blink,
    /// Smooth triangular-cycle intensity envelope, independent of render cadence.
    Breathe,
}
/// Retained RGB pattern; all fields remain validated even when a mode does not currently use them.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LedPattern {
    /// Intensity animation family.
    pub kind: LedPatternKind,
    /// RGB565 channel ratios; hardware active-low mapping is outside this model.
    pub color: Color,
    /// Overall intensity, 0..=100 percent; zero suppresses every channel.
    pub brightness_percent: u8,
    /// Full animation period, 200..=10,000 milliseconds.
    pub period_ms: u16,
    /// Blink on-duty, 10..=90 percent; stored for non-blink modes too.
    pub on_percent: u8,
}
/// Normal/warning selection policy; screen gates suppress warning selection, not the normal pattern.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LedSettings {
    /// Pattern when no eligible warning passes the screen/backlight policy.
    pub normal: LedPattern,
    /// Pattern when an eligible warning passes policy; green/red are user-configurable here.
    pub warning: LedPattern,
    /// Use committed warning popup blink phase when available and the backlight is on.
    pub sync_warning_blink: bool,
    /// Permit warning pattern during non-warning Dashboard feedback overlays.
    pub during_feedback: bool,
    /// Permit warning pattern on Debug.
    pub on_debug: bool,
    /// Permit warning pattern on Logs.
    pub on_logs: bool,
    /// Permit warning pattern while the backlight is off; normal output remains independently configured.
    pub backlight_off: bool,
}
impl LedSettings {
    /// Solid green normal and red 400 ms/50% warning blink; off-screen/feedback overrides disabled.
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
    /// Return [`Self::DEFAULT`], without initializing PWM or touching GPIO.
    fn default() -> Self { Self::DEFAULT }
}
