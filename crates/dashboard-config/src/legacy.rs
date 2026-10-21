//! Frozen host-only v1/v2 wire schemas. Firmware accepts only the current version.
use serde::{Deserialize, Serialize};

use crate::*;

/// Frozen pre-orientation view shape; migration adds explicit inheritance only.
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LegacyView {
    /// Historical nonzero identity candidate; uniqueness is checked after migration.
    id: u8,
    /// Historical view label, at most fourteen bytes before current text validation.
    name: heapless::String<14>,
}

/// Consume `views` and return the same bounded order/IDs/names with inherited orientation.
/// One output per input preserves capacity; full semantic admission is still required.
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
/// Frozen v1 declaration order; never add current fields to this historical wire schema.
pub(crate) struct LegacyConfig {
    /// Historical schema number; upgrade rejects a value different from this frozen container version.
    version: u16,
    /// Documentary catalogue revision, retained for current semantic admission.
    catalogue_version: u16,
    /// Initial view ID, retained without reindexing.
    boot_view: u8,
    /// Bounded historical view list; order and IDs survive migration.
    views: heapless::Vec<LegacyView, MAX_VIEWS>,
    /// Bounded historical widget list; migration never drops an instance to gain capacity.
    widgets: heapless::Vec<LegacyWidget, MAX_WIDGETS>,
    /// Eight channel-indexed documentary source selections, retained for current validation.
    sources: [ChannelSource; 8],
    /// Historical warning presentation policy, migrated without changing selected scope or timing.
    warnings: OldWarningPresentation,
    /// Explicit fuel assumption, retained without inventing measured composition.
    fuel: Fuel,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
/// Frozen v1 widget without gauge/palette/advanced extensions.
struct LegacyWidget {
    /// Historical nonzero identity candidate; uniqueness is checked after migration.
    id: u16,
    /// Referenced view ID, preserved without substituting an array index.
    view: u8,
    /// Canonical measurement channel retained by migration.
    channel: Channel,
    /// Logical historical grid position and span; no automatic repacking during migration.
    placement: Placement,
    /// Historical bounded widget label; retained verbatim for current validation.
    label: heapless::String<24>,
    /// Historical numeric/history options, mapped without losing requested visibility.
    display: OldDisplayOptions,
    /// Historical warning rule; new window/latch/snooze fields receive explicit defaults.
    warning: OldWarningSettings,
}
impl LegacyConfig {
    /// Return current admitted configuration, preserving every v1 field and adding explicit defaults.
    /// Requires inner version one; capacity failures report `Payload`, current invalid fields report `Validation`.
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
            ble: BleSettings::DEFAULT,
        })
        .map_err(ConfigError::Validation)
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
/// Frozen v2 container adding popup and widget appearance while retaining the older warning vocabulary.
pub(crate) struct LegacyConfigV2 {
    /// Historical schema number; upgrade rejects a value different from this frozen container version.
    version: u16,
    /// Documentary catalogue revision, retained for current semantic admission.
    catalogue_version: u16,
    /// Initial view ID, retained without reindexing.
    boot_view: u8,
    /// Bounded historical view list; order and IDs survive migration.
    views: heapless::Vec<LegacyView, MAX_VIEWS>,
    /// Bounded historical widget list; migration never drops an instance to gain capacity.
    widgets: heapless::Vec<LegacyWidgetV2, MAX_WIDGETS>,
    /// Eight channel-indexed documentary source selections, retained for current validation.
    sources: [ChannelSource; 8],
    /// Historical warning presentation policy, migrated without changing selected scope or timing.
    warnings: OldWarningPresentation,
    /// Explicit fuel assumption, retained without inventing measured composition.
    fuel: Fuel,
    /// Retained popup foreground/background colors; current contrast rules still apply.
    warning_colors: WarningColors,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
/// Frozen v2 widget with gauge/palette and no advanced extension.
struct LegacyWidgetV2 {
    /// Historical nonzero identity candidate; uniqueness is checked after migration.
    id: u16,
    /// Referenced view ID, preserved without substituting an array index.
    view: u8,
    /// Canonical measurement channel retained by migration.
    channel: Channel,
    /// Logical historical grid position and span; no automatic repacking during migration.
    placement: Placement,
    /// Historical bounded widget label; retained verbatim for current validation.
    label: heapless::String<24>,
    /// Historical numeric/history options, mapped without losing requested visibility.
    display: OldDisplayOptions,
    /// Historical warning rule; new window/latch/snooze fields receive explicit defaults.
    warning: OldWarningSettings,
    /// Historical gauge shape/range; canonical endpoints are preserved.
    gauge: OldGauge,
    /// Historical per-role palette, including dormant roles.
    colors: WidgetColors,
}
impl LegacyConfigV2 {
    /// Return current admitted configuration with preserved v2 settings and explicit new-field defaults.
    /// Requires inner version two; capacity/current validation failures reject the whole migration.
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
            ble: BleSettings::DEFAULT,
        })
        .map_err(ConfigError::Validation)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// Frozen historical graph-scale discriminants; mapping is exact and never clamps endpoints.
pub enum OldGraphRange {
    /// Automatic historical trace range.
    Auto,
    /// Explicit canonical historical range.
    Fixed {
        /// Lower endpoint retained for current finite/order validation.
        min: f32,
        /// Upper endpoint retained for current finite/order validation.
        max: f32,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// Frozen numeric/history fields; migration separately derives advanced readout from the AFR flag.
pub struct OldDisplayOptions {
    /// Requested fractional digits; current admission enforces 0..=3.
    pub decimals: u8,
    /// Retained history-trace visibility flag.
    pub graph: bool,
    /// Retained average-statistic visibility flag.
    pub average: bool,
    /// Retained minimum/maximum visibility flag.
    pub extrema: bool,
    /// Retained trend-arrow visibility flag.
    pub trend: bool,
    /// Historical AFR flag; migration derives a matching advanced readout for the channel.
    pub afr: bool,
    /// Historical automatic/fixed graph scale, with canonical endpoints preserved.
    pub graph_range: OldGraphRange,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Frozen data-mode eligibility; migration preserves the restriction exactly.
pub enum OldApplicability {
    /// Synthetic operation only.
    DemoOnly,
    /// Both modes, still subject to live qualification.
    VehicleAndDemo,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Frozen non-window warning predicates.
pub enum OldLimitMode {
    /// High threshold with lower clearing threshold.
    High,
    /// Low threshold with higher clearing threshold.
    Low,
    /// Qualified lambda deviation from explicit target.
    LambdaDeviation,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// Frozen rule without window, latch, priority or snooze fields.
pub struct OldWarningSettings {
    /// Historical warning participation flag; dormant fields still undergo current validation.
    pub enabled: bool,
    /// Historical allowed data modes; migration never promotes demo-only warnings to vehicle mode.
    pub applicability: OldApplicability,
    /// Historical warning predicate or presentation layout, mapped by exact enum variant.
    pub mode: OldLimitMode,
    /// Historical canonical trigger threshold, retained without clamping.
    pub trigger: f32,
    /// Historical canonical clear threshold, retained without clamping.
    pub clear: f32,
    /// Historical qualifying load percentage for contextual lambda warnings.
    pub min_load_percent: f32,
    /// Historical activation dwell in milliseconds.
    pub on_ms: u16,
    /// Historical clearing dwell in milliseconds.
    pub off_ms: u16,
    /// Historical warning text, at most twenty bytes before current text validation.
    pub message: heapless::String<20>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Frozen warning view scope.
pub enum OldWarningScope {
    /// Include eligible rules across all views.
    AllViews,
    /// Include selected-view rules only.
    CurrentView,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Frozen popup page layout.
pub enum OldWarningMode {
    /// One warning per page.
    CycleOne,
    /// Up to three warnings per page.
    ListThree,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// Frozen popup selection/timing fields.
pub struct OldWarningPresentation {
    /// Historical all-view/current-view warning selection.
    pub scope: OldWarningScope,
    /// Historical warning predicate or presentation layout, mapped by exact enum variant.
    pub mode: OldWarningMode,
    /// Historical popup page interval in milliseconds.
    pub page_ms: u16,
}

impl From<OldGraphRange> for GraphRange {
    /// Return exact current mapping of `value`, with endpoint admission deferred.
    fn from(value: OldGraphRange) -> Self { value.convert() }
}
impl OldGraphRange {
    /// Return equivalent current scale without normalizing invalid endpoints.
    fn convert(self) -> GraphRange {
        match self {
            OldGraphRange::Auto => GraphRange::Auto,
            OldGraphRange::Fixed { min, max } => GraphRange::Fixed { min, max },
        }
    }
}
impl From<OldDisplayOptions> for DisplayOptions {
    /// Return exact current presentation fields from `value`; semantic validation follows migration.
    fn from(value: OldDisplayOptions) -> Self { value.convert() }
}
impl OldDisplayOptions {
    /// Return retained numeric/visibility fields and converted graph-scale shape.
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
    /// Return the same data-mode restriction represented by `value`.
    fn from(value: OldApplicability) -> Self { value.convert() }
}
impl OldApplicability {
    /// Return the corresponding current applicability without promoting vehicle eligibility.
    fn convert(self) -> Applicability {
        match self {
            OldApplicability::DemoOnly => Applicability::DemoOnly,
            OldApplicability::VehicleAndDemo => Applicability::VehicleAndDemo,
        }
    }
}
impl From<OldLimitMode> for LimitMode {
    /// Return the exact current predicate corresponding to `value`.
    fn from(value: OldLimitMode) -> Self { value.convert() }
}
impl OldLimitMode {
    /// Return equivalent non-window predicate; window modes cannot appear in this frozen schema.
    fn convert(self) -> LimitMode {
        match self {
            OldLimitMode::High => LimitMode::High,
            OldLimitMode::Low => LimitMode::Low,
            OldLimitMode::LambdaDeviation => LimitMode::LambdaDeviation,
        }
    }
}
impl From<OldWarningSettings> for WarningSettings {
    /// Return current rule preserving `value` and adding default advanced settings.
    fn from(value: OldWarningSettings) -> Self { value.convert() }
}
impl OldWarningSettings {
    /// Return retained rule fields with normal priority and disabled latch/snooze/window extension.
    /// Invalid legacy values are preserved for rejection by current admission.
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
    /// Return exact current view scope for `value`.
    fn from(value: OldWarningScope) -> Self { value.convert() }
}
impl OldWarningScope {
    /// Return corresponding current view scope.
    fn convert(self) -> WarningScope {
        match self {
            OldWarningScope::AllViews => WarningScope::AllViews,
            OldWarningScope::CurrentView => WarningScope::CurrentView,
        }
    }
}
impl From<OldWarningMode> for WarningMode {
    /// Return exact current page layout for `value`.
    fn from(value: OldWarningMode) -> Self { value.convert() }
}
impl OldWarningMode {
    /// Return corresponding current popup layout.
    fn convert(self) -> WarningMode {
        match self {
            OldWarningMode::CycleOne => WarningMode::CycleOne,
            OldWarningMode::ListThree => WarningMode::ListThree,
        }
    }
}
impl From<OldWarningPresentation> for WarningPresentation {
    /// Return retained scope/layout/timing from `value`, without timing normalization.
    fn from(value: OldWarningPresentation) -> Self { value.convert() }
}
impl OldWarningPresentation {
    /// Return current popup fields; interval bounds are checked after complete migration.
    fn convert(self) -> WarningPresentation {
        WarningPresentation {
            scope: self.scope.into(),
            mode: self.mode.into(),
            page_ms: self.page_ms,
        }
    }
}
#[derive(Deserialize, Serialize)]
/// Frozen historical gauge discriminants; order is binary-format compatibility data.
pub enum OldGaugeKind {
    /// Numeric/history layout.
    Numeric,
    /// Circular arc.
    Circular,
    /// Needle dial.
    Dial,
    /// Horizontal bar.
    Horizontal,
    /// Vertical bar.
    Vertical,
    /// Segmented bar.
    Segmented,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
/// Frozen gauge shape/range; numeric values are retained verbatim during migration.
pub struct OldGauge {
    /// Historical gauge presentation family.
    pub kind: OldGaugeKind,
    /// Historical lower canonical display endpoint; validation remains deferred.
    pub min: f32,
    /// Historical upper canonical display endpoint; validation remains deferred.
    pub max: f32,
    /// Historical automatic range-expansion policy.
    pub expand: bool,
}
impl From<OldGauge> for Gauge {
    /// Return equivalent current gauge for `v`; no clamping, expansion or semantic validation occurs.
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
