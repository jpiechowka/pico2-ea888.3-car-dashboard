//! Portable bounded model. Serde enforces shape/capacity; [`crate::validate()`] admits semantics.
//! Numeric display limits and documentary decoder selections do not qualify vehicle requests.
use heapless::{String, Vec};
use serde::{Deserialize, Serialize};

/// Current dashboard payload schema; older supported versions migrate before admission.
pub const VERSION: u16 = 6;
/// Fixed documentary source-catalogue revision expected by validation.
pub const CATALOGUE_VERSION: u16 = 1;
/// Maximum named views retained in one configuration.
pub const MAX_VIEWS: usize = 8;
/// Maximum widget instances across all views, not per view.
pub const MAX_WIDGETS: usize = 64;
/// Maximum accepted dashboard binary file bytes, including its envelope.
pub const MAX_FILE_BYTES: usize = 16 * 1024;
/// Maximum accepted JSON source bytes before bounded deserialization.
pub const MAX_JSON_BYTES: usize = 128 * 1024;

/// Editable candidate; fields may be invalid until wrapped by [`crate::ValidatedConfig`].
/// Collections are bounded; strings hold UTF-8 bytes, with printable-ASCII rules at validation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Payload schema, equal to [`VERSION`] after migration.
    pub version: u16,
    /// Documentary catalogue version, equal to [`CATALOGUE_VERSION`].
    pub catalogue_version: u16,
    /// Nonzero ID of a retained view, independent of its array position.
    pub boot_view: u8,
    /// One to eight uniquely identified views in navigation order.
    pub views: Vec<View, MAX_VIEWS>,
    /// Zero to sixty-four instances; admission rejects overlap within each view.
    pub widgets: Vec<Widget, MAX_WIDGETS>,
    /// Source selection indexed by [`Channel::index`]; does not authorize traffic.
    pub sources: [ChannelSource; 8],
    /// Dashboard warning selection and page timing.
    pub warnings: WarningPresentation,
    /// Explicit stoichiometric assumption used only for lambda-to-AFR conversion.
    pub fuel: Fuel,
    /// Shared popup foreground/background with validated contrast.
    pub warning_colors: crate::WarningColors,
    /// Screen-specific input routes with a reserved Y recovery hold.
    pub controls: crate::ControlMap,
    /// Per-channel history duration, 10..=900 seconds, indexed by channel.
    pub history_seconds: [u16; 8],
    /// Reduce decorative animation; does not change sensor freshness or warning thresholds.
    pub reduced_motion: bool,
    /// Dashboard RGB policy; independent of standalone BLE-probe status colors.
    pub led: crate::LedSettings,
    /// Default mounting inherited by views without an override and by diagnostic screens.
    pub orientation: crate::Orientation,
    /// Optional adapter selection and bounded transport timeouts; contains no bond secrets.
    pub ble: crate::BleSettings,
}

/// Named view; widgets reference its ID rather than its position in [`Config::views`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct View {
    /// Unique nonzero identifier within the configuration.
    pub id: u8,
    /// Nonblank printable ASCII, at most fourteen bytes after admission.
    pub name: String<14>,
    /// Mounting override; explicit JSON `null` inherits the device default.
    #[serde(deserialize_with = "crate::orientation::required_override")]
    pub orientation: Option<crate::Orientation>,
}

impl Config {
    /// Return effective mounting for the first view matching `id`, or `None` if absent.
    /// No validation occurs here; admission separately rejects duplicate IDs.
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

/// One channel presentation and warning rule, placed within its referenced view.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Widget {
    /// Unique nonzero identifier across every view.
    pub id: u16,
    /// Existing view ID; not an array index.
    pub view: u8,
    /// Shared measurement channel; multiple widgets may present the same sample.
    pub channel: Channel,
    /// Logical cell position and span; 2×1 remains horizontal in portrait.
    pub placement: Placement,
    /// Printable nonblank label; admission allows twelve bytes per spanned column.
    pub label: String<24>,
    /// Numeric precision and optional graph/statistic visibility.
    pub display: DisplayOptions,
    /// Raw-sample warning rule, independent of presentation smoothing.
    pub warning: WarningSettings,
    /// Gauge shape and canonical range; not an engine operating-limit recommendation.
    pub gauge: crate::Gauge,
    /// Per-role RGB565 colors with required pairwise contrast.
    pub colors: crate::WidgetColors,
    /// Readout units, timing, peak/reference and bounded gauge-band presentation.
    pub advanced: crate::AdvancedDisplay,
}

/// Stable eight-channel order shared by arrays, source selection and runtime models.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Channel {
    /// Relative pressure in canonical bar; requires qualified MAP and ambient inputs.
    Boost,
    /// Engine oil temperature in canonical degrees Celsius.
    Oil,
    /// Engine coolant temperature in canonical degrees Celsius.
    Water,
    /// Transmission oil temperature in canonical degrees Celsius; source remains qualification-dependent.
    Dsg,
    /// Intake-air temperature in canonical degrees Celsius.
    Iat,
    /// Measured exhaust-gas temperature in canonical degrees Celsius; unresolved sources stay unavailable.
    Egt,
    /// ECU module supply voltage in volts, not necessarily a battery-post measurement.
    Battery,
    /// Dimensionless actual lambda; AFR requires an explicit fuel assumption.
    Lambda,
}

impl Channel {
    /// Complete stable array/index order; changing it changes serialized channel-array meaning.
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

    /// Return the zero-based index into eight-element channel arrays.
    pub const fn index(self) -> usize { self as usize }

    /// Return the default short ASCII label; custom widget labels are independent.
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

    /// Return maximum admitted freshness in milliseconds, not a polling guarantee.
    pub const fn freshness_cap(self) -> u16 {
        match self {
            Self::Boost | Self::Lambda => 500,
            Self::Battery => 2_000,
            _ => 3_000,
        }
    }

    /// Return inclusive editor bounds in canonical bar, volts, lambda or degrees Celsius.
    /// These are admission domains, not recommended engine operating limits.
    pub const fn domain(self) -> (f32, f32) {
        match self {
            Self::Boost => (-1.0, 10.0),
            Self::Battery => (0.0, 65.535),
            Self::Lambda => (0.01, 10.0),
            _ => (-100.0, 2_000.0),
        }
    }
}

/// Supported rectangular cell footprints; orientation never transposes a span.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Span {
    /// One column by one row.
    OneByOne,
    /// Two horizontal columns by one row.
    TwoByOne,
    /// Two columns by two rows.
    TwoByTwo,
}

impl Span {
    /// Return horizontal cell count, always one or two.
    pub const fn columns(self) -> u8 { if matches!(self, Self::OneByOne) { 1 } else { 2 } }

    /// Return vertical cell count, always one or two.
    pub const fn rows(self) -> u8 { if matches!(self, Self::TwoByTwo) { 2 } else { 1 } }
}

/// Candidate logical grid rectangle; checked conversion prevents overflow/out-of-panel access.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Placement {
    /// Zero-based left column, interpreted using the effective view geometry.
    pub column: u8,
    /// Zero-based top dashboard row, below the fixed header.
    pub row: u8,
    /// Horizontal/vertical cell extent.
    pub span: Span,
}

/// Logical pixel rectangle with an exclusive right and bottom edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PixelRect {
    /// Left pixel coordinate.
    pub x: u16,
    /// Top pixel coordinate, including header offset for dashboard placements.
    pub y: u16,
    /// Horizontal pixel extent.
    pub width: u16,
    /// Vertical pixel extent; row rounding can make equally sized spans differ by one pixel.
    pub height: u16,
}

impl Placement {
    /// Return landscape pixel bounds or `None` for overflow/out-of-grid placement.
    /// Orientation-aware callers must use [`Self::rect_in`].
    pub fn rect(self) -> Option<PixelRect> { self.rect_in(crate::Geometry::Landscape) }

    /// Return pixel bounds within `geometry`, or `None` if addition overflows or any edge is outside.
    /// No overlap check occurs here; configuration validation handles inter-widget occupancy.
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

    /// Return a row-major landscape occupancy mask, or `None` for invalid placement.
    pub fn occupied(self) -> Option<u8> { self.occupied_in(crate::Geometry::Landscape) }

    /// Return row-major eight-cell mask in `geometry`, or `None` for invalid bounds.
    /// Bit zero is the top-left dashboard cell; validation precedes all shifts/additions.
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

/// Graph scale selection in canonical channel units; independent of gauge clipping.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum GraphRange {
    /// Runtime expands its configured initial range to include qualified history.
    Auto,
    /// Fixed finite ordered limits; out-of-range samples are not numerically falsified.
    Fixed {
        /// Lower canonical limit within the channel's editor domain.
        min: f32,
        /// Upper canonical limit, strictly greater than `min`.
        max: f32,
    },
}

/// Per-widget numeric/graph/statistic presentation; does not alter raw measurements.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DisplayOptions {
    /// Numeric fractional digits, 0..=3 after validation.
    pub decimals: u8,
    /// Show the historical trace when the widget layout supports it.
    pub graph: bool,
    /// Show accumulated average decoration.
    pub average: bool,
    /// Show accumulated minimum and maximum decoration.
    pub extrema: bool,
    /// Show time-based trend decoration.
    pub trend: bool,
    /// Legacy AFR mirror; must equal `advanced.readout.includes_afr()` and be lambda-only.
    pub afr: bool,
    /// Canonical historical trace limits; fixed endpoints must be finite and ordered.
    pub graph_range: GraphRange,
}

/// Data modes where an enabled warning rule may participate.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Applicability {
    /// Synthetic/demo operation only; cannot enable live vehicle warnings.
    DemoOnly,
    /// Both modes, still subject to runtime sample quality and deployment qualification.
    VehicleAndDemo,
}

/// Warning predicate and hysteresis ordering; thresholds use raw canonical samples.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LimitMode {
    /// Trigger at/above the high threshold; clear at/below a strictly smaller threshold.
    High,
    /// Trigger at/below the low threshold; clear at/above a strictly larger threshold.
    Low,
    /// Lambda-only absolute deviation from target with load qualification and high-style hysteresis.
    LambdaDeviation,
    /// Trigger outside the outer window; clear inside the smaller inner window.
    OutsideWindow,
    /// Trigger inside the inner window; clear outside the larger outer window.
    InsideWindow,
}

/// Raw-sample warning settings; custom wire adapters flatten [`crate::AdvancedWarning`].
/// Disabled rules still require valid fields, avoiding latent invalid state on re-enabling.
#[derive(Clone, Debug, PartialEq)]
pub struct WarningSettings {
    /// Whether this valid rule participates in warning evaluation.
    pub enabled: bool,
    /// Allowed data modes; does not replace sample qualification.
    pub applicability: Applicability,
    /// Predicate and threshold-ordering rule.
    pub mode: LimitMode,
    /// Trigger threshold or lower window edge; lambda deviation admits 0..=2.
    pub trigger: f32,
    /// Clear threshold or lower clear-window edge; ordered hysteresis is mandatory.
    pub clear: f32,
    /// Minimum load percent for load-qualified lambda warning evaluation, 0..=100.
    pub min_load_percent: f32,
    /// Required continuous trigger duration in milliseconds, 1..=30,000.
    pub on_ms: u16,
    /// Required continuous clear duration in milliseconds, 1..=30,000.
    pub off_ms: u16,
    /// Printable nonblank warning label, at most twenty bytes.
    pub message: String<20>,
    /// Upper window edges, latching and snooze controls.
    pub advanced: crate::AdvancedWarning,
}

/// View scope considered by the dashboard warning presentation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum WarningScope {
    /// Consider eligible rules from every configured view.
    AllViews,
    /// Consider eligible rules only from the selected view.
    CurrentView,
}
/// Bounded popup page layout; rule evaluation is independent of presentation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum WarningMode {
    /// Present one eligible warning per page.
    CycleOne,
    /// Present up to three eligible warnings per page.
    ListThree,
}
/// Dashboard popup selection and time-based paging policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WarningPresentation {
    /// Eligible view set.
    pub scope: WarningScope,
    /// Number of warnings presented per page.
    pub mode: WarningMode,
    /// Page interval in milliseconds, 500..=10,000 after validation.
    pub page_ms: u16,
}

/// Closed documentary catalogue key; see [`crate::definition`] for exact byte/formula provenance.
/// Candidate membership never proves compatibility with the installed ECU or enables traffic.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DefinitionId {
    /// No request and no decoder.
    Disabled,
    /// Generic service 01/PID 05 coolant-temperature definition.
    ObdCoolant05,
    /// Generic service 01/PID 0F intake-temperature definition.
    ObdIntake0f,
    /// Generic service 01/PID 5C oil-temperature definition.
    ObdOil5c,
    /// Generic service 01/PID 42 control-module voltage definition.
    ObdVoltage42,
    /// Candidate PID 0B minus PID 33 pressure; scaling/provenance constraints unresolved.
    ObdMapMinusAmbient,
    /// Candidate PID 24 lambda; sensor/scaling qualification unresolved.
    ObdLambda24,
    /// Candidate PID 34 lambda; sensor/scaling qualification unresolved.
    ObdLambda34,
    /// Documentary Simos DID 11CD coolant candidate.
    SimosCoolant11cd,
    /// Documentary Simos DID 202F oil candidate.
    SimosOil202f,
    /// Documentary Simos DID 1001 intake candidate.
    SimosIntake1001,
    /// Documentary Simos DID 14A6 voltage candidate.
    SimosVoltage14a6,
    /// Documentary DID 39C0 minus 13CA derived-pressure candidate.
    SimosMapMinusAmbient,
    /// Documentary DID 10C0 actual-lambda candidate.
    SimosLambda10c0,
    /// Historical DSG DID 2104 lead; not exact DQ381 support evidence.
    DsgOil2104,
    /// No established measured-EGT request/decoder; modeled temperatures are not substitutes.
    UnresolvedEgt,
}

/// Declarative request shape, constrained to the selected catalogue definition by validation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Access {
    /// No request metadata.
    None,
    /// Generic current-data service 01 request.
    Obd01 {
        /// Parameter identifier following service 01.
        pid: u8,
    },
    /// UDS ReadDataByIdentifier metadata; candidate routes may remain disabled.
    ReadDid22 {
        /// Big-endian wire identifier following service 22.
        did: u16,
    },
    /// Two independent pressure inputs requiring common provenance and bounded age/skew.
    MapMinusAmbient {
        /// Absolute manifold-pressure PID.
        map_pid: u8,
        /// Absolute atmospheric-pressure PID.
        ambient_pid: u8,
    },
    /// Documentary UDS pressure pair; neither absolute input alone is relative boost.
    DidMapMinusAmbient {
        /// Absolute manifold-pressure DID.
        map_did: u16,
        /// Absolute atmospheric-pressure DID.
        ambient_did: u16,
    },
    /// Request bytes have not been established.
    Unresolved,
}

/// Fixed documentary decoders, never user-authored expressions. Catalogue
/// descriptions specify reply shape, byte selection, units and applicability.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Decoder {
    /// No conversion.
    None,
    /// Unsigned one-byte temperature offset; see the selected definition's reply shape.
    TemperatureAminus40,
    /// Unsigned big-endian millivolt data converted to volts.
    Be16Millivolts,
    /// Unqualified two-PID pressure difference; no executable binding implied.
    MapDifferenceWithScalingPrerequisites,
    /// Unqualified wideband-lambda conversion; scaling and sensor identity remain required.
    WidebandLambdaWithScalingPrerequisites,
    /// Documentary big-endian oil-temperature conversion with the source's exact offset.
    Be16KelvinTenths,
    /// Documentary unsigned intake-temperature conversion.
    IntakeThreeQuartersMinus48,
    /// Documentary unsigned voltage conversion.
    Voltage13Over128,
    /// Documentary two-DID pressure conversion with distinct input scalings.
    SimosMapDifference,
    /// Documentary unsigned big-endian actual-lambda conversion.
    Be16LambdaOver1024,
    /// Documentary signed little-endian Celsius value; exact TCU semantics unqualified.
    Le16SignedCelsius,
    /// No conversion formula established.
    Unresolved,
}

/// Documentary strength of a catalogue entry, separate from live deployment qualification.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Evidence {
    /// Selection intentionally supplies no data.
    Disabled,
    /// Generic OBD definition; actual controller support still needs capability qualification.
    GenericObd,
    /// Source-backed research candidate, not an admitted live binding.
    DocumentedCandidate,
    /// Required measurement/request evidence is unavailable.
    Unresolved,
}
/// Request/response routing selection; validation never establishes actual bus reachability.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Route {
    /// Use the explicitly qualified deployment route supplied by runtime admission.
    DeploymentProfile,
    /// Explicit distinct 11-bit CAN IDs; functional request 0x7DF is rejected.
    Physical11 {
        /// Physical request ID, 0..=0x7FF excluding 0x7DF and `response`.
        request: u16,
        /// Response ID, 0..=0x7FF and distinct from `request`.
        response: u16,
    },
    /// No route established; resolution remains unavailable.
    Unknown,
}
/// Requested acquisition schedule; disabled or unqualified sources cannot become live by setting a period.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Poll {
    /// Do not schedule this source.
    Off,
    /// Period bounded by [`crate::minimum_period`] and 10,000 milliseconds.
    EveryMs(
        /// Requested interval in milliseconds; not a real-time delivery guarantee.
        u16,
    ),
}
/// Per-channel documentary binding; duplicated metadata must match its closed catalogue entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChannelSource {
    /// Catalogue identity validated against the owning channel.
    pub definition: DefinitionId,
    /// Exact catalogue request metadata; arbitrary user-authored requests are rejected.
    pub access: Access,
    /// Exact catalogue conversion identity.
    pub decoder: Decoder,
    /// Exact catalogue evidence classification.
    pub evidence: Evidence,
    /// Explicit or deployment-supplied routing selection.
    pub route: Route,
    /// Requested schedule, subject to source/transport admission.
    pub poll: Poll,
    /// Accepted sample lifetime in milliseconds, 1..=channel freshness cap.
    pub freshness_ms: u16,
}

/// Basis of the supplied ethanol percentage; volume input requires density conversion.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BlendBasis {
    /// Percentage of total blend mass.
    Mass,
    /// Percentage of total blend volume at the declared density reference.
    Volume,
}
/// Provenance label for the supplied composition, not an automatic sensor observation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompositionOrigin {
    /// Manually assumed composition; a pump E5 label is not a measured percentage.
    Assumed,
    /// User declares a measured composition; measured volume requires a common density temperature.
    Measured,
}
/// Temperature basis for component densities; no temperature compensation is performed.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum DensityReference {
    /// Common reference is unknown/assumed; disallowed for declared measured volume composition.
    UnknownAssumed,
    /// Both densities refer to one finite temperature.
    CommonCelsius(
        /// Reference degrees Celsius, -30..=60 after validation.
        f32,
    ),
}
/// Explicit two-component stoichiometric assumption; accepted bounds are modeling limits.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Blend {
    /// Ethanol mass/volume percent according to `basis`, finite 0..=100.
    pub percent: f32,
    /// Mass fractions are direct; volume fractions are converted using both densities.
    pub basis: BlendBasis,
    /// User-supplied composition provenance, not ECU evidence.
    pub composition_origin: CompositionOrigin,
    /// Shared density temperature declaration; no thermal expansion model is applied.
    pub density_reference: DensityReference,
    /// Gasoline stoichiometric air/fuel mass ratio, 5..=25 and greater than ethanol's ratio.
    pub gasoline_stoich: f32,
    /// Ethanol stoichiometric air/fuel mass ratio, 5..=25.
    pub ethanol_stoich: f32,
    /// Gasoline density in kg/L, finite 0.5..=1.2.
    pub gasoline_density: f32,
    /// Ethanol density in kg/L, finite 0.5..=1.2.
    pub ethanol_density: f32,
}

impl Blend {
    /// Return opt-in example inputs, not measured Shell fuel or a measured density
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

/// Assumption used to derive equivalent AFR from lambda; never a measured/commanded AFR target.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Fuel {
    /// Conventional 14.7 air/fuel mass-ratio display equivalence.
    EquivalentGasoline,
    /// Explicit user-selected stoichiometric ratio.
    Stoichiometric {
        /// Finite air/fuel mass ratio in 5..=25.
        ratio: f32,
    },
    /// Calculate a mass-weighted ratio from the declared composition and densities.
    EthanolBlend(
        /// Bounded two-component model validated before arithmetic.
        Blend,
    ),
}

// Host pointer widths are not a substitute for the target's actual layout.
// v6 adds the 16-byte BLE selection. Whole-image RAM/stack gates remain separate.
#[cfg(target_pointer_width = "32")]
/// 32-bit schema storage ceiling; does not establish whole-image RAM or stack headroom.
const _: () = assert!(core::mem::size_of::<Config>() <= 14_596);
