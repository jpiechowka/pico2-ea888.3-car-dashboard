//! Bounded discovery of browser editing fields; labels are data, never selectors.
use crate::config::*;
use crate::editor::*;
use crate::workspace::*;

/// Object family over which a descriptor expands; indices refer to the inspected candidate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    /// One configuration-wide setting.
    Root,
    /// One entry per candidate view, including duplicate IDs.
    View,
    /// One entry per candidate widget, including duplicate IDs.
    Widget,
    /// One entry per fixed acquisition channel.
    Channel,
    /// One entry per retained widget band.
    Band,
    /// Separate normal and warning patterns.
    Led,
    /// Every onboard button on each control page.
    ButtonTap,
    /// Ordinary holds, excluding the reserved Y recovery hold.
    ButtonHold,
    /// One encoder binding per control page.
    Encoder,
}
/// Defines keys and descriptors together so every key has exactly one lookup row.
/// Rows supply object scope, navigation destination, display label and search aliases;
/// generated documentation carries the same binding metadata.
macro_rules! registry {
    ($($key:ident, $scope:ident, $workspace:ident, $group:ident, $label:literal, $aliases:literal;)*) => {
        /// Stable field identity independent of candidate position or DOM lifetime.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
        pub enum FieldKey { $(#[doc = concat!($label, ". Scope: `", stringify!($scope), "`; workspace/group: `", stringify!($workspace), "` / `", stringify!($group), "`. Resolve against current candidate before navigation or editing.")] $key,)* }
        /// Ordered discovery rows; order also determines capped search result order.
        pub const DESCRIPTORS: &[Descriptor] = &[$(Descriptor { key:FieldKey::$key, scope:Scope::$scope, workspace:Workspace::$workspace, group:GroupId::$group, label:$label, aliases:$aliases, slug:stringify!($key) },)*];
    };
}
/// Static discovery/navigation metadata; neither validation proof nor edit authority.
#[derive(Clone, Copy, Debug)]
pub struct Descriptor {
    /// Typed identity used by raw/semantic field mappings.
    pub key: FieldKey,
    /// Candidate objects to enumerate for this setting.
    pub scope: Scope,
    /// Workspace to activate before focusing the control.
    pub workspace: Workspace,
    /// Disclosure group to expand before focus.
    pub group: GroupId,
    /// Human-readable control description, never a CSS selector.
    pub label: &'static str,
    /// Additional case-insensitive substring search terms.
    pub aliases: &'static str,
    /// Compile-time key spelling used in a lowercased DOM suffix.
    slug: &'static str,
}
registry! {
    BleEnabled, Root, Device, Ble, "Enable BLE prototype", "wireless adapter radio usb";
    BleProfile, Root, Device, Ble, "BLE profile", "vlinker mc+ obdlink cx test peer";
    BleAddress, Root, Device, Ble, "BLE peer address", "mac identity public random static";
    BleAddressKind, Root, Device, Ble, "BLE address kind", "public random static pairing";
    BleReconnect, Root, Device, Ble, "Reconnect automatically", "bluetooth ble";
    BleConnectTimeout, Root, Device, Ble, "Connection timeout (ms)", "bluetooth ble deadline";
    BleResponseTimeout, Root, Device, Ble, "Response timeout (ms)", "bluetooth ble deadline";
    BlePollGap, Root, Device, Ble, "Minimum command gap (ms)", "bluetooth ble polling";
    Orientation, Root, Device, Bindings, "Default screen orientation", "landscape portrait mounting flip rotation";
    StatusOrientation, Root, Device, Bindings, "Boot info orientation", "startup system check inherit landscape portrait flip";
    SplashOrientation, Root, Device, Bindings, "Splash orientation", "startup custom artwork inherit landscape portrait flip";
    ViewOrientation, View, Views, Layout, "View orientation", "inherit landscape portrait mounting flip rotation";
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

/// Candidate-position owner; numeric IDs are deliberately insufficient for malformed drafts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TargetOwner {
    /// Configuration-wide setting.
    Root,
    /// Candidate view position.
    View(
        /// Zero-based candidate view index, not its stored ID.
        usize,
    ),
    /// Candidate widget position.
    Widget(
        /// Zero-based candidate widget index, not its stored ID.
        usize,
    ),
    /// Fixed acquisition channel settings.
    Channel(
        /// Channel whose source/history controls are addressed.
        Channel,
    ),
    /// Band within a candidate widget.
    Band(
        /// Zero-based widget index.
        usize,
        /// Zero-based retained band index.
        u8,
    ),
    /// One RGB pattern.
    Led(
        /// True selects warning; false selects normal operation.
        bool,
    ),
    /// Physical button mapping on a control page.
    Button(
        /// Page whose mapping is edited.
        ControlPage,
        /// Physical button identity.
        ButtonId,
    ),
    /// Encoder mapping on a control page.
    Encoder(
        /// Page whose rotation/click binding is edited.
        ControlPage,
    ),
}
impl TargetOwner {
    /// Returns human context using zero-based object positions and one-based band numbers.
    pub fn display_label(self) -> String {
        match self {
            Self::Root => "Global settings".into(),
            Self::View(i) => format!("View [{i}]"),
            Self::Widget(i) => format!("Widget [{i}]"),
            Self::Channel(c) => c.label().into(),
            Self::Band(i, band) => format!("Widget [{i}] · band {}", usize::from(band) + 1),
            Self::Led(false) => "Normal operation".into(),
            Self::Led(true) => "Warning".into(),
            Self::Button(page, button) => format!("{} · {} button", page.label(), button.label()),
            Self::Encoder(page) => format!("{} · encoder", page.label()),
        }
    }

    /// Returns the enclosing inspection object; LED/control subowners share root authority.
    fn inspection(self) -> InspectionOwner {
        match self {
            Self::View(index) => InspectionOwner::View { index },
            Self::Widget(index) | Self::Band(index, _) => InspectionOwner::Widget { index },
            Self::Channel(channel) => InspectionOwner::Channel(channel),
            _ => InspectionOwner::Root,
        }
    }

    /// Returns a label-independent ASCII DOM prefix from numeric positions and enum indices.
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
/// Revision-bound inspection location, revalidated before focus or repair.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FieldTarget {
    /// Descriptor to resolve within the owner.
    key: FieldKey,
    /// Candidate position/subobject captured with the inspection authority.
    owner: TargetOwner,
    /// Editor-issued lineage and location proof; stale drafts invalidate it.
    inspection: InspectionTarget,
}
impl FieldTarget {
    /// Returns the captured descriptor identity without checking freshness.
    pub fn key(self) -> FieldKey { self.key }

    /// Returns the captured candidate owner without checking freshness.
    pub fn owner(self) -> TargetOwner { self.owner }

    /// Returns the editor-issued inspection authority, which callers must revalidate.
    pub fn inspection(self) -> InspectionTarget { self.inspection }

    /// Returns the captured DOM identifier; this does not establish that its control still exists.
    pub fn dom_id(&self) -> String { Registry::dom_id(self.key, self.owner) }
}
/// Whether the current variant renders an ordinary control; retained invalid values remain repairable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Capability {
    /// Current style/mode supports the control.
    Active,
    /// Stored value is inactive, with an explanation for the inspector.
    Inactive(
        /// Static explanation; not a mutation prohibition for retained repairs.
        &'static str,
    ),
}
impl Capability {
    /// Returns whether the ordinary control applies to the current variant.
    pub fn is_active(self) -> bool { matches!(self, Self::Active) }

    /// Returns the inactivity explanation, or `None` for an active control.
    pub fn reason(self) -> Option<&'static str> {
        match self {
            Self::Active => None,
            Self::Inactive(reason) => Some(reason),
        }
    }
}
/// Discovery or lineage failure; no editor state is mutated by registry operations.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RegistryError {
    /// Query exceeds 128 UTF-8 bytes.
    QueryTooLong,
    /// Owner, scope, subindex or semantic field does not match an available control.
    Unavailable,
    /// Editor rejected inspection authority or candidate access.
    Editor(
        /// Underlying stale/location/admission failure.
        EditorError,
    ),
}
impl From<EditorError> for RegistryError {
    /// Returns `error` wrapped without changing its editor failure classification.
    fn from(error: EditorError) -> Self { Self::Editor(error) }
}
/// Deterministic bounded result page; omitted matches are counted, not retained.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchResults {
    /// First at most 40 matches in descriptor/owner enumeration order.
    pub results: Vec<FieldTarget>,
    /// Matching entries beyond the retained result cap.
    pub omitted: usize,
}
/// Retained query whose failed replacement leaves the previous text intact.
#[derive(Default)]
pub struct SearchQuery(
    /// At most 128 UTF-8 bytes; default is empty.
    String,
);
impl SearchQuery {
    /// Returns the retained query without normalization.
    pub fn text(&self) -> &str { &self.0 }

    /// Replaces the query with `text`; returns `QueryTooLong` above 128 UTF-8 bytes,
    /// preserving the old query on error.
    pub fn set(
        &mut self,
        text: &str,
    ) -> Result<(), RegistryError> {
        if text.len() > 128 {
            return Err(RegistryError::QueryTooLong);
        }
        self.0 = text.into();
        Ok(())
    }
}
/// One bounded validation/raw diagnostic with optional navigation authority.
#[derive(Clone, Debug)]
pub struct ErrorTarget {
    /// Exact read-only location when still representable by the editor.
    pub inspection: Option<InspectionTarget>,
    /// Editable field navigation when semantic/raw provenance maps to a known control.
    pub field: Option<FieldTarget>,
    /// Original JSON/list location, retained even without a control mapping.
    pub path: String,
    /// Diagnostic code or raw-parser error label.
    pub label: String,
}
/// Freshly resolved navigation destination and current variant capability.
#[derive(Clone, Debug)]
pub struct ResolvedField {
    /// Workspace containing the control.
    pub workspace: Workspace,
    /// Disclosure group to expand.
    pub group: GroupId,
    /// Numeric owner prefix plus static descriptor suffix.
    pub dom_id: String,
    /// Whether the ordinary control applies; repairs may still be available.
    pub capability: Capability,
    /// Current candidate location and editability metadata.
    pub inspection: ResolvedInspection,
}
/// Stateless mapping between schema fields, candidate locations and browser controls.
pub struct Registry;

/// Persistent control properties resolve their current DOM position on each
/// render. This is a display binding, never edit or inspection authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldId {
    /// Raw field whose stable object ID is looked up on each render.
    Raw(
        /// Parser field, including any stable widget/view ID.
        RawField,
    ),
    /// Typed widget control whose candidate position may change after reorder.
    Widget {
        /// Stored widget ID; duplicate or missing IDs do not resolve.
        id: u16,
        /// Descriptor required to have widget scope.
        key: FieldKey,
    },
}
impl FieldId {
    /// Returns the current DOM ID in `editor`; missing/duplicate owners, wrong scopes
    /// and absent bands return `Unavailable`. This supplies no editing authority.
    pub fn resolve(
        self,
        editor: &Editor,
    ) -> Result<String, RegistryError> {
        let (key, owner) = match self {
            Self::Raw(field) => {
                (
                    Registry::raw_key(field),
                    Registry::raw_owner(editor, field).ok_or(RegistryError::Unavailable)?,
                )
            }
            Self::Widget { id, key } => {
                if Registry::descriptor(key).scope != Scope::Widget {
                    return Err(RegistryError::Unavailable);
                }
                (
                    key,
                    Registry::raw_owner(editor, RawField::Widget(id, WidgetField::Label))
                        .ok_or(RegistryError::Unavailable)?,
                )
            }
        };
        if let TargetOwner::Band(index, band) = owner
            && Registry::candidate_widget(editor, index)
                .is_none_or(|widget| widget.advanced.bands().get(band as usize).is_none())
        {
            return Err(RegistryError::Unavailable);
        }
        Ok(Registry::dom_id(key, owner))
    }

    /// Returns the companion preset-control ID in `editor`, propagating binding failures.
    pub fn preset(
        self,
        editor: &Editor,
    ) -> Result<String, RegistryError> {
        self.resolve(editor).map(|id| format!("{id}-preset"))
    }
}
impl Registry {
    /// Collect current report links without looking up a validation owner's ID.
    /// Unsupported fields retain their actual JSON/list provenance read-only.
    /// Returns links from `editor`'s JSON candidate report in preference to its visual
    /// draft report, or an empty list without either report; unresolved targets stay `None`.
    pub fn error_targets(editor: &Editor) -> Vec<ErrorTarget> {
        let visual = editor.diagnostic();
        let owned_report = visual.as_ref().map(|draft| draft.validation_report());
        let report = match editor.json_draft().map(|draft| draft.state()) {
            Some(crate::json_draft::JsonState::Candidate { report, .. }) => Some(report),
            _ => owned_report.as_ref(),
        };
        let Some(report) = report else {
            return Vec::new();
        };
        let mut links = Vec::new();
        for issue in &report.issues {
            let location = issue.location;
            let inspection = editor.inspection_target(location).ok();
            let owner = if let Some(index) = location.widget_index() {
                match location.field() {
                    Field::BandMinimum(b)
                    | Field::BandMaximum(b)
                    | Field::BandOrder(b)
                    | Field::BandLabel(b)
                    | Field::BandColor(b) => TargetOwner::Band(index, b),
                    _ => TargetOwner::Widget(index),
                }
            } else if let Some(index) = location.view_index() {
                TargetOwner::View(index)
            } else if let Some(index) = location.channel_index() {
                TargetOwner::Channel(Channel::ALL[index])
            } else {
                match location.field() {
                    Field::LedBrightness(w) | Field::LedPeriod(w) | Field::LedOnPercent(w) => TargetOwner::Led(w),
                    _ => TargetOwner::Root,
                }
            };
            let field = Self::validation_key(location.field()).and_then(|key| {
                editor
                    .raw_fields()
                    .iter()
                    .enumerate()
                    .find(|(_, raw)| {
                        Self::raw_key(raw.field) == key && Self::raw_owner(editor, raw.field) == Some(owner)
                    })
                    .and_then(|(index, _)| Self::retained_raw_target(editor, index, owner).ok())
                    .or_else(|| Self::semantic_target(editor, location, key, owner).ok())
            });
            let inspection = field.map(|target| target.inspection()).or(inspection);
            links.push(ErrorTarget {
                inspection,
                field,
                path: location.to_string(),
                label: format!("{:?}", issue.error.code),
            });
        }
        for raw in &report.raw_errors {
            let inspection = editor.raw_inspection_target(raw.raw_index).ok();
            let field = Self::raw_owner(editor, raw.field)
                .and_then(|owner| Self::retained_raw_target(editor, raw.raw_index, owner).ok());
            links.push(ErrorTarget {
                inspection,
                field,
                path: format!("Raw input {} · {:?}", raw.raw_index, raw.field),
                label: format!("{:?}", raw.error),
            });
        }
        links
    }

    /// Canvas layout/hits use the ordinary editing layout, independently of
    /// any read-only inspected object. Reducers still check mutation authority.
    /// Returns the first widget with `id` in `editor`'s editing layout, or `None` if absent.
    pub fn layout_widget(
        editor: &Editor,
        id: u16,
    ) -> Option<Widget> {
        editor
            .editing_config()
            .widgets
            .iter()
            .find(|widget| widget.id == id)
            .cloned()
    }

    /// Synchronous read-only candidate access. A Config is data, never validation proof.
    /// Calls `read` with `editor`'s inspection candidate and returns its result unchanged.
    pub fn with_candidate<R>(
        editor: &Editor,
        read: impl FnOnce(&Config) -> R,
    ) -> R {
        editor.with_inspection_config(read)
    }

    /// Returns a target for retained raw row `index` in `editor` only when its current
    /// owner equals `owner`. Missing/mismatched rows or invalid inspection authority error.
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

    /// Returns a target for `location` in `editor` after checking its mapped `key`,
    /// exact `owner` indices and LED/band identity. Mismatches return `Unavailable`;
    /// editor inspection failures propagate without changing selection or drafts.
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

    /// Returns the representative control for validation `field`, or `None` when no
    /// editable mapping exists. Compound range/order errors focus their first boundary.
    pub fn validation_key(field: Field) -> Option<FieldKey> {
        use FieldKey::*;
        Some(match field {
            Field::BlePeer => BleAddress,
            Field::BleConnectTimeout => BleConnectTimeout,
            Field::BleResponseTimeout => BleResponseTimeout,
            Field::BlePollGap => BlePollGap,
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

    /// Returns a clone at zero-based `index` in `editor`'s inspection candidate,
    /// or `None` when absent; malformed candidate values are intentionally retained.
    pub fn candidate_widget(
        editor: &Editor,
        index: usize,
    ) -> Option<Widget> {
        editor.with_inspection_config(|c| c.widgets.get(index).cloned())
    }

    /// Returns the stored ID at candidate view `index` in `editor`, or `None` if absent.
    /// The returned ID need not be unique in an invalid candidate.
    pub fn candidate_view_id(
        editor: &Editor,
        index: usize,
    ) -> Option<u8> {
        editor.with_inspection_config(|c| c.views.get(index).map(|v| v.id))
    }

    /// Returns every descriptor in deterministic registry declaration order.
    pub fn descriptors() -> &'static [Descriptor] { DESCRIPTORS }

    /// Returns the unique row for `key`; panics only if the generated key/table invariant is broken.
    pub fn descriptor(key: FieldKey) -> &'static Descriptor {
        DESCRIPTORS
            .iter()
            .find(|d| d.key == key)
            .expect("enumerated registry key")
    }

    /// Returns the ASCII DOM identifier for `key` and numeric `owner`, without checking
    /// scope, candidate existence or control activity; labels never become selectors.
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

    /// Returns fresh inspection authority for `key` at `owner` in `editor`, or errors
    /// for inaccessible objects, mismatched scope, absent bands or the reserved Y hold.
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

    /// Calls `visit` for each descriptor/owner in `config`, retaining duplicate object
    /// IDs by position and excluding Y hold. Work follows schema-bounded object/band counts.
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

    /// Returns all current targets in `editor` in declaration/owner order. Any resolution
    /// failure discards the accumulated vector and returns the last observed error.
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

    /// Returns at most 40 matches plus an exact omitted count for `query` in `editor`.
    /// All lowercased whitespace tokens must occur in label/alias/object context; an empty
    /// query matches everything. Rejects over 128 UTF-8 bytes and propagates target failures.
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

    /// Returns current human context for `target` in `editor` after revalidating its
    /// lineage/scope. Stale or unavailable targets error before candidate indexing.
    pub fn search_context(
        editor: &Editor,
        target: FieldTarget,
    ) -> Result<String, RegistryError> {
        Self::resolve(editor, target)?;
        Ok(editor.with_inspection_config(|c| {
            match target.owner {
                TargetOwner::Widget(i) | TargetOwner::Band(i, _) => {
                    format!("Widget [{i}] {} · view {}", c.widgets[i].label, c.widgets[i].view)
                }
                TargetOwner::View(i) => format!("View [{i}] {}", c.views[i].name),
                other => other.display_label(),
            }
        }))
    }

    /// Returns current navigation/capability metadata for `target` in `editor`.
    /// Rejects stale authority, owner/scope mismatch, absent bands and reserved Y holds;
    /// inactive fields resolve with a reason and do not grant edit permission.
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

    /// Returns whether `key` has an ordinary control for widget `w`'s channel, style,
    /// span and option variants. Inactivity does not prevent repair of retained raw errors.
    pub fn widget_capability(
        key: FieldKey,
        w: &Widget,
    ) -> Capability {
        use FieldKey::*;
        let c = w.capabilities();
        let active = match key {
            GaugeMinimum | GaugeMaximum | GaugeExpand => !w.gauge.kind.is_numeric(),
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
        } else if matches!(key, GaugeMinimum | GaugeMaximum | GaugeExpand) {
            Capability::Inactive("Numeric style does not draw a gauge scale; retained invalid values remain repairable")
        } else {
            Capability::Inactive(
                "Stored setting is inactive for this channel, style or size; retained invalid values remain repairable",
            )
        }
    }

    /// Returns the descriptor for parser `field`, discarding its object/band identity.
    pub fn raw_key(field: RawField) -> FieldKey {
        use FieldKey::*;
        match field {
            RawField::Ble(field) => {
                match field {
                    BleField::Address => BleAddress,
                    BleField::ConnectTimeout => BleConnectTimeout,
                    BleField::ResponseTimeout => BleResponseTimeout,
                    BleField::PollGap => BlePollGap,
                }
            }
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

    /// Returns the candidate owner of raw `field` in `editor`; missing or duplicate
    /// widget/view IDs yield `None`. Band indices are carried here, checked by resolution.
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
