//! Complete v4 presentation and warning schema; runtime adapters are separate.
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::{Channel, Color};

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Readout {
    Celsius,
    Fahrenheit,
    FollowBoost,
    Bar,
    Psi,
    Kpa,
    Volts,
    Lambda,
    Afr,
    LambdaAndAfr,
}
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Secondary {
    None,
    Bar,
    Psi,
    Kpa,
}
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PeakSelection {
    Off,
    Maximum,
    Minimum,
}
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReferenceMode {
    Off,
    Zero,
    Manual,
}
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Priority {
    Low,
    Normal,
    High,
    Critical,
}

impl Readout {
    pub const fn includes_afr(self) -> bool { matches!(self, Self::Afr | Self::LambdaAndAfr) }

    pub const fn compatible(
        self,
        channel: Channel,
    ) -> bool {
        match channel {
            Channel::Boost => matches!(self, Self::FollowBoost | Self::Bar | Self::Psi | Self::Kpa),
            Channel::Battery => matches!(self, Self::Volts),
            Channel::Lambda => matches!(self, Self::Lambda | Self::Afr | Self::LambdaAndAfr),
            _ => matches!(self, Self::Celsius | Self::Fahrenheit),
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Band {
    pub min: f32,
    pub max: f32,
    pub color: Color,
    label_len: u8,
    label_bytes: [u8; 5],
}
impl Band {
    pub const EMPTY: Self = Self {
        min: 0.0,
        max: 0.0,
        color: Color(0),
        label_len: 0,
        label_bytes: [0; 5],
    };

    pub fn new(
        min: f32,
        max: f32,
        color: Color,
        label: &str,
    ) -> Result<Self, crate::ErrorCode> {
        let mut band = Self {
            min,
            max,
            color,
            ..Self::EMPTY
        };
        band.set_label(label)?;
        Ok(band)
    }

    pub fn label(&self) -> &str {
        core::str::from_utf8(&self.label_bytes[..usize::from(self.label_len)]).expect("bounded ASCII band label")
    }

    pub fn set_label(
        &mut self,
        label: &str,
    ) -> Result<(), crate::ErrorCode> {
        if label.len() > 5
            || !label.bytes().all(|b| (32..=126).contains(&b))
            || (!label.is_empty() && label.bytes().all(|b| b == b' '))
        {
            return Err(crate::ErrorCode::Text);
        }
        self.label_bytes = [0; 5];
        self.label_bytes[..label.len()].copy_from_slice(label.as_bytes());
        self.label_len = label.len() as u8;
        Ok(())
    }
}
impl Serialize for Band {
    fn serialize<S: Serializer>(
        &self,
        s: S,
    ) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut s = s.serialize_struct("Band", 4)?;
        s.serialize_field("min", &self.min)?;
        s.serialize_field("max", &self.max)?;
        s.serialize_field("color", &self.color)?;
        s.serialize_field("label", self.label())?;
        s.end()
    }
}
impl<'de> Deserialize<'de> for Band {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Wire {
            min: f32,
            max: f32,
            color: Color,
            label: heapless::String<5>,
        }
        let w = Wire::deserialize(d)?;
        Self::new(w.min, w.max, w.color, &w.label).map_err(|_| {
            serde::de::Error::custom("band label must be printable ASCII, at most five bytes and non-space if nonempty")
        })
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AdvancedDisplay {
    bands: [Band; 3],
    pub reference: f32,
    pub smoothing_ms: u16,
    pub peak_hold_ms: u16,
    pub readout: Readout,
    pub secondary: Secondary,
    pub peak_mode: PeakSelection,
    pub reference_mode: ReferenceMode,
    band_count: u8,
}
impl AdvancedDisplay {
    pub const fn for_channel(
        channel: Channel,
        afr: bool,
    ) -> Self {
        Self {
            bands: [Band::EMPTY; 3],
            reference: 0.0,
            smoothing_ms: 0,
            peak_hold_ms: 750,
            readout: match channel {
                Channel::Boost => Readout::FollowBoost,
                Channel::Battery => Readout::Volts,
                Channel::Lambda => {
                    if afr {
                        Readout::LambdaAndAfr
                    } else {
                        Readout::Lambda
                    }
                }
                _ => Readout::Celsius,
            },
            secondary: Secondary::None,
            peak_mode: PeakSelection::Off,
            reference_mode: ReferenceMode::Off,
            band_count: 0,
        }
    }

    pub fn bands(&self) -> &[Band] { &self.bands[..usize::from(self.band_count)] }

    /// Edit a stored active label with precise field context. Rejection leaves
    /// the previous bytes intact; visibility does not affect edit validity.
    pub fn set_band_label(
        &mut self,
        index: usize,
        label: &str,
    ) -> Result<(), crate::ValidationError> {
        if index >= usize::from(self.band_count) {
            return Err(crate::ValidationError::new(
                crate::ErrorCode::Bounds,
                crate::Field::BandCount,
            ));
        }
        self.bands[index]
            .set_label(label)
            .map_err(|code| crate::ValidationError::new(code, crate::Field::BandLabel(index as u8)))
    }

    pub fn set_bands(
        &mut self,
        bands: &[Band],
    ) -> Result<(), crate::ValidationError> {
        if bands.len() > 3 {
            return Err(crate::ValidationError::new(
                crate::ErrorCode::Bounds,
                crate::Field::BandCount,
            ));
        }
        self.bands = [Band::EMPTY; 3];
        self.bands[..bands.len()].copy_from_slice(bands);
        self.band_count = bands.len() as u8;
        Ok(())
    }
}
impl Serialize for AdvancedDisplay {
    fn serialize<S: Serializer>(
        &self,
        s: S,
    ) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut s = s.serialize_struct("AdvancedDisplay", 8)?;
        s.serialize_field("bands", self.bands())?;
        s.serialize_field("reference", &self.reference)?;
        s.serialize_field("smoothing_ms", &self.smoothing_ms)?;
        s.serialize_field("peak_hold_ms", &self.peak_hold_ms)?;
        s.serialize_field("readout", &self.readout)?;
        s.serialize_field("secondary", &self.secondary)?;
        s.serialize_field("peak_mode", &self.peak_mode)?;
        s.serialize_field("reference_mode", &self.reference_mode)?;
        s.end()
    }
}
impl<'de> Deserialize<'de> for AdvancedDisplay {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        // heapless visits one Band at a time. Its length is bounded before the
        // tiny collection is copied; no Widget collection temporary exists.
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Wire {
            bands: heapless::Vec<Band, 3>,
            reference: f32,
            smoothing_ms: u16,
            peak_hold_ms: u16,
            readout: Readout,
            secondary: Secondary,
            peak_mode: PeakSelection,
            reference_mode: ReferenceMode,
        }
        let w = Wire::deserialize(d)?;
        let mut result = Self {
            reference: w.reference,
            smoothing_ms: w.smoothing_ms,
            peak_hold_ms: w.peak_hold_ms,
            readout: w.readout,
            secondary: w.secondary,
            peak_mode: w.peak_mode,
            reference_mode: w.reference_mode,
            ..Self::for_channel(Channel::Boost, false)
        };
        result
            .set_bands(&w.bands)
            .map_err(|_| serde::de::Error::custom("at most three bands"))?;
        Ok(result)
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdvancedWarning {
    pub trigger_high: f32,
    pub clear_high: f32,
    pub snooze_s: u16,
    pub priority: Priority,
    pub latch: bool,
}
impl AdvancedWarning {
    pub const DEFAULT: Self = Self {
        trigger_high: 0.0,
        clear_high: 0.0,
        snooze_s: 0,
        priority: Priority::Normal,
        latch: false,
    };
}
impl Default for AdvancedWarning {
    fn default() -> Self { Self::DEFAULT }
}

// Human fields are flat and Postcard uses precisely the same declaration order.
// serde flatten requires alloc and is unsupported by Postcard; this explicit
// adapter also preserves deny_unknown_fields and duplicate-field rejection.
impl Serialize for crate::WarningSettings {
    fn serialize<S: Serializer>(
        &self,
        s: S,
    ) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut s = s.serialize_struct("WarningSettings", 14)?;
        s.serialize_field("enabled", &self.enabled)?;
        s.serialize_field("applicability", &self.applicability)?;
        s.serialize_field("mode", &self.mode)?;
        s.serialize_field("trigger", &self.trigger)?;
        s.serialize_field("clear", &self.clear)?;
        s.serialize_field("min_load_percent", &self.min_load_percent)?;
        s.serialize_field("on_ms", &self.on_ms)?;
        s.serialize_field("off_ms", &self.off_ms)?;
        s.serialize_field("message", &self.message)?;
        s.serialize_field("trigger_high", &self.advanced.trigger_high)?;
        s.serialize_field("clear_high", &self.advanced.clear_high)?;
        s.serialize_field("snooze_s", &self.advanced.snooze_s)?;
        s.serialize_field("priority", &self.advanced.priority)?;
        s.serialize_field("latch", &self.advanced.latch)?;
        s.end()
    }
}
impl<'de> Deserialize<'de> for crate::WarningSettings {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Wire {
            enabled: bool,
            applicability: crate::Applicability,
            mode: crate::LimitMode,
            trigger: f32,
            clear: f32,
            min_load_percent: f32,
            on_ms: u16,
            off_ms: u16,
            message: heapless::String<20>,
            trigger_high: f32,
            clear_high: f32,
            snooze_s: u16,
            priority: Priority,
            latch: bool,
        }
        let w = Wire::deserialize(d)?;
        Ok(Self {
            enabled: w.enabled,
            applicability: w.applicability,
            mode: w.mode,
            trigger: w.trigger,
            clear: w.clear,
            min_load_percent: w.min_load_percent,
            on_ms: w.on_ms,
            off_ms: w.off_ms,
            message: w.message,
            advanced: AdvancedWarning {
                trigger_high: w.trigger_high,
                clear_high: w.clear_high,
                snooze_s: w.snooze_s,
                priority: w.priority,
                latch: w.latch,
            },
        })
    }
}
const _: () = assert!(core::mem::size_of::<Band>() == 16);
const _: () = assert!(core::mem::size_of::<AdvancedDisplay>() == 64);
const _: () = assert!(core::mem::size_of::<AdvancedWarning>() == 12);
