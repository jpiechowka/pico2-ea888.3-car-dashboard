//! Frozen host-only v1/v2 wire schemas. Firmware accepts only the current version.
use serde::{Deserialize, Serialize};

use crate::*;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LegacyView {
    id: u8,
    name: heapless::String<14>,
}

pub(crate) fn upgrade_views(views: heapless::Vec<LegacyView, MAX_VIEWS>) -> heapless::Vec<View, MAX_VIEWS> {
    // One output per input into the same bounded capacity.
    views
        .into_iter()
        .map(|view| {
            View {
                id: view.id,
                name: view.name,
                orientation: None,
            }
        })
        .collect()
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LegacyConfig {
    version: u16,
    catalogue_version: u16,
    boot_view: u8,
    views: heapless::Vec<LegacyView, MAX_VIEWS>,
    widgets: heapless::Vec<LegacyWidget, MAX_WIDGETS>,
    sources: [ChannelSource; 8],
    warnings: OldWarningPresentation,
    fuel: Fuel,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct LegacyWidget {
    id: u16,
    view: u8,
    channel: Channel,
    placement: Placement,
    label: heapless::String<24>,
    display: OldDisplayOptions,
    warning: OldWarningSettings,
}
impl LegacyConfig {
    pub(crate) fn upgrade(self) -> Result<ValidatedConfig, ConfigError> {
        if self.version != 1 {
            return Err(ConfigError::Version);
        }
        let mut widgets = heapless::Vec::new();
        for w in self.widgets {
            widgets
                .push(Widget {
                    id: w.id,
                    view: w.view,
                    channel: w.channel,
                    placement: w.placement,
                    label: w.label,
                    advanced: AdvancedDisplay::for_channel(w.channel, w.display.afr),
                    display: w.display.into(),
                    warning: w.warning.into(),
                    gauge: Gauge::for_channel(w.channel),
                    colors: WidgetColors::default(),
                })
                .map_err(|_| ConfigError::Payload)?;
        }
        validate(Config {
            version: VERSION,
            catalogue_version: self.catalogue_version,
            boot_view: self.boot_view,
            views: upgrade_views(self.views),
            widgets,
            sources: self.sources,
            warnings: self.warnings.into(),
            fuel: self.fuel,
            warning_colors: WarningColors::default(),
            controls: ControlMap::default(),
            history_seconds: [120; 8],
            reduced_motion: false,
            led: LedSettings::default(),
            orientation: Orientation::Landscape,
        })
        .map_err(ConfigError::Validation)
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LegacyConfigV2 {
    version: u16,
    catalogue_version: u16,
    boot_view: u8,
    views: heapless::Vec<LegacyView, MAX_VIEWS>,
    widgets: heapless::Vec<LegacyWidgetV2, MAX_WIDGETS>,
    sources: [ChannelSource; 8],
    warnings: OldWarningPresentation,
    fuel: Fuel,
    warning_colors: WarningColors,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct LegacyWidgetV2 {
    id: u16,
    view: u8,
    channel: Channel,
    placement: Placement,
    label: heapless::String<24>,
    display: OldDisplayOptions,
    warning: OldWarningSettings,
    gauge: OldGauge,
    colors: WidgetColors,
}
impl LegacyConfigV2 {
    pub(crate) fn upgrade(self) -> Result<ValidatedConfig, ConfigError> {
        if self.version != 2 {
            return Err(ConfigError::Version);
        }
        let mut widgets = heapless::Vec::new();
        for w in self.widgets {
            widgets
                .push(Widget {
                    id: w.id,
                    view: w.view,
                    channel: w.channel,
                    placement: w.placement,
                    label: w.label,
                    advanced: AdvancedDisplay::for_channel(w.channel, w.display.afr),
                    display: w.display.into(),
                    warning: w.warning.into(),
                    gauge: w.gauge.into(),
                    colors: w.colors,
                })
                .map_err(|_| ConfigError::Payload)?;
        }
        validate(Config {
            version: VERSION,
            catalogue_version: self.catalogue_version,
            boot_view: self.boot_view,
            views: upgrade_views(self.views),
            widgets,
            sources: self.sources,
            warnings: self.warnings.into(),
            fuel: self.fuel,
            warning_colors: self.warning_colors,
            controls: ControlMap::default(),
            history_seconds: [120; 8],
            reduced_motion: false,
            led: LedSettings::default(),
            orientation: Orientation::Landscape,
        })
        .map_err(ConfigError::Validation)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum OldGraphRange {
    Auto,
    Fixed { min: f32, max: f32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OldDisplayOptions {
    pub decimals: u8,
    pub graph: bool,
    pub average: bool,
    pub extrema: bool,
    pub trend: bool,
    pub afr: bool,
    pub graph_range: OldGraphRange,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OldApplicability {
    DemoOnly,
    VehicleAndDemo,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OldLimitMode {
    High,
    Low,
    LambdaDeviation,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OldWarningSettings {
    pub enabled: bool,
    pub applicability: OldApplicability,
    pub mode: OldLimitMode,
    pub trigger: f32,
    pub clear: f32,
    pub min_load_percent: f32,
    pub on_ms: u16,
    pub off_ms: u16,
    pub message: heapless::String<20>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OldWarningScope {
    AllViews,
    CurrentView,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OldWarningMode {
    CycleOne,
    ListThree,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OldWarningPresentation {
    pub scope: OldWarningScope,
    pub mode: OldWarningMode,
    pub page_ms: u16,
}

impl From<OldGraphRange> for GraphRange {
    fn from(value: OldGraphRange) -> Self { value.convert() }
}
impl OldGraphRange {
    fn convert(self) -> GraphRange {
        match self {
            OldGraphRange::Auto => GraphRange::Auto,
            OldGraphRange::Fixed { min, max } => GraphRange::Fixed { min, max },
        }
    }
}
impl From<OldDisplayOptions> for DisplayOptions {
    fn from(value: OldDisplayOptions) -> Self { value.convert() }
}
impl OldDisplayOptions {
    fn convert(self) -> DisplayOptions {
        DisplayOptions {
            decimals: self.decimals,
            graph: self.graph,
            average: self.average,
            extrema: self.extrema,
            trend: self.trend,
            afr: self.afr,
            graph_range: self.graph_range.into(),
        }
    }
}
impl From<OldApplicability> for Applicability {
    fn from(value: OldApplicability) -> Self { value.convert() }
}
impl OldApplicability {
    fn convert(self) -> Applicability {
        match self {
            OldApplicability::DemoOnly => Applicability::DemoOnly,
            OldApplicability::VehicleAndDemo => Applicability::VehicleAndDemo,
        }
    }
}
impl From<OldLimitMode> for LimitMode {
    fn from(value: OldLimitMode) -> Self { value.convert() }
}
impl OldLimitMode {
    fn convert(self) -> LimitMode {
        match self {
            OldLimitMode::High => LimitMode::High,
            OldLimitMode::Low => LimitMode::Low,
            OldLimitMode::LambdaDeviation => LimitMode::LambdaDeviation,
        }
    }
}
impl From<OldWarningSettings> for WarningSettings {
    fn from(value: OldWarningSettings) -> Self { value.convert() }
}
impl OldWarningSettings {
    fn convert(self) -> WarningSettings {
        WarningSettings {
            enabled: self.enabled,
            applicability: self.applicability.into(),
            mode: self.mode.into(),
            trigger: self.trigger,
            clear: self.clear,
            min_load_percent: self.min_load_percent,
            on_ms: self.on_ms,
            off_ms: self.off_ms,
            message: self.message,
            advanced: AdvancedWarning::default(),
        }
    }
}
impl From<OldWarningScope> for WarningScope {
    fn from(value: OldWarningScope) -> Self { value.convert() }
}
impl OldWarningScope {
    fn convert(self) -> WarningScope {
        match self {
            OldWarningScope::AllViews => WarningScope::AllViews,
            OldWarningScope::CurrentView => WarningScope::CurrentView,
        }
    }
}
impl From<OldWarningMode> for WarningMode {
    fn from(value: OldWarningMode) -> Self { value.convert() }
}
impl OldWarningMode {
    fn convert(self) -> WarningMode {
        match self {
            OldWarningMode::CycleOne => WarningMode::CycleOne,
            OldWarningMode::ListThree => WarningMode::ListThree,
        }
    }
}
impl From<OldWarningPresentation> for WarningPresentation {
    fn from(value: OldWarningPresentation) -> Self { value.convert() }
}
impl OldWarningPresentation {
    fn convert(self) -> WarningPresentation {
        WarningPresentation {
            scope: self.scope.into(),
            mode: self.mode.into(),
            page_ms: self.page_ms,
        }
    }
}
#[derive(Deserialize, Serialize)]
pub enum OldGaugeKind {
    Numeric,
    Circular,
    Dial,
    Horizontal,
    Vertical,
    Segmented,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OldGauge {
    pub kind: OldGaugeKind,
    pub min: f32,
    pub max: f32,
    pub expand: bool,
}
impl From<OldGauge> for Gauge {
    fn from(v: OldGauge) -> Self {
        Self {
            kind: match v.kind {
                OldGaugeKind::Numeric => GaugeKind::Numeric,
                OldGaugeKind::Circular => GaugeKind::Circular,
                OldGaugeKind::Dial => GaugeKind::Dial,
                OldGaugeKind::Horizontal => GaugeKind::Horizontal,
                OldGaugeKind::Vertical => GaugeKind::Vertical,
                OldGaugeKind::Segmented => GaugeKind::Segmented,
            },
            min: v.min,
            max: v.max,
            expand: v.expand,
        }
    }
}
