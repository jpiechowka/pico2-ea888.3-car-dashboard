//! Procedural-profile controls and inspection through Studio's existing canvas.
use dashboard_core::startup::{StartupProfile, StartupStatus, StartupTheme};

use super::*;

#[derive(Clone, Copy)]
pub struct State {
    pub profile: Signal<StartupProfile>,
    pub reduced: RwSignal<bool>,
    pub message: RwSignal<String>,
}
impl State {
    pub fn new(editor: RwSignal<Editor>) -> Self {
        Self {
            profile: Signal::derive(move || editor.with(Editor::startup_profile)),
            reduced: RwSignal::new(false),
            message: RwSignal::new("Startup belongs to the complete Studio project and shares Undo / Redo.".into()),
        }
    }

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
fn show(ui: Ui) {
    stage::cancel(ui);
    simulation::cancel(ui);
    ui.playing.set(false);
    ui.sample_ms.set(0);
    ui.dock.mode.set(PreviewMode::Startup);
}
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
fn accept_artwork(
    ui: Ui,
    input: startup_artwork::ArtworkImport,
) {
    use dashboard_config_tools::project as p;
    use dashboard_startup_tools as c;
    let o = input.options;
    let recipe = p::Recipe {
        version: 1,
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
#[component]
fn TimingInput(
    ui: Ui,
    field: StartupNumber,
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
#[component]
pub fn Panel(ui: Ui) -> impl IntoView {
    let state = ui.startup;
    let retained = Signal::derive(move || retained_source(ui));
    let on_accept = Callback::new(move |input| accept_artwork(ui, input));
    view! {
        <details class="workspace-group startup-panel" id="studio-startup" open><summary>"Startup"</summary><div class="startup-body">
            <p class="hint startup-intro">"A brief system check, your animation, then the dashboard. Acquisition keeps running throughout."</p>
            <section class="startup-card"><h3>"Playback and timing"</h3><div class="startup-grid"><label>"Animation"<select disabled=move ||ui.snapshot_blocked() aria-label="Startup animation" prop:value=move ||(state.profile.get().theme as u8).to_string() on:change=move |e|{if let Ok(n)=event_target_value(&e).parse::<usize>() {if n==5 {state.edit(ui, |p|p.theme=StartupTheme::Custom);} else if let Some(theme)=StartupTheme::ALL.get(n) {state.edit(ui, |p|p.theme = *theme);}}}>{StartupTheme::ALL.into_iter().enumerate().map(|(i,t)|view!{<option value=i.to_string()>{t.label()}</option>}).collect_view()}<Show when=move ||ui.editor.with(|e|e.startup_artwork().is_some())><option value="5">"Custom artwork"</option></Show></select></label>
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
            <startup_artwork::Panel ui=ui on_accept=on_accept retained=retained/>
            <p role="status" class="startup-status">{move ||state.message.get()}</p>
            <p class="hint">"Save a complete .eavp project to keep dashboard, startup and artwork together. Dashboard-only .eav and JSON omit startup. Choosing a procedural theme replaces custom artwork in the current state; Undo restores it."</p>
        </div></details>
    }
}
#[component]
pub fn Playback(ui: Ui) -> impl IntoView {
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
