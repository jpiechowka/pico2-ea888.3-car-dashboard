//! Presentation and warning schema introduced in v4 and retained in the current payload.
//! Bounded wire adapters preserve declaration order without allocation-dependent serde flattening.
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::{Channel, Color};

/// Display units only; validation rejects combinations incompatible with the measurement channel.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Readout {
    /// Temperature in degrees Celsius.
    Celsius,
    /// Temperature converted to degrees Fahrenheit.
    Fahrenheit,
    /// Pressure follows the runtime bar/PSI toggle.
    FollowBoost,
    /// Fixed bar display, unaffected by the toggle.
    Bar,
    /// Fixed PSI display, unaffected by the toggle.
    Psi,
    /// Fixed kilopascal display.
    Kpa,
    /// ECU supply voltage in volts.
    Volts,
    /// Dimensionless lambda without a fuel conversion.
    Lambda,
    /// Fuel-assumption-derived equivalent AFR.
    Afr,
    /// Lambda primary readout with equivalent AFR decoration.
    LambdaAndAfr,
    /// Four signed correction numbers in degrees.
    IgnitionNumeric,
    /// Four raw-sign statuses: negative is RET, otherwise OK for fresh samples.
    IgnitionStatus,
    /// Four simultaneous raw-sign statuses and signed correction numbers.
    IgnitionStatusAndNumber,
}
/// Ignition warning text layout; the shared threshold, dwell and message remain in WarningSettings.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum IgnitionWarningPresentation {
    /// One configured message for any qualifying cylinder; required for scalar widgets.
    Global,
    /// One bounded line per affected fresh cylinder, prefixing the shared configured message.
    CylinderLines,
}
/// Optional second pressure unit; active visibility additionally depends on widget capabilities.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Secondary {
    /// No secondary pressure readout.
    None,
    /// Secondary bar readout.
    Bar,
    /// Secondary PSI readout.
    Psi,
    /// Secondary kilopascal readout.
    Kpa,
}
/// Extremum selected for gauge peak indication; raw statistics remain independent of smoothing.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PeakSelection {
    /// No peak marker.
    Off,
    /// Indicate the retained maximum.
    Maximum,
    /// Indicate the retained minimum.
    Minimum,
}
/// Gauge reference decoration; none of these modes establishes an ECU target.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReferenceMode {
    /// No reference marker.
    Off,
    /// Canonical zero marker where visible in the gauge span.
    Zero,
    /// Explicit finite channel-domain reference value.
    Manual,
}
/// Warning presentation ordering; severity labels do not qualify thresholds or measurements.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Priority {
    /// Lowest presentation priority.
    Low,
    /// Default presentation priority.
    Normal,
    /// Elevated presentation priority.
    High,
    /// Highest presentation priority.
    Critical,
}

impl Readout {
    /// Return whether this readout represents `channel`, including the independent cylinder group.
    pub const fn compatible_widget(
        self,
        channel: crate::WidgetChannel,
    ) -> bool {
        match channel.scalar() {
            Some(scalar) => self.compatible(scalar),
            None => {
                matches!(
                    self,
                    Self::IgnitionNumeric | Self::IgnitionStatus | Self::IgnitionStatusAndNumber
                )
            }
        }
    }

    /// Return whether this mode includes any fuel-assumption-derived AFR output.
    pub const fn includes_afr(self) -> bool { matches!(self, Self::Afr | Self::LambdaAndAfr) }

    /// Return whether these units can represent `channel`; no value conversion occurs.
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

/// One bounded gauge band. Constructor validates its label only; full admission checks endpoints/contrast.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Band {
    /// Lower canonical endpoint, finite and below `max` after configuration admission.
    pub min: f32,
    /// Upper canonical endpoint; adjacent bands may touch but may not overlap.
    pub max: f32,
    /// RGB565 band color, required to contrast with the gauge track.
    pub color: Color,
    /// Active ASCII label byte count, always at most five through safe constructors.
    label_len: u8,
    /// Inline label storage; inactive bytes are zeroed on replacement.
    label_bytes: [u8; 5],
}
impl Band {
    /// Inactive array filler; equal zero endpoints are not an admitted active band.
    pub const EMPTY: Self = Self {
        min: 0.0,
        max: 0.0,
        color: Color(0),
        label_len: 0,
        label_bytes: [0; 5],
    };

    /// Return a band with `min`, `max`, `color` and validated optional ASCII `label`.
    /// Label errors return `ErrorCode::Text`; endpoint/channel/contrast checks are deferred to admission.
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

    /// Return the active zero-to-five-byte label; safe constructors preserve UTF-8/length invariants.
    pub fn label(&self) -> &str {
        core::str::from_utf8(&self.label_bytes[..usize::from(self.label_len)]).expect("bounded ASCII band label")
    }

    /// Replace with `label`, allowing empty or at most five printable ASCII bytes, never all spaces.
    /// Returns `Text` without modifying prior bytes on rejection; success clears unused storage.
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
    /// Write the four-field logical band to `s`; returns serializer output/error, omitting storage padding.
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
    /// Read a bounded band from `d`; returns shape/label errors, with numeric admission deferred.
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        /// Four-field wire shape; string capacity is enforced before label construction.
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Wire {
            /// Candidate lower canonical endpoint.
            min: f32,
            /// Candidate upper canonical endpoint.
            max: f32,
            /// RGB565 band color.
            color: Color,
            /// Optional printable label, at most five UTF-8 bytes before ASCII validation.
            label: heapless::String<5>,
        }
        let w = Wire::deserialize(d)?;
        Self::new(w.min, w.max, w.color, &w.label).map_err(|_| {
            serde::de::Error::custom("band label must be printable ASCII, at most five bytes and non-space if nonempty")
        })
    }
}

/// Fixed 64-byte presentation extension with at most three inline bands.
/// Safe mutators preserve collection/label invariants; whole-config validation admits numeric semantics.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AdvancedDisplay {
    /// Inline active-prefix band storage; unused entries are `Band::EMPTY`.
    bands: [Band; 3],
    /// Canonical reference value; always finite and channel-bounded in manual mode.
    pub reference: f32,
    /// Smoothing time constant: zero disables, otherwise 50..=5,000 milliseconds.
    pub smoothing_ms: u16,
    /// Peak indicator hold, 100..=60,000 milliseconds.
    pub peak_hold_ms: u16,
    /// Primary display units; compatible with the channel after admission.
    pub readout: Readout,
    /// Optional secondary pressure units; non-boost channels require `None`.
    pub secondary: Secondary,
    /// Selected extremum marker, independent of raw warning evaluation.
    pub peak_mode: PeakSelection,
    /// Reference-marker policy; manual values are not ECU targets.
    pub reference_mode: ReferenceMode,
    /// Ignition popup layout; scalar widgets require Global. Does not alter shared warning evaluation.
    pub ignition_warning: IgnitionWarningPresentation,
    /// Active prefix length, always 0..=3 through safe mutators.
    band_count: u8,
}
impl AdvancedDisplay {
    /// Return four-cylinder defaults without scalar features; one-decimal precision belongs to DisplayOptions.
    pub const fn for_ignition() -> Self {
        Self {
            bands: [Band::EMPTY; 3],
            reference: 0.0,
            smoothing_ms: 0,
            peak_hold_ms: 750,
            readout: Readout::IgnitionStatusAndNumber,
            secondary: Secondary::None,
            peak_mode: PeakSelection::Off,
            reference_mode: ReferenceMode::Off,
            ignition_warning: IgnitionWarningPresentation::Global,
            band_count: 0,
        }
    }

    /// Return defaults for `channel`; `afr` adds AFR only for lambda.
    /// Bands/smoothing/markers are off and peak hold is 750 ms; numeric reference is zero.
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
            ignition_warning: IgnitionWarningPresentation::Global,
            band_count: 0,
        }
    }

    /// Return the zero-to-three active bands, excluding inactive storage.
    pub fn bands(&self) -> &[Band] { &self.bands[..usize::from(self.band_count)] }

    /// Replace active band `index` with `label`, returning precise field context on rejection.
    /// Out-of-range index reports `BandCount`; invalid text reports `BandLabel(index)`.
    /// Failure preserves prior bytes; visibility does not affect edit validity.
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

    /// Copy at most three `bands`, returning a `BandCount` error without mutation if oversized.
    /// Success clears unused entries. Endpoint/order/contrast validation remains a separate admission step.
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
    /// Serialize nine logical fields to `s`; returns serializer output/error and excludes inactive bands.
    fn serialize<S: Serializer>(
        &self,
        s: S,
    ) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut s = s.serialize_struct("AdvancedDisplay", 9)?;
        s.serialize_field("bands", self.bands())?;
        s.serialize_field("reference", &self.reference)?;
        s.serialize_field("smoothing_ms", &self.smoothing_ms)?;
        s.serialize_field("peak_hold_ms", &self.peak_hold_ms)?;
        s.serialize_field("readout", &self.readout)?;
        s.serialize_field("secondary", &self.secondary)?;
        s.serialize_field("peak_mode", &self.peak_mode)?;
        s.serialize_field("reference_mode", &self.reference_mode)?;
        s.serialize_field("ignition_warning", &self.ignition_warning)?;
        s.end()
    }
}
impl<'de> Deserialize<'de> for AdvancedDisplay {
    /// Deserialize `d` into fixed storage; returns wire/label/capacity errors, deferring semantic validation.
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        // heapless visits one Band at a time. Its length is bounded before the
        // tiny collection is copied; no Widget collection temporary exists.
        /// Bounded logical extension; visits one band at a time before copying the tiny collection.
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Wire {
            /// At most three candidate bands, with structurally admitted labels.
            bands: heapless::Vec<Band, 3>,
            /// Candidate canonical reference.
            reference: f32,
            /// Candidate smoothing milliseconds.
            smoothing_ms: u16,
            /// Candidate peak-hold milliseconds.
            peak_hold_ms: u16,
            /// Candidate primary units.
            readout: Readout,
            /// Candidate secondary pressure units.
            secondary: Secondary,
            /// Extremum marker selection.
            peak_mode: PeakSelection,
            /// Reference marker selection.
            reference_mode: ReferenceMode,
            /// Required schema-seven ignition popup layout; scalar candidates require Global.
            ignition_warning: IgnitionWarningPresentation,
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
            ignition_warning: w.ignition_warning,
            ..Self::for_channel(Channel::Boost, false)
        };
        result
            .set_bands(&w.bands)
            .map_err(|_| serde::de::Error::custom("at most three bands"))?;
        Ok(result)
    }
}

/// Fixed warning extension flattened by the explicit [`crate::WarningSettings`] wire adapter.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdvancedWarning {
    /// Upper trigger edge for window modes; exact positive zero is required otherwise.
    pub trigger_high: f32,
    /// Upper clear edge for window modes; exact positive zero is required otherwise.
    pub clear_high: f32,
    /// Snooze seconds: zero disables, otherwise 5..=900; enabled snooze needs onboard acknowledgment.
    pub snooze_s: u16,
    /// Relative popup ordering only; does not qualify the rule as a safe vehicle alarm.
    pub priority: Priority,
    /// Retain acknowledgment state according to runtime warning policy; requires an onboard action.
    pub latch: bool,
}
impl AdvancedWarning {
    /// Non-window positive-zero edges, no latch/snooze and normal priority.
    pub const DEFAULT: Self = Self {
        trigger_high: 0.0,
        clear_high: 0.0,
        snooze_s: 0,
        priority: Priority::Normal,
        latch: false,
    };
}
impl Default for AdvancedWarning {
    /// Return [`Self::DEFAULT`].
    fn default() -> Self { Self::DEFAULT }
}

// Human fields are flat and Postcard uses precisely the same declaration order.
// serde flatten requires alloc and is unsupported by Postcard; this explicit
// adapter also preserves deny_unknown_fields and duplicate-field rejection.
impl Serialize for crate::WarningSettings {
    /// Write fourteen flat fields to `s` in fixed Postcard order; returns serializer output/error.
    /// Explicit flattening preserves no-allocation binary encoding and human field names.
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
    /// Decode fourteen flat fields from `d`; returns shape/capacity errors, with semantic admission deferred.
    /// Derived wire adapter rejects unknown/duplicate map fields and preserves binary declaration order.
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        /// Logical flattened warning wire shape; declaration order is part of the binary format.
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Wire {
            /// Candidate rule participation flag.
            enabled: bool,
            /// Allowed data modes.
            applicability: crate::Applicability,
            /// Predicate and hysteresis shape.
            mode: crate::LimitMode,
            /// Candidate lower/single trigger threshold.
            trigger: f32,
            /// Candidate lower/single clear threshold.
            clear: f32,
            /// Candidate qualifying load percentage.
            min_load_percent: f32,
            /// Candidate activation dwell milliseconds.
            on_ms: u16,
            /// Candidate clear dwell milliseconds.
            off_ms: u16,
            /// Candidate warning message, at most twenty UTF-8 bytes before ASCII admission.
            message: heapless::String<20>,
            /// Candidate upper trigger-window edge.
            trigger_high: f32,
            /// Candidate upper clear-window edge.
            clear_high: f32,
            /// Candidate snooze seconds.
            snooze_s: u16,
            /// Presentation priority.
            priority: Priority,
            /// Acknowledgment-latch policy.
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
/// Exact inline band storage contract.
const _: () = assert!(core::mem::size_of::<Band>() == 16);
/// Exact display-extension storage contract.
const _: () = assert!(core::mem::size_of::<AdvancedDisplay>() == 64);
/// Exact warning-extension storage contract.
const _: () = assert!(core::mem::size_of::<AdvancedWarning>() == 12);
