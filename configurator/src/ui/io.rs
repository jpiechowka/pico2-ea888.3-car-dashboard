use wasm_bindgen::JsValue;
use wasm_bindgen_futures::{JsFuture, spawn_local};

use super::*;

pub const STORAGE_KEY: &str = "ea888-view-studio-v1";

fn storage_error(error: JsValue) -> StorageFailure {
    match error.dyn_ref::<web_sys::DomException>().map(|e| e.name()).as_deref() {
        Some("QuotaExceededError") => StorageFailure::Quota,
        Some("SecurityError" | "NotAllowedError") => StorageFailure::Denied,
        _ => StorageFailure::Unavailable,
    }
}

fn storage() -> Result<web_sys::Storage, StorageFailure> {
    web_sys::window()
        .ok_or(StorageFailure::Unavailable)?
        .local_storage()
        .map_err(storage_error)?
        .ok_or(StorageFailure::Unavailable)
}

pub(super) fn bounded_input(
    input: &web_sys::HtmlInputElement,
    limit: u32,
) -> Option<String> {
    let value = js_sys::Reflect::get(input.as_ref(), &"value".into()).ok()?;
    if !value.is_string() {
        return None;
    }
    let value: js_sys::JsString = value.unchecked_into();
    if value.length() > limit {
        return None;
    }
    let text = value.as_string()?;
    (text.len() <= limit as usize).then_some(text)
}
fn preset_observation() -> Result<Option<String>, StorageFailure> {
    let storage = storage()?;
    // Read as a JS string first so oversized cross-tab records are never copied
    // in full to Rust. A bounded oversize marker retains unavailable semantics.
    let method = js_sys::Reflect::get(storage.as_ref(), &"getItem".into())
        .map_err(storage_error)?
        .dyn_into::<js_sys::Function>()
        .map_err(storage_error)?;
    let value = method
        .call1(storage.as_ref(), &PRESET_STORAGE_KEY.into())
        .map_err(storage_error)?;
    if value.is_null() {
        return Ok(None);
    }
    if !value.is_string() {
        return Err(StorageFailure::Unavailable);
    }
    let text: js_sys::JsString = value.unchecked_into();
    if text.length() as usize > MAX_PRESET_BYTES {
        return Ok(Some(" ".repeat(MAX_PRESET_BYTES + 1)));
    }
    let text = text.as_string().ok_or(StorageFailure::Unavailable)?;
    if text.len() > MAX_PRESET_BYTES {
        return Ok(Some(" ".repeat(MAX_PRESET_BYTES + 1)));
    }
    Ok(Some(text))
}
pub(super) fn restore_presets() -> PresetStore { PresetStore::restore(preset_observation()) }
pub(super) fn presets_changed(ui: Ui) {
    let current = preset_observation();
    let _ = ui.presets.try_update(|store| store.observe(current));
}
pub(super) fn reload_presets(ui: Ui) {
    let observation = preset_observation();
    let mut result = Ok(());
    ui.presets.update(|store| result = store.reload(observation));
    if let Err(error) = result {
        ui.notice.set(error.message().into());
    }
}

pub fn restore() -> (Editor, String, Option<String>) {
    match storage().and_then(|s| s.get_item(STORAGE_KEY).map_err(storage_error)) {
        Ok(Some(json)) => {
            match Editor::from_saved_state(json.as_bytes()) {
                Ok(editor) => (editor, String::new(), Some(json)),
                Err(_) => {
                    (
                        Editor::default(),
                        "The browser draft is invalid or incompatible. Default view loaded; the old draft has not \
                         been overwritten."
                            .into(),
                        Some(json),
                    )
                }
            }
        }
        Ok(None) => (Editor::default(), String::new(), None),
        Err(_) => {
            (
                Editor::default(),
                "Browser storage is unavailable. You can still export your configuration to a file.".into(),
                None,
            )
        }
    }
}

// web-sys 0.3.106 still gates Web Locks behind a global unstable flag. This
// narrow, caught binding follows the published request(name, callback) API.
// Application code remains Rust; wasm-bindgen generates the browser glue.
#[wasm_bindgen::prelude::wasm_bindgen]
extern "C" {
    #[wasm_bindgen::prelude::wasm_bindgen(extends=js_sys::Object)]
    type DraftLocks;
    #[wasm_bindgen::prelude::wasm_bindgen(method,catch,js_name=request)]
    fn request_draft_lock(
        this: &DraftLocks,
        name: &str,
        callback: &js_sys::Function,
    ) -> Result<js_sys::Promise, JsValue>;
}
fn request_lock(
    key: &str,
    callback: &js_sys::Function,
) -> Result<js_sys::Promise, JsValue> {
    let navigator = web_sys::window().ok_or(JsValue::NULL)?.navigator();
    let locks = js_sys::Reflect::get(navigator.as_ref(), &"locks".into())?;
    if locks.is_null() || locks.is_undefined() {
        return Err(JsValue::NULL);
    }
    locks.unchecked_into::<DraftLocks>().request_draft_lock(key, callback)
}

pub(super) fn save_presets(
    ui: Ui,
    replace: bool,
) {
    if ui.preset_lock_busy.get_untracked() {
        ui.notice.set("A preset write is already waiting for its lock".into());
        return;
    }
    let mut prepared = Err(PresetError::Conflict);
    if replace {
        match preset_observation() {
            Ok(current) => {
                ui.presets
                    .update(|store| prepared = store.begin_replace(current.as_deref()))
            }
            Err(_) => {
                ui.notice.set("Preset storage is unavailable; export a backup".into());
                return;
            }
        }
    } else {
        ui.presets.update(|store| prepared = store.begin_save());
    }
    let request = match prepared {
        Ok(request) => std::rc::Rc::new(request),
        Err(error) => {
            ui.notice.set(error.message().into());
            return;
        }
    };
    ui.preset_lock_busy.set(true);
    let captured = request.clone();
    let callback = wasm_bindgen::closure::Closure::<dyn FnMut(JsValue)>::new(move |_| {
        if ui.presets.try_with_untracked(|store| store.save_is_current(&captured)) != Some(true) {
            return;
        }
        let result = storage().and_then(|storage| {
            let current = preset_observation()?;
            let check = ui
                .presets
                .try_with_untracked(|store| store.check_write(&captured, current.as_deref()));
            if check != Some(Ok(())) {
                if check == Some(Err(PresetError::Conflict)) {
                    let _ = ui.presets.try_update(|store| store.observe(Ok(current)));
                }
                return Err(StorageFailure::Denied);
            }
            storage
                .set_item(PRESET_STORAGE_KEY, captured.json())
                .map_err(storage_error)
        });
        let _ = ui.presets.try_update(|store| {
            let _ = store.finish_save(&captured, result);
        });
    });
    let promise = request_lock(PRESET_STORAGE_KEY, callback.as_ref().unchecked_ref());
    spawn_local(async move {
        let result = match promise {
            Ok(promise) => JsFuture::from(promise).await,
            Err(error) => Err(error),
        };
        if result.is_err() {
            let _ = ui.presets.try_update(|store| {
                let _ = store.finish_save(&request, Err(StorageFailure::Unavailable));
            });
            let _ = ui.notice.try_set(
                "Preset saving needs Web Locks and storage permission. Memory presets remain usable; export a backup."
                    .into(),
            );
        }
        let _ = ui.preset_lock_busy.try_set(false);
        drop(callback);
    });
}
pub(super) fn export_presets(ui: Ui) {
    match ui.presets.with_untracked(PresetStore::export) {
        Ok(text) => {
            if download("appearance-presets.json", "application/json", text.as_bytes()).is_err() {
                ui.notice.set("The browser could not download presets".into());
            }
        }
        Err(error) => ui.notice.set(error.message().into()),
    }
}
pub(super) fn import_presets(
    ui: Ui,
    file: web_sys::File,
) {
    if !file.size().is_finite() || file.size() > MAX_PRESET_BYTES as f64 {
        ui.notice.set(PresetError::TooLarge.message().into());
        return;
    }
    let version = ui.presets.with_untracked(PresetStore::version);
    spawn_local(async move {
        let result = JsFuture::from(file.text()).await;
        if ui.presets.try_with_untracked(PresetStore::version) != Some(version) {
            return;
        }
        match result {
            Ok(value) => {
                let text: js_sys::JsString = value.unchecked_into();
                if text.length() as usize > MAX_PRESET_BYTES {
                    let _ = ui.notice.try_set(PresetError::TooLarge.message().into());
                    return;
                }
                let Some(text) = text.as_string() else {
                    return;
                };
                let mut result = Ok(());
                let _ = ui.presets.try_update(|store| result = store.import(&text));
                let _ = ui.notice.try_set(match result {
                    Ok(()) => "Presets imported into memory. Save or export explicitly.".into(),
                    Err(error) => error.message().into(),
                });
            }
            Err(_) => {
                let _ = ui
                    .notice
                    .try_set("Preset file could not be read; previous list retained".into());
            }
        }
    });
}

fn conflict(ui: Ui) {
    ui.conflict.set(true);
    ui.notice.set(
        "The saved draft changed in another tab. Export to keep your edits, or use Save here to replace the browser \
         draft."
            .into(),
    );
}

pub fn storage_changed(ui: Ui) {
    // Events can arrive after a later write. Compare the actual current value.
    if let Ok(current) = storage().and_then(|s| s.get_item(STORAGE_KEY).map_err(storage_error))
        && current != ui.saved_bytes.get_untracked()
    {
        conflict(ui);
    }
}

pub fn save(
    ui: Ui,
    replace: bool,
) {
    let mut prepared = None;
    ui.editor.update(|e| prepared = e.begin_save().ok());
    let Some(request) = prepared else {
        return;
    };
    let token = request.token();
    let callback = wasm_bindgen::closure::Closure::<dyn FnMut(JsValue)>::new(move |_| {
        if !ui
            .editor
            .try_with_untracked(|e| e.save_is_current(token))
            .unwrap_or(false)
        {
            return;
        }
        let result = storage().and_then(|s| {
            let current = s.get_item(STORAGE_KEY).map_err(storage_error)?;
            if !replace && current != ui.saved_bytes.get_untracked() {
                conflict(ui);
                return Err(StorageFailure::Denied);
            }
            s.set_item(STORAGE_KEY, request.json()).map_err(storage_error)?;
            ui.saved_bytes.set(Some(request.json().to_string()));
            ui.conflict.set(false);
            Ok(())
        });
        ui.editor.update(|e| {
            let _ = e.acknowledge_save(token, result);
        });
    });
    let promise = request_lock(STORAGE_KEY, callback.as_ref().unchecked_ref());
    spawn_local(async move {
        let outcome = match promise {
            Ok(p) => JsFuture::from(p).await,
            Err(e) => Err(e),
        };
        if outcome.is_err()
            && ui
                .editor
                .try_with_untracked(|e| e.save_is_current(token))
                .unwrap_or(false)
        {
            let _ = ui.editor.try_update(|e| {
                let _ = e.acknowledge_save(token, Err(StorageFailure::Unavailable));
            });
            let _ = ui.notice.try_set(
                "Browser saving needs Web Locks and storage permission. Export a file to keep your work.".into(),
            );
        }
        // Keep the callback alive until request completion releases the lock.
        drop(callback);
    });
}

pub(super) fn download(
    name: &str,
    mime: &str,
    bytes: &[u8],
) -> Result<(), JsValue> {
    let array = js_sys::Array::new();
    array.push(&js_sys::Uint8Array::from(bytes));
    let options = web_sys::BlobPropertyBag::new();
    options.set_type(mime);
    let blob = web_sys::Blob::new_with_u8_array_sequence_and_options(&array, &options)?;
    let url = web_sys::Url::create_object_url_with_blob(&blob)?;
    let result = (|| {
        let doc = web_sys::window()
            .and_then(|w| w.document())
            .ok_or_else(|| JsValue::from_str("Document unavailable"))?;
        let anchor = doc.create_element("a")?.dyn_into::<web_sys::HtmlAnchorElement>()?;
        anchor.set_href(&url);
        anchor.set_download(name);
        let body = doc.body().ok_or_else(|| JsValue::from_str("Body unavailable"))?;
        body.append_child(&anchor)?;
        anchor.click();
        anchor.remove();
        Ok(())
    })();
    // Let the download consume the object URL before releasing its bytes.
    leptos::leptos_dom::helpers::set_timeout(
        move || {
            let _ = web_sys::Url::revoke_object_url(&url);
        },
        std::time::Duration::from_secs(2),
    );
    result
}

pub fn export(
    ui: Ui,
    json: bool,
) {
    let started = now();
    let result = ui.editor.with_untracked(|e| {
        if json {
            e.export_json().map(String::into_bytes)
        } else {
            e.export_binary()
        }
    });
    match result {
        Ok(bytes) => {
            match download(
                if json { "dashboard.json" } else { "dashboard.eav" },
                if json {
                    "application/json"
                } else {
                    "application/octet-stream"
                },
                &bytes,
            ) {
                Ok(()) => {
                    metrics::file_operation(ui, false, json, bytes.len(), started);
                    ui.notice.set(
                        if json {
                            "Editable JSON exported. Keep it as the source for future changes."
                        } else {
                            "Runtime .eav exported. Use it with Build-Dashboard.ps1 -Config to embed it."
                        }
                        .into(),
                    )
                }
                Err(_) => {
                    ui.notice
                        .set("The browser could not start the download. Check its download permissions.".into())
                }
            }
        }
        Err(error) => ui.notice.set(error_text(error)),
    }
}

fn import_file(
    ui: Ui,
    file: web_sys::File,
) {
    let started = now();
    if !file.size().is_finite() || file.size() > MAX_JSON_BYTES as f64 {
        ui.notice.set("Import is limited to 128 KiB. No file was read.".into());
        return;
    }
    let mut request = Err(EditorError::InvalidDraft);
    ui.editor.update(|e| request = e.begin_import());
    let token = match request {
        Ok(t) => t,
        Err(e) => {
            ui.notice.set(error_text(e));
            return;
        }
    };
    spawn_local(async move {
        let outcome = JsFuture::from(file.array_buffer()).await;
        if !ui
            .editor
            .try_with_untracked(|e| e.import_is_current(token))
            .unwrap_or(false)
        {
            return;
        }
        match outcome {
            Ok(buffer) => {
                let array = js_sys::Uint8Array::new(&buffer);
                if array.length() as usize > MAX_JSON_BYTES {
                    ui.editor.update(|e| {
                        let _ = e.cancel_import(token);
                    });
                    ui.notice.set("Import exceeds 128 KiB.".into());
                    return;
                }
                let bytes = array.to_vec();
                let format = if bytes.starts_with(b"EAV1") || file.name().to_ascii_lowercase().ends_with(".eav") {
                    ImportFormat::Binary
                } else {
                    ImportFormat::Json
                };
                let count = bytes.len();
                if ui.run(move |e| e.complete_import(token, format, &bytes)) {
                    metrics::file_operation(ui, true, matches!(format, ImportFormat::Json), count, started);
                }
            }
            Err(_) => {
                ui.editor.update(|e| {
                    let _ = e.cancel_import(token);
                });
                ui.notice
                    .set("The file could not be read. Your current document is unchanged.".into());
            }
        }
    });
}

#[component]
pub fn Files(ui: Ui) -> impl IntoView {
    let pasted = RwSignal::new(String::new());
    view! {
        <div class="settings-content"><div class="settings-copy"><h3>"Configuration files"</h3><p>"Keep the JSON to edit later. The compact .eav contains the same validated settings and is parsed once when the firmware starts."</p>

        </div><div class="settings-form">
            <button class="primary" disabled=move ||ui.snapshot_blocked() on:click=move |_|export(ui,false)>"Export runtime .eav"</button>
            <button disabled=move ||ui.snapshot_blocked() on:click=move |_|export(ui,true)>"Export editable JSON"</button>
            <label class="field"><span>"Import JSON or .eav"</span><input type="file" accept=".json,.eav,application/json,application/octet-stream" disabled=move ||ui.snapshot_blocked() on:change=move |event|{
                let input=event_target::<web_sys::HtmlInputElement>(&event);
                if let Some(file)=input.files().and_then(|files|files.get(0)){import_file(ui,file);}input.set_value("");
            }/><small>"Maximum 128 KiB JSON or 16 KiB device binary"</small></label>
            <details><summary>"Paste editable JSON"</summary><label class="field"><span>"Configuration JSON"</span><textarea rows="7" maxlength="131072" prop:value=move ||pasted.get() on:input=move |e|pasted.set(event_target_value(&e))></textarea></label>
                <button disabled=move ||ui.snapshot_blocked() on:click=move |_|{let text=pasted.get_untracked();ui.run(move |e|{let token=e.begin_import()?;e.complete_import(token,ImportFormat::Json,text.as_bytes())});}>"Import pasted JSON"</button>
            </details>
            <p class="hint">"Local saving is per browser and origin. Export a file for a portable backup. Unknown fields, bad checksums and invalid limits are rejected."</p>
            <super::metrics::Measurements ui/>
        </div></div>
    }
}
