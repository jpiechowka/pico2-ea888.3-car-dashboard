//! Atomic complete-project persistence. One current IDB record, one writer per UI.
use std::cell::Cell;
use std::rc::Rc;

use dashboard_config_tools::project::{ExportMode, MAX_PROJECT_BYTES};
use wasm_bindgen::JsValue;
use wasm_bindgen::closure::Closure;
use wasm_bindgen_futures::{JsFuture, spawn_local};

use super::*;
const DATABASE: &str = "ea888-studio-project-v1";
const STORE: &str = "projects";
const CURRENT: &str = "current";
#[derive(Clone, Copy)]
pub struct State {
    pub ready: RwSignal<bool>,
    busy: RwSignal<bool>,
    pending: RwSignal<bool>,
    base: RwSignal<Option<String>>,
}
impl State {
    pub fn new() -> Self {
        Self {
            ready: RwSignal::new(false),
            busy: RwSignal::new(false),
            pending: RwSignal::new(false),
            base: RwSignal::new(None),
        }
    }
}
pub(super) async fn request(request: &web_sys::IdbRequest) -> Result<JsValue, JsValue> {
    let mut success = None;
    let mut error = None;
    let promise = js_sys::Promise::new(&mut |resolve, reject| {
        let failure = reject.clone();
        let req = request.clone();
        success = Some(Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
            match req.result() {
                Ok(value) => {
                    let _ = resolve.call1(&JsValue::NULL, &value);
                }
                Err(e) => {
                    let _ = failure.call1(&JsValue::NULL, &e);
                }
            }
        }));
        let req = request.clone();
        let reject = reject.clone();
        error = Some(Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
            let value = req.error().ok().flatten().map(JsValue::from).unwrap_or(JsValue::NULL);
            let _ = reject.call1(&JsValue::NULL, &value);
        }));
        request.set_onsuccess(success.as_ref().map(|c| c.as_ref().unchecked_ref()));
        request.set_onerror(error.as_ref().map(|c| c.as_ref().unchecked_ref()));
    });
    let result = JsFuture::from(promise).await;
    request.set_onsuccess(None);
    request.set_onerror(None);
    drop(success);
    drop(error);
    result
}
pub(super) async fn database() -> Result<web_sys::IdbDatabase, JsValue> {
    let factory = web_sys::window()
        .ok_or(JsValue::NULL)?
        .indexed_db()?
        .ok_or(JsValue::NULL)?;
    let open = factory.open_with_u32(DATABASE, 2)?;
    let upgrade_request = open.clone();
    let upgrade_failed = Rc::new(Cell::new(false));
    let failed = upgrade_failed.clone();
    let upgrade = Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
        let result = upgrade_request
            .result()
            .and_then(|v| v.dyn_into::<web_sys::IdbDatabase>())
            .and_then(|db| {
                for name in [STORE, "originals", "original-pins"] {
                    if !db.object_store_names().contains(name) {
                        db.create_object_store(name)?;
                    }
                }
                Ok(())
            });
        if result.is_err() {
            failed.set(true);
            if let Some(tx) = upgrade_request.transaction() {
                let _ = tx.abort();
            }
        }
    });
    open.set_onupgradeneeded(Some(upgrade.as_ref().unchecked_ref()));
    let result = request(open.unchecked_ref()).await;
    open.set_onupgradeneeded(None);
    drop(upgrade);
    let db = result?.dyn_into::<web_sys::IdbDatabase>()?;
    if upgrade_failed.get() {
        db.close();
        return Err(JsValue::NULL);
    }
    Ok(db)
}
fn envelope(record: &JsValue) -> Result<Option<String>, JsValue> {
    if record.is_undefined() {
        return Ok(None);
    }
    let value = js_sys::Reflect::get(record, &"envelope".into())?;
    if !value.is_string() {
        return Err(JsValue::NULL);
    }
    let js: js_sys::JsString = value.unchecked_into();
    if js.length() as usize > MAX_SAVED_BYTES {
        return Err(JsValue::NULL);
    }
    let text = js.as_string().ok_or(JsValue::NULL)?;
    if text.len() > MAX_SAVED_BYTES {
        return Err(JsValue::NULL);
    }
    Ok(Some(text))
}
pub fn restore(ui: Ui) {
    let stamp = ui.editor.with_untracked(Editor::stamp);
    ui.project_store.busy.set(true);
    spawn_local(async move {
        let result = originals::locked(async {
            let _lease = originals::Materialization::acquire()?;
            let db = database().await?;
            let outcome = async {
                let tx = db.transaction_with_str(STORE)?;
                let get = tx.object_store(STORE)?.get(&CURRENT.into())?;
                request(&get).await
            }
            .await;
            db.close();
            let record = outcome?;
            let decoded = async {
                let text = envelope(&record).ok().flatten()?;
                let value = js_sys::Reflect::get(&record, &"project".into()).ok()?;
                let array = value.dyn_into::<js_sys::Uint8Array>().ok()?;
                if array.length() as usize > MAX_PROJECT_BYTES {
                    return None;
                }
                let project = dashboard_config_tools::project::Project::decode(&array.to_vec()).ok()?;
                let project = originals::restore_locked(project, &record).await.ok()?;
                Editor::from_prepared_project_saved(project, &text).ok()
            }
            .await;
            Ok((record, decoded))
        })
        .await;
        if ui.editor.try_with_untracked(Editor::stamp).is_none() {
            return;
        }
        let mut conflict = false;
        match result {
            Ok((record, decoded)) => {
                match envelope(&record) {
                    Ok(Some(text)) => {
                        ui.project_store.base.set(Some(text.clone()));
                        match decoded {
                            Some(editor)
                                if ui
                                    .editor
                                    .with_untracked(|e| e.stamp() == stamp && e.action_readiness().block.is_none()) =>
                            {
                                ui.editor.set(editor);
                                ui.saved_bytes.set(Some(text));
                                ui.notice
                                    .set("Complete Studio project restored from this browser.".into());
                            }
                            Some(_) => {
                                conflict = true;
                                ui.notice.set(
                                    "A saved project is available, but you already started editing. Current edits \
                                     were kept; Save here explicitly replaces the stored project."
                                        .into(),
                                );
                            }
                            None => {
                                conflict = true;
                                ui.notice.set(
                                    "Saved complete project is invalid. Current document and stored bytes were kept; \
                                     import a backup or explicitly Save here."
                                        .into(),
                                );
                            }
                        }
                    }
                    Ok(None) => (),
                    Err(_) => {
                        conflict = true;
                        ui.notice
                            .set("Stored project metadata is invalid. Current document was kept.".into());
                    }
                }
            }
            Err(_) => {
                ui.notice
                    .set("Complete-project browser storage is unavailable. Export .eavp to keep your work.".into());
                ui.storage_read_outcome(
                    StorageArea::Draft,
                    Err(StorageIssue::Failure(StorageFailure::Unavailable)),
                );
            }
        }
        ui.conflict.set(conflict);
        ui.project_store.ready.set(true);
        ui.project_store.busy.set(false);
        if ui.project_store.pending.get_untracked() && !conflict {
            save(ui, false);
        }
    });
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum WriteOutcome {
    Written,
    Stale,
    Conflict,
    Failed,
}
async fn write(
    db: &web_sys::IdbDatabase,
    ui: Ui,
    token: SaveToken,
    base: Option<String>,
    replace: bool,
    record: JsValue,
) -> Result<WriteOutcome, JsValue> {
    let tx = db.transaction_with_str_and_mode(STORE, web_sys::IdbTransactionMode::Readwrite)?;
    let store = tx.object_store(STORE)?;
    let get = store.get(&CURRENT.into())?;
    let outcome = Rc::new(Cell::new(WriteOutcome::Failed));
    let result = outcome.clone();
    let put_store = store.clone();
    let transaction = tx.clone();
    let observed = get.clone();
    let read = Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
        if ui.editor.try_with_untracked(|e| e.save_is_current(token)) != Some(true) {
            result.set(WriteOutcome::Stale);
            let _ = transaction.abort();
            return;
        }
        let previous = observed.result().and_then(|r| envelope(&r));
        if !replace && previous.as_ref().ok() != Some(&base) {
            result.set(WriteOutcome::Conflict);
            let _ = transaction.abort();
            return;
        }
        match put_store.put_with_key(&record, &CURRENT.into()) {
            Ok(_) => result.set(WriteOutcome::Written),
            Err(_) => {
                let _ = transaction.abort();
            }
        }
    });
    get.set_onsuccess(Some(read.as_ref().unchecked_ref()));
    let mut complete = None;
    let mut abort = None;
    let promise = js_sys::Promise::new(&mut |resolve, _reject| {
        let success = resolve.clone();
        complete = Some(Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
            let _ = success.call0(&JsValue::NULL);
        }));
        let result = outcome.clone();
        abort = Some(Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
            if result.get() == WriteOutcome::Written {
                result.set(WriteOutcome::Failed);
            }
            let _ = resolve.call0(&JsValue::NULL);
        }));
        tx.set_oncomplete(complete.as_ref().map(|c| c.as_ref().unchecked_ref()));
        tx.set_onabort(abort.as_ref().map(|c| c.as_ref().unchecked_ref()));
    });
    let settled = JsFuture::from(promise).await;
    tx.set_oncomplete(None);
    tx.set_onabort(None);
    get.set_onsuccess(None);
    drop((complete, abort, read));
    settled?;
    Ok(outcome.get())
}
pub fn save(
    ui: Ui,
    replace: bool,
) {
    let state = ui.project_store;
    if !state.ready.get_untracked() || state.busy.get_untracked() {
        state.pending.set(true);
        return;
    }
    if ui.conflict.get_untracked() && !replace {
        return;
    }
    state.pending.set(false);
    let mut prepared = None;
    ui.editor.update(|e| prepared = e.begin_save().ok());
    let Some(snapshot) = prepared else {
        return;
    };
    let token = snapshot.token();
    let text = snapshot.json().to_owned();
    let base = state.base.get_untracked();
    state.busy.set(true);
    spawn_local(async move {
        let result = async {
            let bytes = snapshot
                .project()
                .encode(ExportMode::ConvertedOnly)
                .map_err(|_| JsValue::NULL)?;
            let record = js_sys::Object::new();
            js_sys::Reflect::set(&record, &"envelope".into(), &text.clone().into())?;
            js_sys::Reflect::set(&record, &"project".into(), &js_sys::Uint8Array::from(bytes.as_slice()))?;
            if let Some(reference) = snapshot.project().artwork().and_then(|a| a.original_reference()) {
                js_sys::Reflect::set(&record, &"original".into(), &reference.key().into())?;
            }
            originals::locked(async move {
                // Keep this immutable capture pinned through the transaction completion.
                let _snapshot = snapshot;
                let db = database().await?;
                let result = write(&db, ui, token, base, replace, record.into()).await;
                db.close();
                result
            })
            .await
        }
        .await;
        if ui.editor.try_with_untracked(Editor::stamp).is_none() {
            return;
        }
        match result {
            Ok(WriteOutcome::Written) => {
                state.base.set(Some(text.clone()));
                ui.saved_bytes.set(Some(text));
                ui.conflict.set(false);
                ui.editor.update(|e| {
                    let _ = e.acknowledge_save(token, Ok(()));
                });
                ui.storage_outcome(StorageArea::Draft, Ok(()));
            }
            Ok(WriteOutcome::Stale) => (),
            Ok(WriteOutcome::Conflict) => {
                ui.conflict.set(true);
                ui.editor.update(|e| {
                    let _ = e.acknowledge_save(token, Err(StorageFailure::Unavailable));
                });
                ui.notice.set(
                    "Another tab changed the stored project. Export your edits or use Save here to replace that \
                     record."
                        .into(),
                );
                ui.storage_outcome(StorageArea::Draft, Err(StorageIssue::Conflict));
            }
            _ => {
                ui.editor.update(|e| {
                    let _ = e.acknowledge_save(token, Err(StorageFailure::Unavailable));
                });
                ui.notice.set(
                    "Complete project could not be saved in this browser. Export .eavp to preserve startup and \
                     original artwork."
                        .into(),
                );
                ui.storage_outcome(
                    StorageArea::Draft,
                    Err(StorageIssue::Failure(StorageFailure::Unavailable)),
                );
            }
        }
        state.busy.set(false);
        if state.pending.get_untracked() && !ui.conflict.get_untracked() {
            save(ui, false);
        }
    });
}

pub fn export(
    ui: Ui,
    mode: ExportMode,
) {
    let project = match ui.editor.with_untracked(Editor::accepted_project) {
        Ok(p) => p,
        Err(e) => {
            ui.notice.set(error_text(e));
            return;
        }
    };
    let lease = match originals::Materialization::acquire() {
        Ok(lease) => lease,
        Err(_) => {
            ui.notice
                .set("Another original-artwork operation is running. Try again when it finishes.".into());
            return;
        }
    };
    spawn_local(async move {
        let _lease = lease;
        let result = async {
            let project = if mode == ExportMode::Complete {
                originals::materialize(project).await?
            } else {
                project
            };
            project.encode(mode).map_err(|_| JsValue::NULL)
        }
        .await;
        if ui.editor.try_with_untracked(Editor::stamp).is_none() {
            return;
        }
        match result {
            Ok(bytes) => {
                match io::download(
                    ui,
                    if mode == ExportMode::Complete {
                        "dashboard.eavp"
                    } else {
                        "dashboard-converted.eavp"
                    },
                    "application/octet-stream",
                    &bytes,
                ) {
                    Ok(()) => {
                        ui.event(Notice::info(
                            Category::ImportExport,
                            "project.exported",
                            Some("Project"),
                            format!("Project download started: {} bytes", bytes.len()),
                        ));
                        ui.notice.set(
                            if mode == ExportMode::Complete {
                                "Complete project download started; available original artwork is included."
                            } else {
                                "Smaller project download started. Current original artwork was retained in Studio."
                            }
                            .into(),
                        );
                    }
                    Err(_) => {
                        ui.notice
                            .set("Project download could not start. Check browser download permissions.".into())
                    }
                }
            }
            Err(_) => {
                ui.notice.set(
                    "Original artwork could not be loaded or verified. Complete export was not created; your current \
                     project is unchanged."
                        .into(),
                )
            }
        }
    });
}
