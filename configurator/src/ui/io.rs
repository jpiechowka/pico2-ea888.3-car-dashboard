//! Browser local storage, preset lock ownership and bounded file import/download adapters.
use dashboard_configurator::event_adapters::{
    ImportFailure,
    TransferFailure,
    import_outcome,
    reload_preset_attempt,
    transfer_failure,
};
use wasm_bindgen::JsValue;
use wasm_bindgen_futures::{JsFuture, spawn_local};

use super::*;

/// Legacy localStorage dashboard envelope key, superseded by complete-project IDB storage.
pub const STORAGE_KEY: &str = "ea888-view-studio-v1";

/// Returns quota/permission/unavailable classification for native `error`; unknown
/// exceptions remain unavailable instead of being treated as empty storage.
pub(super) fn storage_error(error: JsValue) -> StorageFailure {
    match error.dyn_ref::<web_sys::DomException>().map(|e| e.name()).as_deref() {
        Some("QuotaExceededError") => StorageFailure::Quota,
        Some("SecurityError" | "NotAllowedError") => StorageFailure::Denied,
        _ => StorageFailure::Unavailable,
    }
}

/// Returns origin localStorage, preserving permission/quota classification and treating
/// missing window/storage as unavailable. Access may fail even when the API exists.
pub(super) fn storage() -> Result<web_sys::Storage, StorageFailure> {
    web_sys::window()
        .ok_or(StorageFailure::Unavailable)?
        .local_storage()
        .map_err(storage_error)?
        .ok_or(StorageFailure::Unavailable)
}

/// Returns `input` text only within `limit` UTF-8 bytes, first bounding UTF-16 code units
/// to avoid copying an arbitrarily large JS string. Reflection/type/size failure returns None.
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
/// Returns current preset JSON, None for a missing key, or a classified storage error.
/// Oversized JS/UTF-8 values become a bounded oversize marker for model rejection; no
/// unbounded JS string is copied wholesale into the preset model.
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
/// Returns restored in-memory presets plus read status, distinguishing unavailable,
/// corrupt and exhausted storage from a valid empty store; never overwrites saved bytes.
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
/// Publishes `ui`'s settled preset status as a read outcome when `reading`, otherwise
/// as a write outcome. Dirty/saving and old write failures during reads are ignored.
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
/// Rereads actual stored presets into `ui`'s observation model and reports read status;
/// conflicts preserve local edits rather than trusting possibly delayed storage-event values.
pub(super) fn presets_changed(ui: Ui) {
    let current = preset_observation();
    if let Err(failure) = current.as_ref() {
        ui.storage_read_outcome(StorageArea::Presets, Err(StorageIssue::Failure(*failure)));
    }
    let _ = ui.presets.try_update(|store| store.observe(current));
    report_preset_status(ui, true);
}
/// Explicitly attempts reload of stored presets into `ui`, retaining model-defined
/// failure behavior and reporting read/error state without writing storage.
pub(super) fn reload_presets(ui: Ui) {
    let observation = preset_observation();
    let mut attempt = None;
    ui.presets
        .update(|store| attempt = Some(reload_preset_attempt(store, observation)));
    let Some((result, read)) = attempt else {
        return;
    };
    ui.storage_read_outcome(StorageArea::Presets, read);
    if let Err(error) = result {
        ui.notice.set(error.message().into());
    }
}

/// Returns legacy restored editor, notice, observed saved envelope and read status.
/// Invalid/unavailable data falls back to defaults without overwriting storage. Migrates
/// a bounded separate startup profile only when the envelope has no startup property.
/// Legacy getItem reads allocate the browser string before codec bounds are checked.
pub fn restore() -> (Editor, String, Option<String>, Result<(), StorageIssue>) {
    let (mut editor, mut notice, saved, status) =
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
        };
    // Migrate the earlier separate profile only when the dashboard envelope predates project state.
    let has_startup = saved
        .as_deref()
        .filter(|text| text.len() <= MAX_SAVED_BYTES)
        .and_then(|text| serde_json::from_str::<serde_json::Value>(text).ok())
        .is_some_and(|v| v.get("startup").is_some());
    if !has_startup
        && let Some(text) = storage()
            .ok()
            .and_then(|s| s.get_item("ea888-startup-profile-v1").ok().flatten())
            .filter(|s| s.len() <= 1024)
        && let Ok(profile) = serde_json::from_str::<dashboard_core::startup::StartupProfile>(&text)
        && editor.set_startup_profile(profile).is_ok()
    {
        notice =
            "Earlier startup settings migrated into the complete project. Export .eavp for a portable backup.".into();
    }
    (editor, notice, saved, status)
}

// web-sys 0.3.106 still gates Web Locks behind a global unstable flag. This
// narrow, caught binding follows the published request(name, callback) API.
// Application code remains Rust; wasm-bindgen generates the browser glue.
#[wasm_bindgen::prelude::wasm_bindgen]
extern "C" {
    #[wasm_bindgen::prelude::wasm_bindgen(extends=js_sys::Object)]
    /// Narrow caught binding to navigator.locks while web-sys gates the typed API.
    type DraftLocks;
    #[wasm_bindgen::prelude::wasm_bindgen(method,catch,js_name=request)]
    /// Returns the lock-request Promise from manager `this` for `name`, with native
    /// `options` (including abort signal) and `callback` invoked while held; synchronous
    /// Web Locks exceptions are returned. Caller retains callback until settlement.
    fn request_preset_lock(
        this: &DraftLocks,
        name: &str,
        options: &js_sys::Object,
        callback: &js_sys::Function,
    ) -> Result<js_sys::Promise, JsValue>;
}
/// Returns a native AbortController created through reflection, or its JS failure.
fn preset_abort() -> Result<js_sys::Object, JsValue> {
    let constructor =
        js_sys::Reflect::get(&js_sys::global(), &"AbortController".into())?.dyn_into::<js_sys::Function>()?;
    js_sys::Reflect::construct(&constructor, &js_sys::Array::new())?.dyn_into()
}
/// Invalidates preset-save authority in `ui` and aborts any queued lock request;
/// native abort lookup/call failures are ignored and the controller slot is cleared.
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

/// Captures `ui`'s preset write, using `replace` for explicit observed-record replacement.
/// One queued writer holds callback ownership until Web Lock settlement; cancellation
/// invalidates its token and aborts acquisition. Inside the lock it rereads/checks the
/// saved base before localStorage setItem. Failures retain memory presets and report status;
/// this path has no acquisition timeout, so callers may cancel a blocked write explicitly.
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
            let mut rejected = false;
            let _ = ui.presets.try_update(|store| {
                rejected = store.reject_lock(&request);
            });
            if rejected {
                let _ = ui.notice.try_set(
                    "Preset saving needs Web Locks and storage permission. Memory presets remain usable; export a \
                     backup."
                        .into(),
                );
                report_preset_status(ui, false);
            }
        }
        let _ = ui.preset_lock_busy.try_set(false);
        let _ = ui.preset_abort.try_update_value(|slot| *slot = None);
        drop(callback);
    });
}
/// Serializes `ui`'s memory presets and starts a JSON download; serialization/download
/// failures emit notices without changing memory or saved state.
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
/// Reads bounded preset `file` into `ui`'s memory store only while its captured version
/// remains current. One file read is admitted at a time; stale completions are ignored.
/// Browser/size/schema failures preserve the old list; success still requires explicit save.
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

/// Emits bounded import classification for `result` through `ui`, without including file payloads.
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

/// Marks `ui` conflicted, emits a read/write outcome selected by `reading`, and offers
/// export or explicit replacement without mutating current editor values.
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

/// Before complete-project storage is ready, compares actual legacy bytes with `ui`
/// observations and reports conflict/failure. After readiness, legacy events are ignored.
pub fn storage_changed(ui: Ui) {
    if ui.project_store.ready.get_untracked() {
        return;
    }
    // Events can arrive after a later write. Compare the actual current value.
    match storage().and_then(|s| s.get_item(STORAGE_KEY).map_err(storage_error)) {
        Ok(current) if current != ui.saved_bytes.get_untracked() => conflict(ui, true),
        Ok(_) => ui.storage_read_outcome(StorageArea::Draft, Ok(())),
        Err(failure) => ui.storage_read_outcome(StorageArea::Draft, Err(StorageIssue::Failure(failure))),
    }
}

/// Delegates `ui`'s complete-project save with explicit overwrite authority `replace`;
/// serialization, coalescing and transaction acknowledgement belong to project_io.
pub fn save(
    ui: Ui,
    replace: bool,
) {
    super::project_io::save(ui, replace);
}

/// Starts a browser download in `ui` named `name`, with MIME `mime` and copied `bytes`.
/// Returns success after anchor activation and cleanup scheduling, not disk persistence.
/// At most 16 live URLs are retained; DOM/Blob/URL/timer failures return JS errors.
/// URLs revoke after two seconds or owner disposal; timer failure revokes immediately
/// even though the browser download may already have started.
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

/// Shared object-URL owner with idempotent revocation across timers and UI teardown.
struct DownloadUrl {
    /// Browser-created blob URL, never a filesystem path.
    url: String,
    /// Ensures native revocation is attempted at most once.
    revoked: std::cell::Cell<bool>,
}
impl DownloadUrl {
    /// Marks revoked and attempts native URL release once, ignoring browser errors.
    fn revoke(&self) {
        if !self.revoked.replace(true) {
            let _ = web_sys::Url::revoke_object_url(&self.url);
        }
    }
}
impl Drop for DownloadUrl {
    /// Releases the URL if no earlier timer/disposal path did so.
    fn drop(&mut self) { self.revoke(); }
}
#[derive(Default)]
/// UI-owned bounded pool whose teardown revokes URLs even while timer clones survive.
pub(super) struct Downloads(
    /// At most 16 active download URL owners.
    Vec<std::rc::Rc<DownloadUrl>>,
);
impl Drop for Downloads {
    /// Revokes every retained URL before dropping the pool.
    fn drop(&mut self) {
        for url in &self.0 {
            url.revoke();
        }
    }
}
/// Revokes `owned` and removes its exact Rc identity from `ui`'s pool; disposed pool is ignored.
fn release_download(
    ui: Ui,
    owned: &std::rc::Rc<DownloadUrl>,
) {
    owned.revoke();
    let _ = ui
        .downloads
        .try_update_value(|pool| pool.0.retain(|url| !std::rc::Rc::ptr_eq(url, owned)));
}

/// Exports `ui`'s admitted dashboard as JSON when `json`, otherwise binary EAV.
/// Startup/artwork are omitted and the notice says so. Readiness/codec/download failures
/// preserve state and report an event; success records host timing and initiated download.
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
                    metrics::file_operation(
                        ui,
                        false,
                        if json { ImportFormat::Json } else { ImportFormat::Binary },
                        bytes.len(),
                        started,
                    );
                    ui.event(Notice::info(
                        Category::ImportExport,
                        "export.started",
                        Some(if json { "JSON" } else { "EAV" }),
                        format!("Browser download started: {} bytes", bytes.len()),
                    ));
                    ui.notice.set(
                        if json {
                            "Dashboard-only JSON exported. Use .eavp to retain startup and artwork."
                        } else {
                            "Dashboard-only .eav exported. Startup settings and artwork are not included."
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

/// Reads `file` for `ui` after metadata size/readiness admission and captures an import
/// token. EAVP extension grants the project byte bound; other names use the JSON bound
/// before magic/extension format detection. A single materialization lease bounds source
/// copies. Read/decode/backing failures retain accepted state; stale completions cannot
/// publish. Complete project originals are externalized before reducer publication.
fn import_file(
    ui: Ui,
    file: web_sys::File,
) {
    let started = now();
    let project_file = file.name().to_ascii_lowercase().ends_with(".eavp");
    let limit = if project_file {
        dashboard_config_tools::project::MAX_PROJECT_BYTES
    } else {
        MAX_JSON_BYTES
    };
    if !file.size().is_finite() || file.size() < 0.0 || file.size() > limit as f64 {
        let notice = import_outcome(Err(ImportFailure::TooLarge { limit_bytes: limit }));
        ui.notice.set(format!("{}. No file was read.", notice.message));
        ui.event(notice);
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
        let _lease = match originals::Materialization::acquire() {
            Ok(lease) => lease,
            Err(_) => {
                ui.editor.update(|e| {
                    let _ = e.cancel_import(token);
                });
                ui.notice.set(
                    "Another original-artwork operation is running. Try the import again when it finishes.".into(),
                );
                return;
            }
        };
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
                if array.length() as usize > limit {
                    ui.editor.update(|e| {
                        let _ = e.cancel_import(token);
                    });
                    let notice = import_outcome(Err(ImportFailure::TooLarge { limit_bytes: limit }));
                    ui.notice.set(notice.message.clone());
                    ui.event(notice);
                    return;
                }
                let bytes = array.to_vec();
                let format = if project_file || bytes.starts_with(b"EAVPROJ\0") {
                    ImportFormat::Project
                } else if bytes.starts_with(b"EAV1") || file.name().to_ascii_lowercase().ends_with(".eav") {
                    ImportFormat::Binary
                } else {
                    ImportFormat::Json
                };
                let count = bytes.len();
                let success = if matches!(format, ImportFormat::Project) {
                    let prepared = async {
                        let project =
                            dashboard_config_tools::project::Project::decode(&bytes).map_err(|_| JsValue::NULL)?;
                        originals::prepare(project).await
                    }
                    .await;
                    match prepared {
                        Ok(project) => {
                            let before = ui.editor.with_untracked(Editor::accepted_config_key);
                            let success = ui.run_quiet(|e| e.complete_prepared_project_import(token, project));
                            if success {
                                ui.accepted_field(before, "Imported complete project");
                            }
                            success
                        }
                        Err(_) => {
                            ui.editor.update(|e| {
                                let _ = e.cancel_import(token);
                            });
                            ui.notice.set(
                                "Project import could not validate or retain its original artwork. Current project \
                                 kept."
                                    .into(),
                            );
                            false
                        }
                    }
                } else {
                    complete_import(ui, token, format, &bytes)
                };
                if success {
                    metrics::file_operation(ui, true, format, count, started);
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

/// Admits `bytes` of `format` into `ui` using exact import `token`; returns reducer
/// success and reports bounded diagnostics. Stale/invalid data cannot replace accepted
/// state; successful configuration import emits grouped accepted-field feedback.
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

/// Returns `ui`'s complete/dashboard export, bounded import and retained JSON controls.
/// Readiness locks snapshots while repairs remain in the JSON editor; file input is
/// cleared after selection so the same file can be retried. No I/O starts on render.
#[component]
pub fn Files(
    /// Shared accepted project, persistence state and guarded import/export operations.
    ui: Ui
) -> impl IntoView {
    view! {
        <div class="settings-content"><div class="settings-copy"><h3>"Studio project files"</h3><p>"Keep a complete project to preserve dashboard views, startup settings and original artwork. Only the converted animation enters firmware."</p>

        </div><div class="settings-form">
            <button class="primary" disabled=move ||ui.snapshot_blocked() on:click=move |_|super::project_io::export(ui,dashboard_config_tools::project::ExportMode::Complete)>"Export complete project (.eavp)"</button>
            <p class="hint">"Includes original artwork when available, so it can be converted again later. An imported smaller project cannot restore an omitted original."</p>
            <button disabled=move ||ui.snapshot_blocked() on:click=move |_|super::project_io::export(ui,dashboard_config_tools::project::ExportMode::ConvertedOnly)>"Smaller project: converted animation only"</button>
            <p class="hint">"Keeps exact pixels and settings. Reimport the original to change crop or conversion later. This export does not remove the original from your current project."</p>
            <button disabled=move ||ui.snapshot_blocked() on:click=move |_|export(ui,false)>"Export dashboard-only .eav"</button>
            <button disabled=move ||ui.snapshot_blocked() on:click=move |_|export(ui,true)>"Export dashboard-only JSON"</button>
            <label class="field"><span>"Import .eavp project or dashboard-only JSON / .eav"</span><input type="file" accept=".eavp,.json,.eav,application/json,application/octet-stream" disabled=move ||ui.snapshot_blocked() on:change=move |event|{
                let input=event_target::<web_sys::HtmlInputElement>(&event);
                if let Some(file)=input.files().and_then(|files|files.get(0)){import_file(ui,file);}input.set_value("");
            }/><small>"Maximum 9 MiB project, 128 KiB JSON or 16 KiB .eav. Dashboard-only import keeps current startup and artwork."</small></label>
            <super::json_editor::JsonEditor ui/>
            <p class="hint">"Local saving is per browser and origin. Export a file for a portable backup. Unknown fields, bad checksums and invalid limits are rejected."</p>
            <super::metrics::Measurements ui/>
        </div></div>
    }
}
