//! One replaceable, deadline-bounded Rust Worker. Acceptance belongs to Editor.
use std::sync::Arc;

use dashboard_startup_tools::{Converted, Fit, GifBackground, MAX_SOURCE_BYTES, Options, Sheet, SourceKind};
use leptos::leptos_dom::helpers::{TimeoutHandle, set_timeout_with_handle};
use wasm_bindgen::closure::Closure;

use super::*;

#[derive(Clone)]
pub struct RetainedSource {
    pub source: Arc<[u8]>,
    pub label: String,
    pub options: Options,
}
pub struct ArtworkImport {
    pub source: Arc<[u8]>,
    pub converted: Converted,
    pub options: Options,
    pub label: String,
    pub stamp: EditorStamp,
}
struct Job {
    worker: web_sys::Worker,
    _message: Closure<dyn FnMut(web_sys::MessageEvent)>,
    _error: Closure<dyn FnMut(web_sys::ErrorEvent)>,
    timeout: TimeoutHandle,
}
impl Drop for Job {
    fn drop(&mut self) {
        self.worker.set_onmessage(None);
        self.worker.set_onerror(None);
        self.worker.terminate();
        self.timeout.clear();
    }
}
#[derive(Clone, Copy)]
struct Importer {
    job: StoredValue<Option<Job>, LocalStorage>,
    generation: RwSignal<u64>,
    busy: RwSignal<bool>,
    message: RwSignal<String>,
    stamp: RwSignal<Option<EditorStamp>>,
}
impl Importer {
    fn cancel(self) {
        if self.busy.try_get_untracked() == Some(true) {
            let _ = self.message.try_set("Conversion cancelled; accepted artwork retained.".into());
        }
        let _ = self.generation.try_update(|n| *n = n.wrapping_add(1));
        let _ = self.job.try_update_value(|job| *job = None);
        let _ = self.busy.try_set(false);
    }

    fn stale(
        self,
        ui: Ui,
        generation: u64,
        stamp: EditorStamp,
    ) -> bool {
        if self.generation.try_get_untracked() != Some(generation) {
            return true;
        }
        if ui.editor.try_with_untracked(Editor::stamp) != Some(stamp) {
            self.fail("Project changed; conversion cancelled.");
            return true;
        }
        false
    }

    fn fail(
        self,
        message: &str,
    ) {
        self.cancel();
        let _ = self
            .message
            .try_set(format!("{message} Previous accepted artwork retained."));
    }
}
fn start(
    ui: Ui,
    state: Importer,
    file: web_sys::File,
    options: Options,
    on_accept: Callback<ArtworkImport>,
) {
    state.cancel();
    if file.size() > MAX_SOURCE_BYTES as f64 {
        state.fail("Source exceeds 8 MiB.");
        return;
    }
    if ui.inspection_active() {
        state.fail("Resolve the current draft before importing artwork.");
        return;
    }
    let generation = state.generation.get_untracked();
    let stamp = ui.editor.with_untracked(Editor::stamp);
    state.stamp.set(Some(stamp));
    state.busy.set(true);
    state
        .message
        .set("Converting in a background worker; accepted preview remains unchanged.".into());
    wasm_bindgen_futures::spawn_local(async move {
        let source_result = wasm_bindgen_futures::JsFuture::from(file.array_buffer()).await;
        if state.stale(ui, generation, stamp) {
            return;
        }
        let source = match source_result {
            Ok(value) => value,
            Err(_) => {
                state.fail("Could not read image.");
                return;
            }
        };
        if state.stale(ui, generation, stamp) {
            return;
        }
        let worker = match web_sys::Worker::new("./startup-worker_loader.js") {
            Ok(worker) => worker,
            Err(_) => {
                state.fail("Artwork worker unavailable.");
                return;
            }
        };
        let sender = worker.clone();
        let options_text = serde_json::to_string(&options).expect("bounded options");
        let mut pending = Some(source);
        let message = Closure::<dyn FnMut(web_sys::MessageEvent)>::new(move |event: web_sys::MessageEvent| {
            if state.stale(ui, generation, stamp) {
                return;
            }
            let data = event.data();
            if data.as_string().as_deref() == Some("ready") {
                if let Some(source) = pending.take() {
                    let request = js_sys::Object::new();
                    let _ = js_sys::Reflect::set(&request, &"source".into(), &source);
                    let _ = js_sys::Reflect::set(&request, &"options".into(), &options_text.clone().into());
                    let transfer = js_sys::Array::new();
                    transfer.push(&source);
                    if sender.post_message_with_transfer(&request, &transfer).is_err() {
                        state.fail("Could not send artwork to worker.");
                    }
                }
                return;
            }
            if let Ok(error) = js_sys::Reflect::get(&data, &"error".into())
                && error.is_string()
            {
                let text: js_sys::JsString = error.unchecked_into();
                if text.length() <= 512 {
                    state.fail(&String::from(text));
                } else {
                    state.fail("Invalid worker response.");
                }
                return;
            }
            let result = (|| -> Option<(Vec<u8>, (u32, u32, u16, u32, u16, bool, u32))> {
                let asset = js_sys::Reflect::get(&data, &"asset".into())
                    .ok()?
                    .dyn_into::<js_sys::ArrayBuffer>()
                    .ok()?;
                if asset.byte_length() as usize > dashboard_configurator::config::startup_asset::MAX_ASSET_BYTES {
                    return None;
                }
                let summary = js_sys::Reflect::get(&data, &"summary".into()).ok()?;
                if !summary.is_string() {
                    return None;
                }
                let summary: js_sys::JsString = summary.unchecked_into();
                if summary.length() > 512 {
                    return None;
                }
                Some((
                    js_sys::Uint8Array::new(&asset).to_vec(),
                    serde_json::from_str(&String::from(summary)).ok()?,
                ))
            })();
            let Some((asset, (width, height, frames, duration, zero, timing, end))) = result else {
                state.fail("Invalid worker response.");
                return;
            };
            if dashboard_configurator::config::startup_asset::Asset::parse(&asset).is_err() {
                state.fail("Converted animation failed validation.");
                return;
            }
            let file = file.clone();
            let options = options.clone();
            // Terminate before materializing the one original for project acceptance.
            sender.terminate();
            wasm_bindgen_futures::spawn_local(async move {
                let original = wasm_bindgen_futures::JsFuture::from(file.array_buffer()).await;
                if state.stale(ui, generation, stamp) {
                    return;
                }
                let Ok(original) = original else {
                    state.fail("Could not retain original artwork.");
                    return;
                };
                let source = js_sys::Uint8Array::new(&original);
                if source.length() as usize > MAX_SOURCE_BYTES {
                    state.fail("Original artwork exceeds the size limit.");
                    return;
                }
                let label = file
                    .name()
                    .chars()
                    .filter(|c| !c.is_control() && *c != '/' && *c != '\\')
                    .take(32)
                    .collect::<String>();
                on_accept.run(ArtworkImport {
                    source: Arc::from(source.to_vec()),
                    converted: Converted {
                        asset,
                        source_width: width,
                        source_height: height,
                        source_frames: frames,
                        source_duration_ms: duration,
                        zero_delay_frames: zero,
                        timing_changed: timing,
                        trim_end_ms: end,
                    },
                    options,
                    label,
                    stamp,
                });
                state.cancel();
                state.message.set(format!(
                    "Conversion complete: {width}×{height}, {frames} source frames. {}{} sRGB assumed; pixels \
                     composited over the selected matte.",
                    if timing { "Timing resampled. " } else { "" },
                    if zero > 0 {
                        format!("{zero} zero delays treated as 100 ms. ")
                    } else {
                        String::new()
                    }
                ));
            });
        });
        let error = Closure::<dyn FnMut(web_sys::ErrorEvent)>::new(move |_| {
            if !state.stale(ui, generation, stamp) { state.fail("Artwork worker failed."); }
        });
        worker.set_onmessage(Some(message.as_ref().unchecked_ref()));
        worker.set_onerror(Some(error.as_ref().unchecked_ref()));
        let timeout = match set_timeout_with_handle(
            move || {
                if state.generation.try_get_untracked() == Some(generation) {
                    state.fail("Conversion exceeded its 10-second deadline.");
                }
            },
            std::time::Duration::from_secs(10),
        ) {
            Ok(handle) => handle,
            Err(_) => {
                worker.terminate();
                state.fail("Could not establish conversion deadline.");
                return;
            }
        };
        state.job.set_value(Some(Job {
            worker,
            _message: message,
            _error: error,
            timeout,
        }));
    });
}

#[component]
pub fn Panel(
    ui: Ui,
    on_accept: Callback<ArtworkImport>,
    #[prop(optional)] retained: Option<Signal<Option<RetainedSource>>>,
) -> impl IntoView {
    let options = RwSignal::new(Options::default());
    let state = Importer {
        job: StoredValue::new_local(None),
        generation: RwSignal::new(0),
        busy: RwSignal::new(false),
        message: RwSignal::new(String::new()),
        stamp: RwSignal::new(None),
    };
    on_cleanup(move || state.cancel());
    Effect::new(move |_| {
        let current = ui.editor.with(Editor::stamp);
        if state.busy.get() && state.stamp.get() != Some(current) {
            state.fail("Project changed; conversion cancelled.");
        }
    });
    view! {<details class="workspace-group"><summary>"Custom artwork"</summary>
        <p class="hint">"PNG, PNG sprite sheet or animated GIF. Original artwork is retained with the project; only converted pixels enter the device. Maximum 8 MiB input and 512 KiB animation."</p>
        <label>"Source format"<select prop:value=move ||match options.get().kind{SourceKind::Png=>"png",SourceKind::Gif=>"gif",SourceKind::SpriteSheet=>"sheet"} on:change=move |e|{state.cancel();options.update(|o|{o.kind=match event_target_value(&e).as_str(){"gif"=>SourceKind::Gif,"sheet"=>SourceKind::SpriteSheet,_=>SourceKind::Png};o.sheet=if o.kind==SourceKind::SpriteSheet{Some(Sheet{columns:1,rows:1,frame_ms:100})}else{None};});}><option value="png">"PNG still"</option><option value="gif">"Animated GIF"</option><option value="sheet">"PNG sprite sheet"</option></select></label>
        <label>"Fit"<select prop:value=move ||match options.get().fit{Fit::Contain=>"contain",Fit::Cover=>"cover",Fit::Stretch=>"stretch"} on:change=move |e|{state.cancel();options.update(|o|o.fit=match event_target_value(&e).as_str(){"cover"=>Fit::Cover,"stretch"=>Fit::Stretch,_=>Fit::Contain});}><option value="contain">"Contain"</option><option value="cover">"Crop to fill"</option><option value="stretch">"Stretch"</option></select></label>
        <label>"Matte"<input type="color" prop:value=move ||{let c=options.get().matte_rgb;format!("#{:02x}{:02x}{:02x}",c[0],c[1],c[2])} on:input=move |e|{state.cancel();let value=event_target_value(&e);if value.len()==7&&value.starts_with('#'){if let Ok(color)=u32::from_str_radix(&value[1..],16){options.update(|o|o.matte_rgb=[(color>>16)as u8,(color>>8)as u8,color as u8]);}}}/></label>
        <label>"Output fps (1–20)"<input type="number" min="1" max="20" prop:value=move ||options.get().sample_fps.to_string() on:change=move |e|{state.cancel();if let Ok(value)=event_target_value(&e).parse(){options.update(|o|o.sample_fps=value);}}/></label>
        <label>"Clip start (ms)"<input type="number" min="0" max="59999" prop:value=move ||options.get().trim_start_ms.to_string() on:change=move |e|{state.cancel();if let Ok(value)=event_target_value(&e).parse(){options.update(|o|o.trim_start_ms=value);}}/></label>
        <label>"Clip end (ms)"<input type="number" min="1" max="60000" prop:value=move ||options.get().trim_end_ms.to_string() on:change=move |e|{state.cancel();if let Ok(value)=event_target_value(&e).parse(){options.update(|o|o.trim_end_ms=value);}}/></label>
        <p class="hint">"Select at most 10 seconds. Frames are sampled from the source timeline; very short final intervals merge into the preceding frame. GIF loop counts are informational; the startup Loop setting controls playback."</p>
        <div hidden=move ||options.get().kind!=SourceKind::SpriteSheet>
            <label>"Sheet columns"<input type="number" min="1" max="300" prop:value=move ||options.get().sheet.map_or(1,|s|s.columns).to_string() on:change=move |e|{state.cancel();if let Ok(value)=event_target_value(&e).parse(){options.update(|o|{if let Some(s)=&mut o.sheet{s.columns=value;}});}}/></label>
            <label>"Sheet rows"<input type="number" min="1" max="300" prop:value=move ||options.get().sheet.map_or(1,|s|s.rows).to_string() on:change=move |e|{state.cancel();if let Ok(value)=event_target_value(&e).parse(){options.update(|o|{if let Some(s)=&mut o.sheet{s.rows=value;}});}}/></label>
            <label>"Sheet frame duration (ms)"<input type="number" min="1" max="60000" prop:value=move ||options.get().sheet.map_or(100,|s|s.frame_ms).to_string() on:change=move |e|{state.cancel();if let Ok(value)=event_target_value(&e).parse(){options.update(|o|{if let Some(s)=&mut o.sheet{s.frame_ms=value;}});}}/></label>
        </div>
        <label hidden=move ||options.get().kind!=SourceKind::Gif>"GIF disposal background"<select prop:value=move ||if options.get().gif_background==GifBackground::Logical{"logical"}else{"transparent"} on:change=move |e|{state.cancel();options.update(|o|o.gif_background=if event_target_value(&e)=="transparent"{GifBackground::Transparent}else{GifBackground::Logical});}><option value="logical">"GIF logical background"</option><option value="transparent">"Transparent compatibility"</option></select></label>
        <div><button disabled=move ||retained.and_then(|r|r.get()).is_none() on:click=move |_|{if let Some(source)=retained.and_then(|r|r.get_untracked()){state.cancel();options.set(source.options);}}>"Restore original conversion settings"</button>
        <button disabled=move ||retained.and_then(|r|r.get()).is_none() on:click=move |_|{if let Some(source)=retained.and_then(|r|r.get_untracked()){let parts=js_sys::Array::new();parts.push(&js_sys::Uint8Array::from(source.source.as_ref()));match web_sys::File::new_with_u8_array_sequence(&parts,&source.label){Ok(file)=>start(ui,state,file,options.get_untracked(),on_accept),Err(_)=>state.fail("Could not reopen retained original.")}}}>"Convert retained original"</button></div>
        <p class="hint" hidden=move ||retained.and_then(|r|r.get()).is_some()>"No retained original is available. Reimport the source to change conversion options."</p>
        <label>"Choose artwork"<input type="file" accept="image/png,image/gif,.png,.gif" on:change=move |e|{let input=event_target::<web_sys::HtmlInputElement>(&e);let file=input.files().and_then(|files|files.get(0));input.set_value("");if let Some(file)=file{start(ui,state,file,options.get_untracked(),on_accept);}}/></label>
        <button disabled=move ||!state.busy.get() on:click=move |_|{state.cancel();state.message.set("Conversion cancelled; accepted artwork retained.".into());}>"Cancel conversion"</button>
        <p class="hint" role="status">{move ||state.message.get()}</p>
    </details>}
}
