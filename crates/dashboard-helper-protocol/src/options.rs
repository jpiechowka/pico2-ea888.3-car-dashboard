//! Closed Studio build options and canonical Cargo-feature derivation.
//! Normalization checks representability and Expert acknowledgement, not electrical stability.

use dashboard_build_profiles::{ClockSelection, ProfileId, ResolvedClock};
use serde::{Deserialize, Serialize};

use crate::{BoundedVec, Error, Wire};

/// Acquisition implementation selected for the dashboard image; the standalone probe is separate.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    /// Shared synthetic signal generator; no real vehicle samples.
    Demo,
    /// Optional BLE adapter prototype; qualification rules still gate ECU traffic.
    Vehicle,
}
/// Cargo release optimization override, serialized by stable symbolic name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Optimization {
    /// Rust `opt-level=3`; favor execution speed.
    Speed3,
    /// Rust `opt-level=2`; use Cargo's level-two optimization setting.
    Speed2,
    /// Rust `opt-level=s`; favor code size.
    SizeS,
    /// Rust `opt-level=z`; prioritize size, including disabling loop vectorization.
    SizeZ,
}
/// Requested dashboard build configuration; decoding alone does not normalize or admit clocks.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuildOptions {
    /// Synthetic or BLE acquisition route; exactly one is emitted as a feature.
    pub mode: Mode,
    /// Named profile or custom requested CPU/SPI frequency and voltage tuple.
    pub clock: ClockSelection,
    /// Release optimization override, applied outside Cargo's feature list.
    pub optimization: Optimization,
    /// Use the simplified display outline implementation.
    pub simple_outline: bool,
    /// Serialize display work through the optional display feature path.
    pub serial_display: bool,
    /// Force full-frame transfers instead of eligible partial updates.
    pub full_frame: bool,
    /// Enable bounded frame/pipeline trace instrumentation.
    pub frame_trace: bool,
    /// Enable tearing-effect signal trace instrumentation; no scanout guarantee follows.
    pub te_trace: bool,
    /// Explicit acknowledgement permitting Expert/custom clock selection.
    pub expert: bool,
}
impl Default for BuildOptions {
    /// Return Demo/stock/level-3 with simple outlines and both traces enabled; Expert is off.
    fn default() -> Self {
        Self {
            mode: Mode::Demo,
            clock: ClockSelection::Named(ProfileId::Stock),
            optimization: Optimization::Speed3,
            simple_outline: true,
            serial_display: false,
            full_frame: false,
            frame_trace: true,
            te_trace: true,
            expert: false,
        }
    }
}
/// Canonical request plus resolved physical clock metadata and sorted, closed Cargo features.
/// [`Wire::validate`] rejects any supplied derived value that differs from recomputation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NormalizedBuildOptions {
    /// Canonical format revision; currently exactly 1.
    pub schema_version: u16,
    /// Original requested settings, including explicit Expert acknowledgement.
    pub requested: BuildOptions,
    /// Resolved PLL, actual peripheral rates and voltage; not measured device observations.
    pub clock: ResolvedClock,
    /// At most sixteen features, sorted lexicographically with no caller-defined strings.
    pub features: BoundedVec<Feature, 16>,
}
/// Define the closed feature enum and exact case-sensitive Cargo spelling from `$v`, `$s`, `$doc` entries.
macro_rules! features {
    ($($v:ident => $s:literal, $doc:literal);+ $(;)?) => {
        /// Admitted Cargo features; profile labels are requests, not guaranteed achieved SPI frequencies.
        #[derive(Clone,Copy,Debug,PartialEq,Eq,Serialize,Deserialize)]
        pub enum Feature { $(#[doc=$doc] #[serde(rename=$s)] $v),+ }
        impl Feature {
            /// Return the exact static Cargo feature spelling.
            pub fn as_str(self)->&'static str {match self {$(Self::$v=>$s),+}}
        }
        /// Parse exact feature spelling `s`; return its closed variant or [`Error::Schema`].
        fn from_feature(s:&str)->Result<Feature,Error> {match s {$($s=>Ok(Feature::$v),)+ _=>Err(Error::Schema)}}
    }
}
features! {
    Stock=>"cpu150-spi62-1v10", "Stock CPU profile; actual SPI divisor is resolved separately.";
    Cpu132=>"cpu132-spi66-1v10", "Expert 132/66 MHz profile at 1.10 V.";
    Cpu140=>"cpu140-spi70-1v10", "Expert 140/70 MHz profile at 1.10 V.";
    Timing150=>"cpu150-spi15-1v10", "Slow-SPI 150/15 MHz timing profile at 1.10 V; no Expert gate.";
    Cpu250=>"cpu250-spi62-1v10", "Expert 250/62.5 MHz requested profile at 1.10 V.";
    Cpu280=>"cpu280-spi70-1v30", "Expert 280/70 MHz profile at 1.30 V; device observations are not electrical qualification.";
    Legacy290=>"cpu290-spi72-1v30", "Legacy Expert 290/72.5 MHz request with historical PLL rounding; retained for compatibility.";
    Cpu300=>"cpu300-spi75-1v30", "Expert 300/75 MHz profile at 1.30 V.";
    Demo=>"demo", "Synthetic acquisition implementation.";
    Vehicle=>"ble-obd", "BLE adapter prototype acquisition implementation.";
    SimpleOutline=>"simple-outline", "Simplified widget outline rendering.";
    SerialDisplay=>"serial-display", "Serialized display execution mode.";
    FullFrame=>"full-frame", "Full-frame transfer override.";
    FrameTrace=>"frame-trace", "Frame ownership/timing trace instrumentation.";
    TeTrace=>"te-trace", "Display tearing-effect signal trace instrumentation."
}
impl NormalizedBuildOptions {
    /// Return feature spellings in stored order; canonical instances are lexicographically sorted.
    pub fn feature_names(&self) -> Vec<&'static str> { self.features.iter().map(|f| f.as_str()).collect() }
}
impl Optimization {
    /// Return the Cargo `opt-level` value: `3`, `2`, `s`, or `z`.
    pub fn cargo_value(self) -> &'static str {
        match self {
            Self::Speed3 => "3",
            Self::Speed2 => "2",
            Self::SizeS => "s",
            Self::SizeZ => "z",
        }
    }
}
/// Resolve `options` into a canonical request/clock/features record and return it.
/// Reject unacknowledged Expert/custom profiles before clock resolution, then reject unrepresentable
/// clocks. Custom clocks emit no named-profile feature; mode is always included. No build is launched.
pub fn normalize(options: &BuildOptions) -> Result<NormalizedBuildOptions, Error> {
    let mut features = Vec::new();
    match options.clock {
        ClockSelection::Named(id) => {
            let p = dashboard_build_profiles::catalogue()
                .iter()
                .find(|p| p.id == id)
                .ok_or(Error::Schema)?;
            if p.requires_expert && !options.expert {
                return Err(Error::ExpertRequired);
            }
            features.push(from_feature(p.feature)?);
        }
        ClockSelection::Custom(_) if !options.expert => return Err(Error::ExpertRequired),
        ClockSelection::Custom(_) => {}
    }
    features.push(match options.mode {
        Mode::Demo => Feature::Demo,
        Mode::Vehicle => Feature::Vehicle,
    });
    for (enabled, feature) in [
        (options.simple_outline, Feature::SimpleOutline),
        (options.serial_display, Feature::SerialDisplay),
        (options.full_frame, Feature::FullFrame),
        (options.frame_trace, Feature::FrameTrace),
        (options.te_trace, Feature::TeTrace),
    ] {
        if enabled {
            features.push(feature);
        }
    }
    features.sort_by_key(|f| f.as_str());
    Ok(NormalizedBuildOptions {
        schema_version: 1,
        requested: options.clone(),
        clock: dashboard_build_profiles::resolve(options.clock).map_err(|_| Error::Clock)?,
        features: BoundedVec::new(features)?,
    })
}
impl Wire for NormalizedBuildOptions {
    /// Maximum JSON envelope size in bytes, including received whitespace.
    const MAX_BYTES: usize = 4096;

    /// Return success only when complete normalization matches every stored field and revision.
    fn validate(&self) -> Result<(), Error> {
        if normalize(&self.requested)? == *self {
            Ok(())
        } else {
            Err(Error::Schema)
        }
    }
}
