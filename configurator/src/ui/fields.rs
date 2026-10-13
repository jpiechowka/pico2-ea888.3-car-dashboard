use super::*;

pub fn field_id(field: RawField) -> String {
    format!("field-{field:?}")
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect()
}

fn validation_raw(error: ValidationError) -> Option<RawField> {
    let field = match error.field {
        Field::HistorySeconds(c) => return Some(RawField::HistorySeconds(c)),
        Field::LedBrightness(warning) => {
            return Some(RawField::Led {
                warning,
                field: LedField::Brightness,
            });
        }
        Field::LedPeriod(warning) => {
            return Some(RawField::Led {
                warning,
                field: LedField::Period,
            });
        }
        Field::LedOnPercent(warning) => {
            return Some(RawField::Led {
                warning,
                field: LedField::OnPercent,
            });
        }
        Field::WarningPage => return Some(RawField::WarningPageMilliseconds),
        Field::WarningColors => return Some(RawField::WarningColor { foreground: true }),
        Field::ViewName => return Some(RawField::ViewName(error.view?)),
        Field::FuelRatio => return Some(RawField::Fuel(FuelField::Ratio)),
        Field::FuelPercent => return Some(RawField::Fuel(FuelField::Percent)),
        Field::FuelGasolineRatio => return Some(RawField::Fuel(FuelField::GasolineRatio)),
        Field::FuelEthanolRatio => return Some(RawField::Fuel(FuelField::EthanolRatio)),
        Field::FuelGasolineDensity => return Some(RawField::Fuel(FuelField::GasolineDensity)),
        Field::FuelEthanolDensity => return Some(RawField::Fuel(FuelField::EthanolDensity)),
        Field::FuelReference => return Some(RawField::Fuel(FuelField::ReferenceCelsius)),
        Field::Smoothing => WidgetField::SmoothingMilliseconds,
        Field::PeakHold => WidgetField::PeakHoldMilliseconds,
        Field::Reference => WidgetField::Reference,
        Field::BandMinimum(i) => WidgetField::BandMinimum(i),
        Field::BandMaximum(i) => WidgetField::BandMaximum(i),
        Field::BandOrder(i) => WidgetField::BandMinimum(i),
        Field::BandLabel(i) => WidgetField::BandLabel(i),
        Field::BandColor(i) => WidgetField::BandColor(i),
        Field::WarningTriggerHigh => WidgetField::WarningTriggerHigh,
        Field::WarningClearHigh => WidgetField::WarningClearHigh,
        Field::WarningSnooze => WidgetField::WarningSnoozeSeconds,
        Field::WarningTrigger => WidgetField::WarningTrigger,
        Field::WarningClear => WidgetField::WarningClear,
        Field::WarningOn => WidgetField::WarningOn,
        Field::WarningOff => WidgetField::WarningOff,
        Field::WarningMessage => WidgetField::WarningMessage,
        Field::WarningLoad => WidgetField::WarningLoad,
        Field::GraphRange => WidgetField::GraphMinimum,
        Field::Color(role) => WidgetField::Color(role),
        Field::GaugeRange => WidgetField::GaugeMinimum,
        Field::Label => WidgetField::Label,
        Field::Decimals => WidgetField::Decimals,
        _ => return None,
    };
    Some(RawField::Widget(error.widget?, field))
}

#[component]
pub fn DiagnosticFocus(ui: Ui) -> impl IntoView {
    let target = move || {
        ui.editor.with(|e| {
            match e.draft_error()? {
                EditorError::Parse(field) => Some(field_id(field)),
                EditorError::Validation(error) => {
                    validation_raw(error).map(field_id).or_else(|| {
                        match error.field {
                            Field::WarningAcknowledge => Some("control-routes".into()),
                            Field::Readout | Field::Afr => error.widget.map(|id| format!("widget-{id}-readout")),
                            Field::Secondary => error.widget.map(|id| format!("widget-{id}-secondary")),
                            Field::Placement => error.widget.map(|id| format!("widget-{id}")),
                            _ => None,
                        }
                    })
                }
                _ => None,
            }
        })
    };
    view! { <button hidden=move ||target().is_none() on:click=move |_| {
        if let Some(target) = target() {
            if target == "control-routes" { ui.global_tab.set("controls"); }
            if target.contains("field-led-") {ui.global_tab.set("led");}
            if target.contains("field-warning") {ui.global_tab.set("warnings");}
            if target.contains("field-fuel") {ui.global_tab.set("fuel");}
            leptos::leptos_dom::helpers::request_animation_frame(move ||{
                if let Some(element)=web_sys::window().and_then(|w|w.document()).and_then(|d|d.get_element_by_id(&target)).and_then(|e|e.dyn_into::<web_sys::HtmlElement>().ok()) {
                    if let Ok(Some(details))=element.closest("details") { let _=details.set_attribute("open",""); }
                    let _=element.focus(); element.scroll_into_view();
                }
            });
        }
    }>"Focus invalid field"</button> }
}

pub fn raw_value(
    editor: &Editor,
    field: RawField,
) -> String {
    if let Some(raw) = editor.raw_fields().iter().find(|raw| raw.field == field) {
        return raw.text.clone();
    }
    let c = editor.editing_config();
    match field {
        RawField::HistorySeconds(channel) => c.history_seconds[channel.index()].to_string(),
        RawField::Led { warning, field } => {
            let p = if warning { c.led.warning } else { c.led.normal };
            match field {
                LedField::Color => p.color.hex().to_string(),
                LedField::Brightness => p.brightness_percent.to_string(),
                LedField::Period => p.period_ms.to_string(),
                LedField::OnPercent => p.on_percent.to_string(),
            }
        }
        RawField::ControlHoldMilliseconds => c.controls.hold_ms.to_string(),
        RawField::ViewName(id) => {
            c.views
                .iter()
                .find(|v| v.id == id)
                .map(|v| v.name.to_string())
                .unwrap_or_default()
        }
        RawField::Widget(id, field) => {
            c.widgets
                .iter()
                .find(|w| w.id == id)
                .map(|w| {
                    match field {
                        WidgetField::SmoothingMilliseconds => w.advanced.smoothing_ms.to_string(),
                        WidgetField::PeakHoldMilliseconds => w.advanced.peak_hold_ms.to_string(),
                        WidgetField::Reference => w.advanced.reference.to_string(),
                        WidgetField::WarningTriggerHigh => w.warning.advanced.trigger_high.to_string(),
                        WidgetField::WarningClearHigh => w.warning.advanced.clear_high.to_string(),
                        WidgetField::WarningSnoozeSeconds => w.warning.advanced.snooze_s.to_string(),
                        WidgetField::BandMinimum(i)
                        | WidgetField::BandMaximum(i)
                        | WidgetField::BandColor(i)
                        | WidgetField::BandLabel(i) => {
                            w.advanced.bands().get(usize::from(i)).map_or(String::new(), |b| {
                                match field {
                                    WidgetField::BandMinimum(_) => b.min.to_string(),
                                    WidgetField::BandMaximum(_) => b.max.to_string(),
                                    WidgetField::BandColor(_) => b.color.hex().to_string(),
                                    _ => b.label().to_string(),
                                }
                            })
                        }
                        WidgetField::Label => w.label.to_string(),
                        WidgetField::Color(role) => role.get(&w.colors).hex().to_string(),
                        WidgetField::Decimals => w.display.decimals.to_string(),
                        WidgetField::GaugeMinimum => w.gauge.min.to_string(),
                        WidgetField::GaugeMaximum => w.gauge.max.to_string(),
                        WidgetField::GraphMinimum => {
                            match w.display.graph_range {
                                GraphRange::Fixed { min, .. } => min.to_string(),
                                _ => String::new(),
                            }
                        }
                        WidgetField::GraphMaximum => {
                            match w.display.graph_range {
                                GraphRange::Fixed { max, .. } => max.to_string(),
                                _ => String::new(),
                            }
                        }
                        WidgetField::WarningMessage => w.warning.message.to_string(),
                        WidgetField::WarningTrigger => w.warning.trigger.to_string(),
                        WidgetField::WarningClear => w.warning.clear.to_string(),
                        WidgetField::WarningLoad => w.warning.min_load_percent.to_string(),
                        WidgetField::WarningOn => w.warning.on_ms.to_string(),
                        WidgetField::WarningOff => w.warning.off_ms.to_string(),
                    }
                })
                .unwrap_or_default()
        }
        RawField::WarningPageMilliseconds => c.warnings.page_ms.to_string(),
        RawField::WarningColor { foreground } => {
            if foreground {
                c.warning_colors.foreground
            } else {
                c.warning_colors.background
            }
            .hex()
            .to_string()
        }
        RawField::Fuel(field) => {
            match c.fuel {
                Fuel::Stoichiometric { ratio } => ratio.to_string(),
                Fuel::EthanolBlend(b) => {
                    match field {
                        FuelField::Percent => b.percent.to_string(),
                        FuelField::GasolineRatio => b.gasoline_stoich.to_string(),
                        FuelField::EthanolRatio => b.ethanol_stoich.to_string(),
                        FuelField::GasolineDensity => b.gasoline_density.to_string(),
                        FuelField::EthanolDensity => b.ethanol_density.to_string(),
                        FuelField::ReferenceCelsius => {
                            match b.density_reference {
                                DensityReference::CommonCelsius(t) => t.to_string(),
                                _ => String::new(),
                            }
                        }
                        FuelField::Ratio => String::new(),
                    }
                }
                _ => String::new(),
            }
        }
        RawField::Source(channel, field) => {
            let source = c.sources[channel.index()];
            match field {
                SourceField::Request => {
                    match source.route {
                        Route::Physical11 { request, .. } => format!("0x{request:03X}"),
                        _ => String::new(),
                    }
                }
                SourceField::Response => {
                    match source.route {
                        Route::Physical11 { response, .. } => format!("0x{response:03X}"),
                        _ => String::new(),
                    }
                }
                SourceField::PollMilliseconds => {
                    match source.poll {
                        Poll::EveryMs(t) => t.to_string(),
                        _ => String::new(),
                    }
                }
                SourceField::FreshnessMilliseconds => source.freshness_ms.to_string(),
            }
        }
    }
}

#[component]
pub fn TextField(
    ui: Ui,
    field: RawField,
    label: &'static str,
    #[prop(default = "")] hint: &'static str,
    #[prop(default = String::new())] id: String,
    #[prop(optional)] accessible_label: Option<String>,
    #[prop(optional)] disabled: Option<Signal<bool>>,
) -> impl IntoView {
    let id = if id.is_empty() { field_id(field) } else { id };
    let text = RwSignal::new(ui.editor.with_untracked(|e| raw_value(e, field)));
    let focused = RwSignal::new(false);
    let binding = RwSignal::new(ui.editor.with_untracked(Editor::field_token));
    Effect::new(move |_| {
        ui.editor.with(|editor| {
            let current = editor.field_token();
            // Keep a local numeric prefix ("0.") only for this field's own
            // acknowledged input. Import/undo/external revisions replace it,
            // even when focus never left this input.
            if !focused.get_untracked() || binding.get_untracked() != current {
                text.set(raw_value(editor, field));
            }
            binding.set(current);
        });
    });
    view! {
        <label class="field">
            <span>{label}</span>
            <input id=id aria-label=accessible_label disabled=move ||disabled.is_some_and(|d|d.get()) type="text" maxlength="1024" autocomplete="off" spellcheck="false"
                prop:value=move || text.get()
                aria-invalid=move || ui.editor.with(|e| e.raw_fields().iter().any(|r| r.field == field)).to_string()
                on:focus=move |_| focused.set(true)
                on:blur=move |_| { focused.set(false); text.set(ui.editor.with_untracked(|e| raw_value(e, field))); }
                on:input=move |event| {
                    let Some(token) = binding.try_get_untracked() else { return; };
                    let value = event_target_value(&event);
                    text.set(value.clone());
                    let result = ui.run(move |editor| editor.set_raw(token, field, value));
                    if result { binding.set(ui.editor.with_untracked(Editor::field_token)); }
                    else { text.set(ui.editor.with_untracked(|e| raw_value(e, field))); }
                } />
            <small>{hint}</small>
            <small class="control-error">{move || ui.editor.with(|e| e.draft_error().filter(|error| match error {
                EditorError::Parse(f) => *f == field,
                EditorError::Validation(v) => validation_raw(*v) == Some(field),
                _ => false,
            }).map(error_text))}</small>
        </label>
    }
}

#[component]
pub fn Choice(
    ui: Ui,
    label: &'static str,
    value: Signal<String>,
    options: Vec<(String, String)>,
    on_change: Callback<(FieldToken, String)>,
    #[prop(default = false)] layout: bool,
    #[prop(default = false)] repair_invalid_draft: bool,
    #[prop(default = String::new())] id: String,
    #[prop(optional)] accessible_label: Option<String>,
    #[prop(optional)] disabled: Option<Signal<bool>>,
    #[prop(optional)] disabled_values: Option<Signal<Vec<String>>>,
) -> impl IntoView {
    let binding = RwSignal::new(ui.editor.with_untracked(Editor::field_token));
    Effect::new(move |_| binding.set(ui.editor.with(Editor::field_token)));
    view! {
        <label class="field"><span>{label}</span>
            <select id=id aria-label=accessible_label prop:value=move || value.get() disabled=move || disabled.is_some_and(|d| d.get()) || if repair_invalid_draft { false } else if layout { ui.layout_locked() } else { ui.invalid() }
                on:change=move |event| {
                    if let Some(token) = binding.try_get_untracked() { on_change.run((token,event_target_value(&event))); }
                    event_target::<web_sys::HtmlSelectElement>(&event).set_value(&value.get_untracked());
                }>
                {options.into_iter().map(|(value,label)| {let check=value.clone();view! { <option value=value disabled=move ||disabled_values.is_some_and(|d|d.get().contains(&check))>{label}</option> }}).collect_view()}
            </select>
        </label>
    }
}

#[component]
pub fn Toggle(
    ui: Ui,
    label: &'static str,
    value: Signal<bool>,
    on_change: Callback<(FieldToken, bool)>,
    #[prop(default = false)] repair_invalid_draft: bool,
    #[prop(optional)] disabled: Option<Signal<bool>>,
) -> impl IntoView {
    let binding = RwSignal::new(ui.editor.with_untracked(Editor::field_token));
    Effect::new(move |_| binding.set(ui.editor.with(Editor::field_token)));
    view! {
        <label class="toggle"><span>{label}</span><input type="checkbox"
            prop:checked=move || value.get() disabled=move || disabled.is_some_and(|d|d.get()) || (!repair_invalid_draft && ui.invalid())
            on:change=move |event| {
                if let Some(token) = binding.try_get_untracked() { on_change.run((token,event_target_checked(&event))); }
                event_target::<web_sys::HtmlInputElement>(&event).set_checked(value.get_untracked());
            } /><i aria-hidden="true"></i></label>
    }
}

pub fn choices(items: &[(&str, &str)]) -> Vec<(String, String)> {
    items
        .iter()
        .map(|(value, label)| (value.to_string(), label.to_string()))
        .collect()
}

pub fn error_text(error: EditorError) -> String {
    match error {
        EditorError::Validation(e) => {
            let reason = match e.code {
                ErrorCode::Contrast => {
                    "Colours need at least 3:1 contrast after RGB565 conversion. Repair the palette or discard the \
                     edit."
                }
                ErrorCode::Overlap if matches!(e.field, Field::BandOrder(_)) => {
                    "Bands must be ordered without overlap. Touching endpoints are allowed."
                }
                ErrorCode::Overlap => "Widgets overlap. Choose free cells or a smaller size.",
                ErrorCode::Text if matches!(e.field, Field::BandLabel(_)) => {
                    "Use 0–5 printable ASCII characters. Empty is allowed; a nonempty label cannot be only spaces. The \
                     invalid preview keeps the last bounded label; exact typed text stays here."
                }
                ErrorCode::Text => "Use nonempty printable ASCII within the shown character limit.",
                ErrorCode::Hysteresis => {
                    "High: clear < trigger; low: trigger < clear. Outside window: trigger low < clear low ≤ clear high \
                     < trigger high. Inside window: clear low < trigger low ≤ trigger high < clear high."
                }
                ErrorCode::Bounds => "The value is outside the allowed range.",
                ErrorCode::Incompatible => "This option does not apply to this channel.",
                ErrorCode::FuelBasis => "Measured volume blends need a common density reference temperature.",
                ErrorCode::SourceMetadata => "Source metadata must match the selected catalogue definition.",
                _ if e.field == Field::WarningAcknowledge => {
                    "Bind a Dashboard onboard button to Acknowledge warnings in Device controls before enabling latch \
                     or snooze."
                }
                _ => "The configuration has an invalid or missing reference.",
            };
            let field = match e.field {
                Field::Color(role) => role.label().to_owned(),
                Field::WarningColors => "Combined warning colours".to_owned(),
                Field::GaugeRange => "Gauge limits (finite, at least 0.001 apart)".to_owned(),
                _ => {
                    format!("{:?}", e.field)
                        .replace("Warning", "Warning ")
                        .replace("Fuel", "Fuel ")
                }
            };
            format!("{field}: {reason}")
        }
        EditorError::InvalidDraft => "Correct the highlighted fields, or discard the invalid edits first.".into(),
        EditorError::Parse(
            RawField::Widget(_, WidgetField::Color(_) | WidgetField::BandColor(_))
            | RawField::WarningColor { .. }
            | RawField::Led {
                field: LedField::Color, ..
            },
        ) => "Enter a colour as #RRGGBB, for example #FF3048.".into(),
        EditorError::Parse(field) => format!("Correct {field:?}: enter a complete number before applying this change."),
        EditorError::LastView => "Keep at least one view.".into(),
        EditorError::ViewCapacity => "Up to eight views are supported.".into(),
        EditorError::WidgetCapacity => "Up to 64 widgets are supported.".into(),
        EditorError::StaleToken => "That action belongs to an older edit and was ignored.".into(),
        EditorError::TextCapacity | EditorError::RawTooLong => "Text exceeds the field limit.".into(),
        EditorError::Codec(ConfigError::Json { line, column }) => {
            format!("Invalid JSON at line {line}, column {column}. Check fields, types and duplicate keys.")
        }
        EditorError::Codec(_) => "The file is invalid, incompatible, oversized or has a failed checksum.".into(),
        _ => format!("Cannot apply this action: {error:?}."),
    }
}

/// Both picker and text entry use the same transactional raw-field path.
#[component]
pub fn ColorField(
    ui: Ui,
    field: RawField,
    label: &'static str,
) -> impl IntoView {
    let binding = RwSignal::new(ui.editor.with_untracked(Editor::field_token));
    Effect::new(move |_| binding.set(ui.editor.with(Editor::field_token)));
    view! {
        <div class="colour-control">
            <TextField ui field label/>
            <input type="color" aria-label=format!("Pick {label}")
                prop:value=move || ui.editor.with(|e|Color::parse(&raw_value(e,field)).unwrap_or(Color(0)).hex().to_string())
                on:input=move |event| {
                    if let Some(token) = binding.try_get_untracked() {
                        let value = event_target_value(&event);
                        ui.run(move |e|e.set_raw(token,field,value));
                    }
                }/>
        </div>
    }
}
