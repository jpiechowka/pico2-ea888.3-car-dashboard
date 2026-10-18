use heapless::{String, Vec};
use serde::{Deserialize, Serialize};

pub const VERSION: u16 = 6;
pub const CATALOGUE_VERSION: u16 = 1;
pub const MAX_VIEWS: usize = 8;
pub const MAX_WIDGETS: usize = 64;
pub const MAX_FILE_BYTES: usize = 16 * 1024;
pub const MAX_JSON_BYTES: usize = 128 * 1024;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub version: u16,
    pub catalogue_version: u16,
    pub boot_view: u8,
    pub views: Vec<View, MAX_VIEWS>,
    pub widgets: Vec<Widget, MAX_WIDGETS>,
    pub sources: [ChannelSource; 8],
    pub warnings: WarningPresentation,
    pub fuel: Fuel,
    pub warning_colors: crate::WarningColors,
    pub controls: crate::ControlMap,
    pub history_seconds: [u16; 8],
    pub reduced_motion: bool,
    pub led: crate::LedSettings,
    pub orientation: crate::Orientation,
    pub ble: crate::BleSettings,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct View {
    pub id: u8,
    pub name: String<14>,
    #[serde(deserialize_with = "crate::orientation::required_override")]
    pub orientation: Option<crate::Orientation>,
}

impl Config {
    pub fn view_orientation(
        &self,
        id: u8,
    ) -> Option<crate::Orientation> {
        self.views
            .iter()
            .find(|view| view.id == id)
            .map(|view| view.orientation.unwrap_or(self.orientation))
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Widget {
    pub id: u16,
    pub view: u8,
    pub channel: Channel,
    pub placement: Placement,
    pub label: String<24>,
    pub display: DisplayOptions,
    pub warning: WarningSettings,
    pub gauge: crate::Gauge,
    pub colors: crate::WidgetColors,
    pub advanced: crate::AdvancedDisplay,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Channel {
    Boost,
    Oil,
    Water,
    Dsg,
    Iat,
    Egt,
    Battery,
    Lambda,
}

impl Channel {
    pub const ALL: [Self; 8] = [
        Self::Boost,
        Self::Oil,
        Self::Water,
        Self::Dsg,
        Self::Iat,
        Self::Egt,
        Self::Battery,
        Self::Lambda,
    ];

    pub const fn index(self) -> usize { self as usize }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Boost => "BOOST REL",
            Self::Oil => "OIL",
            Self::Water => "COOL",
            Self::Dsg => "DSG OIL",
            Self::Iat => "IAT",
            Self::Egt => "EGT",
            Self::Battery => "ECU V",
            Self::Lambda => "LAMBDA",
        }
    }

    pub const fn freshness_cap(self) -> u16 {
        match self {
            Self::Boost | Self::Lambda => 500,
            Self::Battery => 2_000,
            _ => 3_000,
        }
    }

    /// Bounded editor domains, not recommended engine operating limits.
    pub const fn domain(self) -> (f32, f32) {
        match self {
            Self::Boost => (-1.0, 10.0),
            Self::Battery => (0.0, 65.535),
            Self::Lambda => (0.01, 10.0),
            _ => (-100.0, 2_000.0),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Span {
    OneByOne,
    TwoByOne,
    TwoByTwo,
}

impl Span {
    pub const fn columns(self) -> u8 { if matches!(self, Self::OneByOne) { 1 } else { 2 } }

    pub const fn rows(self) -> u8 { if matches!(self, Self::TwoByTwo) { 2 } else { 1 } }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Placement {
    pub column: u8,
    pub row: u8,
    pub span: Span,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PixelRect {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
}

impl Placement {
    pub fn rect(self) -> Option<PixelRect> { self.rect_in(crate::Geometry::Landscape) }

    pub fn rect_in(
        self,
        geometry: crate::Geometry,
    ) -> Option<PixelRect> {
        let x = geometry.column_edge(self.column)?;
        let y = geometry.row_edge(self.row)?;
        let right = geometry.column_edge(self.column.checked_add(self.span.columns())?)?;
        let bottom = geometry.row_edge(self.row.checked_add(self.span.rows())?)?;
        Some(PixelRect {
            x,
            y,
            width: right - x,
            height: bottom - y,
        })
    }

    pub fn occupied(self) -> Option<u8> { self.occupied_in(crate::Geometry::Landscape) }

    pub fn occupied_in(
        self,
        geometry: crate::Geometry,
    ) -> Option<u8> {
        self.rect_in(geometry)?;
        let mut cells = 0;
        for row in self.row..self.row + self.span.rows() {
            for column in self.column..self.column + self.span.columns() {
                cells |= 1 << (row * geometry.columns() + column);
            }
        }
        Some(cells)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum GraphRange {
    Auto,
    Fixed { min: f32, max: f32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DisplayOptions {
    pub decimals: u8,
    pub graph: bool,
    pub average: bool,
    pub extrema: bool,
    pub trend: bool,
    pub afr: bool,
    pub graph_range: GraphRange,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Applicability {
    DemoOnly,
    VehicleAndDemo,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LimitMode {
    High,
    Low,
    LambdaDeviation,
    OutsideWindow,
    InsideWindow,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WarningSettings {
    pub enabled: bool,
    pub applicability: Applicability,
    pub mode: LimitMode,
    pub trigger: f32,
    pub clear: f32,
    pub min_load_percent: f32,
    pub on_ms: u16,
    pub off_ms: u16,
    pub message: String<20>,
    pub advanced: crate::AdvancedWarning,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum WarningScope {
    AllViews,
    CurrentView,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum WarningMode {
    CycleOne,
    ListThree,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WarningPresentation {
    pub scope: WarningScope,
    pub mode: WarningMode,
    pub page_ms: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DefinitionId {
    Disabled,
    ObdCoolant05,
    ObdIntake0f,
    ObdOil5c,
    ObdVoltage42,
    ObdMapMinusAmbient,
    ObdLambda24,
    ObdLambda34,
    SimosCoolant11cd,
    SimosOil202f,
    SimosIntake1001,
    SimosVoltage14a6,
    SimosMapMinusAmbient,
    SimosLambda10c0,
    DsgOil2104,
    UnresolvedEgt,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Access {
    None,
    Obd01 { pid: u8 },
    ReadDid22 { did: u16 },
    MapMinusAmbient { map_pid: u8, ambient_pid: u8 },
    DidMapMinusAmbient { map_did: u16, ambient_did: u16 },
    Unresolved,
}

/// Fixed documentary decoders, never user-authored expressions. Catalogue
/// descriptions specify reply shape, byte selection, units and applicability.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Decoder {
    None,
    TemperatureAminus40,
    Be16Millivolts,
    MapDifferenceWithScalingPrerequisites,
    WidebandLambdaWithScalingPrerequisites,
    Be16KelvinTenths,
    IntakeThreeQuartersMinus48,
    Voltage13Over128,
    SimosMapDifference,
    Be16LambdaOver1024,
    Le16SignedCelsius,
    Unresolved,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Evidence {
    Disabled,
    GenericObd,
    DocumentedCandidate,
    Unresolved,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Route {
    DeploymentProfile,
    Physical11 { request: u16, response: u16 },
    Unknown,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Poll {
    Off,
    EveryMs(u16),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChannelSource {
    pub definition: DefinitionId,
    pub access: Access,
    pub decoder: Decoder,
    pub evidence: Evidence,
    pub route: Route,
    pub poll: Poll,
    pub freshness_ms: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BlendBasis {
    Mass,
    Volume,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompositionOrigin {
    Assumed,
    Measured,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum DensityReference {
    UnknownAssumed,
    CommonCelsius(f32),
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Blend {
    pub percent: f32,
    pub basis: BlendBasis,
    pub composition_origin: CompositionOrigin,
    pub density_reference: DensityReference,
    pub gasoline_stoich: f32,
    pub ethanol_stoich: f32,
    pub gasoline_density: f32,
    pub ethanol_density: f32,
}

impl Blend {
    /// Opt-in example inputs, not measured Shell fuel or a measured density
    /// reference temperature. E5 does not establish this percentage.
    pub const fn illustrative() -> Self {
        Self {
            percent: 0.0,
            basis: BlendBasis::Volume,
            composition_origin: CompositionOrigin::Assumed,
            density_reference: DensityReference::UnknownAssumed,
            gasoline_stoich: 14.7,
            ethanol_stoich: 9.0,
            gasoline_density: 0.737,
            ethanol_density: 0.789,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Fuel {
    EquivalentGasoline,
    Stoichiometric { ratio: f32 },
    EthanolBlend(Blend),
}

// Host pointer widths are not a substitute for the target's actual layout.
// v6 adds the 16-byte BLE selection. Whole-image RAM/stack gates remain separate.
#[cfg(target_pointer_width = "32")]
const _: () = assert!(core::mem::size_of::<Config>() <= 14_596);
