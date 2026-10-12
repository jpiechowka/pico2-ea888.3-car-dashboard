use crate::*;

#[path = "slot.rs"]
mod slot;
pub use slot::ConfigSlot;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorCode {
    Version,
    Empty,
    Duplicate,
    Reference,
    Text,
    Bounds,
    Overlap,
    Incompatible,
    SourceMetadata,
    Hysteresis,
    FuelBasis,
    Contrast,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    Controls(ControlError),
    Version,
    CatalogueVersion,
    Views,
    ViewId,
    ViewName,
    BootView,
    WidgetId,
    WidgetView,
    Placement,
    Label,
    Decimals,
    Afr,
    GraphRange,
    GaugeRange,
    Color(ColorRole),
    WarningColors,
    WarningMode,
    WarningTrigger,
    WarningClear,
    WarningLoad,
    WarningOn,
    WarningOff,
    WarningMessage,
    WarningPage,
    SourceDefinition,
    SourceAccess,
    SourceDecoder,
    SourceEvidence,
    SourceRoute,
    SourcePoll,
    SourceFreshness,
    FuelRatio,
    FuelPercent,
    FuelGasolineRatio,
    FuelEthanolRatio,
    FuelGasolineDensity,
    FuelEthanolDensity,
    FuelReference,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ValidationError {
    pub code: ErrorCode,
    pub field: Field,
    pub view: Option<u8>,
    pub widget: Option<u16>,
    pub channel: Option<Channel>,
}

impl ValidationError {
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
#[derive(Clone, Debug, PartialEq)]
pub struct ValidatedConfig {
    config: Config,
    stoich: f32,
}

#[cfg(target_pointer_width = "32")]
const _: () = assert!(core::mem::size_of::<ValidatedConfig>() <= 10_240);

impl ValidatedConfig {
    pub const fn config(&self) -> &Config { &self.config }

    pub const fn stoich(&self) -> f32 { self.stoich }
}

pub(crate) fn in_range(
    value: f32,
    min: f32,
    max: f32,
) -> bool {
    value.is_finite() && value >= min && value <= max
}

fn text_valid(
    text: &str,
    max: usize,
) -> bool {
    !text.is_empty()
        && text.len() <= max
        && text.bytes().all(|b| (32..=126).contains(&b))
        && text.bytes().any(|b| b != b' ')
}

/// Validation is bounded by eight views and sixty-four widget instances. The
/// quadratic ID checks use no secondary heap or target-sized scratch buffer.
pub fn validate(config: Config) -> Result<ValidatedConfig, ValidationError> {
    let stoich = validate_ref(&config)?;
    Ok(ValidatedConfig { config, stoich })
}

pub(crate) fn validate_ref(config: &Config) -> Result<f32, ValidationError> {
    let error = ValidationError::new;
    if config.version != VERSION {
        return Err(error(ErrorCode::Version, Field::Version));
    }
    if config.catalogue_version != CATALOGUE_VERSION {
        return Err(error(ErrorCode::Version, Field::CatalogueVersion));
    }
    validate_controls(&config.controls).map_err(|cause| {
        let code = match cause {
            ControlError::HoldDuration => ErrorCode::Bounds,
            ControlError::UnreachableScreen { .. } | ControlError::MissingViewNavigation => ErrorCode::Reference,
            _ => ErrorCode::Incompatible,
        };
        error(code, Field::Controls(cause))
    })?;
    if config.views.is_empty() {
        return Err(error(ErrorCode::Empty, Field::Views));
    }
    for (i, view) in config.views.iter().enumerate() {
        let at = |code, field| {
            ValidationError {
                view: Some(view.id),
                ..error(code, field)
            }
        };
        if view.id == 0 {
            return Err(at(ErrorCode::Bounds, Field::ViewId));
        }
        if config.views[..i].iter().any(|v| v.id == view.id) {
            return Err(at(ErrorCode::Duplicate, Field::ViewId));
        }
        if !text_valid(&view.name, 14) {
            return Err(at(ErrorCode::Text, Field::ViewName));
        }
    }
    if !config.views.iter().any(|v| v.id == config.boot_view) {
        return Err(error(ErrorCode::Reference, Field::BootView));
    }
    let mut occupied = [0_u8; MAX_VIEWS];
    for (i, widget) in config.widgets.iter().enumerate() {
        let at = |code, field| {
            ValidationError {
                view: Some(widget.view),
                widget: Some(widget.id),
                channel: Some(widget.channel),
                ..error(code, field)
            }
        };
        if widget.id == 0 {
            return Err(at(ErrorCode::Bounds, Field::WidgetId));
        }
        if config.widgets[..i].iter().any(|w| w.id == widget.id) {
            return Err(at(ErrorCode::Duplicate, Field::WidgetId));
        }
        let Some(view_index) = config.views.iter().position(|v| v.id == widget.view) else {
            return Err(at(ErrorCode::Reference, Field::WidgetView));
        };
        let Some(cells) = widget.placement.occupied() else {
            return Err(at(ErrorCode::Bounds, Field::Placement));
        };
        if occupied[view_index] & cells != 0 {
            return Err(at(ErrorCode::Overlap, Field::Placement));
        }
        occupied[view_index] |= cells;
        if !text_valid(&widget.label, 12 * usize::from(widget.placement.span.columns())) {
            return Err(at(ErrorCode::Text, Field::Label));
        }
        if let Some(role) = widget.colors.invalid_role() {
            return Err(at(ErrorCode::Contrast, Field::Color(role)));
        }
        let display = widget.display;
        if !widget.gauge.valid(widget.channel) {
            return Err(at(ErrorCode::Bounds, Field::GaugeRange));
        }
        if display.decimals > 3 {
            return Err(at(ErrorCode::Bounds, Field::Decimals));
        }
        if display.afr && widget.channel != Channel::Lambda {
            return Err(at(ErrorCode::Incompatible, Field::Afr));
        }
        let (low, high) = widget.channel.domain();
        if let GraphRange::Fixed { min, max } = display.graph_range
            && (!in_range(min, low, high) || !in_range(max, low, high) || min >= max)
        {
            return Err(at(ErrorCode::Bounds, Field::GraphRange));
        }
        let warning = &widget.warning;
        if (widget.channel == Channel::Lambda) != (warning.mode == LimitMode::LambdaDeviation) {
            return Err(at(ErrorCode::Incompatible, Field::WarningMode));
        }
        let (low, high) = if warning.mode == LimitMode::LambdaDeviation {
            (0.0, 2.0)
        } else {
            (low, high)
        };
        if !in_range(warning.trigger, low, high) {
            return Err(at(ErrorCode::Bounds, Field::WarningTrigger));
        }
        if !in_range(warning.clear, low, high) {
            return Err(at(ErrorCode::Bounds, Field::WarningClear));
        }
        let ordered = match warning.mode {
            LimitMode::High | LimitMode::LambdaDeviation => warning.clear < warning.trigger,
            LimitMode::Low => warning.clear > warning.trigger,
        };
        if !ordered {
            return Err(at(ErrorCode::Hysteresis, Field::WarningClear));
        }
        if !in_range(warning.min_load_percent, 0.0, 100.0) {
            return Err(at(ErrorCode::Bounds, Field::WarningLoad));
        }
        if !(1..=30_000).contains(&warning.on_ms) {
            return Err(at(ErrorCode::Bounds, Field::WarningOn));
        }
        if !(1..=30_000).contains(&warning.off_ms) {
            return Err(at(ErrorCode::Bounds, Field::WarningOff));
        }
        if !text_valid(&warning.message, 20) {
            return Err(at(ErrorCode::Text, Field::WarningMessage));
        }
    }
    if !(500..=10_000).contains(&config.warnings.page_ms) {
        return Err(error(ErrorCode::Bounds, Field::WarningPage));
    }
    if !config
        .warning_colors
        .foreground
        .contrasts(config.warning_colors.background)
    {
        return Err(error(ErrorCode::Contrast, Field::WarningColors));
    }
    for channel in Channel::ALL {
        let source = config.sources[channel.index()];
        let at = |code, field| {
            ValidationError {
                channel: Some(channel),
                ..error(code, field)
            }
        };
        let def = definition(source.definition);
        if def.channel.is_some_and(|c| c != channel) {
            return Err(at(ErrorCode::Incompatible, Field::SourceDefinition));
        }
        if source.access != def.access {
            return Err(at(ErrorCode::SourceMetadata, Field::SourceAccess));
        }
        if source.decoder != def.decoder {
            return Err(at(ErrorCode::SourceMetadata, Field::SourceDecoder));
        }
        if source.evidence != def.evidence {
            return Err(at(ErrorCode::SourceMetadata, Field::SourceEvidence));
        }
        if let Route::Physical11 { request, response } = source.route
            && (request > 0x7FF || response > 0x7FF || request == response || request == 0x7DF)
        {
            return Err(at(ErrorCode::Bounds, Field::SourceRoute));
        }
        if let Poll::EveryMs(period) = source.poll
            && (source.definition == DefinitionId::Disabled || !(minimum_period(channel)..=10_000).contains(&period))
        {
            return Err(at(ErrorCode::Bounds, Field::SourcePoll));
        }
        if !(1..=channel.freshness_cap()).contains(&source.freshness_ms) {
            return Err(at(ErrorCode::Bounds, Field::SourceFreshness));
        }
    }
    crate::fuel::stoichiometry(config.fuel)
}
