//! One mounted textarea for the Editor-owned draft; browser undo remains native.
use dashboard_configurator::json_diff::{DiffKind, DiffReport, diff_configs};
use dashboard_configurator::json_draft::{
    JsonDraftHandle,
    JsonParseToken,
    JsonPhase,
    JsonPreviewProof,
    JsonProjection,
    JsonState,
};
use leptos::leptos_dom::helpers::set_timeout_with_handle;

use super::*;

#[derive(Clone, Copy)]
pub(super) struct State {
    area: NodeRef<leptos::html::Textarea>,
    timer: StoredValue<
        Option<(
            JsonProjection,
            JsonParseToken,
            Option<leptos::leptos_dom::helpers::TimeoutHandle>,
        )>,
    >,
    binding: StoredValue<Option<JsonDraftHandle>>,
    caret: StoredValue<Caret>,
    error: RwSignal<String>,
    proof: StoredValue<Option<JsonPreviewProof>>,
    pub(super) pixels: RwSignal<Option<JsonProjection>>,
}
#[derive(Clone, Copy, Default)]
struct Caret {
    handle: Option<JsonDraftHandle>,
    start: u32,
    end: u32,
    direction: &'static str,
    scroll: i32,
}
impl State {
    pub fn new() -> Self {
        Self {
            area: NodeRef::new(),
            timer: StoredValue::new(None),
            binding: StoredValue::new(None),
            caret: StoredValue::new(Caret::default()),
            error: RwSignal::new(String::new()),
            proof: StoredValue::new(None),
            pixels: RwSignal::new(None),
        }
    }

    /// Cache only an immutable shared validation proof of this exact owned
    /// candidate. It is a draw cache, never an editable/accepted config owner.
    pub(super) fn with_candidate<T>(
        self,
        editor: &Editor,
        render: impl FnOnce(&JsonPreviewProof) -> T,
    ) -> Result<T, EditorError> {
        let draft = editor.json_draft().ok_or(EditorError::StaleToken)?;
        let projection = draft.projection();
        if !self
            .proof
            .with_value(|proof| proof.as_ref().is_some_and(|p| p.projection() == projection))
        {
            self.proof.set_value(Some(draft.preview_proof()?));
        }
        Ok(self
            .proof
            .with_value(|proof| render(proof.as_ref().expect("current checked draw proof"))))
    }

    fn cancel(self) {
        let _ = self.timer.try_update_value(|timer| {
            if let Some((_, _, Some(handle))) = timer.take() {
                handle.clear();
            }
        });
    }

    fn remember(self) {
        let Some(area) = self.area.get_untracked() else {
            return;
        };
        let direction = match area.selection_direction().ok().flatten().as_deref() {
            Some("backward") => "backward",
            Some("forward") => "forward",
            _ => "none",
        };
        self.caret.set_value(Caret {
            handle: self.binding.get_value(),
            start: area.selection_start().ok().flatten().unwrap_or(0),
            end: area.selection_end().ok().flatten().unwrap_or(0),
            direction,
            scroll: area.scroll_top(),
        });
    }

    fn restore(
        self,
        ui: Ui,
    ) {
        let Some(area) = self.area.get_untracked() else {
            return;
        };
        let caret = self.caret.get_value();
        ui.editor.with_untracked(|e| {
            if let Some(draft) = e.json_draft() {
                area.set_value(draft.text());
                if caret.handle == Some(draft.handle()) {
                    let _ = area.set_selection_range_with_direction(caret.start, caret.end, caret.direction);
                    area.set_scroll_top(caret.scroll);
                }
            }
        });
    }
}

pub(super) fn projection(ui: Ui) -> Option<JsonProjection> {
    ui.editor.with(|e| e.json_draft().map(|d| d.projection()))
}
pub(super) fn label(ui: Ui) -> &'static str { projection(ui).map_or("", |p| p.phase().label()) }

pub(super) fn open(ui: Ui) {
    let mut outcome = Err(EditorError::StaleToken);
    ui.editor.update(|editor| outcome = editor.open_json());
    match outcome {
        Ok(handle) => focus(ui, handle),
        Err(error) => ui.notice.set(error_text(error)),
    }
}
pub(super) fn focus(
    ui: Ui,
    handle: JsonDraftHandle,
) {
    workspaces::open(ui, Workspace::Diagnostics, GroupId::Files);
    command_palette::focus_json(ui, handle);
}

fn input(ui: Ui) {
    let state = ui.json;
    let Some(area) = state.area.get_untracked() else {
        return;
    };
    let Some(handle) = state.binding.get_value() else {
        return;
    };
    // A native code-unit precheck bounds the temporary UTF-8 bridge allocation.
    if area.text_length() as usize > MAX_JSON_BYTES {
        state.restore(ui);
        state
            .error
            .set("JSON is limited to 128 KiB of UTF-8. Previous text and selection retained.".into());
        return;
    }
    let text = area.value();
    let mut outcome = Err(EditorError::StaleToken);
    ui.editor.update(|editor| outcome = editor.set_json_text(handle, text));
    let token = match outcome {
        Ok(token) => token,
        Err(error) => {
            state.restore(ui);
            state.error.set(if error == EditorError::JsonTooLong {
                "JSON is limited to 128 KiB of UTF-8. Previous text and selection retained.".into()
            } else {
                error_text(error)
            });
            return;
        }
    };
    state.error.set(String::new());
    state.cancel();
    let identity = projection(ui).expect("admitted owned draft");
    match set_timeout_with_handle(
        move || {
            if state
                .timer
                .try_with_value(|timer| timer.as_ref().is_some_and(|(_, current, _)| *current == token))
                != Some(true)
            {
                return;
            }
            let mut result = Err(EditorError::StaleToken);
            if ui.editor.try_update(|e| result = e.finish_json_parse(token)).is_none() {
                return;
            }
            // Callback authority is the original parse token, even after cancellation.
            if result.is_ok() {
                let _ = state.timer.try_set_value(None);
            }
        },
        std::time::Duration::from_millis(250),
    ) {
        Ok(timer) => state.timer.set_value(Some((identity, token, Some(timer)))),
        Err(_) => {
            // Retain the original admitted token for explicit synchronous Apply.
            state.timer.set_value(Some((identity, token, None)));
            state.error.set(
                "Parsing could not be scheduled. Apply validates the current text; editing retries scheduling.".into(),
            )
        }
    }
}

fn apply(ui: Ui) {
    if let Some(token) = ui
        .json
        .timer
        .with_value(|timer| timer.as_ref().map(|(_, token, _)| *token))
    {
        let mut parsed = Err(EditorError::StaleToken);
        ui.editor.update(|editor| parsed = editor.finish_json_parse(token));
        if let Err(error) = parsed {
            ui.notice.set(error_text(error));
            return;
        }
    }
    // The reducer reparses and validates the exact current owned text.
    if ui.run(Editor::apply_json) {
        ui.json.cancel();
        ui.json.error.set(String::new());
        ui.inspection.set(None);
        ui.error_inspection.set(None);
        ui.event(Notice::info(
            Category::Editor,
            "json.applied",
            None,
            "JSON applied as one accepted undo entry",
        ));
    }
}
fn discard(ui: Ui) {
    ui.json.cancel();
    ui.editor.update(|editor| {
        let _ = editor.discard_json();
    });
    ui.json.error.set(String::new());
    ui.inspection.set(None);
    ui.error_inspection.set(None);
    ui.event(Notice::info(
        Category::Editor,
        "json.discarded",
        None,
        "JSON draft discarded; accepted configuration retained",
    ));
}

fn diff_view(report: DiffReport) -> impl IntoView {
    let omitted = report.omitted;
    let count = report.changes.len();
    view! {
        <p class="hint">{format!("{count} retained changed paths · {omitted} omitted. Each pointer is labelled accepted or candidate JSON. IDs are labels only.")}</p>
        <ol class="json-diff">{report.changes.into_iter().map(|entry| {
            let kind = match entry.kind { DiffKind::Added=>"Added", DiffKind::Removed=>"Removed", DiffKind::Changed=>"Changed", DiffKind::Reordered=>"Reordered", DiffKind::Ambiguous=>"Ambiguous (array index)" };
            let pointer_side = entry.pointer_side().label();
            let value = |summary: Option<dashboard_configurator::json_diff::ValueSummary>| summary.map_or("Absent".into(), |summary|format!("{}{}",summary.text,if summary.truncated{" … [truncated]"}else{""}));
            view! { <li><strong>{kind}</strong><small>{pointer_side}</small><code>{entry.pointer}</code><small>{entry.object_label}</small><div><span>"Before: "</span><code>{value(entry.before)}</code></div><div><span>"After: "</span><code>{value(entry.after)}</code></div></li> }
        }).collect_view()}</ol>
    }
}

#[component]
pub(super) fn JsonEditor(ui: Ui) -> impl IntoView {
    let state = ui.json;
    let key = Memo::new(move |_| projection(ui));
    let last_phase = StoredValue::new(None::<JsonPhase>);
    Effect::new(move |_| {
        let current = key.get();
        let Some(area) = state.area.get() else {
            return;
        };
        let phase = current.map(|p| p.phase());
        if state
            .proof
            .with_value(|proof| proof.as_ref().is_some_and(|p| Some(p.projection()) != current))
        {
            state.proof.set_value(None);
        }
        // Per-state transition summaries, never keystrokes or full document text.
        if phase != Some(JsonPhase::Pending) && last_phase.get_value() != phase {
            last_phase.set_value(phase);
            if let Some(phase) = phase.filter(|phase| !matches!(phase, JsonPhase::Clean | JsonPhase::Pending)) {
                ui.event(Notice::info(Category::Editor, "json.parse-state", None, phase.label()));
            }
        }
        if state
            .timer
            .with_value(|timer| timer.as_ref().is_some_and(|(identity, ..)| Some(*identity) != current))
        {
            state.cancel();
        }
        ui.editor.with_untracked(|e| {
            let draft = e.json_draft();
            if state.binding.get_value() != draft.map(|draft| draft.handle()) {
                state.error.set(String::new());
                state.caret.set_value(Caret::default());
            }
            state.binding.set_value(draft.map(|draft| draft.handle()));
            if let Some(draft) = draft {
                // Normal input already updated the native value: no rewrite/undo history.
                if area.text_length() as usize > MAX_JSON_BYTES || area.value() != draft.text() {
                    area.set_value(draft.text());
                }
            }
        });
    });
    on_cleanup(move || state.cancel());
    let diff = Memo::new(move |_| {
        key.get();
        ui.editor.with_untracked(|e| {
            match e.json_draft().map(|draft| draft.state()) {
                Some(JsonState::Candidate { config, .. }) => diff_configs(e.preview().config(), config).ok(),
                _ => None,
            }
        })
    });
    view! {
        <section class="json-editor" aria-label="Owned JSON editor">
            <button id="studio-open-json" on:click=move |_|open(ui)>"Open JSON"</button>
            <div hidden=move ||key.get().is_none()>
                <h3>"Configuration JSON"</h3>
                <p id="studio-json-state" role="status">{move ||label(ui)}</p>
                <label for="studio-json-text">"Current version JSON · 128 KiB UTF-8 maximum"</label>
                <textarea id="studio-json-text" node_ref=state.area rows="16" spellcheck="false" autocomplete="off" aria-describedby="studio-json-state studio-json-errors" readonly=move ||ui.editor.with(|e|e.json_draft().is_none_or(|draft|draft.is_read_only()))
                    on:beforeinput=move |_|state.remember() on:select=move |_|state.remember() on:input=move |_|input(ui)></textarea>
                <p id="studio-json-errors" class="control-error" role="status">{move ||ui.editor.with(|e|match e.json_draft().map(|d|d.state()){Some(JsonState::SyntaxError{message,..})=>message.clone(),Some(JsonState::Candidate{report,..}) if !report.issues.is_empty()=>format!("{} semantic issues shown · {} omitted. Repair or Discard before applying.",report.issues.len(),report.omitted),_=>String::new()})}</p>
                <p class="control-error" role="alert" hidden=move ||state.error.get().is_empty()>{move ||state.error.get()}</p>
                <div class="json-actions"><button class="primary" id="studio-json-apply" disabled=move ||key.get().is_none_or(|p|!matches!(p.phase(),JsonPhase::ValidUnapplied|JsonPhase::Pending)) on:click=move |_|apply(ui)>"Apply JSON"</button><button id="studio-json-discard" on:click=move |_|discard(ui)>"Discard JSON"</button></div>
                <p class="hint">"Typing keeps accepted configuration and undo history intact. Apply or Discard to unlock visual edits, saving and export. Ctrl / Cmd + Z edits this textarea. Use Import for legacy files."</p>
                <details><summary>"Structural diff against accepted configuration"</summary>{move ||diff.get().map(diff_view)}</details>
            </div>
        </section>
    }
}
