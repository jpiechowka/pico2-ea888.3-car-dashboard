//! Browser field bindings, retained raw repairs and revision-aware accepted-edit feedback.
use dashboard_configurator::event_adapters::FieldCommit;

use super::*;

/// Finalizes `commit` against `ui` and emits one accepted-revision event with `label`;
/// disposed signals or no accepted change produce no event or simulator restart.
fn finish_field(
    ui: Ui,
    commit: StoredValue<FieldCommit>,
    label: &'static str,
) {
    let mut revision = None;
    let _ = commit.try_update_value(|commit| {
        let _ = ui.editor.try_with_untracked(|editor| revision = commit.finish(editor));
    });
    if let Some(revision) = revision {
        ui.event(Notice::info(
            Category::Editor,
            "editor.accepted",
            Some(label),
            format!("Accepted configuration revision {}", revision.get()),
        ));
        ui.editor.with_untracked(|e| ui.dock.synchronize(ui, e));
        ui.dock.report_restart(ui);
    }
}

/// Returns a legacy ASCII control ID from `field`'s debug spelling; non-alphanumerics
/// become hyphens. Registry bindings provide candidate-position IDs where available.
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

/// Returns the representative raw control for `error`, or `None` for unsupported
/// semantic fields or missing widget/view IDs; range errors focus the first boundary.
fn validation_raw(error: ValidationError) -> Option<RawField> {
    let field = match error.field {
        Field::BlePeer => return Some(RawField::Ble(BleField::Address)),
        Field::BleConnectTimeout => return Some(RawField::Ble(BleField::ConnectTimeout)),
        Field::BleResponseTimeout => return Some(RawField::Ble(BleField::ResponseTimeout)),
        Field::BlePollGap => return Some(RawField::Ble(BleField::PollGap)),
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

/// Returns `ui`'s bounded issue navigation, preserving exact candidate/raw provenance.
/// Unsupported locations remain read-only; JSON syntax errors offer the retained editor.
#[component]
pub fn DiagnosticFocus(
    /// Shared editor, diagnostic inspection and guarded field authority.
    ui: Ui
) -> impl IntoView {
    let syntax = move || {
        ui.editor.with(|e| {
            match e.json_draft().map(|d| d.state()) {
                Some(dashboard_configurator::json_draft::JsonState::SyntaxError { line, column, .. }) => {
                    Some((*line, *column))
                }
                _ => None,
            }
        })
    };
    let count = move || ui.editor.with(|e| Registry::error_targets(e).len()) + usize::from(syntax().is_some());
    view! { <details class="error-navigation" hidden=move ||count()==0>
        <summary>{move ||format!("{} configuration issues · inspect fields",count())}</summary>
        <ul>{move ||ui.editor.with(|e|Registry::error_targets(e).into_iter().map(|link| {
            let target=link.inspection;
            let field=link.field;
            view!{<li><button disabled=target.is_none() on:click=move |_|{
                if let Some(field)=field {workspaces::inspect(ui,field);}
                else if let Some(target)=target {command_palette::inspect_location(ui,target);}
            }>{link.path}</button><span>{link.label}</span></li>}
        }).collect_view())}</ul>
        <p>{move ||syntax().map(|(line,column)|format!("Invalid JSON at line {line}, column {column}. Repair the retained JSON text or Discard."))}</p>
        <button hidden=move ||syntax().is_none() on:click=move |_|json_editor::open(ui)>"Repair JSON"</button>
        <p>{move ||ui.editor.with(|e| {
            let omitted=match e.json_draft().map(|d|d.state()) {
                Some(dashboard_configurator::json_draft::JsonState::Candidate{report,..})=>report.omitted,
                _=>e.diagnostic().map(|d|d.validation_report().omitted).unwrap_or(0),
            };
            if omitted>0 {format!("{omitted} more issues omitted from the bounded report")}else{String::new()}
        })}</p>
    </details> }
}

/// Captured reducer authority for one browser input callback, rechecked on dispatch.
#[derive(Clone, Copy, PartialEq, Eq)]
enum RawBinding {
    /// Ordinary field edit within the captured editor revision.
    Normal(
        /// Revision/selection-aware field capability.
        FieldToken,
    ),
    /// New raw input scoped to a current inspected object.
    Located(
        /// Object inspection proof with unambiguous stable ID.
        InspectionTarget,
    ),
    /// Repair of an already retained raw row.
    Retained(
        /// Exact raw-row inspection authority.
        InspectionTarget,
    ),
}
/// Returns suitable authority for `field` in `editor` and `ui`'s current inspection.
/// Pending JSON, stale/ambiguous inspection or unsupported located repairs return `None`.
/// Fields outside the inspected owner use the ordinary reducer token.
fn raw_binding(
    ui: Ui,
    editor: &Editor,
    field: RawField,
) -> Option<RawBinding> {
    if editor.has_pending_json() {
        return None;
    }
    if let Some(target) = ui.inspection.get() {
        let resolved = Registry::resolve(editor, target).ok()?;
        if !resolved.inspection.id_lookup_unambiguous {
            return None;
        }
        let owner = Registry::raw_owner(editor, field)?;
        let same = owner == target.owner()
            || matches!((owner,target.owner()),(TargetOwner::Widget(a),TargetOwner::Band(b,_))|(TargetOwner::Band(a,_),TargetOwner::Widget(b))|(TargetOwner::Band(a,_),TargetOwner::Band(b,_)) if a==b);
        if same {
            if let Some(index) = editor.raw_fields().iter().position(|r| r.field == field) {
                return editor.raw_inspection_target(index).ok().map(RawBinding::Retained);
            }
            if resolved.inspection.field.is_none()
                && resolved.inspection.raw_field.is_none()
                && editor.action_readiness().block.is_none()
            {
                return editor
                    .object_inspection_target(match owner {
                        TargetOwner::Widget(index) | TargetOwner::Band(index, _) => InspectionOwner::Widget { index },
                        TargetOwner::View(index) => InspectionOwner::View { index },
                        TargetOwner::Channel(c) => InspectionOwner::Channel(c),
                        _ => InspectionOwner::Root,
                    })
                    .ok()
                    .map(RawBinding::Located);
            }
            return None;
        }
    }
    Some(RawBinding::Normal(editor.field_token()))
}
/// Applies `text` for `field` through captured `binding` in `ui`; returns the quiet
/// reducer outcome and refreshes inspection only on success. Reducers reject stale tokens
/// and may retain invalid raw values without replacing the accepted configuration.
fn dispatch_raw(
    ui: Ui,
    binding: RawBinding,
    field: RawField,
    text: String,
) -> bool {
    let inspection = ui.inspection.get_untracked();
    let ok = ui.run_quiet(move |e| {
        match binding {
            RawBinding::Normal(token) => e.set_raw(token, field, text),
            RawBinding::Located(target) => e.set_located_raw(target, field, text),
            RawBinding::Retained(target) => e.set_inspected_raw(target, text),
        }
    });
    if ok && let Some(target) = inspection {
        ui.refresh_inspection(target);
    }
    ok
}

/// Applies preset `text` to `field` in `ui` only if captured `token` is current and
/// a suitable raw binding exists; otherwise shows a stale/unavailable notice.
pub(super) fn preset_raw(
    ui: Ui,
    token: FieldToken,
    field: RawField,
    text: String,
) {
    let binding = ui.editor.with_untracked(|e| {
        if token == e.field_token() {
            raw_binding(ui, e, field)
        } else {
            None
        }
    });
    if let Some(binding) = binding {
        dispatch_raw(ui, binding, field, text);
    } else {
        ui.notice.set("That field callback is stale or unavailable.".into());
    }
}

/// Returns retained text for `field` in `editor` before formatting candidate values.
/// Absent objects/bands and inactive optional values produce an empty string; numbers
/// retain schema units, colours use RGB hex and physical CAN IDs use hexadecimal.
pub fn raw_value(
    editor: &Editor,
    field: RawField,
) -> String {
    if let Some(raw) = editor.raw_fields().iter().find(|raw| raw.field == field) {
        return raw.text.clone();
    }
    Registry::with_candidate(editor, |c| {
        match field {
            RawField::Ble(field) => {
                match field {
                    BleField::Address => dashboard_configurator::ble::format_address(c.ble.peer.address),
                    BleField::ConnectTimeout => c.ble.connect_timeout_ms.to_string(),
                    BleField::ResponseTimeout => c.ble.response_timeout_ms.to_string(),
                    BleField::PollGap => c.ble.poll_gap_ms.to_string(),
                }
            }
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
    })
}

/// Returns a raw text control in `ui` for `field` with visible `label`/`hint`.
/// `id` overrides the current registry ID when nonempty; `accessible_label` overrides
/// screen-reader text and `disabled` adds a reactive lock. Input retains numeric prefixes
/// only across its own acknowledged edits; external revisions replace local text.
/// DOM length is 1024 characters; the reducer independently enforces byte/schema bounds.
#[component]
pub fn TextField(
    /// Shared editor, diagnostics and inspection signals.
    ui: Ui,
    /// Raw parser field and stable object identity.
    field: RawField,
    /// Visible label and accepted-edit event context.
    label: &'static str,
    /// Supplemental field guidance; empty by default.
    #[prop(default = "")]
    hint: &'static str,
    /// Explicit DOM ID; empty resolves the registry binding each render.
    #[prop(default = String::new())]
    id: String,
    /// Optional accessible label independent of visible text.
    #[prop(optional)]
    accessible_label: Option<String>,
    /// Optional additional disabled condition; absence leaves binding guards in charge.
    #[prop(optional)]
    disabled: Option<Signal<bool>>,
) -> impl IntoView {
    let id = move || if id.is_empty() { ui.raw_dom(field) } else { id.clone() };
    let text = RwSignal::new(ui.editor.with_untracked(|e| raw_value(e, field)));
    let focused = RwSignal::new(false);
    let focus_key = StoredValue::new(ui.editor.with_untracked(FieldCommit::begin));
    let binding = RwSignal::new(ui.editor.with_untracked(|e| raw_binding(ui, e, field)));
    Effect::new(move |_| {
        ui.editor.with(|editor| {
            let current = raw_binding(ui, editor, field);
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
            <input id=id aria-label=accessible_label disabled=move ||disabled.is_some_and(|d|d.get()) ||binding.get().is_none() type="text" maxlength="1024" autocomplete="off" spellcheck="false"
                prop:value=move || text.get()
                aria-invalid=move || ui.editor.with(|e| e.raw_fields().iter().any(|r| r.field == field)).to_string()
                on:focus=move |_| {focused.set(true); focus_key.set_value(ui.editor.with_untracked(FieldCommit::begin));}
                on:blur=move |_| { focused.set(false); text.set(ui.editor.with_untracked(|e| raw_value(e, field))); finish_field(ui,focus_key,label); }
                on:input=move |event| {
                    let Some(Some(token)) = binding.try_get_untracked() else { return; };
                    let value = event_target_value(&event);
                    text.set(value.clone());
                    let before=ui.editor.with_untracked(Editor::accepted_config_key);
                    let result = dispatch_raw(ui,token,field,value);
                    focus_key.update_value(|commit|ui.editor.with_untracked(|editor|commit.observe(before,editor)));
                    if result { binding.set(ui.editor.with_untracked(|e|raw_binding(ui,e,field))); }
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

/// Returns a select control in `ui` with `label`, reactive `value`, and value/label
/// `options`. `on_change` receives current field authority and selected text; the DOM
/// is then restored from model state. `layout` selects layout locks, while
/// `repair_invalid_draft` permits repair controls through the ordinary invalid guard.
/// `dynamic_id` precedes nonempty `id` and inferred label binding; `accessible_label`,
/// `disabled` and `disabled_values` add presentation/accessibility constraints.
#[component]
pub fn Choice(
    /// Shared editor and inspection state.
    ui: Ui,
    /// Visible text and inferred registry binding name.
    label: &'static str,
    /// Current model selection, restored after every callback.
    value: Signal<String>,
    /// Ordered (serialized value, visible label) choices.
    options: Vec<(String, String)>,
    /// Reducer callback responsible for validating captured authority and value.
    on_change: Callback<(FieldToken, String)>,
    /// Use layout readiness instead of the general invalid-state lock.
    #[prop(default = false)]
    layout: bool,
    /// Allow invalid-draft repair; read-only inspection and explicit locks still apply.
    #[prop(default = false)]
    repair_invalid_draft: bool,
    /// Static DOM ID; empty falls back to label binding.
    #[prop(default = String::new())]
    id: String,
    /// Reactive DOM ID taking precedence over the static ID.
    #[prop(optional)]
    dynamic_id: Option<Signal<String>>,
    /// Optional screen-reader label.
    #[prop(optional)]
    accessible_label: Option<String>,
    /// Optional additional lock for the whole select.
    #[prop(optional)]
    disabled: Option<Signal<bool>>,
    /// Values whose individual options cannot be selected in the browser.
    #[prop(optional)]
    disabled_values: Option<Signal<Vec<String>>>,
) -> impl IntoView {
    let id = move || {
        dynamic_id.map(|id| id.get()).unwrap_or_else(|| {
            if id.is_empty() {
                ui.control_id(label)
            } else {
                id.clone()
            }
        })
    };
    let binding = RwSignal::new(ui.editor.with_untracked(Editor::field_token));
    Effect::new(move |_| binding.set(ui.editor.with(Editor::field_token)));
    view! {
        <label class="field"><span>{label}</span>
            <select id=id title=move ||ui.control_reason(label) aria-label=accessible_label prop:value=move || value.get() disabled=move || ui.inspect_read_only() || disabled.is_some_and(|d| d.get()) || if repair_invalid_draft { false } else if layout { ui.layout_locked() } else { ui.invalid() }
                on:change=move |event| {
                    let before=ui.editor.with_untracked(Editor::accepted_config_key);
                    if let Some(token) = binding.try_get_untracked() { on_change.run((token,event_target_value(&event))); }
                    ui.accepted_field(before,label);
                    event_target::<web_sys::HtmlSelectElement>(&event).set_value(&value.get_untracked());
                }>
                {options.into_iter().map(|(value,label)| {let check=value.clone();view! { <option value=value disabled=move ||disabled_values.is_some_and(|d|d.get().contains(&check))>{label}</option> }}).collect_view()}
            </select>
            <small class="hint">{move ||ui.control_reason(label)}</small>
        </label>
    }
}

/// Returns a checkbox in `ui` with `label` and reactive `value`; `on_change` receives
/// captured field authority and the new boolean. The DOM is restored from model state.
/// `repair_invalid_draft` bypasses the ordinary invalid guard; `disabled` and read-only
/// inspection still lock it. Nonempty `id` overrides the inferred registry binding.
#[component]
pub fn Toggle(
    /// Shared editor and inspection state.
    ui: Ui,
    /// Visible text and inferred registry binding name.
    label: &'static str,
    /// Model boolean restored after callback completion.
    value: Signal<bool>,
    /// Reducer callback receiving current authority and requested value.
    on_change: Callback<(FieldToken, bool)>,
    /// Permit retained repairs while the draft is invalid.
    #[prop(default = false)]
    repair_invalid_draft: bool,
    /// Explicit DOM ID, or empty for registry lookup.
    #[prop(default = String::new())]
    id: String,
    /// Additional reactive lock independent of editor readiness.
    #[prop(optional)]
    disabled: Option<Signal<bool>>,
) -> impl IntoView {
    let id = move || {
        if id.is_empty() {
            ui.control_id(label)
        } else {
            id.clone()
        }
    };
    let binding = RwSignal::new(ui.editor.with_untracked(Editor::field_token));
    Effect::new(move |_| binding.set(ui.editor.with(Editor::field_token)));
    view! {
        <label class="toggle"><span>{label}</span><input id=id type="checkbox"
            prop:checked=move || value.get() disabled=move || ui.inspect_read_only() || disabled.is_some_and(|d|d.get()) || (!repair_invalid_draft && ui.invalid())
            on:change=move |event| {
                let before=ui.editor.with_untracked(Editor::accepted_config_key);
                if let Some(token) = binding.try_get_untracked() { on_change.run((token,event_target_checked(&event))); }
                ui.accepted_field(before,label);
                event_target::<web_sys::HtmlInputElement>(&event).set_checked(value.get_untracked());
            } /><i aria-hidden="true"></i></label>
    }
}

/// Returns owned value/label pairs copied from `items`, preserving order and duplicates.
pub fn choices(items: &[(&str, &str)]) -> Vec<(String, String)> {
    items
        .iter()
        .map(|(value, label)| (value.to_string(), label.to_string()))
        .collect()
}

/// Returns concise repair guidance for `error`, retaining debug classification for
/// unsupported cases. This formats diagnostics without changing accepted or draft state.
pub fn error_text(error: EditorError) -> String {
    match error {
        EditorError::Validation(e) => {
            let reason = match e.code {
                _ if e.field == Field::BlePeer => {
                    "Enter the explicit adapter address. Zero is allowed only with BLE disabled; broadcast and private \
                     rotating addresses are unsupported. Random static addresses require top bits 11 and a nontrivial \
                     random part."
                }
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
        EditorError::StartupPending => {
            "Apply or discard startup timing before changing or exporting the project.".into()
        }
        EditorError::JsonPending => {
            "Apply or discard the pending JSON edits before changing or exporting the configuration.".into()
        }
        EditorError::PointerActive => "Finish or cancel the current placement before exporting.".into(),
        EditorError::ImportPending => "Wait for or cancel the current import before exporting.".into(),
        EditorError::RepairFieldFirst(field) => {
            let label = match field {
                RawField::Widget(_, WidgetField::GraphMinimum) => "Graph minimum",
                RawField::Widget(_, WidgetField::GraphMaximum) => "Graph maximum",
                RawField::Fuel(FuelField::Ratio) => "Stoichiometric ratio",
                RawField::Fuel(FuelField::Percent) => "Ethanol content",
                RawField::Fuel(FuelField::GasolineRatio) => "Gasoline ratio",
                RawField::Fuel(FuelField::EthanolRatio) => "Ethanol ratio",
                RawField::Fuel(FuelField::GasolineDensity) => "Gasoline density",
                RawField::Fuel(FuelField::EthanolDensity) => "Ethanol density",
                RawField::Fuel(FuelField::ReferenceCelsius) => "Reference temperature",
                RawField::Source(_, SourceField::Request) => "Request ID",
                RawField::Source(_, SourceField::Response) => "Response ID",
                RawField::Source(_, SourceField::PollMilliseconds) => "Poll period",
                _ => "the highlighted field",
            };
            format!(
                "Resolve the {label} edit before switching this setting. Repair any other highlighted fields too. Its \
                 current option and raw input remain available."
            )
        }
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
        EditorError::LayoutCannotFit(view) => {
            format!(
                "View {view} cannot fit this shape with its current spans. Keep the current orientation or repair the \
                 layout."
            )
        }
        EditorError::Project(dashboard_config_tools::project::ProjectError::Orientation) => {
            "Splash artwork has a different shape. Reconvert the retained original for the effective Splash \
             orientation, or reimport the source."
                .into()
        }
        EditorError::Project(_) => {
            "Project or startup data is invalid, incomplete, oversized or failed its integrity checks. Current \
             accepted state was kept."
                .into()
        }
        EditorError::Codec(_) => "The file is invalid, incompatible, oversized or has a failed checksum.".into(),
        _ => format!("Cannot apply this action: {error:?}."),
    }
}

/// Both picker and text entry use the same transactional raw-field path.
/// Returns paired controls for `field` in `ui` with `label`; invalid colour text is
/// retained while the picker displays black as a fallback. Accepted changes emit grouped feedback.
#[component]
pub fn ColorField(
    /// Shared editor and inspection state.
    ui: Ui,
    /// RGB raw field used by both text entry and picker.
    field: RawField,
    /// Visible text and picker accessibility label.
    label: &'static str,
) -> impl IntoView {
    let focus_key = StoredValue::new(ui.editor.with_untracked(FieldCommit::begin));
    let binding = RwSignal::new(ui.editor.with_untracked(|e| raw_binding(ui, e, field)));
    Effect::new(move |_| binding.set(ui.editor.with(|e| raw_binding(ui, e, field))));
    view! {
        <div class="colour-control">
            <TextField ui field label/>
            <input type="color" disabled=move ||binding.get().is_none() aria-label=format!("Pick {label}")
                prop:value=move || ui.editor.with(|e|Color::parse(&raw_value(e,field)).unwrap_or(Color(0)).hex().to_string())
                on:focus=move |_|focus_key.set_value(ui.editor.with_untracked(FieldCommit::begin))
                on:change=move |_|finish_field(ui,focus_key,label)
                on:input=move |event| {
                    if let Some(Some(token)) = binding.try_get_untracked() {
                        let value = event_target_value(&event);
                        let before=ui.editor.with_untracked(Editor::accepted_config_key);
                        dispatch_raw(ui,token,field,value);
                        focus_key.update_value(|commit|ui.editor.with_untracked(|editor|commit.observe(before,editor)));
                    }
                }/>
        </div>
    }
}
