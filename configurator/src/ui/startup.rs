//! Procedural-profile controls and inspection through Studio's existing canvas.
use dashboard_core::startup::{StartupProfile, StartupStatus, StartupTheme};

use super::*;

/// Scope both panel and worker feedback without a reactive reset racing publication.
#[derive(Clone, Copy)]
pub(super) struct ProjectMessage {
    /// Current editor used to bind and filter feedback by accepted project identity.
    editor: RwSignal<Editor>,
    /// Last project-bound message, hidden when its captured project is no longer current.
    value: RwSignal<dashboard_configurator::artwork_draft::ProjectFeedback>,
}
impl ProjectMessage {
    /// Returns feedback ownership for `editor`, binding initial `text` to its current project.
    pub(super) fn new(
        editor: RwSignal<Editor>,
        text: String,
    ) -> Self {
        Self {
            editor,
            value: RwSignal::new(
                editor.with_untracked(|e| dashboard_configurator::artwork_draft::ProjectFeedback::new(e, text)),
            ),
        }
    }

    /// Binds `text` to the current project; disposed editor/message signals are ignored.
    pub(super) fn set(
        self,
        text: String,
    ) {
        self.try_set(text);
    }

    /// Binds `text` only if editor and message owners still exist; disposal is a no-op.
    pub(super) fn try_set(
        self,
        text: String,
    ) {
        if let Some(value) = self
            .editor
            .try_with_untracked(|e| dashboard_configurator::artwork_draft::ProjectFeedback::new(e, text))
        {
            let _ = self.value.try_set(value);
        }
    }

    /// Returns the message only for its current project identity, otherwise an empty string.
    pub(super) fn get(self) -> String { self.editor.with(|e| self.value.with(|value| value.text(e).to_owned())) }
}

#[derive(Clone, Copy)]
/// Startup UI projection and preview-only controls; accepted settings live in the editor.
pub struct State {
    /// Reactive accepted startup profile shared with complete-project history.
    pub profile: Signal<StartupProfile>,
    /// Preview-only reduced-motion toggle, initially false; does not rewrite the profile.
    pub reduced: RwSignal<bool>,
    /// Project-scoped panel and worker feedback.
    pub message: ProjectMessage,
}
impl State {
    /// Returns startup projections for `editor`, preview defaults and project-bound guidance.
    pub fn new(editor: RwSignal<Editor>) -> Self {
        Self {
            profile: Signal::derive(move || editor.with(Editor::startup_profile)),
            reduced: RwSignal::new(false),
            message: ProjectMessage::new(
                editor,
                "Startup belongs to the complete Studio project and shares Undo / Redo.".into(),
            ),
        }
    }

    /// Applies `f` to the current profile and submits it through `ui` as a project edit.
    /// Reports accepted history semantics or the reducer error without silently changing artwork.
    fn edit(
        self,
        ui: Ui,
        f: impl FnOnce(&mut StartupProfile),
    ) {
        let mut next = self.profile.get_untracked();
        f(&mut next);
        if ui.run(|e| e.set_startup_profile(next)) {
            self.message
                .set("Startup accepted. Undo restores the previous project state.".into());
        } else {
            self.message.set(ui.notice.get_untracked());
        }
    }
}
/// Cancels `ui`'s placement/simulation callbacks and enters paused Startup inspection
/// at zero milliseconds; this does not replay the full acquisition/control lifecycle.
fn show(ui: Ui) {
    stage::cancel(ui);
    simulation::cancel(ui);
    ui.playing.set(false);
    ui.sample_ms.set(0);
    ui.dock.mode.set(PreviewMode::Startup);
}
/// Returns `ui`'s retained original artwork and exact conversion recipe projected to
/// worker options, or None without artwork/original bytes or reference. Access is read-only.
fn retained_source(ui: Ui) -> Option<startup_artwork::RetainedSource> {
    use dashboard_config_tools::project as p;
    use dashboard_startup_tools as c;
    ui.editor.with(|e| {
        let art = e.startup_artwork()?;
        let r = art.recipe();
        Some(startup_artwork::RetainedSource {
            source: {
                if !art.has_original() {
                    return None;
                }
                art.clone()
            },
            label: r.label.clone(),
            options: c::Options {
                geometry: r.geometry,
                kind: match r.source_kind {
                    p::SourceKind::Png => c::SourceKind::Png,
                    p::SourceKind::Gif => c::SourceKind::Gif,
                    p::SourceKind::SpriteSheet => c::SourceKind::SpriteSheet,
                },
                fit: match r.fit {
                    p::Fit::Contain => c::Fit::Contain,
                    p::Fit::Cover => c::Fit::Cover,
                    p::Fit::Stretch => c::Fit::Stretch,
                },
                matte_rgb: r.matte_rgb,
                sample_fps: r.sample_fps,
                trim_start_ms: r.trim_start_ms,
                trim_end_ms: r.trim_end_ms,
                sheet: r.sheet.as_ref().map(|s| {
                    c::Sheet {
                        columns: s.columns,
                        rows: s.rows,
                        frame_ms: s.frame_ms,
                    }
                }),
                gif_background: match r.gif_background {
                    p::GifBackground::Logical => c::GifBackground::Logical,
                    p::GifBackground::Transparent => c::GifBackground::Transparent,
                },
            },
        })
    })
}
/// Converts worker `input` into a hashed version-two project recipe for `ui`, validates
/// artwork, then externalizes its original under the retained materialization lease.
/// Rechecks import authority before/after storage and calls finished with publication
/// success; stale, invalid or failed backing admission cannot replace accepted artwork.
fn accept_artwork(
    ui: Ui,
    input: startup_artwork::ArtworkImport,
) {
    use dashboard_config_tools::project as p;
    use dashboard_startup_tools as c;
    let o = input.options;
    let recipe = p::Recipe {
        geometry: o.geometry,
        version: 2,
        converter: p::CONVERTER.into(),
        label: input.label,
        source_kind: match o.kind {
            c::SourceKind::Png => p::SourceKind::Png,
            c::SourceKind::Gif => p::SourceKind::Gif,
            c::SourceKind::SpriteSheet => p::SourceKind::SpriteSheet,
        },
        source_sha256: p::digest(&input.source),
        source_length: input.source.len() as u32,
        original_included: true,
        asset_sha256: p::digest(&input.converted.asset),
        fit: match o.fit {
            c::Fit::Contain => p::Fit::Contain,
            c::Fit::Cover => p::Fit::Cover,
            c::Fit::Stretch => p::Fit::Stretch,
        },
        matte_rgb: o.matte_rgb,
        sample_fps: o.sample_fps,
        trim_start_ms: o.trim_start_ms,
        trim_end_ms: input.converted.trim_end_ms,
        sheet: o.sheet.map(|s| {
            p::Sheet {
                columns: s.columns,
                rows: s.rows,
                frame_ms: s.frame_ms,
            }
        }),
        gif_background: match o.gif_background {
            c::GifBackground::Logical => p::GifBackground::Logical,
            c::GifBackground::Transparent => p::GifBackground::Transparent,
        },
    };
    match p::Artwork::new(input.converted.asset.into(), recipe, Some(input.source)) {
        Ok(art) => {
            wasm_bindgen_futures::spawn_local(async move {
                let _lease = input.lease;
                if ui.editor.try_with_untracked(|e| e.import_is_current(input.token)) != Some(true) {
                    input.finished.run(false);
                    return;
                }
                let prepared = originals::externalize(std::sync::Arc::new(art)).await;
                if ui.editor.try_with_untracked(|e| e.import_is_current(input.token)) != Some(true) {
                    input.finished.run(false);
                    return;
                }
                match prepared {
                    Ok(art) => {
                        if ui.run(|e| e.complete_artwork_import(input.token, art)) {
                            ui.startup.message.set(
                                "Custom artwork accepted with original source. Undo restores the previous project."
                                    .into(),
                            );
                            show(ui);
                            input.finished.run(true);
                        } else {
                            input.finished.run(false);
                        }
                    }
                    Err(_) => {
                        ui.editor.update(|e| {
                            let _ = e.cancel_import(input.token);
                        });
                        ui.startup.message.set(
                            "Original artwork could not be stored. Current project retained; check browser storage \
                             permissions and space."
                                .into(),
                        );
                        input.finished.run(false);
                    }
                }
            });
        }
        Err(_) => {
            ui.editor.update(|e| {
                let _ = e.cancel_import(input.token);
            });
            input.finished.run(false);
            ui.startup
                .message
                .set("Converted artwork did not pass project validation. Current project retained.".into())
        }
    }
}
/// Returns a draft timing input for `field` in `ui` with `label`; values remain text
/// until explicit Apply. Bounds UTF-16/UTF-8 copying to 32 and feeds an invalid marker
/// on overflow so the rejected draft stays visible and blocks snapshots.
#[component]
fn TimingInput(
    /// Shared startup draft, artwork, history and synthetic playback state.
    ui: Ui,
    /// Startup timing field and its shared numeric admission limits.
    field: StartupNumber,
    /// Visible and accessible name for the raw timing input.
    label: &'static str,
) -> impl IntoView {
    view! {<label>{label}<input type="text" inputmode="numeric" maxlength="32" aria-label=label
    aria-invalid=move ||ui.editor.with(|e|e.has_startup_draft() && !e.startup_draft_valid()).to_string()
    prop:value=move ||ui.editor.with(|e|e.startup_timing_text(field))
    on:input=move |event| {
        let input=event_target::<web_sys::HtmlInputElement>(&event);
        let text=io::bounded_input(&input,32).unwrap_or_else(||"[input exceeds 32-character limit]".into());
        ui.run(|e|e.set_startup_number(e.stamp(),field,&text));
    }/></label>}
}
/// Returns `ui`'s startup profile/artwork editor, transactional timing controls and
/// preview/replay actions. Imports require exact 16-byte EASP metadata and current import
/// authority; conversions retain originals before publication. Geometry mismatch stays
/// visible and blocks save/export/build until repaired, without dropping artwork.
#[component]
pub fn Panel(
    /// Shared startup draft, artwork, history and synthetic playback state.
    ui: Ui
) -> impl IntoView {
    let state = ui.startup;
    let retained = Signal::derive(move || retained_source(ui));
    let on_accept = Callback::new(move |input| accept_artwork(ui, input));
    view! {
        <details class="workspace-group startup-panel" id="studio-startup" open><summary>"Startup"</summary><div class="startup-body">
            <p class="hint startup-intro">"A brief system check, your animation, then the dashboard. Acquisition keeps running throughout."</p>
            <orientation::Picker ui scope=orientation::Scope::Status/><orientation::Picker ui scope=orientation::Scope::Splash/>
            <section class="startup-card"><h3>"Playback and timing"</h3><div class="startup-grid"><label>"Animation"<select disabled=move ||ui.snapshot_blocked() aria-label="Startup animation" prop:value=move ||state.profile.get().theme.wire_tag().to_string() on:change=move |e|{if let Ok(n)=event_target_value(&e).parse::<u8>() && let Some(theme)=StartupTheme::from_wire_tag(n) {state.edit(ui, |p|p.theme=theme);}}>{StartupTheme::ALL.into_iter().map(|t|view!{<option value=t.wire_tag().to_string()>{t.label()}</option>}).collect_view()}<Show when=move ||ui.editor.with(|e|e.startup_artwork().is_some())><option value="5">"Custom artwork"</option></Show></select></label>
            <label>"System check"<select aria-label="Startup system check" prop:value=move ||(state.profile.get().status as u8).to_string() on:change=move |e|{let value=match event_target_value(&e).as_str(){"0"=>StartupStatus::Off,"2"=>StartupStatus::Detailed,_=>StartupStatus::Brief};state.edit(ui, |p|p.status=value);}>
                <option value="0">"Off"</option><option value="1">"Brief"</option><option value="2">"Detailed"</option>
            </select></label>
            </div><p class="hint">"Shows completed checks and the current OBD state. Configured sources do not prove ECU support or a connection."</p>
            <div class="startup-grid"><TimingInput ui=ui field=StartupNumber::StatusMilliseconds label="Status duration (ms)"/><TimingInput ui=ui field=StartupNumber::SplashMilliseconds label="Splash duration (ms)"/></div>
            <p class="hint">"Status: 0–2000 ms. Splash: 0–10000 ms; 0 skips the animation independently of the system check."</p>
            <Show when=move ||ui.editor.with(Editor::has_startup_draft)>
                <p role="status">{move ||ui.editor.with(Editor::startup_draft_label).unwrap_or_default()}</p>
                <div class="startup-actions"><button disabled=move ||!ui.editor.with(Editor::startup_draft_valid) on:click=move |_|{ui.run(|e|e.apply_startup_timing(e.stamp()));}>"Apply timing"</button><button on:click=move |_|{ui.run(|e|e.discard_startup_timing(e.stamp()));}>"Discard timing"</button></div>
                <p class="hint">"Preview keeps the last accepted settings. Apply or discard timing before saving, exporting, building or changing other project settings."</p>
            </Show>
            </section><section class="startup-card"><h3>"During startup"</h3><div class="startup-behavior"><div><label class="startup-toggle"><input type="checkbox" prop:checked=move ||state.profile.get().loop_animation on:change=move |e|state.edit(ui, |p|p.loop_animation=event_target_checked(&e))/>{" Loop animation"}</label>
            <p class="hint">"With looping off, the animation plays once and holds its last frame until the splash duration ends."</p>
            </div><div><label class="startup-toggle"><input type="checkbox" prop:checked=move ||state.profile.get().interrupt_on_attention on:change=move |e|state.edit(ui, |p|p.interrupt_on_attention=event_target_checked(&e))/>{" Show dashboard immediately for attention warnings"}</label>
            <p class="hint">"By default warnings wait for the dashboard. Faults and protected Y recovery always exit startup; a fresh tap or rotation skips it without also activating its usual action."</p>
            </div><div><label class="startup-toggle"><input type="checkbox" prop:checked=move ||state.profile.get().warning_led on:change=move |e|state.edit(ui, |p|p.warning_led=event_target_checked(&e))/>{" Warning LED during startup"}</label>
            <p class="hint">"The configured LED pattern can indicate warnings while splash text stays hidden. A hidden warning cannot be acknowledged."</p>
            </div></div></section><div class="startup-actions"><button on:click=move |_|show(ui)>"Preview startup"</button><button disabled=move ||ui.snapshot_blocked() on:click=move |_|simulation::replay_startup(ui)>"Replay full startup"</button><button disabled=move ||ui.snapshot_blocked() on:click=move |_|{
                if let Some(bytes)=state.profile.get_untracked().encode() && io::download(ui,"startup.easp","application/octet-stream",&bytes).is_err(){state.message.set("Could not download the startup profile.".into());}
            }>"Export startup profile"</button></div>
            <p class="hint">"Preview startup seeks through artwork. Replay full startup runs the shared startup controller with simulated acquisition, controls, warning LED and frame completions."</p>
            <label>"Import startup profile (.easp)"<input type="file" disabled=move ||ui.snapshot_blocked() accept=".easp" on:change=move |event| {
                let input=event_target::<web_sys::HtmlInputElement>(&event);
                let file=input.files().and_then(|files|files.get(0)); input.set_value("");
                if let Some(file)=file {
                    if file.size()!=16.0 {state.message.set("Select a 16-byte .easp startup profile.".into());return;}
                    let mut request=Err(EditorError::InvalidDraft);ui.editor.update(|e|request=e.begin_import());let Ok(token)=request else{state.message.set("Finish current edits before importing startup settings.".into());return;};
                    wasm_bindgen_futures::spawn_local(async move {
                        let result=wasm_bindgen_futures::JsFuture::from(file.array_buffer()).await.ok().and_then(|buffer|StartupProfile::decode(&js_sys::Uint8Array::new(&buffer).to_vec()));
                        if ui.editor.try_with_untracked(|e|e.import_is_current(token))!=Some(true){return;}
                        ui.editor.update(|e|{let _=e.cancel_import(token);});
                        match result {Some(profile)=>state.edit(ui, |p|*p=profile),None=>state.message.set("Invalid startup profile; current settings retained.".into())}
                    });
                }
            }/></label>
            <Show when=move ||ui.editor.with(Editor::artwork_geometry_mismatch)><div class="draft-alert" role="alert"><strong>"Splash artwork needs conversion"</strong><p>"The retained pixels have a different shape from the effective Splash orientation. Save, export and build are blocked. Convert the retained original below, or reimport it if this project contains converted artwork only. Undo restores the complete previous project."</p></div></Show>
            <startup_artwork::Panel ui=ui on_accept=on_accept retained=retained/>
            <p role="status" class="startup-status">{move ||state.message.get()}</p>
            <p class="hint">"Save a complete .eavp project to keep dashboard, startup and artwork together. Dashboard-only .eav and JSON omit startup. Choosing a procedural theme replaces custom artwork in the current state; Undo restores it."</p>
        </div></details>
    }
}
/// Returns `ui`'s Startup inspection play/pause/seek controls and preview-only reduced
/// motion. Seek timestamps are milliseconds; preview pixels/status do not establish
/// physical display completion, LED timing or ECU qualification.
#[component]
pub fn Playback(
    /// Shared startup draft, artwork, history and synthetic playback state.
    ui: Ui
) -> impl IntoView {
    view! {<div class="startup-playback" hidden=move ||ui.dock.mode.get()!=PreviewMode::Startup>
        <div class="preview-toolbar"><span>"STARTUP PREVIEW"</span><div>
            <button on:click=move |_|{if !ui.playing.get_untracked() && ui.sample_ms.get_untracked()>=ui.startup.profile.get_untracked().total_duration(){ui.sample_ms.set(0);}ui.playing.update(|p|*p = !*p);}>{move ||if ui.playing.get(){"Pause"}else{"Play"}}</button>
            <button on:click=move |_|{ui.playing.set(false);ui.sample_ms.set(0);}>"Restart"</button>
        </div></div>
        <label>"Seek startup"<input type="range" min="0" max=move ||ui.startup.profile.get().total_duration().to_string() step="1" prop:value=move ||ui.sample_ms.get().min(ui.startup.profile.get().total_duration()).to_string() on:input=move |e|{ui.playing.set(false);if let Ok(t)=event_target_value(&e).parse(){ui.sample_ms.set(t);}}/></label>
        <label><input type="checkbox" prop:checked=move ||ui.startup.reduced.get() on:change=move |e|ui.startup.reduced.set(event_target_checked(&e))/>{" Reduced-motion preview"}</label>
        <p class="hint">"Shared Rust pixels with synthetic OBD status. Preview timing does not qualify physical display, controls or LED behavior."</p>
    </div>}
}
