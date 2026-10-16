//! One replaceable, deadline-bounded Rust Worker. Acceptance belongs to Editor.
use std::sync::Arc;

use dashboard_configurator::artwork_draft::{Number, Numbers};
use dashboard_startup_tools::{Converted, Fit, GifBackground, MAX_SOURCE_BYTES, Options, Sheet, SourceKind};
use leptos::leptos_dom::helpers::{TimeoutHandle, set_timeout_with_handle};
use wasm_bindgen::closure::Closure;

use super::*;

#[derive(Clone)]
pub struct RetainedSource {
    pub source: Arc<dashboard_config_tools::project::Artwork>,
    pub label: String,
    pub options: Options,
}
type WorkerSummary = (u32, u32, u16, u32, u16, bool, u32);
type WorkerResult = (Vec<u8>, WorkerSummary);

pub struct ArtworkImport {
    pub source: Arc<[u8]>,
    pub converted: Converted,
    pub options: Options,
    pub label: String,
    pub token: ImportToken,
    pub lease: originals::Materialization,
    pub finished: Callback<bool>,
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
    message: startup::ProjectMessage,
    stamp: RwSignal<Option<EditorStamp>>,
    token: RwSignal<Option<ImportToken>>,
    lease: StoredValue<Option<originals::Materialization>, LocalStorage>,
    editor: RwSignal<Editor>,
}
impl Importer {
    fn cancel(self) {
        if self.busy.try_get_untracked() == Some(true) {
            self.message
                .try_set("Conversion cancelled; accepted artwork retained.".into());
        }
        let _ = self.generation.try_update(|n| *n = n.saturating_add(1));
        if let Some(Some(token)) = self.token.try_get_untracked() {
            let _ = self.editor.try_update(|e| {
                let _ = e.cancel_import(token);
            });
        }
        let _ = self.token.try_set(None);
        let _ = self.lease.try_update_value(|lease| *lease = None);
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
        self.message
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
    if state.generation.get_untracked() == u64::MAX {
        state.fail("Import generation exhausted. Reload Studio after exporting your work.");
        return;
    }
    let lease = match originals::Materialization::acquire() {
        Ok(v) => v,
        Err(_) => {
            state.fail("Another original-artwork operation is running.");
            return;
        }
    };
    let mut token = Err(EditorError::InvalidDraft);
    ui.editor.update(|e| token = e.begin_import());
    let Ok(token) = token else {
        state.fail("Finish current edits before importing artwork.");
        return;
    };
    state.token.set(Some(token));
    state.lease.set_value(Some(lease));
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
            let result = (|| -> Option<WorkerResult> {
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
                state.job.update_value(|job| *job = None);
                state
                    .message
                    .set("Conversion validated; retaining original in browser storage…".into());
                let finished = Callback::new(move |accepted| {
                    if state.generation.try_get_untracked() != Some(generation) {
                        return;
                    }
                    state.cancel();
                    state.message.set(if accepted {
                        format!(
                            "Conversion accepted: {width}×{height}, {frames} source frames. {}{} sRGB assumed; pixels \
                             composited over the selected matte.",
                            if timing { "Timing resampled. " } else { "" },
                            if zero > 0 {
                                format!("{zero} zero delays treated as 100 ms. ")
                            } else {
                                String::new()
                            }
                        )
                    } else {
                        "Artwork admission failed; previous project retained.".into()
                    });
                });
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
                    token,
                    finished,
                    lease: {
                        let mut lease = None;
                        state.lease.update_value(|v| lease = v.take());
                        let Some(lease) = lease else {
                            return;
                        };
                        lease
                    },
                });
            });
        });
        let error = Closure::<dyn FnMut(web_sys::ErrorEvent)>::new(move |_| {
            if !state.stale(ui, generation, stamp) {
                state.fail("Artwork worker failed.");
            }
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

fn start_retained(
    ui: Ui,
    state: Importer,
    source: RetainedSource,
    options: Options,
    on_accept: Callback<ArtworkImport>,
) {
    state.cancel();
    if state.generation.get_untracked() == u64::MAX {
        state.fail("Import generation exhausted.");
        return;
    }
    let mut token = Err(EditorError::InvalidDraft);
    ui.editor.update(|e| token = e.begin_import());
    let Ok(token) = token else {
        state.fail("Finish current edits before reopening artwork.");
        return;
    };
    let stamp = ui.editor.with_untracked(Editor::stamp);
    let generation = state.generation.get_untracked();
    state.token.set(Some(token));
    state.stamp.set(Some(stamp));
    state.busy.set(true);
    state.message.set("Verifying retained original…".into());
    wasm_bindgen_futures::spawn_local(async move {
        let result = originals::verified_blob(source.source).await;
        if state.stale(ui, generation, stamp)
            || ui.editor.try_with_untracked(|e| e.import_is_current(token)) != Some(true)
        {
            return;
        }
        match result {
            Ok(blob) => {
                let parts = js_sys::Array::new();
                parts.push(&blob);
                match web_sys::File::new_with_blob_sequence(&parts, &source.label) {
                    Ok(file) => start(ui, state, file, options, on_accept),
                    Err(_) => state.fail("Could not reopen retained original."),
                }
            }
            Err(_) => state.fail("Retained original could not be verified."),
        }
    });
}

#[component]
fn NumericOption(
    state: Importer,
    numbers: RwSignal<Numbers>,
    field: Number,
) -> impl IntoView {
    view! {<label>{field.label()}<input type="text" inputmode="numeric" maxlength="32" aria-label=field.label()
    prop:value=move ||numbers.with(|n|n.text(field).to_owned()) on:input=move |event| {
        state.cancel();
        let input=event_target::<web_sys::HtmlInputElement>(&event);
        let text=io::bounded_input(&input,32).unwrap_or_else(||"[input exceeds limit]".into());
        numbers.update(|n|n.set(field,&text));
    }/></label>}
}

#[component]
pub fn Panel(
    ui: Ui,
    on_accept: Callback<ArtworkImport>,
    #[prop(optional)] retained: Option<Signal<Option<RetainedSource>>>,
) -> impl IntoView {
    let options = RwSignal::new(Options::default());
    let numbers = RwSignal::new(Numbers::new(&Options::default()));
    let admitted = Signal::derive(move || numbers.with(|n| n.options(&options.get())));
    let state = Importer {
        job: StoredValue::new_local(None),
        generation: RwSignal::new(0),
        busy: RwSignal::new(false),
        message: startup::ProjectMessage::new(ui.editor, String::new()),
        stamp: RwSignal::new(None),
        token: RwSignal::new(None),
        lease: StoredValue::new_local(None),
        editor: ui.editor,
    };
    on_cleanup(move || state.cancel());
    Effect::new(move |_| {
        let current = ui.editor.with(Editor::stamp);
        if state.busy.get() && state.stamp.get() != Some(current) {
            state.fail("Project changed; conversion cancelled.");
        }
    });
    view! {<details class="workspace-group startup-artwork"><summary>"Custom artwork"</summary><div class="startup-body">
        <div class="startup-source"><div><strong>{move ||retained.and_then(|r|r.get()).map_or("No original selected".into(), |s|s.label)}</strong><small>{move ||retained.and_then(|r|r.get()).map_or("Choose a PNG or GIF to create your animation".into(), |s|format!("Retained original · {:.1} KiB",s.source.recipe().source_length as f64/1024.0))}</small></div><span class="startup-limit">"SOURCE ≤ 8 MiB"<br/>"DEVICE ≤ 512 KiB"</span></div>
        <p class="hint">"PNG, PNG sprite sheet or animated GIF. Original artwork is retained with the project; only converted pixels enter the device. Maximum 8 MiB input and 512 KiB animation."</p>
        <div class="startup-grid startup-grid-three"><label>"Source format"<select prop:value=move ||match options.get().kind{SourceKind::Png=>"png",SourceKind::Gif=>"gif",SourceKind::SpriteSheet=>"sheet"} on:change=move |e|{state.cancel();options.update(|o|{o.kind=match event_target_value(&e).as_str(){"gif"=>SourceKind::Gif,"sheet"=>SourceKind::SpriteSheet,_=>SourceKind::Png};o.sheet=if o.kind==SourceKind::SpriteSheet{Some(Sheet{columns:1,rows:1,frame_ms:100})}else{None};});}><option value="png">"PNG still"</option><option value="gif">"Animated GIF"</option><option value="sheet">"PNG sprite sheet"</option></select></label>
        <label>"Fit"<select prop:value=move ||match options.get().fit{Fit::Contain=>"contain",Fit::Cover=>"cover",Fit::Stretch=>"stretch"} on:change=move |e|{state.cancel();options.update(|o|o.fit=match event_target_value(&e).as_str(){"cover"=>Fit::Cover,"stretch"=>Fit::Stretch,_=>Fit::Contain});}><option value="contain">"Contain"</option><option value="cover">"Crop to fill"</option><option value="stretch">"Stretch"</option></select></label>
        <label>"Matte"<input type="color" prop:value=move ||{let c=options.get().matte_rgb;format!("#{:02x}{:02x}{:02x}",c[0],c[1],c[2])} on:input=move |e|{state.cancel();let value=event_target_value(&e);if value.len()==7&&value.starts_with('#')&& let Ok(color)=u32::from_str_radix(&value[1..],16){options.update(|o|o.matte_rgb=[(color>>16)as u8,(color>>8)as u8,color as u8]);}}/></label>
        </div><div class="startup-grid startup-grid-three"><NumericOption state=state numbers=numbers field=Number::Fps/><NumericOption state=state numbers=numbers field=Number::Start/><NumericOption state=state numbers=numbers field=Number::End/></div>
        <p class="hint">"Select at most 10 seconds. Frames are sampled from the source timeline; very short final intervals merge into the preceding frame. GIF loop counts are informational; the startup Loop setting controls playback."</p>
        <div class="startup-grid startup-grid-three" hidden=move ||options.get().kind!=SourceKind::SpriteSheet><NumericOption state=state numbers=numbers field=Number::Columns/><NumericOption state=state numbers=numbers field=Number::Rows/><NumericOption state=state numbers=numbers field=Number::Frame/></div>
        <p role="status" hidden=move ||admitted.get().is_ok()>"INVALID CONVERSION OPTIONS · correct the numbers before converting. Accepted artwork remains unchanged."</p>
        <label hidden=move ||options.get().kind!=SourceKind::Gif>"GIF disposal background"<select prop:value=move ||if options.get().gif_background==GifBackground::Logical{"logical"}else{"transparent"} on:change=move |e|{state.cancel();options.update(|o|o.gif_background=if event_target_value(&e)=="transparent"{GifBackground::Transparent}else{GifBackground::Logical});}><option value="logical">"GIF logical background"</option><option value="transparent">"Transparent compatibility"</option></select></label>
        <div class="startup-actions"><button disabled=move ||retained.and_then(|r|r.get()).is_none() on:click=move |_|{if let Some(source)=retained.and_then(|r|r.get_untracked()){state.cancel();numbers.set(Numbers::new(&source.options));options.set(source.options);}}>"Restore original conversion settings"</button>
        <button disabled=move ||retained.and_then(|r|r.get()).is_none()||admitted.get().is_err() on:click=move |_|{if let (Some(source),Ok(options))=(retained.and_then(|r|r.get_untracked()),admitted.get_untracked()){start_retained(ui,state,source,options,on_accept);}}>"Convert retained original"</button></div>
        <p class="hint" hidden=move ||retained.and_then(|r|r.get()).is_some()>"No retained original is available. Reimport the source to change conversion options."</p>
        <label>"Choose artwork"<input type="file" disabled=move ||admitted.get().is_err() accept="image/png,image/gif,.png,.gif" on:change=move |e|{let input=event_target::<web_sys::HtmlInputElement>(&e);let file=input.files().and_then(|files|files.get(0));input.set_value("");if let (Some(file),Ok(options))=(file,admitted.get_untracked()){start(ui,state,file,options,on_accept);}}/></label>
        <button disabled=move ||!state.busy.get() on:click=move |_|{state.cancel();state.message.set("Conversion cancelled; accepted artwork retained.".into());}>"Cancel conversion"</button>
        <p class="startup-status" role="status">{move ||state.message.get()}</p>
    </div></details>}
}
