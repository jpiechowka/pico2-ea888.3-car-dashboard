use dashboard_configurator::event_adapters::{ImportFailure, TransferFailure, import_outcome, transfer_failure};
use wasm_bindgen::JsValue;
use wasm_bindgen_futures::{JsFuture, spawn_local};

use super::*;

pub const STORAGE_KEY: &str = "ea888-view-studio-v1";

pub(super) fn storage_error(error: JsValue) -> StorageFailure {
    match error.dyn_ref::<web_sys::DomException>().map(|e| e.name()).as_deref() {
        Some("QuotaExceededError") => StorageFailure::Quota,
        Some("SecurityError" | "NotAllowedError") => StorageFailure::Denied,
        _ => StorageFailure::Unavailable,
    }
}

pub(super) fn storage() -> Result<web_sys::Storage, StorageFailure> {
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
pub(super) fn restore_presets() -> (PresetStore, Result<(), StorageIssue>) {
    let observation = preset_observation();
    let failure = observation.as_ref().err().copied();
    let store = PresetStore::restore(observation);
    let result = match (failure, store.status()) {
        (Some(failure), _) => Err(StorageIssue::Failure(failure)),
        (_, PresetStatus::Unavailable(PresetError::Exhausted)) => Err(StorageIssue::Exhausted),
        (_, PresetStatus::Unavailable(_)) => Err(StorageIssue::Corrupt),
        _ => Ok(()),
    };
    (store, result)
}
pub(super) fn report_preset_status(
    ui: Ui,
    reading: bool,
) {
    let Some(status) = ui.presets.try_with_untracked(PresetStore::status) else {
        return;
    };
    let outcome = match status {
        PresetStatus::Conflict => Err(StorageIssue::Conflict),
        PresetStatus::Failed(_) if reading => return,
        PresetStatus::Failed(failure) | PresetStatus::Unavailable(PresetError::Storage(failure)) => {
            Err(StorageIssue::Failure(failure))
        }
        PresetStatus::Unavailable(PresetError::Exhausted) => Err(StorageIssue::Exhausted),
        PresetStatus::Unavailable(_) => Err(StorageIssue::Corrupt),
        PresetStatus::Saved | PresetStatus::Empty => Ok(()),
        PresetStatus::Dirty | PresetStatus::Saving => return,
    };
    if reading {
        ui.storage_read_outcome(StorageArea::Presets, outcome);
    } else {
        ui.storage_outcome(StorageArea::Presets, outcome);
        if outcome.is_ok() {
            ui.storage_read_outcome(StorageArea::Presets, Ok(()));
        }
    }
}
pub(super) fn presets_changed(ui: Ui) {
    let current = preset_observation();
    if let Err(failure) = current.as_ref() {
        ui.storage_read_outcome(StorageArea::Presets, Err(StorageIssue::Failure(*failure)));
    }
    let _ = ui.presets.try_update(|store| store.observe(current));
    report_preset_status(ui, true);
}
pub(super) fn reload_presets(ui: Ui) {
    let observation = preset_observation();
    let mut result = Ok(());
    ui.presets.update(|store| result = store.reload(observation));
    report_preset_status(ui, true);
    if let Err(error) = result {
        ui.notice.set(error.message().into());
    }
}

pub fn restore() -> (Editor, String, Option<String>, Result<(), StorageIssue>) {
    match storage().and_then(|s| s.get_item(STORAGE_KEY).map_err(storage_error)) {
        Ok(Some(json)) => {
            match Editor::from_saved_state(json.as_bytes()) {
                Ok(editor) => (editor, String::new(), Some(json), Ok(())),
                Err(_) => {
                    (
                        Editor::default(),
                        "The browser draft is invalid or incompatible. Default view loaded; the old draft has not \
                         been overwritten."
                            .into(),
                        Some(json),
                        Err(StorageIssue::Corrupt),
                    )
                }
            }
        }
        Ok(None) => (Editor::default(), String::new(), None, Ok(())),
        Err(failure) => {
            (
                Editor::default(),
                "Browser storage is unavailable. You can still export your configuration to a file.".into(),
                None,
                Err(StorageIssue::Failure(failure)),
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
    #[wasm_bindgen::prelude::wasm_bindgen(method,catch,js_name=request)]
    fn request_preset_lock(
        this: &DraftLocks,
        name: &str,
        options: &js_sys::Object,
        callback: &js_sys::Function,
    ) -> Result<js_sys::Promise, JsValue>;
}
fn preset_abort() -> Result<js_sys::Object, JsValue> {
    let constructor =
        js_sys::Reflect::get(&js_sys::global(), &"AbortController".into())?.dyn_into::<js_sys::Function>()?;
    js_sys::Reflect::construct(&constructor, &js_sys::Array::new())?.dyn_into()
}
pub(super) fn cancel_presets(ui: Ui) {
    let _ = ui.presets.try_update(PresetStore::cancel);
    if let Some(Some(controller)) = ui.preset_abort.try_get_value()
        && let Ok(method) =
            js_sys::Reflect::get(&controller, &"abort".into()).and_then(|v| v.dyn_into::<js_sys::Function>())
    {
        let _ = method.call0(&controller);
    }
    let _ = ui.preset_abort.try_update_value(|slot| *slot = None);
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
        let observation = preset_observation();
        ui.presets
            .update(|store| prepared = store.begin_replace_observation(observation));
    } else {
        ui.presets.update(|store| prepared = store.begin_save());
    }
    let request = match prepared {
        Ok(request) => std::rc::Rc::new(request),
        Err(error) => {
            report_preset_status(ui, false);
            preset_import_notice(ui, Err(error));
            ui.notice.set(error.message().into());
            return;
        }
    };
    let controller = match preset_abort() {
        Ok(controller) => controller,
        Err(_) => {
            let _ = ui.presets.try_update(|store| {
                let _ = store.finish_save(&request, Err(StorageFailure::Unavailable));
            });
            report_preset_status(ui, false);
            ui.notice
                .set("Cancelable preset storage is unavailable; export a backup".into());
            return;
        }
    };
    ui.preset_abort.set_value(Some(controller.clone()));
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
        report_preset_status(ui, false);
    });
    let promise = (|| {
        let options = js_sys::Object::new();
        let signal = js_sys::Reflect::get(&controller, &"signal".into())?;
        js_sys::Reflect::set(&options, &"signal".into(), &signal)?;
        let navigator = web_sys::window().ok_or(JsValue::NULL)?.navigator();
        let locks = js_sys::Reflect::get(navigator.as_ref(), &"locks".into())?;
        if locks.is_null() || locks.is_undefined() {
            return Err(JsValue::NULL);
        }
        locks.unchecked_into::<DraftLocks>().request_preset_lock(
            PRESET_STORAGE_KEY,
            &options,
            callback.as_ref().unchecked_ref(),
        )
    })();
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
        report_preset_status(ui, false);
        let _ = ui.preset_lock_busy.try_set(false);
        let _ = ui.preset_abort.try_update_value(|slot| *slot = None);
        drop(callback);
    });
}
pub(super) fn export_presets(ui: Ui) {
    match ui.presets.with_untracked(PresetStore::export) {
        Ok(text) => {
            if download(ui, "appearance-presets.json", "application/json", text.as_bytes()).is_err() {
                ui.event(transfer_failure(TransferFailure::Download));
                ui.notice.set("The browser could not download presets".into());
            }
        }
        Err(error) => {
            ui.event(transfer_failure(TransferFailure::Serialization));
            ui.notice.set(error.message().into());
        }
    }
}
pub(super) fn import_presets(
    ui: Ui,
    file: web_sys::File,
) {
    if ui.preset_import_busy.get_untracked() {
        preset_import_notice(ui, Err(PresetError::Stale));
        ui.notice.set("A preset file is already being read".into());
        return;
    }
    if !file.size().is_finite() || file.size() > MAX_PRESET_BYTES as f64 {
        preset_import_notice(ui, Err(PresetError::TooLarge));
        ui.notice.set(PresetError::TooLarge.message().into());
        return;
    }
    let version = ui.presets.with_untracked(PresetStore::version);
    ui.preset_import_busy.set(true);
    spawn_local(async move {
        let result = JsFuture::from(file.text()).await;
        let _ = ui.preset_import_busy.try_set(false);
        if ui.presets.try_with_untracked(PresetStore::version) != Some(version) {
            return;
        }
        match result {
            Ok(value) => {
                let text: js_sys::JsString = value.unchecked_into();
                if text.length() as usize > MAX_PRESET_BYTES {
                    preset_import_notice(ui, Err(PresetError::TooLarge));
                    let _ = ui.notice.try_set(PresetError::TooLarge.message().into());
                    return;
                }
                let Some(text) = text.as_string() else {
                    return;
                };
                let mut result = Ok(());
                let _ = ui.presets.try_update(|store| result = store.import(&text));
                preset_import_notice(ui, result);
                let _ = ui.notice.try_set(match result {
                    Ok(()) => "Presets imported into memory. Save or export explicitly.".into(),
                    Err(error) => error.message().into(),
                });
            }
            Err(_) => {
                ui.event(Notice {
                    category: Category::ImportExport,
                    severity: dashboard_configurator::events::Severity::Warn,
                    code: "presets.read_failed",
                    subject: Some("Presets"),
                    message: "Preset file could not be read; previous list retained".into(),
                });
                let _ = ui
                    .notice
                    .try_set("Preset file could not be read; previous list retained".into());
            }
        }
    });
}

fn preset_import_notice(
    ui: Ui,
    result: Result<(), PresetError>,
) {
    ui.event(Notice {
        category: Category::ImportExport,
        severity: if result.is_ok() {
            dashboard_configurator::events::Severity::Info
        } else {
            dashboard_configurator::events::Severity::Warn
        },
        code: if result.is_ok() {
            "presets.imported"
        } else {
            "presets.rejected"
        },
        subject: Some("Presets"),
        message: match result {
            Ok(()) => "Presets imported into memory; explicit save required",
            Err(error) => error.message(),
        }
        .into(),
    });
}

fn conflict(
    ui: Ui,
    reading: bool,
) {
    if reading {
        ui.storage_read_outcome(StorageArea::Draft, Err(StorageIssue::Conflict));
    } else {
        ui.storage_outcome(StorageArea::Draft, Err(StorageIssue::Conflict));
    }
    ui.conflict.set(true);
    ui.notice.set(
        "The saved draft changed in another tab. Export to keep your edits, or use Save here to replace the browser \
         draft."
            .into(),
    );
}

pub fn storage_changed(ui: Ui) {
    // Events can arrive after a later write. Compare the actual current value.
    match storage().and_then(|s| s.get_item(STORAGE_KEY).map_err(storage_error)) {
        Ok(current) if current != ui.saved_bytes.get_untracked() => conflict(ui, true),
        Ok(_) => ui.storage_read_outcome(StorageArea::Draft, Ok(())),
        Err(failure) => ui.storage_read_outcome(StorageArea::Draft, Err(StorageIssue::Failure(failure))),
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
                conflict(ui, false);
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
        if !ui.conflict.get_untracked() {
            ui.storage_outcome(StorageArea::Draft, result.map_err(StorageIssue::Failure));
            if result.is_ok() {
                ui.storage_read_outcome(StorageArea::Draft, Ok(()));
            }
        }
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
            ui.storage_outcome(
                StorageArea::Draft,
                Err(StorageIssue::Failure(StorageFailure::Unavailable)),
            );
            let _ = ui.notice.try_set(
                "Browser saving needs Web Locks and storage permission. Export a file to keep your work.".into(),
            );
        }
        // Keep the callback alive until request completion releases the lock.
        drop(callback);
    });
}

pub(super) fn download(
    ui: Ui,
    name: &str,
    mime: &str,
    bytes: &[u8],
) -> Result<(), JsValue> {
    if ui.downloads.try_with_value(|pool| pool.0.len() < 16) != Some(true) {
        return Err(JsValue::from_str("Download capacity unavailable"));
    }
    let array = js_sys::Array::new();
    array.push(&js_sys::Uint8Array::from(bytes));
    let options = web_sys::BlobPropertyBag::new();
    options.set_type(mime);
    let blob = web_sys::Blob::new_with_u8_array_sequence_and_options(&array, &options)?;
    let url = web_sys::Url::create_object_url_with_blob(&blob)?;
    let owned = std::rc::Rc::new(DownloadUrl {
        url,
        revoked: std::cell::Cell::new(false),
    });
    if ui
        .downloads
        .try_update_value(|pool| pool.0.push(owned.clone()))
        .is_none()
    {
        return Err(JsValue::from_str("Download owner disposed"));
    }
    let result = (|| {
        let doc = web_sys::window()
            .and_then(|w| w.document())
            .ok_or_else(|| JsValue::from_str("Document unavailable"))?;
        let anchor = doc.create_element("a")?.dyn_into::<web_sys::HtmlAnchorElement>()?;
        anchor.set_href(&owned.url);
        anchor.set_download(name);
        let body = doc.body().ok_or_else(|| JsValue::from_str("Body unavailable"))?;
        body.append_child(&anchor)?;
        anchor.click();
        anchor.remove();
        Ok(())
    })();
    if result.is_err() {
        release_download(ui, &owned);
        return result;
    }
    // The bounded pool revokes on owner disposal too. A failed timer must not
    // leak the URL until browser GC; release its resource immediately.
    let delayed = owned.clone();
    let timer = leptos::leptos_dom::helpers::set_timeout_with_handle(
        move || {
            release_download(ui, &delayed);
        },
        std::time::Duration::from_secs(2),
    );
    if timer.is_err() {
        release_download(ui, &owned);
        return Err(JsValue::from_str("Download cleanup scheduling failed"));
    }
    result
}

struct DownloadUrl {
    url: String,
    revoked: std::cell::Cell<bool>,
}
impl DownloadUrl {
    fn revoke(&self) {
        if !self.revoked.replace(true) {
            let _ = web_sys::Url::revoke_object_url(&self.url);
        }
    }
}
impl Drop for DownloadUrl {
    fn drop(&mut self) { self.revoke(); }
}
#[derive(Default)]
pub(super) struct Downloads(Vec<std::rc::Rc<DownloadUrl>>);
impl Drop for Downloads {
    fn drop(&mut self) {
        for url in &self.0 {
            url.revoke();
        }
    }
}
fn release_download(
    ui: Ui,
    owned: &std::rc::Rc<DownloadUrl>,
) {
    owned.revoke();
    let _ = ui
        .downloads
        .try_update_value(|pool| pool.0.retain(|url| !std::rc::Rc::ptr_eq(url, owned)));
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
                ui,
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
                    ui.event(Notice::info(
                        Category::ImportExport,
                        "export.started",
                        Some(if json { "JSON" } else { "EAV" }),
                        format!("Browser download started: {} bytes", bytes.len()),
                    ));
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
                    ui.event(transfer_failure(TransferFailure::Download));
                    ui.notice
                        .set("The browser could not start the download. Check its download permissions.".into())
                }
            }
        }
        Err(error) => {
            ui.event(Notice {
                category: Category::ImportExport,
                severity: dashboard_configurator::events::Severity::Warn,
                code: "export.rejected",
                subject: None,
                message: "Configuration export blocked by current readiness or codec limits".into(),
            });
            ui.notice.set(error_text(error));
        }
    }
}

fn import_file(
    ui: Ui,
    file: web_sys::File,
) {
    let started = now();
    if !file.size().is_finite() || file.size() > MAX_JSON_BYTES as f64 {
        ui.event(import_outcome(Err(ImportFailure::TooLarge)));
        ui.notice.set("Import is limited to 128 KiB. No file was read.".into());
        return;
    }
    let mut request = Err(EditorError::InvalidDraft);
    ui.editor.update(|e| request = e.begin_import());
    let token = match request {
        Ok(t) => t,
        Err(e) => {
            ui.event(import_outcome(Err(ImportFailure::Rejected(e))));
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
                    ui.event(import_outcome(Err(ImportFailure::TooLarge)));
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
                if complete_import(ui, token, format, &bytes) {
                    metrics::file_operation(ui, true, matches!(format, ImportFormat::Json), count, started);
                }
            }
            Err(_) => {
                ui.editor.update(|e| {
                    let _ = e.cancel_import(token);
                });
                ui.event(import_outcome(Err(ImportFailure::Read)));
                ui.notice
                    .set("The file could not be read. Your current document is unchanged.".into());
            }
        }
    });
}

fn complete_import(
    ui: Ui,
    token: ImportToken,
    format: ImportFormat,
    bytes: &[u8],
) -> bool {
    let before = ui.editor.with_untracked(Editor::accepted_config_key);
    let mut outcome = Err(EditorError::InvalidDraft);
    let success = ui.run_quiet(|editor| {
        outcome = editor.complete_import(token, format, bytes);
        outcome
    });
    ui.event(import_outcome(outcome.map(|_| ()).map_err(ImportFailure::Rejected)));
    if success {
        ui.accepted_field(before, "Imported configuration");
    }
    success
}

fn import_pasted(
    ui: Ui,
    text: &str,
) {
    let mut token = Err(EditorError::InvalidDraft);
    ui.editor.update(|editor| token = editor.begin_import());
    match token {
        Ok(token) => {
            complete_import(ui, token, ImportFormat::Json, text.as_bytes());
        }
        Err(error) => {
            ui.event(import_outcome(Err(ImportFailure::Rejected(error))));
            ui.notice.set(error_text(error));
        }
    }
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
                <button disabled=move ||ui.snapshot_blocked() on:click=move |_|{let text=pasted.get_untracked();import_pasted(ui,&text);}>"Import pasted JSON"</button>
            </details>
            <p class="hint">"Local saving is per browser and origin. Export a file for a portable backup. Unknown fields, bad checksums and invalid limits are rejected."</p>
            <super::metrics::Measurements ui/>
        </div></div>
    }
}
