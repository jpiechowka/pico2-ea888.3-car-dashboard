//! Procedural-profile controls and inspection through Studio's existing canvas.
use dashboard_core::startup::{StartupProfile, StartupStatus, StartupTheme};

use super::*;

const KEY: &str = "ea888-startup-profile-v1";
#[derive(Clone, Copy)]
pub struct State {
    pub profile: RwSignal<StartupProfile>,
    pub reduced: RwSignal<bool>,
    revision: RwSignal<u64>,
    pub message: RwSignal<String>,
}
impl State {
    pub fn new() -> Self {
        let restored = io::storage().ok().and_then(|s| s.get_item(KEY).ok().flatten());
        let profile = restored
            .as_deref()
            .filter(|text| text.len() <= 1024)
            .and_then(|text| serde_json::from_str::<StartupProfile>(text).ok())
            .filter(|p| p.valid());
        let message = if restored.is_some() && profile.is_none() {
            "Saved startup profile is invalid; defaults shown. Export a profile to keep a portable copy."
        } else {
            "Startup profile is saved separately in this browser."
        };
        Self {
            profile: RwSignal::new(profile.unwrap_or_default()),
            reduced: RwSignal::new(false),
            revision: RwSignal::new(0),
            message: RwSignal::new(message.into()),
        }
    }

    fn edit(
        self,
        f: impl FnOnce(&mut StartupProfile),
    ) {
        let mut next = self.profile.get_untracked();
        f(&mut next);
        if !next.valid() {
            self.message
                .set("Status must be 0–2000 ms; splash must be 0–10000 ms.".into());
            return;
        }
        self.profile.set(next);
        self.revision.update(|r| *r = r.wrapping_add(1));
        let saved = serde_json::to_string(&next)
            .ok()
            .is_some_and(|text| io::storage().ok().is_some_and(|s| s.set_item(KEY, &text).is_ok()));
        self.message.set(
            if saved {
                "Startup profile saved in this browser."
            } else {
                "Browser storage unavailable. Export startup.easp to preserve these settings."
            }
            .into(),
        );
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
            <label>"Animation"<select aria-label="Startup animation" prop:value=move ||(state.profile.get().theme as u8).to_string() on:change=move |e|{if let Ok(n)=event_target_value(&e).parse::<usize>() && let Some(theme)=StartupTheme::ALL.get(n) {state.edit(|p|p.theme = *theme);}}>{StartupTheme::ALL.into_iter().enumerate().map(|(i,t)|view!{<option value=i.to_string()>{t.label()}</option>}).collect_view()}</select></label>
            <label>"System check"<select aria-label="Startup system check" prop:value=move ||(state.profile.get().status as u8).to_string() on:change=move |e|{let value=match event_target_value(&e).as_str(){"0"=>StartupStatus::Off,"2"=>StartupStatus::Detailed,_=>StartupStatus::Brief};state.edit(|p|p.status=value);}>
                <option value="0">"Off"</option><option value="1">"Brief"</option><option value="2">"Detailed"</option>
            </select></label>
            <p class="hint">"Shows completed checks and the current OBD state. Configured sources do not prove ECU support or a connection."</p>
            <label>"Status duration (ms)"<input type="number" min="0" max="2000" step="50" aria-label="Startup status duration" prop:value=move ||state.profile.get().status_ms.to_string() on:change=move |e|{if let Ok(v)=event_target_value(&e).parse::<u16>(){state.edit(|p|p.status_ms=v);}else{state.message.set("Enter a status duration from 0 to 2000 ms.".into());}}/></label>
            <label>"Splash duration (ms)"<input type="number" min="0" max="10000" step="50" aria-label="Startup splash duration" prop:value=move ||state.profile.get().duration_ms.to_string() on:change=move |e|{if let Ok(v)=event_target_value(&e).parse::<u16>(){state.edit(|p|p.duration_ms=v);}else{state.message.set("Enter a splash duration from 0 to 10000 ms.".into());}}/></label>
            <p class="hint">"0 skips the animation; the system check has its own duration."</p>
            <label><input type="checkbox" prop:checked=move ||state.profile.get().loop_animation on:change=move |e|state.edit(|p|p.loop_animation=event_target_checked(&e))/>{" Loop animation"}</label>
            <p class="hint">"With looping off, the animation plays once and holds its last frame until the splash duration ends."</p>
            <label><input type="checkbox" prop:checked=move ||state.profile.get().interrupt_on_attention on:change=move |e|state.edit(|p|p.interrupt_on_attention=event_target_checked(&e))/>{" Show dashboard immediately for attention warnings"}</label>
            <p class="hint">"By default warnings wait for the dashboard. Faults and protected Y recovery always exit startup; a fresh tap or rotation skips it without also activating its usual action."</p>
            <label><input type="checkbox" prop:checked=move ||state.profile.get().warning_led on:change=move |e|state.edit(|p|p.warning_led=event_target_checked(&e))/>{" Warning LED during startup"}</label>
            <p class="hint">"The configured LED pattern can indicate warnings while splash text stays hidden. A hidden warning cannot be acknowledged."</p>
            <div class="toolbar"><button on:click=move |_|show(ui)>"Preview startup"</button><button on:click=move |_|{
                if let Some(bytes)=state.profile.get_untracked().encode() && io::download(ui,"startup.easp","application/octet-stream",&bytes).is_err(){state.message.set("Could not download the startup profile.".into());}
            }>"Export startup profile"</button></div>
            <label>"Import startup profile (.easp)"<input type="file" accept=".easp" on:change=move |event| {
                let input=event_target::<web_sys::HtmlInputElement>(&event);
                let file=input.files().and_then(|files|files.get(0)); input.set_value("");
                if let Some(file)=file {
                    if file.size()!=16.0 {state.message.set("Select a 16-byte .easp startup profile.".into());return;}
                    state.revision.update(|r|*r=r.wrapping_add(1)); let revision=state.revision.get_untracked();
                    wasm_bindgen_futures::spawn_local(async move {
                        let result=wasm_bindgen_futures::JsFuture::from(file.array_buffer()).await.ok().and_then(|buffer|StartupProfile::decode(&js_sys::Uint8Array::new(&buffer).to_vec()));
                        if state.revision.try_get_untracked()!=Some(revision){return;}
                        match result {Some(profile)=>state.edit(|p|*p=profile),None=>state.message.set("Invalid startup profile; current settings retained.".into())}
                    });
                }
            }/></label>
            <p role="status" class="hint">{move ||state.message.get()}</p>
            <p class="hint">"Legacy .eav files still contain dashboard settings only. Startup uses a separate .easp profile; complete projects, custom artwork and helper upload are the next delivery."</p>
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
