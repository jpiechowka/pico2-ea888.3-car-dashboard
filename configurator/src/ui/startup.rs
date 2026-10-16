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
#[component]
pub fn Panel(ui: Ui) -> impl IntoView {
    let state = ui.startup;
    view! {
        <details class="workspace-group" id="studio-startup" open><summary>"Startup"</summary>
            <p class="hint">"A brief system check, your animation, then the dashboard. Acquisition keeps running throughout."</p>
            <label>"Animation"<select disabled=move ||ui.snapshot_blocked() aria-label="Startup animation" prop:value=move ||(state.profile.get().theme as u8).to_string() on:change=move |e|{if let Ok(n)=event_target_value(&e).parse::<usize>() {if n==5 {state.edit(ui, |p|p.theme=StartupTheme::Custom);} else if let Some(theme)=StartupTheme::ALL.get(n) {state.edit(ui, |p|p.theme = *theme);}}}>{StartupTheme::ALL.into_iter().enumerate().map(|(i,t)|view!{<option value=i.to_string()>{t.label()}</option>}).collect_view()}<Show when=move ||ui.editor.with(|e|e.startup_artwork().is_some())><option value="5">"Custom artwork"</option></Show></select></label>
            <label>"System check"<select aria-label="Startup system check" prop:value=move ||(state.profile.get().status as u8).to_string() on:change=move |e|{let value=match event_target_value(&e).as_str(){"0"=>StartupStatus::Off,"2"=>StartupStatus::Detailed,_=>StartupStatus::Brief};state.edit(ui, |p|p.status=value);}>
                <option value="0">"Off"</option><option value="1">"Brief"</option><option value="2">"Detailed"</option>
            </select></label>
            <p class="hint">"Shows completed checks and the current OBD state. Configured sources do not prove ECU support or a connection."</p>
            <label>"Status duration (ms)"<input type="number" min="0" max="2000" step="50" aria-label="Startup status duration" prop:value=move ||state.profile.get().status_ms.to_string() on:change=move |e|{if let Ok(v)=event_target_value(&e).parse::<u16>(){state.edit(ui, |p|p.status_ms=v);}else{state.message.set("Enter a status duration from 0 to 2000 ms; the accepted duration was kept.".into());}event_target::<web_sys::HtmlInputElement>(&e).set_value(&state.profile.get_untracked().status_ms.to_string());}/></label>
            <label>"Splash duration (ms)"<input type="number" min="0" max="10000" step="50" aria-label="Startup splash duration" prop:value=move ||state.profile.get().duration_ms.to_string() on:change=move |e|{if let Ok(v)=event_target_value(&e).parse::<u16>(){state.edit(ui, |p|p.duration_ms=v);}else{state.message.set("Enter a splash duration from 0 to 10000 ms; the accepted duration was kept.".into());}event_target::<web_sys::HtmlInputElement>(&e).set_value(&state.profile.get_untracked().duration_ms.to_string());}/></label>
            <p class="hint">"0 skips the animation; the system check has its own duration."</p>
            <label><input type="checkbox" prop:checked=move ||state.profile.get().loop_animation on:change=move |e|state.edit(ui, |p|p.loop_animation=event_target_checked(&e))/>{" Loop animation"}</label>
            <p class="hint">"With looping off, the animation plays once and holds its last frame until the splash duration ends."</p>
            <label><input type="checkbox" prop:checked=move ||state.profile.get().interrupt_on_attention on:change=move |e|state.edit(ui, |p|p.interrupt_on_attention=event_target_checked(&e))/>{" Show dashboard immediately for attention warnings"}</label>
            <p class="hint">"By default warnings wait for the dashboard. Faults and protected Y recovery always exit startup; a fresh tap or rotation skips it without also activating its usual action."</p>
            <label><input type="checkbox" prop:checked=move ||state.profile.get().warning_led on:change=move |e|state.edit(ui, |p|p.warning_led=event_target_checked(&e))/>{" Warning LED during startup"}</label>
            <p class="hint">"The configured LED pattern can indicate warnings while splash text stays hidden. A hidden warning cannot be acknowledged."</p>
            <div class="toolbar"><button on:click=move |_|show(ui)>"Preview startup"</button><button disabled=move ||ui.snapshot_blocked() on:click=move |_|{
                if let Some(bytes)=state.profile.get_untracked().encode() && io::download(ui,"startup.easp","application/octet-stream",&bytes).is_err(){state.message.set("Could not download the startup profile.".into());}
            }>"Export startup profile"</button></div>
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
            <p role="status" class="hint">{move ||state.message.get()}</p>
            <p class="hint">"Save a complete .eavp project to keep dashboard, startup and artwork together. Dashboard-only .eav and JSON omit startup. Choosing a procedural theme replaces custom artwork in the current state; Undo restores it."</p>
        </details>
    }
}
#[component]
pub fn Playback(ui: Ui) -> impl IntoView {
    view! {<div hidden=move ||ui.dock.mode.get()!=PreviewMode::Startup>
        <div class="preview-toolbar"><span>"STARTUP PREVIEW"</span><div>
            <button on:click=move |_|{if !ui.playing.get_untracked() && ui.sample_ms.get_untracked()>=ui.startup.profile.get_untracked().total_duration(){ui.sample_ms.set(0);}ui.playing.update(|p|*p = !*p);}>{move ||if ui.playing.get(){"Pause"}else{"Play"}}</button>
            <button on:click=move |_|{ui.playing.set(false);ui.sample_ms.set(0);}>"Restart"</button>
        </div></div>
        <label>"Seek startup"<input type="range" min="0" max=move ||ui.startup.profile.get().total_duration().to_string() step="1" prop:value=move ||ui.sample_ms.get().min(ui.startup.profile.get().total_duration()).to_string() on:input=move |e|{ui.playing.set(false);if let Ok(t)=event_target_value(&e).parse(){ui.sample_ms.set(t);}}/></label>
        <label><input type="checkbox" prop:checked=move ||ui.startup.reduced.get() on:change=move |e|ui.startup.reduced.set(event_target_checked(&e))/>{" Reduced-motion preview"}</label>
        <p class="hint">"Shared Rust pixels with synthetic OBD status. Preview timing does not qualify physical display, controls or LED behavior."</p>
    </div>}
}
