use super::*;

pub fn raw_value(
    editor: &Editor,
    field: RawField,
) -> String {
    if let Some(raw) = editor.raw_fields().iter().find(|raw| raw.field == field) {
        return raw.text.clone();
    }
    let c = editor.editing_config();
    match field {
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
) -> impl IntoView {
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
            <input id=id aria-label=accessible_label type="text" maxlength="1024" autocomplete="off" spellcheck="false"
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
) -> impl IntoView {
    let binding = RwSignal::new(ui.editor.with_untracked(Editor::field_token));
    Effect::new(move |_| binding.set(ui.editor.with(Editor::field_token)));
    view! {
        <label class="field"><span>{label}</span>
            <select id=id aria-label=accessible_label prop:value=move || value.get() disabled=move || if repair_invalid_draft { false } else if layout { ui.layout_locked() } else { ui.invalid() }
                on:change=move |event| {
                    if let Some(token) = binding.try_get_untracked() { on_change.run((token,event_target_value(&event))); }
                    event_target::<web_sys::HtmlSelectElement>(&event).set_value(&value.get_untracked());
                }>
                {options.into_iter().map(|(value,label)| view! { <option value=value>{label}</option> }).collect_view()}
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
) -> impl IntoView {
    let binding = RwSignal::new(ui.editor.with_untracked(Editor::field_token));
    Effect::new(move |_| binding.set(ui.editor.with(Editor::field_token)));
    view! {
        <label class="toggle"><span>{label}</span><input type="checkbox"
            prop:checked=move || value.get() disabled=move || ui.invalid()
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
                ErrorCode::Overlap => "Widgets overlap. Choose free cells or a smaller size.",
                ErrorCode::Text => "Use nonempty printable ASCII within the shown character limit.",
                ErrorCode::Hysteresis => "Clear must be below a high trigger, or above a low trigger.",
                ErrorCode::Bounds => "The value is outside the allowed range.",
                ErrorCode::Incompatible => "This option does not apply to this channel.",
                ErrorCode::FuelBasis => "Measured volume blends need a common density reference temperature.",
                ErrorCode::SourceMetadata => "Source metadata must match the selected catalogue definition.",
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
        EditorError::Parse(RawField::Widget(_, WidgetField::Color(_)) | RawField::WarningColor { .. }) => {
            "Enter a colour as #RRGGBB, for example #FF3048.".into()
        }
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
