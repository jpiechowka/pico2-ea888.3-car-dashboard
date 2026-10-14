use dashboard_build_profiles::{ClockSelection, ProfileId, ResolvedClock};
use serde::{Deserialize, Serialize};

use crate::{BoundedVec, Error, Wire};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Demo,
    Vehicle,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Optimization {
    Speed3,
    Speed2,
    SizeS,
    SizeZ,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuildOptions {
    pub mode: Mode,
    pub clock: ClockSelection,
    pub optimization: Optimization,
    pub simple_outline: bool,
    pub serial_display: bool,
    pub full_frame: bool,
    pub frame_trace: bool,
    pub te_trace: bool,
    pub expert: bool,
}
impl Default for BuildOptions {
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
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NormalizedBuildOptions {
    pub schema_version: u16,
    pub requested: BuildOptions,
    pub clock: ResolvedClock,
    pub features: BoundedVec<Feature, 16>,
}
macro_rules! features {
    ($($v:ident => $s:literal),+ $(,)?) => {
        #[derive(Clone,Copy,Debug,PartialEq,Eq,Serialize,Deserialize)]
        pub enum Feature { $(#[serde(rename=$s)] $v),+ }
        impl Feature { pub fn as_str(self)->&'static str {match self {$(Self::$v=>$s),+}} }
        fn from_feature(s:&str)->Result<Feature,Error> {match s {$($s=>Ok(Feature::$v),)+ _=>Err(Error::Schema)}}
    }
}
features! { Stock=>"cpu150-spi62-1v10", Cpu132=>"cpu132-spi66-1v10", Cpu140=>"cpu140-spi70-1v10", Timing150=>"cpu150-spi15-1v10", Cpu250=>"cpu250-spi62-1v10", Cpu280=>"cpu280-spi70-1v30", Legacy290=>"cpu290-spi72-1v30", Cpu300=>"cpu300-spi75-1v30", Demo=>"demo", Vehicle=>"longan-rx12", SimpleOutline=>"simple-outline", SerialDisplay=>"serial-display", FullFrame=>"full-frame", FrameTrace=>"frame-trace", TeTrace=>"te-trace" }
impl NormalizedBuildOptions {
    pub fn feature_names(&self) -> Vec<&'static str> { self.features.iter().map(|f| f.as_str()).collect() }
}
impl Optimization {
    pub fn cargo_value(self) -> &'static str {
        match self {
            Self::Speed3 => "3",
            Self::Speed2 => "2",
            Self::SizeS => "s",
            Self::SizeZ => "z",
        }
    }
}
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
    const MAX_BYTES: usize = 4096;

    fn validate(&self) -> Result<(), Error> {
        if normalize(&self.requested)? == *self {
            Ok(())
        } else {
            Err(Error::Schema)
        }
    }
}
