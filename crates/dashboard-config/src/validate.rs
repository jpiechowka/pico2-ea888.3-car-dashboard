//! Shared candidate traversal, precise invalid-field locations and immutable admission proof.
//! The same ordered visitor supports first-error firmware admission and full editor repair lists.
use core::ops::ControlFlow;

use crate::*;

#[path = "slot.rs"]
mod slot;
pub use slot::ConfigSlot;

/// Stable validation category; [`Field`] and candidate coordinates supply the precise context.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorCode {
    /// Unsupported dashboard or catalogue version.
    Version,
    /// Required collection is empty.
    Empty,
    /// Reused view/widget ID.
    Duplicate,
    /// Missing reference or unreachable required action.
    Reference,
    /// Empty, blank, oversized or non-printable-ASCII text where disallowed.
    Text,
    /// Numeric, geometric, timing or collection bound violated; includes nonfinite numbers.
    Bounds,
    /// Widget occupancy or ordered gauge bands overlap.
    Overlap,
    /// Field combination is incompatible with its channel, screen or policy.
    Incompatible,
    /// Editable source metadata differs from its closed catalogue definition.
    SourceMetadata,
    /// Finite warning endpoints lack the required hysteresis ordering.
    Hysteresis,
    /// Fuel composition/ratio/reference assumptions are inconsistent.
    FuelBasis,
    /// Required foreground/background pair lacks the configured contrast floor.
    Contrast,
}

/// Repair field, including bounded subcoordinates for bands, controls and LED patterns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    /// BLE address/type/enable combination, displayed at the address field.
    BlePeer,
    /// BLE connection deadline in milliseconds.
    BleConnectTimeout,
    /// BLE response deadline in milliseconds.
    BleResponseTimeout,
    /// BLE inter-transaction gap in milliseconds.
    BlePollGap,
    /// Input binding/reachability coordinate.
    Controls(
        /// Specific page/button/gesture or global control failure.
        ControlError,
    ),
    /// Dashboard payload schema revision.
    Version,
    /// Documentary source catalogue revision.
    CatalogueVersion,
    /// Root named-view collection.
    Views,
    /// A view's nonzero unique ID.
    ViewId,
    /// A view's printable nonblank name.
    ViewName,
    /// Initial selected view reference.
    BootView,
    /// A widget's nonzero globally unique ID.
    WidgetId,
    /// A widget's referenced view ID.
    WidgetView,
    /// Widget grid rectangle or overlap.
    Placement,
    /// Widget printable label and span-dependent length.
    Label,
    /// Decimal precision.
    Decimals,
    /// AFR/channel/readout consistency.
    Afr,
    /// Primary readout units and channel compatibility.
    Readout,
    /// Optional secondary pressure units.
    Secondary,
    /// Numeric smoothing time constant.
    Smoothing,
    /// Peak-hold duration.
    PeakHold,
    /// Manual reference value or finite dormant value.
    Reference,
    /// Active band count or out-of-range band edit index.
    BandCount,
    /// Optional short band text.
    BandLabel(
        /// Zero-based active band index.
        u8,
    ),
    /// Band lower canonical endpoint.
    BandMinimum(
        /// Zero-based active band index.
        u8,
    ),
    /// Band upper endpoint or within-band ordering.
    BandMaximum(
        /// Zero-based active band index.
        u8,
    ),
    /// Band ordering/overlap with its predecessor.
    BandOrder(
        /// Zero-based later band index.
        u8,
    ),
    /// Band/track contrast.
    BandColor(
        /// Zero-based active band index.
        u8,
    ),
    /// Per-channel history duration.
    HistorySeconds(
        /// Channel whose indexed duration needs repair.
        Channel,
    ),
    /// Upper warning trigger edge or non-window canonical zero.
    WarningTriggerHigh,
    /// Upper warning clear edge or non-window canonical zero.
    WarningClearHigh,
    /// Warning snooze duration.
    WarningSnooze,
    /// Missing onboard acknowledgment action for enabled latch/snooze behavior.
    WarningAcknowledge,
    /// RGB pattern brightness percentage.
    LedBrightness(
        /// `true` selects warning; `false` selects normal pattern.
        bool,
    ),
    /// RGB pattern period.
    LedPeriod(
        /// `true` selects warning; `false` selects normal pattern.
        bool,
    ),
    /// RGB blink on-percentage.
    LedOnPercent(
        /// `true` selects warning; `false` selects normal pattern.
        bool,
    ),
    /// Fixed graph endpoints or ordering.
    GraphRange,
    /// Gauge mode/range/channel compatibility.
    GaugeRange,
    /// Per-widget contrast role.
    Color(
        /// Foreground/decorative role that failed its required comparison.
        ColorRole,
    ),
    /// Shared warning-popup foreground/background contrast.
    WarningColors,
    /// Warning predicate/channel compatibility.
    WarningMode,
    /// Trigger threshold or lower trigger-window edge.
    WarningTrigger,
    /// Clear threshold, lower clear-window edge or hysteresis relation.
    WarningClear,
    /// Minimum qualifying load percentage.
    WarningLoad,
    /// Activation dwell in milliseconds.
    WarningOn,
    /// Clear dwell in milliseconds.
    WarningOff,
    /// Printable warning label.
    WarningMessage,
    /// Popup page interval.
    WarningPage,
    /// Catalogue definition/channel compatibility.
    SourceDefinition,
    /// Request metadata/catalogue mismatch.
    SourceAccess,
    /// Decoder metadata/catalogue mismatch.
    SourceDecoder,
    /// Evidence classification/catalogue mismatch.
    SourceEvidence,
    /// Explicit CAN route bounds/identity.
    SourceRoute,
    /// Poll interval or disabled-source scheduling.
    SourcePoll,
    /// Source freshness lifetime.
    SourceFreshness,
    /// Final or explicit stoichiometric air/fuel ratio.
    FuelRatio,
    /// Ethanol composition percentage.
    FuelPercent,
    /// Gasoline ratio or ordering against ethanol's ratio.
    FuelGasolineRatio,
    /// Ethanol stoichiometric ratio.
    FuelEthanolRatio,
    /// Gasoline density.
    FuelGasolineDensity,
    /// Ethanol density.
    FuelEthanolDensity,
    /// Shared density temperature or measured-volume reference requirement.
    FuelReference,
}

/// First-error-compatible diagnostic carrying candidate IDs, which may themselves be invalid.
/// Use [`ValidationIssue::location`] when duplicate IDs require unambiguous editor selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ValidationError {
    /// Failure category.
    pub code: ErrorCode,
    /// Specific repair field.
    pub field: Field,
    /// Candidate view ID when applicable; not a validated reference/index.
    pub view: Option<u8>,
    /// Candidate widget ID when applicable; may be zero or duplicated.
    pub widget: Option<u16>,
    /// Owning channel when applicable.
    pub channel: Option<Channel>,
}

/// Candidate list coordinates, independent of IDs that may be duplicated.
/// Field carries bounded band, control page/button/gesture and LED coordinates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ValidationLocation {
    /// Candidate view-array index, only retained for unambiguous widget view references.
    view: Option<u8>,
    /// Candidate root widget-array index, independent of its ID.
    widget: Option<u8>,
    /// Owning source/history channel or widget channel.
    channel: Option<Channel>,
    /// Field and any bounded subcoordinate.
    field: Field,
}

impl ValidationLocation {
    /// Return a root-only coordinate for `field`, without view/widget/channel indices.
    const fn root(field: Field) -> Self {
        Self {
            view: None,
            widget: None,
            channel: None,
            field,
        }
    }

    /// Return the candidate view-array index, or `None` for root/ambiguous references.
    pub const fn view_index(self) -> Option<usize> {
        match self.view {
            Some(i) => Some(i as usize),
            None => None,
        }
    }

    /// Return the candidate root widget-array index, or `None` outside widget validation.
    pub const fn widget_index(self) -> Option<usize> {
        match self.widget {
            Some(i) => Some(i as usize),
            None => None,
        }
    }

    /// Return the stable owning-channel array index, or `None` without channel context.
    pub const fn channel_index(self) -> Option<usize> {
        match self.channel {
            Some(c) => Some(c.index()),
            None => None,
        }
    }

    /// Return the specific repair field, including any band/control/LED subcoordinate.
    pub const fn field(self) -> Field { self.field }
}

/// Formatting follows the JSON arrays: widgets are a root list.
/// No heap strings are retained or constructed by firmware validation.
impl core::fmt::Display for ValidationLocation {
    /// Write a JSON-style repair path to `f`; returns only sink formatting errors.
    /// Candidate list indices take precedence over IDs; no heap allocation is performed here.
    fn fmt(
        &self,
        f: &mut core::fmt::Formatter<'_>,
    ) -> core::fmt::Result {
        use Field::*;
        if let Some(i) = self.widget {
            write!(f, "/widgets/{i}")?;
        } else if let Some(i) = self.view {
            write!(f, "/views/{i}")?;
        } else if let Some(channel) = self.channel {
            let group = if matches!(self.field, HistorySeconds(_)) {
                "history_seconds"
            } else {
                "sources"
            };
            write!(f, "/{group}/{}", channel.index())?;
        }
        let path = match self.field {
            BlePeer => "ble/peer/address",
            BleConnectTimeout => "ble/connect_timeout_ms",
            BleResponseTimeout => "ble/response_timeout_ms",
            BlePollGap => "ble/poll_gap_ms",
            Version => "version",
            CatalogueVersion => "catalogue_version",
            Views => "views",
            ViewId | WidgetId => "id",
            ViewName => "name",
            BootView => "boot_view",
            WidgetView => "view",
            Placement => "placement",
            Label => "label",
            Decimals => "display/decimals",
            Afr => "display/afr",
            GraphRange => "display/graph_range",
            GaugeRange => "gauge",
            Readout => "advanced/readout",
            Secondary => "advanced/secondary",
            Smoothing => "advanced/smoothing_ms",
            PeakHold => "advanced/peak_hold_ms",
            Reference => "advanced/reference",
            BandCount => "advanced/bands",
            BandLabel(i) | BandMinimum(i) | BandMaximum(i) | BandOrder(i) | BandColor(i) => {
                let key = match self.field {
                    BandLabel(_) => "label",
                    BandMinimum(_) => "min",
                    BandMaximum(_) => "max",
                    BandColor(_) => "color",
                    _ => "min",
                };
                return write!(f, "/advanced/bands/{i}/{key}");
            }
            Color(role) => {
                let key = match role {
                    ColorRole::Background => "background",
                    ColorRole::Border => "border",
                    ColorRole::Label => "label",
                    ColorRole::Value => "value",
                    ColorRole::Secondary => "secondary",
                    ColorRole::Graph => "graph",
                    ColorRole::Gauge => "gauge",
                    ColorRole::Track => "track",
                    ColorRole::Ticks => "ticks",
                    ColorRole::Average => "average",
                    ColorRole::Minimum => "minimum",
                    ColorRole::Maximum => "maximum",
                    ColorRole::Trend => "trend",
                    ColorRole::Peak => "peak",
                    ColorRole::WarningForeground => "warning/foreground",
                    ColorRole::WarningBackground => "warning/background",
                };
                return write!(f, "/colors/{key}");
            }
            HistorySeconds(_) => return Ok(()),
            WarningTriggerHigh => "warning/trigger_high",
            WarningClearHigh => "warning/clear_high",
            WarningSnooze => "warning/snooze_s",
            WarningAcknowledge => "warning/latch",
            WarningMode => "warning/mode",
            WarningTrigger => "warning/trigger",
            WarningClear => "warning/clear",
            WarningLoad => "warning/min_load_percent",
            WarningOn => "warning/on_ms",
            WarningOff => "warning/off_ms",
            WarningMessage => "warning/message",
            WarningPage => "warnings/page_ms",
            WarningColors => "warning_colors",
            LedBrightness(warning) | LedPeriod(warning) | LedOnPercent(warning) => {
                let pattern = if warning { "warning" } else { "normal" };
                let key = match self.field {
                    LedBrightness(_) => "brightness_percent",
                    LedPeriod(_) => "period_ms",
                    _ => "on_percent",
                };
                return write!(f, "/led/{pattern}/{key}");
            }
            SourceDefinition => "definition",
            SourceAccess => "access",
            SourceDecoder => "decoder",
            SourceEvidence => "evidence",
            SourceRoute => "route",
            SourcePoll => "poll",
            SourceFreshness => "freshness_ms",
            FuelRatio => "fuel/Stoichiometric/ratio",
            FuelPercent => "fuel/EthanolBlend/percent",
            FuelGasolineRatio => "fuel/EthanolBlend/gasoline_stoich",
            FuelEthanolRatio => "fuel/EthanolBlend/ethanol_stoich",
            FuelGasolineDensity => "fuel/EthanolBlend/gasoline_density",
            FuelEthanolDensity => "fuel/EthanolBlend/ethanol_density",
            FuelReference => "fuel/EthanolBlend/density_reference",
            Controls(cause) => {
                use ControlError::*;
                match cause {
                    HoldDuration => return f.write_str("/controls/hold_ms"),
                    ReservedRecovery { page } => return write!(f, "/controls/screens/{}/buttons/3/hold", page.index()),
                    IncompatibleButton { page, button, hold } => {
                        return write!(
                            f,
                            "/controls/screens/{}/buttons/{}/{}",
                            page.index(),
                            button.index(),
                            if hold { "hold" } else { "tap" }
                        );
                    }
                    IncompatibleEncoderClick { page } => {
                        return write!(f, "/controls/screens/{}/encoder_click", page.index());
                    }
                    IncompatibleEncoderRotation { page } => {
                        return write!(f, "/controls/screens/{}/encoder_rotation", page.index());
                    }
                    UnreachableScreen { page } => return write!(f, "/controls/screens/{}", page.index()),
                    MissingViewNavigation => return f.write_str("/controls/screens/0/buttons"),
                }
            }
        };
        write!(f, "/{path}")
    }
}

/// Complete repair issue combining compatibility error IDs with unambiguous candidate coordinates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ValidationIssue {
    /// Category, field and candidate identity values.
    pub error: ValidationError,
    /// Candidate array/subfield coordinate for editor navigation.
    pub location: ValidationLocation,
}

impl ValidationError {
    /// Return root-level `code`/`field` failure; callers add applicable IDs/channel context.
    pub(crate) const fn new(
        code: ErrorCode,
        field: Field,
    ) -> Self {
        Self {
            code,
            field,
            view: None,
            widget: None,
            channel: None,
        }
    }
}

/// Immutable proof that every editable field passed the shared validation.
///
/// ```compile_fail
/// let mut config = dashboard_config::validate(dashboard_config::default_config()).unwrap();
/// config.config().boot_view = 200;
/// ```
/// ```compile_fail
/// let config = dashboard_config::ValidatedConfig {
///     config: dashboard_config::default_config(), stoich: f32::NAN,
/// };
/// ```
#[derive(Clone, Debug)]
pub struct ValidatedConfig {
    /// Admitted immutable candidate; mutation requires new validation.
    config: Config,
    /// Validated finite stoichiometric air/fuel mass ratio, 5..=25.
    stoich: f32,
    /// Process-local exact cache key; `None` requires cache bypass.
    cache_identity: Option<ConfigCacheIdentity>,
    /// Distinguishes never-issued storage from permanent global issuer exhaustion.
    cache_status: CacheIdentityStatus,
}

// Config storage plus runtime proof metadata; whole-image checks run separately.
#[cfg(target_pointer_width = "32")]
/// Target storage ceiling for model plus admission/cache metadata; not whole-image headroom.
const _: () = assert!(core::mem::size_of::<ValidatedConfig>() <= 14_608);

impl PartialEq for ValidatedConfig {
    /// Return semantic equality with `other`, ignoring runtime cache-key/status differences.
    fn eq(
        &self,
        other: &Self,
    ) -> bool {
        self.config == other.config && self.stoich == other.stoich
    }
}

impl ValidatedConfig {
    /// Return the exact process-local cache key or `None` when caching must be bypassed.
    pub const fn cache_identity(&self) -> Option<ConfigCacheIdentity> { self.cache_identity }

    /// Return why an exact cache key is present or unavailable; does not change validation status.
    pub const fn cache_identity_status(&self) -> CacheIdentityStatus { self.cache_status }

    /// Attempt fresh global issuance after admission; exhaustion preserves valid configuration.
    fn publish_identity(&mut self) { self.publish_issued(crate::cache_identity::issue()); }

    /// Apply `issued` atomically under exclusive borrow; error clears the key and marks exhaustion.
    /// Existing configuration and stoichiometry remain unchanged.
    fn publish_issued(
        &mut self,
        issued: Result<ConfigCacheIdentity, crate::cache_identity::IssueError>,
    ) {
        match issued {
            Ok(identity) => {
                self.cache_identity = Some(identity);
                self.cache_status = CacheIdentityStatus::Tracked;
            }
            Err(_) => {
                self.cache_identity = None;
                self.cache_status = CacheIdentityStatus::Exhausted;
            }
        }
    }

    /// Return immutable admitted model data; no mutable accessor can invalidate the proof.
    pub const fn config(&self) -> &Config { &self.config }

    /// Return admitted stoichiometric air/fuel mass ratio; not a measured lambda/AFR target.
    pub const fn stoich(&self) -> f32 { self.stoich }
}

/// Return whether `value` is finite and inclusively within caller-supplied `min`, `max`.
/// Endpoints are not independently validated; reversed/NaN endpoints naturally reject values.
pub(crate) fn in_range(
    value: f32,
    min: f32,
    max: f32,
) -> bool {
    value.is_finite() && value >= min && value <= max
}

/// Return whether `text` is nonempty printable ASCII, at most `max` bytes and not all spaces.
fn text_valid(
    text: &str,
    max: usize,
) -> bool {
    !text.is_empty()
        && text.len() <= max
        && text.bytes().all(|b| (32..=126).contains(&b))
        && text.bytes().any(|b| b != b' ')
}

/// Consume candidate `config` and return immutable admission proof or its first validation error.
/// Validation is bounded by eight views and sixty-four widget instances; quadratic ID checks
/// use no secondary heap. Cache-key exhaustion preserves success with caching disabled.
pub fn validate(config: Config) -> Result<ValidatedConfig, ValidationError> {
    let stoich = validate_ref(&config)?;
    let mut validated = ValidatedConfig {
        config,
        stoich,
        cache_identity: None,
        cache_status: CacheIdentityStatus::Untracked,
    };
    validated.publish_identity();
    Ok(validated)
}

/// Validate borrowed `config` and return admitted stoichiometric ratio or the first failure.
/// Does not issue a cache key or mutate the candidate; first-error order matches the full visitor.
pub(crate) fn validate_ref(config: &Config) -> Result<f32, ValidationError> {
    let mut first = None;
    let result = visit_validation(config, |issue| {
        first = Some(issue.error);
        ControlFlow::Break(())
    });
    match result {
        ControlFlow::Continue(Some(stoich)) => Ok(stoich),
        _ => Err(first.expect("first-error visitor stops at every invalid field")),
    }
}

/// Visit candidate `config` in deterministic admission order, passing each issue to `emit`.
/// Returns `Break(())` immediately when requested; `Continue(Some(stoich))` means every check
/// succeeded, and `Continue(None)` means at least one issue. No proof/key is published.
/// Independent fields remain inspectable after earlier errors; invalid/ambiguous placement
/// never indexes occupancy. Disabled/dormant fields still undergo their applicable checks.
pub fn visit_validation(
    config: &Config,
    mut emit: impl FnMut(ValidationIssue) -> ControlFlow<()>,
) -> ControlFlow<(), Option<f32>> {
    let mut valid = true;
    let mut location = ValidationLocation::root(Field::Version);
    /// Mark failure, attach current candidate coordinates and propagate visitor cancellation.
    macro_rules! issue {
        ($error:expr) => {{
            let error = $error;
            valid = false;
            emit(ValidationIssue {
                error,
                location: ValidationLocation {
                    field: error.field,
                    ..location
                },
            })?;
        }};
    }
    let error = ValidationError::new;
    if config.version != VERSION {
        issue!(error(ErrorCode::Version, Field::Version));
    }
    if config.catalogue_version != CATALOGUE_VERSION {
        issue!(error(ErrorCode::Version, Field::CatalogueVersion));
    }
    if !config.ble.peer.valid(config.ble.enabled) {
        issue!(error(ErrorCode::Bounds, Field::BlePeer));
    }
    for (value, min, max, field) in [
        (
            config.ble.connect_timeout_ms,
            BLE_CONNECT_TIMEOUT_MIN_MS,
            BLE_CONNECT_TIMEOUT_MAX_MS,
            Field::BleConnectTimeout,
        ),
        (
            config.ble.response_timeout_ms,
            BLE_RESPONSE_TIMEOUT_MIN_MS,
            BLE_RESPONSE_TIMEOUT_MAX_MS,
            Field::BleResponseTimeout,
        ),
        (
            config.ble.poll_gap_ms,
            BLE_POLL_GAP_MIN_MS,
            BLE_POLL_GAP_MAX_MS,
            Field::BlePollGap,
        ),
    ] {
        if !(min..=max).contains(&value) {
            issue!(error(ErrorCode::Bounds, field));
        }
    }
    crate::controls::visit_controls(&config.controls, |cause| {
        let code = match cause {
            ControlError::HoldDuration => ErrorCode::Bounds,
            ControlError::UnreachableScreen { .. } | ControlError::MissingViewNavigation => ErrorCode::Reference,
            _ => ErrorCode::Incompatible,
        };
        let field = Field::Controls(cause);
        valid = false;
        emit(ValidationIssue {
            error: error(code, field),
            location: ValidationLocation::root(field),
        })
    })?;
    for channel in Channel::ALL {
        location = ValidationLocation {
            channel: Some(channel),
            ..ValidationLocation::root(Field::Version)
        };
        if !(10..=900).contains(&config.history_seconds[channel.index()]) {
            issue!(error(ErrorCode::Bounds, Field::HistorySeconds(channel)));
        }
    }
    location = ValidationLocation::root(Field::Version);
    for (warning, pattern) in [(false, config.led.normal), (true, config.led.warning)] {
        if pattern.brightness_percent > 100 {
            issue!(error(ErrorCode::Bounds, Field::LedBrightness(warning)));
        }
        if !(200..=10_000).contains(&pattern.period_ms) {
            issue!(error(ErrorCode::Bounds, Field::LedPeriod(warning)));
        }
        if !(10..=90).contains(&pattern.on_percent) {
            issue!(error(ErrorCode::Bounds, Field::LedOnPercent(warning)));
        }
    }
    if config.views.is_empty() {
        issue!(error(ErrorCode::Empty, Field::Views));
    }
    for (i, view) in config.views.iter().enumerate() {
        location = ValidationLocation {
            view: Some(i as u8),
            ..ValidationLocation::root(Field::Version)
        };
        let at = |code, field| {
            ValidationError {
                view: Some(view.id),
                ..error(code, field)
            }
        };
        if view.id == 0 {
            issue!(at(ErrorCode::Bounds, Field::ViewId));
        }
        if config.views[..i].iter().any(|v| v.id == view.id) {
            issue!(at(ErrorCode::Duplicate, Field::ViewId));
        }
        if !text_valid(&view.name, 14) {
            issue!(at(ErrorCode::Text, Field::ViewName));
        }
    }
    location = ValidationLocation::root(Field::BootView);
    if !config.views.iter().any(|v| v.id == config.boot_view) {
        issue!(error(ErrorCode::Reference, Field::BootView));
    }
    let mut occupied = [0_u8; MAX_VIEWS];
    for (i, widget) in config.widgets.iter().enumerate() {
        let view_index = config.views.iter().position(|v| v.id == widget.view);
        let unambiguous_view = widget.view != 0 && config.views.iter().filter(|v| v.id == widget.view).count() == 1;
        location = ValidationLocation {
            view: view_index.filter(|_| unambiguous_view).map(|i| i as u8),
            widget: Some(i as u8),
            channel: Some(widget.channel),
            field: Field::Version,
        };
        let at = |code, field| {
            ValidationError {
                view: Some(widget.view),
                widget: Some(widget.id),
                channel: Some(widget.channel),
                ..error(code, field)
            }
        };
        if widget.id == 0 {
            issue!(at(ErrorCode::Bounds, Field::WidgetId));
        }
        if config.widgets[..i].iter().any(|w| w.id == widget.id) {
            issue!(at(ErrorCode::Duplicate, Field::WidgetId));
        }
        if view_index.is_none() {
            issue!(at(ErrorCode::Reference, Field::WidgetView));
        }
        let geometry = view_index
            .map(|index| config.views[index].orientation.unwrap_or(config.orientation))
            .unwrap_or(config.orientation)
            .geometry();
        let cells = widget.placement.occupied_in(geometry);
        if cells.is_none() {
            issue!(at(ErrorCode::Bounds, Field::Placement));
        }
        // Only usable geometry and an unambiguous view may touch occupancy.
        if let (Some(view_index), Some(cells)) = (view_index, cells)
            && unambiguous_view
        {
            if occupied[view_index] & cells != 0 {
                issue!(at(ErrorCode::Overlap, Field::Placement));
            }
            occupied[view_index] |= cells;
        }
        if !text_valid(&widget.label, 12 * usize::from(widget.placement.span.columns())) {
            issue!(at(ErrorCode::Text, Field::Label));
        }
        widget.colors.visit_invalid_roles(|role| {
            valid = false;
            let error = at(ErrorCode::Contrast, Field::Color(role));
            emit(ValidationIssue {
                error,
                location: ValidationLocation {
                    field: error.field,
                    ..location
                },
            })
        })?;
        let display = widget.display;
        if !widget.gauge.valid(widget.channel) {
            issue!(at(ErrorCode::Bounds, Field::GaugeRange));
        }
        if display.decimals > 3 {
            issue!(at(ErrorCode::Bounds, Field::Decimals));
        }
        if display.afr && widget.channel != Channel::Lambda {
            issue!(at(ErrorCode::Incompatible, Field::Afr));
        }
        let (low, high) = widget.channel.domain();
        if let GraphRange::Fixed { min, max } = display.graph_range
            && (!in_range(min, low, high) || !in_range(max, low, high) || min >= max)
        {
            issue!(at(ErrorCode::Bounds, Field::GraphRange));
        }
        let advanced = &widget.advanced;
        if !advanced.readout.compatible(widget.channel) {
            issue!(at(ErrorCode::Incompatible, Field::Readout));
        }
        if display.afr != advanced.readout.includes_afr() {
            issue!(at(ErrorCode::Incompatible, Field::Afr));
        }
        if widget.channel != Channel::Boost && advanced.secondary != Secondary::None {
            issue!(at(ErrorCode::Incompatible, Field::Secondary));
        }
        if advanced.smoothing_ms != 0 && !(50..=5_000).contains(&advanced.smoothing_ms) {
            issue!(at(ErrorCode::Bounds, Field::Smoothing));
        }
        if !(100..=60_000).contains(&advanced.peak_hold_ms) {
            issue!(at(ErrorCode::Bounds, Field::PeakHold));
        }
        if !advanced.reference.is_finite()
            || (advanced.reference_mode == ReferenceMode::Manual && !in_range(advanced.reference, low, high))
        {
            issue!(at(ErrorCode::Bounds, Field::Reference));
        }
        for (index, band) in advanced.bands().iter().enumerate() {
            let index = index as u8;
            if !in_range(band.min, low, high) {
                issue!(at(ErrorCode::Bounds, Field::BandMinimum(index)));
            }
            if !in_range(band.max, low, high) || (in_range(band.min, low, high) && band.min >= band.max) {
                issue!(at(ErrorCode::Bounds, Field::BandMaximum(index)));
            }
            if index > 0
                && in_range(band.min, low, high)
                && in_range(advanced.bands()[usize::from(index) - 1].max, low, high)
                && advanced.bands()[usize::from(index) - 1].max > band.min
            {
                issue!(at(ErrorCode::Overlap, Field::BandOrder(index)));
            }
            if !band.color.contrasts(widget.colors.track) {
                issue!(at(ErrorCode::Contrast, Field::BandColor(index)));
            }
        }
        let warning = &widget.warning;
        let extra = warning.advanced;
        if extra.snooze_s != 0 && !(5..=900).contains(&extra.snooze_s) {
            issue!(at(ErrorCode::Bounds, Field::WarningSnooze));
        }
        if warning.enabled
            && (extra.latch || extra.snooze_s != 0)
            && !config.controls.screens[ControlPage::Dashboard.index()]
                .buttons
                .iter()
                .any(|b| [b.tap, b.hold].contains(&ControlAction::AcknowledgeWarnings))
        {
            issue!(at(ErrorCode::Reference, Field::WarningAcknowledge));
        }
        let window = matches!(warning.mode, LimitMode::OutsideWindow | LimitMode::InsideWindow);
        if if window {
            !in_range(extra.trigger_high, low, high)
        } else {
            extra.trigger_high.to_bits() != 0
        } {
            issue!(at(ErrorCode::Bounds, Field::WarningTriggerHigh));
        }
        if if window {
            !in_range(extra.clear_high, low, high)
        } else {
            extra.clear_high.to_bits() != 0
        } {
            issue!(at(ErrorCode::Bounds, Field::WarningClearHigh));
        }
        if (widget.channel == Channel::Lambda) != (warning.mode == LimitMode::LambdaDeviation) {
            issue!(at(ErrorCode::Incompatible, Field::WarningMode));
        }
        let (low, high) = if warning.mode == LimitMode::LambdaDeviation {
            (0.0, 2.0)
        } else {
            (low, high)
        };
        if !in_range(warning.trigger, low, high) {
            issue!(at(ErrorCode::Bounds, Field::WarningTrigger));
        }
        if !in_range(warning.clear, low, high) {
            issue!(at(ErrorCode::Bounds, Field::WarningClear));
        }
        let ordered = match warning.mode {
            LimitMode::High | LimitMode::LambdaDeviation => warning.clear < warning.trigger,
            LimitMode::Low => warning.clear > warning.trigger,
            LimitMode::OutsideWindow => {
                warning.trigger < warning.clear
                    && warning.clear <= extra.clear_high
                    && extra.clear_high < extra.trigger_high
            }
            LimitMode::InsideWindow => {
                warning.clear < warning.trigger
                    && warning.trigger <= extra.trigger_high
                    && extra.trigger_high < extra.clear_high
            }
        };
        let bounds_valid = in_range(warning.trigger, low, high)
            && in_range(warning.clear, low, high)
            && (!window || (in_range(extra.trigger_high, low, high) && in_range(extra.clear_high, low, high)));
        if bounds_valid && !ordered {
            issue!(at(ErrorCode::Hysteresis, Field::WarningClear));
        }
        if !in_range(warning.min_load_percent, 0.0, 100.0) {
            issue!(at(ErrorCode::Bounds, Field::WarningLoad));
        }
        if !(1..=30_000).contains(&warning.on_ms) {
            issue!(at(ErrorCode::Bounds, Field::WarningOn));
        }
        if !(1..=30_000).contains(&warning.off_ms) {
            issue!(at(ErrorCode::Bounds, Field::WarningOff));
        }
        if !text_valid(&warning.message, 20) {
            issue!(at(ErrorCode::Text, Field::WarningMessage));
        }
    }
    location = ValidationLocation::root(Field::WarningPage);
    if !(500..=10_000).contains(&config.warnings.page_ms) {
        issue!(error(ErrorCode::Bounds, Field::WarningPage));
    }
    if !config
        .warning_colors
        .foreground
        .contrasts(config.warning_colors.background)
    {
        issue!(error(ErrorCode::Contrast, Field::WarningColors));
    }
    for channel in Channel::ALL {
        location = ValidationLocation {
            channel: Some(channel),
            ..ValidationLocation::root(Field::Version)
        };
        let source = config.sources[channel.index()];
        let at = |code, field| {
            ValidationError {
                channel: Some(channel),
                ..error(code, field)
            }
        };
        let def = definition(source.definition);
        let compatible = def.channel.is_none_or(|c| c == channel);
        if !compatible {
            issue!(at(ErrorCode::Incompatible, Field::SourceDefinition));
        }
        if compatible && source.access != def.access {
            issue!(at(ErrorCode::SourceMetadata, Field::SourceAccess));
        }
        if compatible && source.decoder != def.decoder {
            issue!(at(ErrorCode::SourceMetadata, Field::SourceDecoder));
        }
        if compatible && source.evidence != def.evidence {
            issue!(at(ErrorCode::SourceMetadata, Field::SourceEvidence));
        }
        if let Route::Physical11 { request, response } = source.route
            && (request > 0x7FF || response > 0x7FF || request == response || request == 0x7DF)
        {
            issue!(at(ErrorCode::Bounds, Field::SourceRoute));
        }
        if let Poll::EveryMs(period) = source.poll
            && (source.definition == DefinitionId::Disabled || !(minimum_period(channel)..=10_000).contains(&period))
        {
            issue!(at(ErrorCode::Bounds, Field::SourcePoll));
        }
        if !(1..=channel.freshness_cap()).contains(&source.freshness_ms) {
            issue!(at(ErrorCode::Bounds, Field::SourceFreshness));
        }
    }
    let stoich = crate::fuel::visit_stoichiometry(config.fuel, |error| {
        valid = false;
        emit(ValidationIssue {
            error,
            location: ValidationLocation::root(error.field),
        })
    })?;
    ControlFlow::Continue(if valid { stoich } else { None })
}

#[cfg(test)]
/// Visitor-order and identity-exhaustion regressions for otherwise valid configurations.
mod cache_tests {
    use super::*;
    #[test]
    /// First-error visitor and borrowed admission stop at the same malformed placement.
    fn visitor_first_error_matches_borrowed_validator() {
        let mut config = default_config();
        config.widgets[0].placement.row = 255;
        config.widgets[1].label.clear();
        let mut first = None;
        let result = visit_validation(&config, |issue| {
            first = Some(issue.error);
            core::ops::ControlFlow::Break(())
        });
        assert_eq!(result, core::ops::ControlFlow::Break(()));
        assert_eq!(first, validate_ref(&config).err());
        assert_eq!(first.unwrap().field, Field::Placement);
    }
    #[test]
    /// Exhaustion disables exact caching without changing valid model semantics or clone state.
    fn actual_issuance_failure_preserves_validation_and_semantic_equality() {
        let issuer = crate::cache_identity::Issuer::with_next_for_test(u32::MAX - 1);
        let mut config = validate(default_config()).unwrap();
        let original = config.clone();
        config.publish_issued(issuer.issue());
        assert_eq!(config.cache_identity_status(), CacheIdentityStatus::Tracked);
        config.publish_issued(issuer.issue());
        assert_eq!(config.cache_identity_status(), CacheIdentityStatus::Exhausted);
        assert_eq!(config.cache_identity(), None);
        assert_eq!(config.clone().cache_identity_status(), CacheIdentityStatus::Exhausted);
        assert_eq!(config, original);
    }
}
