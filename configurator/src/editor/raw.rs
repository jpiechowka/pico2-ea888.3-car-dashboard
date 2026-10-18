//! Raw field applicability and parsing; acceptance and history belong to Editor.
use super::*;

pub(super) fn check_raw_available(
    config: &Config,
    field: RawField,
) -> Result<(), EditorError> {
    // Availability is independent of current selection. Reuse the same target
    // checks as text entry without treating navigation as a variant change.
    let selection = match field {
        RawField::Widget(id, _) => {
            Selection {
                view: config.widgets[widget_index(config, id)?].view,
                widget: Some(id),
            }
        }
        RawField::ViewName(view) => Selection { view, widget: None },
        _ => {
            Selection {
                view: config.boot_view,
                widget: None,
            }
        }
    };
    check_raw_target(config, selection, field)
}

pub(super) fn check_raw_target(
    config: &Config,
    selection: Selection,
    field: RawField,
) -> Result<(), EditorError> {
    match field {
        RawField::ViewName(id) => {
            view_index(config, id)?;
            if selection.view != id {
                return Err(EditorError::SelectionMismatch);
            }
        }
        RawField::Widget(id, field) => {
            let widget = &config.widgets[widget_index(config, id)?];
            if selection.widget != Some(id) || selection.view != widget.view {
                return Err(EditorError::SelectionMismatch);
            }
            if let WidgetField::BandMinimum(i)
            | WidgetField::BandMaximum(i)
            | WidgetField::BandLabel(i)
            | WidgetField::BandColor(i) = field
                && usize::from(i) >= widget.advanced.bands().len()
            {
                return Err(EditorError::FieldUnavailable);
            }
            if matches!(field, WidgetField::GraphMinimum | WidgetField::GraphMaximum)
                && !matches!(widget.display.graph_range, GraphRange::Fixed { .. })
            {
                return Err(EditorError::FieldUnavailable);
            }
        }
        RawField::Fuel(field) => {
            match (field, config.fuel) {
                (FuelField::Ratio, Fuel::Stoichiometric { .. }) => {}
                (
                    FuelField::ReferenceCelsius,
                    Fuel::EthanolBlend(Blend {
                        density_reference: DensityReference::CommonCelsius(_),
                        ..
                    }),
                ) => {}
                (
                    FuelField::Percent
                    | FuelField::GasolineRatio
                    | FuelField::EthanolRatio
                    | FuelField::GasolineDensity
                    | FuelField::EthanolDensity,
                    Fuel::EthanolBlend(_),
                ) => {}
                _ => return Err(EditorError::FieldUnavailable),
            }
        }
        RawField::Source(channel, field) => {
            let source = config.sources[channel.index()];
            match field {
                SourceField::Request | SourceField::Response if !matches!(source.route, Route::Physical11 { .. }) => {
                    return Err(EditorError::FieldUnavailable);
                }
                SourceField::PollMilliseconds if matches!(source.poll, Poll::Off) => {
                    return Err(EditorError::FieldUnavailable);
                }
                _ => {}
            }
        }
        RawField::ControlHoldMilliseconds
        | RawField::WarningPageMilliseconds
        | RawField::WarningColor { .. }
        | RawField::HistorySeconds(_)
        | RawField::Led { .. }
        | RawField::Ble(_) => {}
    }
    Ok(())
}

pub(crate) fn apply_raw(
    config: &mut Config,
    input: &RawInput,
) -> Result<(), EditorError> {
    let RawInput { field, text } = input;
    let parse_error = || EditorError::Parse(*field);
    let number = || text.trim().parse::<f32>().map_err(|_| parse_error());
    let integer = || text.trim().parse::<u16>().map_err(|_| parse_error());
    match *field {
        RawField::Ble(field) => {
            match field {
                BleField::Address => {
                    config.ble.peer.address = crate::ble::parse_address(text).ok_or_else(parse_error)?
                }
                BleField::ConnectTimeout => config.ble.connect_timeout_ms = integer()?,
                BleField::ResponseTimeout => config.ble.response_timeout_ms = integer()?,
                BleField::PollGap => config.ble.poll_gap_ms = integer()?,
            }
        }
        RawField::HistorySeconds(channel) => config.history_seconds[channel.index()] = integer()?,
        RawField::Led { warning, field } => {
            let pattern = if warning {
                &mut config.led.warning
            } else {
                &mut config.led.normal
            };
            match field {
                LedField::Color => pattern.color = Color::parse(text).ok_or_else(parse_error)?,
                LedField::Brightness => pattern.brightness_percent = text.trim().parse().map_err(|_| parse_error())?,
                LedField::Period => pattern.period_ms = integer()?,
                LedField::OnPercent => pattern.on_percent = text.trim().parse().map_err(|_| parse_error())?,
            }
        }
        RawField::ControlHoldMilliseconds => config.controls.hold_ms = integer()?,
        RawField::ViewName(id) => {
            let index = view_index(config, id)?;
            config.views[index].name = bounded(text)?;
        }
        RawField::Widget(id, field) => {
            let index = widget_index(config, id)?;
            let widget = &mut config.widgets[index];
            match field {
                WidgetField::SmoothingMilliseconds => widget.advanced.smoothing_ms = integer()?,
                WidgetField::PeakHoldMilliseconds => widget.advanced.peak_hold_ms = integer()?,
                WidgetField::Reference => widget.advanced.reference = number()?,
                WidgetField::WarningTriggerHigh => widget.warning.advanced.trigger_high = number()?,
                WidgetField::WarningClearHigh => widget.warning.advanced.clear_high = number()?,
                WidgetField::WarningSnoozeSeconds => widget.warning.advanced.snooze_s = integer()?,
                WidgetField::BandMinimum(i)
                | WidgetField::BandMaximum(i)
                | WidgetField::BandColor(i)
                | WidgetField::BandLabel(i) => {
                    let mut bands = widget.advanced.bands().to_vec();
                    let band = bands.get_mut(usize::from(i)).ok_or(EditorError::FieldUnavailable)?;
                    match field {
                        WidgetField::BandMinimum(_) => band.min = number()?,
                        WidgetField::BandMaximum(_) => band.max = number()?,
                        WidgetField::BandColor(_) => band.color = Color::parse(text).ok_or_else(parse_error)?,
                        WidgetField::BandLabel(_) => {
                            band.set_label(text).map_err(|code| {
                                EditorError::Validation(ValidationError {
                                    code,
                                    field: Field::BandLabel(i),
                                    widget: Some(id),
                                    view: Some(widget.view),
                                    channel: Some(widget.channel),
                                })
                            })?
                        }
                        _ => unreachable!(),
                    }
                    widget.advanced.set_bands(&bands).map_err(EditorError::Validation)?;
                }
                WidgetField::Label => widget.label = bounded(text)?,
                WidgetField::Decimals => widget.display.decimals = text.trim().parse().map_err(|_| parse_error())?,
                WidgetField::Color(role) => role.set(&mut widget.colors, Color::parse(text).ok_or_else(parse_error)?),
                WidgetField::GaugeMinimum => widget.gauge.min = number()?,
                WidgetField::GaugeMaximum => widget.gauge.max = number()?,
                WidgetField::GraphMinimum | WidgetField::GraphMaximum => {
                    let GraphRange::Fixed { min, max } = &mut widget.display.graph_range else {
                        return Err(EditorError::FieldUnavailable);
                    };
                    if field == WidgetField::GraphMinimum {
                        *min = number()?;
                    } else {
                        *max = number()?;
                    }
                }
                WidgetField::WarningMessage => widget.warning.message = bounded(text)?,
                WidgetField::WarningTrigger => widget.warning.trigger = number()?,
                WidgetField::WarningClear => widget.warning.clear = number()?,
                WidgetField::WarningLoad => widget.warning.min_load_percent = number()?,
                WidgetField::WarningOn => widget.warning.on_ms = integer()?,
                WidgetField::WarningOff => widget.warning.off_ms = integer()?,
            }
        }
        RawField::Fuel(field) => {
            match &mut config.fuel {
                Fuel::Stoichiometric { ratio } if field == FuelField::Ratio => *ratio = number()?,
                Fuel::EthanolBlend(blend) => {
                    match field {
                        FuelField::Percent => blend.percent = number()?,
                        FuelField::GasolineRatio => blend.gasoline_stoich = number()?,
                        FuelField::EthanolRatio => blend.ethanol_stoich = number()?,
                        FuelField::GasolineDensity => blend.gasoline_density = number()?,
                        FuelField::EthanolDensity => blend.ethanol_density = number()?,
                        FuelField::ReferenceCelsius => {
                            blend.density_reference = DensityReference::CommonCelsius(number()?)
                        }
                        FuelField::Ratio => return Err(EditorError::FieldUnavailable),
                    }
                }
                _ => return Err(EditorError::FieldUnavailable),
            }
        }
        RawField::Source(channel, field) => {
            let source = &mut config.sources[channel.index()];
            match field {
                SourceField::Request | SourceField::Response => {
                    let Route::Physical11 { request, response } = &mut source.route else {
                        return Err(EditorError::FieldUnavailable);
                    };
                    let trimmed = text.trim();
                    let value = if let Some(hex) = trimmed.strip_prefix("0x").or_else(|| trimmed.strip_prefix("0X")) {
                        u16::from_str_radix(hex, 16).map_err(|_| parse_error())?
                    } else {
                        integer()?
                    };
                    if field == SourceField::Request {
                        *request = value;
                    } else {
                        *response = value;
                    }
                }
                SourceField::PollMilliseconds => source.poll = Poll::EveryMs(integer()?),
                SourceField::FreshnessMilliseconds => source.freshness_ms = integer()?,
            }
        }
        RawField::WarningPageMilliseconds => config.warnings.page_ms = integer()?,
        RawField::WarningColor { foreground } => {
            let color = Color::parse(text).ok_or_else(parse_error)?;
            if foreground {
                config.warning_colors.foreground = color;
            } else {
                config.warning_colors.background = color;
            }
        }
    }
    Ok(())
}
