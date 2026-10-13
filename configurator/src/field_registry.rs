//! Bounded discovery of browser editing fields; labels are data, never selectors.
use crate::config::*;
use crate::editor::*;
use crate::workspace::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    Root,
    View,
    Widget,
    Channel,
    Band,
    Led,
    ButtonTap,
    ButtonHold,
    Encoder,
}
macro_rules! registry {
    ($($key:ident, $scope:ident, $workspace:ident, $group:ident, $label:literal, $aliases:literal;)*) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
        pub enum FieldKey { $($key,)* }
        pub const DESCRIPTORS: &[Descriptor] = &[$(Descriptor { key:FieldKey::$key, scope:Scope::$scope, workspace:Workspace::$workspace, group:GroupId::$group, label:$label, aliases:$aliases, slug:stringify!($key) },)*];
    };
}
#[derive(Clone, Copy, Debug)]
pub struct Descriptor {
    pub key: FieldKey,
    pub scope: Scope,
    pub workspace: Workspace,
    pub group: GroupId,
    pub label: &'static str,
    pub aliases: &'static str,
    slug: &'static str,
}
registry! {
    BootView, Root, Views, Layout, "Boot view", "startup dashboard";
    ViewName, View, Views, Layout, "View name", "dashboard title";
    Label, Widget, Views, Layout, "Label", "widget name";
    Span, Widget, Views, Layout, "Size", "span tile dimensions";
    Column, Widget, Views, Layout, "Column", "placement position";
    Row, Widget, Views, Layout, "Row", "placement position";
    GaugeStyle, Widget, Views, Gauge, "Gauge style", "numeric circular dial horizontal vertical segmented";
    GaugeMinimum, Widget, Views, Gauge, "Scale minimum", "gauge boost canonical range";
    GaugeMaximum, Widget, Views, Gauge, "Scale maximum", "gauge boost canonical range";
    GaugeExpand, Widget, Views, Gauge, "Expand with observed values", "gauge extrema";
    Decimals, Widget, Views, Readout, "Decimal places", "precision";
    Readout, Widget, Views, Readout, "Primary readout", "units boost psi bar kpa lambda afr celsius fahrenheit";
    Secondary, Widget, Views, Readout, "Secondary boost unit", "psi bar kpa companion";
    Smoothing, Widget, Views, Readout, "Smoothing time (ms)", "current-value smoothing interpolation";
    Graph, Widget, Views, HistoryStatistics, "History graph", "channel trend";
    Average, Widget, Views, HistoryStatistics, "Average", "statistics";
    Extrema, Widget, Views, HistoryStatistics, "Minimum & maximum", "extrema statistics";
    Trend, Widget, Views, HistoryStatistics, "Trend arrow", "history";
    GraphMode, Widget, Views, HistoryStatistics, "Graph range", "automatic fixed history";
    GraphMinimum, Widget, Views, HistoryStatistics, "Graph minimum", "history range";
    GraphMaximum, Widget, Views, HistoryStatistics, "Graph maximum", "history range";
    PeakMode, Widget, Views, Gauge, "Peak marker", "maximum minimum hold";
    PeakHold, Widget, Views, Gauge, "Peak hold (ms)", "duration";
    ReferenceMode, Widget, Views, Gauge, "Reference marker", "zero manual";
    Reference, Widget, Views, Gauge, "Manual reference (canonical units)", "gauge reference";
    BandMinimum, Band, Views, Gauge, "Band minimum", "gauge interval";
    BandMaximum, Band, Views, Gauge, "Band maximum", "gauge interval";
    BandLabel, Band, Views, Gauge, "Band label", "gauge text";
    BandColor, Band, Views, Gauge, "Band colour", "gauge color";
    WarningEnabled, Widget, Views, Warnings, "Enable warning", "alarm";
    WarningApplicability, Widget, Views, Warnings, "Applies to", "warning vehicle demo";
    WarningMode, Widget, Views, Warnings, "Rule type", "warning high low outside inside lambda";
    WarningTrigger, Widget, Views, Warnings, "Trigger", "warning threshold low";
    WarningClear, Widget, Views, Warnings, "Clear", "warning hysteresis low";
    WarningTriggerHigh, Widget, Views, Warnings, "Trigger high", "warning window";
    WarningClearHigh, Widget, Views, Warnings, "Clear high", "warning window";
    WarningLoad, Widget, Views, Warnings, "Minimum load (%)", "lambda warning context";
    WarningOn, Widget, Views, Warnings, "On dwell (ms)", "warning activation time";
    WarningOff, Widget, Views, Warnings, "Off dwell (ms)", "warning clear time";
    WarningMessage, Widget, Views, Warnings, "Warning message", "alarm text";
    WarningPriority, Widget, Views, Warnings, "Warning priority", "critical high normal low";
    WarningLatch, Widget, Views, Warnings, "Latch until acknowledged and cleared", "warning latch";
    WarningSnooze, Widget, Views, Warnings, "Acknowledgment snooze (seconds)", "warning attention";
    ColorBackground, Widget, Views, Appearance, "Background", "colour color";
    ColorBorder, Widget, Views, Appearance, "Border", "colour color";
    ColorLabel, Widget, Views, Appearance, "Label colour", "color";
    ColorValue, Widget, Views, Appearance, "Primary value", "colour color";
    ColorSecondary, Widget, Views, Appearance, "Units / AFR", "colour color lambda";
    ColorGraph, Widget, Views, Appearance, "History line", "colour color";
    ColorGauge, Widget, Views, Appearance, "Gauge indicator", "colour color";
    ColorTrack, Widget, Views, Appearance, "Gauge track", "colour color";
    ColorTicks, Widget, Views, Appearance, "Scale labels / ticks", "colour color";
    ColorAverage, Widget, Views, Appearance, "Average colour", "color";
    ColorMinimum, Widget, Views, Appearance, "Minimum", "colour color";
    ColorMaximum, Widget, Views, Appearance, "Maximum", "colour color";
    ColorTrend, Widget, Views, Appearance, "Trend arrow colour", "color";
    ColorPeak, Widget, Views, Appearance, "New peak", "colour color";
    ColorWarningForeground, Widget, Views, Appearance, "Warning text", "colour color";
    ColorWarningBackground, Widget, Views, Appearance, "Warning background", "colour color";
    HistorySeconds, Channel, DataWarnings, Channels, "Channel-shared history (seconds)", "duration smoothing trend";
    SourceDefinition, Channel, DataWarnings, Sources, "Source definition", "ecu decoder channel";
    SourceRoute, Channel, DataWarnings, Sources, "Route", "physical deployment";
    SourceRequest, Channel, DataWarnings, Sources, "Request ID", "physical ecu";
    SourceResponse, Channel, DataWarnings, Sources, "Response ID", "physical ecu";
    SourcePoll, Channel, DataWarnings, Sources, "Polling requested", "source acquisition";
    SourcePeriod, Channel, DataWarnings, Sources, "Poll period (ms)", "polling rate";
    SourceFreshness, Channel, DataWarnings, Sources, "Freshness limit (ms)", "stale source";
    FuelMode, Root, DataWarnings, Fuel, "AFR scale", "lambda stoichiometry fuel";
    FuelRatio, Root, DataWarnings, Fuel, "Stoichiometric ratio", "afr lambda gasoline";
    FuelPercent, Root, DataWarnings, Fuel, "Ethanol content (%)", "blend fuel";
    FuelBasis, Root, DataWarnings, Fuel, "Percentage basis", "volume mass ethanol";
    FuelOrigin, Root, DataWarnings, Fuel, "Composition input", "measured assumed fuel";
    FuelDensityMode, Root, DataWarnings, Fuel, "Density reference", "temperature fuel";
    FuelTemperature, Root, DataWarnings, Fuel, "Reference temperature (°C)", "fuel density";
    FuelGasolineRatio, Root, DataWarnings, Fuel, "Gasoline ratio", "afr";
    FuelEthanolRatio, Root, DataWarnings, Fuel, "Ethanol ratio", "afr";
    FuelGasolineDensity, Root, DataWarnings, Fuel, "Gasoline kg/L", "density";
    FuelEthanolDensity, Root, DataWarnings, Fuel, "Ethanol kg/L", "density";
    WarningScope, Root, DataWarnings, CombinedWarnings, "Show warnings from", "scope";
    WarningPresentation, Root, DataWarnings, CombinedWarnings, "Presentation", "warning cycle list";
    WarningPage, Root, DataWarnings, CombinedWarnings, "Page interval (ms)", "warning dwell";
    WarningForeground, Root, DataWarnings, CombinedWarnings, "Combined text", "warning color colour";
    WarningBackground, Root, DataWarnings, CombinedWarnings, "Combined background", "warning color colour";
    ControlDuration, Root, Device, Bindings, "Ordinary hold duration", "button timing recovery";
    ControlTap, ButtonTap, Device, Bindings, "Tap", "button action";
    ControlHold, ButtonHold, Device, Bindings, "Hold", "button action recovery";
    EncoderRotation, Encoder, Device, Bindings, "Encoder rotation", "navigation";
    EncoderClick, Encoder, Device, Bindings, "Encoder click", "action";
    LedPattern, Led, Device, Indicator, "Pattern", "rgb indicator normal warning blink breathe";
    LedColor, Led, Device, Indicator, "LED colour", "rgb color";
    LedBrightness, Led, Device, Indicator, "LED brightness (%)", "rgb duty";
    LedPeriod, Led, Device, Indicator, "LED period (ms)", "rgb blink breathe";
    LedOnPercent, Led, Device, Indicator, "LED on-time (%)", "rgb blink duty";
    ReducedMotion, Root, Device, Indicator, "Reduced motion", "smoothing breathe";
    LedSync, Root, Device, Indicator, "Synchronize warning Blink with popup", "rgb sync";
    LedFeedback, Root, Device, Indicator, "Indicate warnings during Dashboard feedback", "rgb policy";
    LedDebug, Root, Device, Indicator, "Indicate warnings on Debug", "rgb policy";
    LedLogs, Root, Device, Indicator, "Indicate warnings on Logs", "rgb policy";
    LedBacklight, Root, Device, Indicator, "Allow warning indication with backlight off", "rgb policy";
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TargetOwner {
    Root,
    View(usize),
    Widget(usize),
    Channel(Channel),
    Band(usize, u8),
    Led(bool),
    Button(ControlPage, ButtonId),
    Encoder(ControlPage),
}
impl TargetOwner {
    fn inspection(self) -> InspectionOwner {
        match self {
            Self::View(index) => InspectionOwner::View { index },
            Self::Widget(index) | Self::Band(index, _) => InspectionOwner::Widget { index },
            Self::Channel(channel) => InspectionOwner::Channel(channel),
            _ => InspectionOwner::Root,
        }
    }

    fn prefix(self) -> String {
        match self {
            Self::Root => "root".into(),
            Self::View(i) => format!("v-{i}"),
            Self::Widget(i) => format!("w-{i}"),
            Self::Channel(c) => format!("c-{}", c.index()),
            Self::Band(i, b) => format!("w-{i}-band-{b}"),
            Self::Led(w) => format!("led-{}", u8::from(w)),
            Self::Button(p, b) => format!("control-{}-button-{}", p.index(), b.index()),
            Self::Encoder(p) => format!("control-{}-encoder", p.index()),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FieldTarget {
    key: FieldKey,
    owner: TargetOwner,
    inspection: InspectionTarget,
}
impl FieldTarget {
    pub fn key(self) -> FieldKey { self.key }

    pub fn owner(self) -> TargetOwner { self.owner }

    pub fn inspection(self) -> InspectionTarget { self.inspection }

    pub fn dom_id(&self) -> String { Registry::dom_id(self.key, self.owner) }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Capability {
    Active,
    Inactive(&'static str),
}
impl Capability {
    pub fn is_active(self) -> bool { matches!(self, Self::Active) }

    pub fn reason(self) -> Option<&'static str> {
        match self {
            Self::Active => None,
            Self::Inactive(reason) => Some(reason),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RegistryError {
    QueryTooLong,
    Unavailable,
    Editor(EditorError),
}
impl From<EditorError> for RegistryError {
    fn from(error: EditorError) -> Self { Self::Editor(error) }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchResults {
    pub results: Vec<FieldTarget>,
    pub omitted: usize,
}
#[derive(Clone, Debug)]
pub struct ResolvedField {
    pub workspace: Workspace,
    pub group: GroupId,
    pub dom_id: String,
    pub capability: Capability,
    pub inspection: ResolvedInspection,
}
pub struct Registry;
impl Registry {
    /// Synchronous read-only candidate access. A Config is data, never validation proof.
    pub fn with_candidate<R>(
        editor: &Editor,
        read: impl FnOnce(&Config) -> R,
    ) -> R {
        editor.with_inspection_config(read)
    }

    pub fn retained_raw_target(
        editor: &Editor,
        index: usize,
        owner: TargetOwner,
    ) -> Result<FieldTarget, RegistryError> {
        let raw = editor.raw_fields().get(index).ok_or(RegistryError::Unavailable)?;
        if Self::raw_owner(editor, raw.field) != Some(owner) {
            return Err(RegistryError::Unavailable);
        }
        let target = FieldTarget {
            key: Self::raw_key(raw.field),
            owner,
            inspection: editor.raw_inspection_target(index)?,
        };
        Self::resolve(editor, target)?;
        Ok(target)
    }

    pub fn semantic_target(
        editor: &Editor,
        location: ValidationLocation,
        key: FieldKey,
        owner: TargetOwner,
    ) -> Result<FieldTarget, RegistryError> {
        if Self::validation_key(location.field()) != Some(key) {
            return Err(RegistryError::Unavailable);
        }
        let subowner_matches = match (owner, location.field()) {
            (
                TargetOwner::Led(actual),
                Field::LedBrightness(expected) | Field::LedPeriod(expected) | Field::LedOnPercent(expected),
            ) => actual == expected,
            (
                TargetOwner::Band(_, actual),
                Field::BandMinimum(expected)
                | Field::BandMaximum(expected)
                | Field::BandLabel(expected)
                | Field::BandColor(expected)
                | Field::BandOrder(expected),
            ) => actual == expected,
            (TargetOwner::Led(_) | TargetOwner::Band(..), _) => false,
            _ => true,
        };
        if !subowner_matches {
            return Err(RegistryError::Unavailable);
        }
        let target = FieldTarget {
            key,
            owner,
            inspection: editor.inspection_target(location)?,
        };
        let resolved = Self::resolve(editor, target)?;
        if match owner {
            TargetOwner::Widget(i) | TargetOwner::Band(i, _) => resolved.inspection.widget_index != Some(i),
            TargetOwner::View(i) => resolved.inspection.view_index != Some(i),
            TargetOwner::Channel(c) => resolved.inspection.channel_index != Some(c.index()),
            _ => {
                resolved.inspection.widget_index.is_some()
                    || resolved.inspection.view_index.is_some()
                    || resolved.inspection.channel_index.is_some()
            }
        } {
            return Err(RegistryError::Unavailable);
        }
        Ok(target)
    }

    pub fn validation_key(field: Field) -> Option<FieldKey> {
        use FieldKey::*;
        Some(match field {
            Field::BootView => BootView,
            Field::ViewName => ViewName,
            Field::Placement => Span,
            Field::Label => Label,
            Field::Decimals => Decimals,
            Field::Readout | Field::Afr => Readout,
            Field::Secondary => Secondary,
            Field::Smoothing => Smoothing,
            Field::PeakHold => PeakHold,
            Field::Reference => Reference,
            Field::BandMinimum(_) | Field::BandOrder(_) => BandMinimum,
            Field::BandMaximum(_) => BandMaximum,
            Field::BandLabel(_) => BandLabel,
            Field::BandColor(_) => BandColor,
            Field::HistorySeconds(_) => HistorySeconds,
            Field::WarningTriggerHigh => WarningTriggerHigh,
            Field::WarningClearHigh => WarningClearHigh,
            Field::WarningSnooze => WarningSnooze,
            Field::LedBrightness(_) => LedBrightness,
            Field::LedPeriod(_) => LedPeriod,
            Field::LedOnPercent(_) => LedOnPercent,
            Field::GraphRange => GraphMinimum,
            Field::GaugeRange => GaugeMinimum,
            Field::Color(role) => Self::raw_key(RawField::Widget(0, WidgetField::Color(role))),
            Field::WarningColors => WarningForeground,
            Field::WarningMode => WarningMode,
            Field::WarningTrigger => WarningTrigger,
            Field::WarningClear => WarningClear,
            Field::WarningLoad => WarningLoad,
            Field::WarningOn => WarningOn,
            Field::WarningOff => WarningOff,
            Field::WarningMessage => WarningMessage,
            Field::WarningPage => WarningPage,
            Field::SourceDefinition | Field::SourceAccess | Field::SourceDecoder | Field::SourceEvidence => {
                SourceDefinition
            }
            Field::SourceRoute => SourceRoute,
            Field::SourcePoll => SourcePoll,
            Field::SourceFreshness => SourceFreshness,
            Field::FuelRatio => FuelRatio,
            Field::FuelPercent => FuelPercent,
            Field::FuelGasolineRatio => FuelGasolineRatio,
            Field::FuelEthanolRatio => FuelEthanolRatio,
            Field::FuelGasolineDensity => FuelGasolineDensity,
            Field::FuelEthanolDensity => FuelEthanolDensity,
            Field::FuelReference => FuelTemperature,
            Field::Controls(ControlError::HoldDuration) => ControlDuration,
            _ => return None,
        })
    }

    pub fn candidate_widget(
        editor: &Editor,
        index: usize,
    ) -> Option<Widget> {
        editor.with_inspection_config(|c| c.widgets.get(index).cloned())
    }

    pub fn candidate_view_id(
        editor: &Editor,
        index: usize,
    ) -> Option<u8> {
        editor.with_inspection_config(|c| c.views.get(index).map(|v| v.id))
    }

    pub fn descriptors() -> &'static [Descriptor] { DESCRIPTORS }

    pub fn descriptor(key: FieldKey) -> &'static Descriptor {
        DESCRIPTORS
            .iter()
            .find(|d| d.key == key)
            .expect("enumerated registry key")
    }

    pub fn dom_id(
        key: FieldKey,
        owner: TargetOwner,
    ) -> String {
        format!(
            "studio-field-{}-{}",
            owner.prefix(),
            Self::descriptor(key).slug.to_ascii_lowercase()
        )
    }

    pub fn target(
        editor: &Editor,
        key: FieldKey,
        owner: TargetOwner,
    ) -> Result<FieldTarget, RegistryError> {
        let target = FieldTarget {
            key,
            owner,
            inspection: editor.object_inspection_target(owner.inspection())?,
        };
        Self::resolve(editor, target)?;
        Ok(target)
    }

    fn visit(
        config: &Config,
        mut visit: impl FnMut(&Descriptor, TargetOwner),
    ) {
        for d in DESCRIPTORS {
            match d.scope {
                Scope::Root => visit(d, TargetOwner::Root),
                Scope::View => {
                    for i in 0..config.views.len() {
                        visit(d, TargetOwner::View(i));
                    }
                }
                Scope::Widget => {
                    for i in 0..config.widgets.len() {
                        visit(d, TargetOwner::Widget(i));
                    }
                }
                Scope::Channel => {
                    for c in Channel::ALL {
                        visit(d, TargetOwner::Channel(c));
                    }
                }
                Scope::Band => {
                    for (i, w) in config.widgets.iter().enumerate() {
                        for b in 0..w.advanced.bands().len() {
                            visit(d, TargetOwner::Band(i, b as u8));
                        }
                    }
                }
                Scope::Led => {
                    for warning in [false, true] {
                        visit(d, TargetOwner::Led(warning));
                    }
                }
                Scope::ButtonTap | Scope::ButtonHold => {
                    for p in ControlPage::ALL {
                        for b in ButtonId::ALL {
                            if d.scope == Scope::ButtonTap || b != ButtonId::Y {
                                visit(d, TargetOwner::Button(p, b));
                            }
                        }
                    }
                }
                Scope::Encoder => {
                    for p in ControlPage::ALL {
                        visit(d, TargetOwner::Encoder(p));
                    }
                }
            }
        }
    }

    pub fn targets(editor: &Editor) -> Result<Vec<FieldTarget>, RegistryError> {
        editor.with_inspection_config(|c| {
            let mut result = Vec::new();
            let mut error = None;
            Self::visit(c, |d, o| {
                match Self::target(editor, d.key, o) {
                    Ok(t) => result.push(t),
                    Err(e) => error = Some(e),
                }
            });
            error.map_or(Ok(result), Err)
        })
    }

    pub fn search(
        editor: &Editor,
        query: &str,
    ) -> Result<SearchResults, RegistryError> {
        if query.len() > 128 {
            return Err(RegistryError::QueryTooLong);
        }
        let query = query.to_lowercase();
        let tokens = query.split_whitespace().collect::<Vec<_>>();
        editor.with_inspection_config(|c| {
            let mut results = Vec::new();
            let mut omitted = 0;
            let mut error = None;
            Self::visit(c, |d, o| {
                let label = match o {
                    TargetOwner::Widget(i) | TargetOwner::Band(i, _) => {
                        format!("{} {}", c.widgets[i].label, c.widgets[i].channel.label())
                    }
                    TargetOwner::View(i) => c.views[i].name.to_string(),
                    TargetOwner::Channel(channel) => channel.label().into(),
                    _ => String::new(),
                };
                let hay = format!("{} {} {label}", d.label, d.aliases).to_lowercase();
                if tokens.iter().all(|t| hay.contains(t)) {
                    if results.len() < 40 {
                        match Self::target(editor, d.key, o) {
                            Ok(t) => results.push(t),
                            Err(e) => error = Some(e),
                        }
                    } else {
                        omitted += 1;
                    }
                }
            });
            error.map_or(Ok(SearchResults { results, omitted }), Err)
        })
    }

    pub fn resolve(
        editor: &Editor,
        target: FieldTarget,
    ) -> Result<ResolvedField, RegistryError> {
        let inspection = editor.inspect(target.inspection)?;
        let d = Self::descriptor(target.key);
        let capability = editor.with_inspection_config(|c| -> Result<Capability, RegistryError> {
            let scope = match target.owner {
                TargetOwner::Root => Scope::Root,
                TargetOwner::View(_) => Scope::View,
                TargetOwner::Widget(_) => Scope::Widget,
                TargetOwner::Channel(_) => Scope::Channel,
                TargetOwner::Band(i, b) => {
                    if c.widgets
                        .get(i)
                        .and_then(|w| w.advanced.bands().get(b as usize))
                        .is_none()
                    {
                        return Err(RegistryError::Unavailable);
                    }
                    Scope::Band
                }
                TargetOwner::Led(_) => Scope::Led,
                TargetOwner::Button(_, b) => {
                    if !matches!(d.scope, Scope::ButtonTap | Scope::ButtonHold)
                        || (d.scope == Scope::ButtonHold && b == ButtonId::Y)
                    {
                        return Err(RegistryError::Unavailable);
                    }
                    d.scope
                }
                TargetOwner::Encoder(_) => Scope::Encoder,
            };
            if scope != d.scope {
                return Err(RegistryError::Unavailable);
            }
            Ok(match target.owner {
                TargetOwner::Widget(i) | TargetOwner::Band(i, _) => Self::widget_capability(target.key, &c.widgets[i]),
                TargetOwner::Channel(channel) => {
                    let source = c.sources[channel.index()];
                    if matches!(target.key, FieldKey::SourceRequest | FieldKey::SourceResponse)
                        && !matches!(source.route, Route::Physical11 { .. })
                    {
                        Capability::Inactive("Choose an explicit physical route to edit request and response IDs")
                    } else if target.key == FieldKey::SourcePeriod && matches!(source.poll, Poll::Off) {
                        Capability::Inactive("Enable polling to edit its period")
                    } else {
                        Capability::Active
                    }
                }
                TargetOwner::Root => {
                    let active = match target.key {
                        FieldKey::FuelRatio => matches!(c.fuel, Fuel::Stoichiometric { .. }),
                        FieldKey::FuelTemperature => {
                            matches!(
                                c.fuel,
                                Fuel::EthanolBlend(Blend {
                                    density_reference: DensityReference::CommonCelsius(_),
                                    ..
                                })
                            )
                        }
                        FieldKey::FuelPercent
                        | FieldKey::FuelBasis
                        | FieldKey::FuelOrigin
                        | FieldKey::FuelDensityMode
                        | FieldKey::FuelGasolineRatio
                        | FieldKey::FuelEthanolRatio
                        | FieldKey::FuelGasolineDensity
                        | FieldKey::FuelEthanolDensity => matches!(c.fuel, Fuel::EthanolBlend(_)),
                        _ => true,
                    };
                    if active {
                        Capability::Active
                    } else {
                        Capability::Inactive("Choose the applicable Fuel & AFR mode to edit this field")
                    }
                }
                TargetOwner::Led(w) => {
                    let p = if w { c.led.warning } else { c.led.normal };
                    if (target.key == FieldKey::LedPeriod
                        && (matches!(p.kind, LedPatternKind::Off | LedPatternKind::Solid)
                            || (w && p.kind == LedPatternKind::Blink && c.led.sync_warning_blink)))
                        || (target.key == FieldKey::LedOnPercent
                            && (p.kind != LedPatternKind::Blink || (w && c.led.sync_warning_blink)))
                    {
                        Capability::Inactive("Stored timing is inactive for this pattern or synchronized Blink")
                    } else {
                        Capability::Active
                    }
                }
                _ => Capability::Active,
            })
        })?;
        Ok(ResolvedField {
            workspace: d.workspace,
            group: d.group,
            dom_id: target.dom_id(),
            capability,
            inspection,
        })
    }

    pub fn widget_capability(
        key: FieldKey,
        w: &Widget,
    ) -> Capability {
        use FieldKey::*;
        let c = w.capabilities();
        let active = match key {
            Graph | GraphMode | Trend => c.history,
            GraphMinimum | GraphMaximum => c.history && matches!(w.display.graph_range, GraphRange::Fixed { .. }),
            Average | Extrema => c.statistics,
            PeakMode | PeakHold | ReferenceMode | Reference | BandMinimum | BandMaximum | BandColor => c.markers,
            BandLabel => c.band_labels,
            Secondary => w.channel == Channel::Boost && c.secondary,
            WarningLoad => w.channel == Channel::Lambda,
            WarningTriggerHigh | WarningClearHigh => {
                matches!(w.warning.mode, LimitMode::OutsideWindow | LimitMode::InsideWindow)
            }
            _ => true,
        };
        if active {
            Capability::Active
        } else {
            Capability::Inactive(
                "Stored setting is inactive for this channel, style or size; retained invalid values remain repairable",
            )
        }
    }

    pub fn raw_key(field: RawField) -> FieldKey {
        use FieldKey::*;
        match field {
            RawField::ViewName(_) => ViewName,
            RawField::ControlHoldMilliseconds => ControlDuration,
            RawField::HistorySeconds(_) => HistorySeconds,
            RawField::WarningPageMilliseconds => WarningPage,
            RawField::WarningColor { foreground: true } => WarningForeground,
            RawField::WarningColor { foreground: false } => WarningBackground,
            RawField::Led { field, .. } => {
                match field {
                    LedField::Color => LedColor,
                    LedField::Brightness => LedBrightness,
                    LedField::Period => LedPeriod,
                    LedField::OnPercent => LedOnPercent,
                }
            }
            RawField::Source(_, f) => {
                match f {
                    SourceField::Request => SourceRequest,
                    SourceField::Response => SourceResponse,
                    SourceField::PollMilliseconds => SourcePeriod,
                    SourceField::FreshnessMilliseconds => SourceFreshness,
                }
            }
            RawField::Fuel(f) => {
                match f {
                    FuelField::Ratio => FuelRatio,
                    FuelField::Percent => FuelPercent,
                    FuelField::GasolineRatio => FuelGasolineRatio,
                    FuelField::EthanolRatio => FuelEthanolRatio,
                    FuelField::GasolineDensity => FuelGasolineDensity,
                    FuelField::EthanolDensity => FuelEthanolDensity,
                    FuelField::ReferenceCelsius => FuelTemperature,
                }
            }
            RawField::Widget(_, f) => {
                match f {
                    WidgetField::Label => Label,
                    WidgetField::Decimals => Decimals,
                    WidgetField::GraphMinimum => GraphMinimum,
                    WidgetField::GraphMaximum => GraphMaximum,
                    WidgetField::GaugeMinimum => GaugeMinimum,
                    WidgetField::GaugeMaximum => GaugeMaximum,
                    WidgetField::Color(role) => {
                        match role {
                            ColorRole::Background => ColorBackground,
                            ColorRole::Border => ColorBorder,
                            ColorRole::Label => ColorLabel,
                            ColorRole::Value => ColorValue,
                            ColorRole::Secondary => ColorSecondary,
                            ColorRole::Graph => ColorGraph,
                            ColorRole::Gauge => ColorGauge,
                            ColorRole::Track => ColorTrack,
                            ColorRole::Ticks => ColorTicks,
                            ColorRole::Average => ColorAverage,
                            ColorRole::Minimum => ColorMinimum,
                            ColorRole::Maximum => ColorMaximum,
                            ColorRole::Trend => ColorTrend,
                            ColorRole::Peak => ColorPeak,
                            ColorRole::WarningForeground => ColorWarningForeground,
                            ColorRole::WarningBackground => ColorWarningBackground,
                        }
                    }
                    WidgetField::WarningMessage => WarningMessage,
                    WidgetField::WarningTrigger => WarningTrigger,
                    WidgetField::WarningClear => WarningClear,
                    WidgetField::WarningLoad => WarningLoad,
                    WidgetField::WarningOn => WarningOn,
                    WidgetField::WarningOff => WarningOff,
                    WidgetField::SmoothingMilliseconds => Smoothing,
                    WidgetField::PeakHoldMilliseconds => PeakHold,
                    WidgetField::Reference => Reference,
                    WidgetField::BandMinimum(_) => BandMinimum,
                    WidgetField::BandMaximum(_) => BandMaximum,
                    WidgetField::BandLabel(_) => BandLabel,
                    WidgetField::BandColor(_) => BandColor,
                    WidgetField::WarningTriggerHigh => WarningTriggerHigh,
                    WidgetField::WarningClearHigh => WarningClearHigh,
                    WidgetField::WarningSnoozeSeconds => WarningSnooze,
                }
            }
        }
    }

    pub fn raw_owner(
        editor: &Editor,
        field: RawField,
    ) -> Option<TargetOwner> {
        editor.with_inspection_config(|c| {
            Some(match field {
                RawField::Widget(id, f) => {
                    let mut indices = c.widgets.iter().enumerate().filter(|(_, w)| w.id == id);
                    let (i, _) = indices.next()?;
                    if indices.next().is_some() {
                        return None;
                    }
                    match f {
                        WidgetField::BandMinimum(b)
                        | WidgetField::BandMaximum(b)
                        | WidgetField::BandLabel(b)
                        | WidgetField::BandColor(b) => TargetOwner::Band(i, b),
                        _ => TargetOwner::Widget(i),
                    }
                }
                RawField::ViewName(id) => {
                    let mut indices = c.views.iter().enumerate().filter(|(_, v)| v.id == id);
                    let (i, _) = indices.next()?;
                    if indices.next().is_some() {
                        return None;
                    }
                    TargetOwner::View(i)
                }
                RawField::Source(c, _) | RawField::HistorySeconds(c) => TargetOwner::Channel(c),
                RawField::Led { warning, .. } => TargetOwner::Led(warning),
                _ => TargetOwner::Root,
            })
        })
    }
}
