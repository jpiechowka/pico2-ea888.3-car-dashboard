//! IndexedDB event ownership. Request success is not a committed write.
use std::cell::{Cell, RefCell};
use std::ops::Deref;
use std::rc::Rc;

use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;

const DATABASE: &str = "ea888-studio-project-v1";
pub(super) const PROJECTS: &str = "projects";
pub(super) const ORIGINALS: &str = "originals";
pub(super) const PINS: &str = "original-pins";
pub(super) const CURRENT: &str = "current";
pub(super) const DEADLINE_MS: i32 = 10_000;
thread_local! {
    // An IDB open cannot be cancelled. Retain at most one retired observer until
    // its terminal event, so retries cannot accumulate closures or late upgrades.
    static OPEN_PENDING: Cell<bool> = const { Cell::new(false) };
}

pub(super) struct Deadline {
    window: web_sys::Window,
    handle: i32,
    _callback: Closure<dyn FnMut()>,
}
impl Deadline {
    pub(super) fn new(
        ms: i32,
        callback: impl FnMut() + 'static,
    ) -> Result<Self, JsValue> {
        let window = web_sys::window().ok_or(JsValue::NULL)?;
        let callback = Closure::<dyn FnMut()>::new(callback);
        let handle =
            window.set_timeout_with_callback_and_timeout_and_arguments_0(callback.as_ref().unchecked_ref(), ms)?;
        Ok(Self {
            window,
            handle,
            _callback: callback,
        })
    }

    pub(super) fn cancel(&self) { self.window.clear_timeout_with_handle(self.handle); }
}
impl Drop for Deadline {
    fn drop(&mut self) { self.cancel(); }
}

struct RequestEvents {
    request: web_sys::IdbRequest,
    _success: Closure<dyn FnMut(web_sys::Event)>,
    _error: Closure<dyn FnMut(web_sys::Event)>,
}
impl Drop for RequestEvents {
    fn drop(&mut self) {
        self.request.set_onsuccess(None);
        self.request.set_onerror(None);
    }
}
/// Used for reads. Timed-out reads abort their transaction before returning.
pub(super) async fn request(request: &web_sys::IdbRequest) -> Result<JsValue, JsValue> {
    let mut events = None;
    let mut deadline = None;
    let promise = js_sys::Promise::new(&mut |resolve, reject| {
        let req = request.clone();
        let failure = reject.clone();
        let success = Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
            match req.result() {
                Ok(value) => {
                    let _ = resolve.call1(&JsValue::NULL, &value);
                }
                Err(error) => {
                    let _ = failure.call1(&JsValue::NULL, &error);
                }
            }
        });
        let req = request.clone();
        let failure = reject.clone();
        let error = Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
            let error = req.error().ok().flatten().map(JsValue::from).unwrap_or(JsValue::NULL);
            let _ = failure.call1(&JsValue::NULL, &error);
        });
        request.set_onsuccess(Some(success.as_ref().unchecked_ref()));
        request.set_onerror(Some(error.as_ref().unchecked_ref()));
        events = Some(RequestEvents {
            request: request.clone(),
            _success: success,
            _error: error,
        });
        let req = request.clone();
        let failure = reject.clone();
        deadline = Some(Deadline::new(DEADLINE_MS, move || {
            if let Some(tx) = req.transaction() {
                let _ = tx.abort();
            }
            let _ = failure.call1(&JsValue::NULL, &"Browser storage read timed out".into());
        }));
    });
    let _deadline = deadline.ok_or(JsValue::NULL)??;
    let _events = events;
    JsFuture::from(promise).await
}

/// Connections close on every return/cancellation path and yield to schema upgrades.
pub(super) struct Database {
    db: web_sys::IdbDatabase,
    _version_change: Closure<dyn FnMut(web_sys::Event)>,
}
impl Database {
    fn new(db: web_sys::IdbDatabase) -> Self {
        let connection = db.clone();
        let callback = Closure::<dyn FnMut(web_sys::Event)>::new(move |_| connection.close());
        db.set_onversionchange(Some(callback.as_ref().unchecked_ref()));
        Self {
            db,
            _version_change: callback,
        }
    }
}
impl Deref for Database {
    type Target = web_sys::IdbDatabase;

    fn deref(&self) -> &Self::Target { &self.db }
}
impl Drop for Database {
    fn drop(&mut self) {
        self.db.set_onversionchange(None);
        self.db.close();
    }
}
#[derive(Default)]
struct OpenState {
    retired: Cell<bool>,
    result: RefCell<Option<web_sys::IdbDatabase>>,
}
impl OpenState {
    fn retire(&self) {
        self.retired.set(true);
        if let Some(db) = self.result.borrow_mut().take() {
            db.close();
        }
    }
}
struct OpenCaller(Rc<OpenState>, web_sys::IdbOpenDbRequest);
impl Drop for OpenCaller {
    fn drop(&mut self) {
        self.0.retire();
        if let Some(tx) = self.1.transaction() {
            let _ = tx.abort();
        }
    }
}
struct OpenEvents {
    open: web_sys::IdbOpenDbRequest,
    _request: RequestEvents,
    _upgrade: Closure<dyn FnMut(web_sys::Event)>,
    _blocked: Closure<dyn FnMut(web_sys::Event)>,
}
impl Drop for OpenEvents {
    fn drop(&mut self) {
        self.open.set_onupgradeneeded(None);
        self.open.set_onblocked(None);
        OPEN_PENDING.with(|pending| pending.set(false));
    }
}
pub(super) async fn database() -> Result<Database, JsValue> {
    if OPEN_PENDING.with(Cell::get) {
        return Err(
            "Browser storage is waiting for an earlier open to close; close older Studio tabs and retry".into(),
        );
    }
    let factory = web_sys::window()
        .ok_or(JsValue::NULL)?
        .indexed_db()?
        .ok_or(JsValue::NULL)?;
    let open = factory.open_with_u32(DATABASE, 2)?;
    OPEN_PENDING.with(|pending| pending.set(true));
    let state = Rc::new(OpenState::default());
    let caller = OpenCaller(state.clone(), open.clone());
    let mut terminal_resolve = None;
    let terminal = js_sys::Promise::new(&mut |resolve, _| terminal_resolve = Some(resolve));
    let terminal_resolve = terminal_resolve.ok_or(JsValue::NULL)?;
    let mut events = None;
    let mut deadline = None;
    let response = js_sys::Promise::new(&mut |resolve, reject| {
        let req = open.clone();
        let s = state.clone();
        let finish = terminal_resolve.clone();
        let failure = reject.clone();
        let success = Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
            match req.result().and_then(|value| value.dyn_into::<web_sys::IdbDatabase>()) {
                Ok(db) if s.retired.get() => db.close(),
                Ok(db) => {
                    *s.result.borrow_mut() = Some(db);
                    let _ = resolve.call0(&JsValue::NULL);
                }
                Err(error) => {
                    let _ = failure.call1(&JsValue::NULL, &error);
                }
            }
            let _ = finish.call0(&JsValue::NULL);
        });
        let failure = reject.clone();
        let req = open.clone();
        let finish = terminal_resolve.clone();
        let error = Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
            let error = req.error().ok().flatten().map(JsValue::from).unwrap_or(JsValue::NULL);
            let _ = failure.call1(&JsValue::NULL, &error);
            let _ = finish.call0(&JsValue::NULL);
        });
        let req = open.clone();
        let s = state.clone();
        let failure = reject.clone();
        let upgrade = Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
            if s.retired.get() {
                if let Some(tx) = req.transaction() {
                    let _ = tx.abort();
                }
                return;
            }
            let result = req
                .result()
                .and_then(|value| value.dyn_into::<web_sys::IdbDatabase>())
                .and_then(|db| {
                    for name in [PROJECTS, ORIGINALS, PINS] {
                        if !db.object_store_names().contains(name) {
                            db.create_object_store(name)?;
                        }
                    }
                    Ok(())
                });
            if let Err(error) = result {
                s.retire();
                if let Some(tx) = req.transaction() {
                    let _ = tx.abort();
                }
                let _ = failure.call1(&JsValue::NULL, &error);
            }
        });
        let s = state.clone();
        let failure = reject.clone();
        let blocked = Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
            s.retire();
            let _ = failure.call1(
                &JsValue::NULL,
                &"Browser storage upgrade blocked; close older Studio tabs and retry".into(),
            );
        });
        open.set_onsuccess(Some(success.as_ref().unchecked_ref()));
        open.set_onerror(Some(error.as_ref().unchecked_ref()));
        open.set_onupgradeneeded(Some(upgrade.as_ref().unchecked_ref()));
        open.set_onblocked(Some(blocked.as_ref().unchecked_ref()));
        events = Some(OpenEvents {
            open: open.clone(),
            _request: RequestEvents {
                request: open.clone().unchecked_into(),
                _success: success,
                _error: error,
            },
            _upgrade: upgrade,
            _blocked: blocked,
        });
        let s = state.clone();
        let req = open.clone();
        let failure = reject.clone();
        deadline = Some(Deadline::new(DEADLINE_MS, move || {
            s.retire();
            if let Some(tx) = req.transaction() {
                let _ = tx.abort();
            }
            let _ = failure.call1(
                &JsValue::NULL,
                &"Browser storage open timed out; close older Studio tabs and retry".into(),
            );
        }));
    });
    // Open requests have no cancel API. The cleanup task owns its listeners until
    // a terminal event even when the caller times out or drops its future.
    wasm_bindgen_futures::spawn_local(async move {
        let _events = events;
        let _ = JsFuture::from(terminal).await;
    });
    let _deadline = deadline.ok_or(JsValue::NULL)??;
    JsFuture::from(response).await?;
    let db = state.result.borrow_mut().take().ok_or(JsValue::NULL)?;
    drop(caller);
    Ok(Database::new(db))
}

struct TransactionEvents {
    tx: web_sys::IdbTransaction,
    finished: Rc<Cell<bool>>,
    _complete: Closure<dyn FnMut(web_sys::Event)>,
    _abort: Closure<dyn FnMut(web_sys::Event)>,
}
impl Drop for TransactionEvents {
    fn drop(&mut self) {
        if !self.finished.get() {
            let _ = self.tx.abort();
        }
        self.tx.set_oncomplete(None);
        self.tx.set_onabort(None);
    }
}
/// Register in the task that enqueues writes; only complete acknowledges a commit.
pub(super) async fn settled(tx: &web_sys::IdbTransaction) -> Result<(), JsValue> {
    let finished = Rc::new(Cell::new(false));
    let mut events = None;
    let mut deadline = None;
    let promise = js_sys::Promise::new(&mut |resolve, reject| {
        let done = finished.clone();
        let complete = Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
            done.set(true);
            let _ = resolve.call0(&JsValue::NULL);
        });
        let done = finished.clone();
        let failure = reject.clone();
        let abort = Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
            done.set(true);
            let _ = failure.call1(&JsValue::NULL, &"Browser storage transaction aborted".into());
        });
        tx.set_oncomplete(Some(complete.as_ref().unchecked_ref()));
        tx.set_onabort(Some(abort.as_ref().unchecked_ref()));
        events = Some(TransactionEvents {
            tx: tx.clone(),
            finished: finished.clone(),
            _complete: complete,
            _abort: abort,
        });
        let transaction = tx.clone();
        let failure = reject.clone();
        deadline = Some(Deadline::new(DEADLINE_MS, move || {
            // A committing transaction cannot be aborted. Never acknowledge it
            // from a deadline: report an unknown outcome and let IDB serialize
            // subsequent transactions behind its actual commit/rollback.
            let message = if transaction.abort().is_ok() {
                "Browser storage transaction timed out and was aborted"
            } else {
                "Browser storage commit outcome is unknown; reload before replacing stored work"
            };
            let _ = failure.call1(&JsValue::NULL, &message.into());
        }));
    });
    let _events = events;
    let _deadline = deadline.ok_or(JsValue::NULL)??;
    JsFuture::from(promise).await.map(|_| ())
}

#[cfg(feature = "browser-tests")]
pub(super) async fn browser_cases() -> Result<(), JsValue> {
    use std::cell::RefCell;

    use super::browser_tests::{tick, until};
    let factory = web_sys::window().unwrap().indexed_db()?.unwrap();
    request(factory.delete_database(DATABASE)?.unchecked_ref()).await?;
    let old = factory.open_with_u32(DATABASE, 1)?;
    let blocker = request(old.unchecked_ref()).await?.dyn_into::<web_sys::IdbDatabase>()?;
    let outcome = Rc::new(RefCell::new(None));
    let result = outcome.clone();
    wasm_bindgen_futures::spawn_local(async move {
        *result.borrow_mut() = Some(database().await);
    });
    for _ in 0..50 {
        if outcome.borrow().is_some() {
            break;
        }
        tick().await;
    }
    let failed_promptly = outcome.borrow().as_ref().is_some_and(Result::is_err);
    blocker.close();
    until(|| outcome.borrow().is_some()).await;
    if let Some(Ok(db)) = outcome.borrow_mut().take() {
        db.close();
    }
    if !failed_promptly {
        return Err("blocked database open never reported a recoverable failure".into());
    }
    tick().await;
    let old = factory.open_with_u32(DATABASE, 1)?;
    let unchanged = request(old.unchecked_ref()).await?.dyn_into::<web_sys::IdbDatabase>()?;
    unchanged.close();
    let repaired = database().await?;
    for store in [PROJECTS, ORIGINALS, PINS] {
        if !repaired.object_store_names().contains(store) {
            return Err("retry omitted a required store".into());
        }
    }
    // Drop suspended futures before IDB dispatches their completion events.
    // The callback properties must detach synchronously and an unacknowledged
    // write must roll back, so a later task cannot publish it.
    use std::future::Future;
    let waker = std::task::Waker::noop();
    let mut context = std::task::Context::from_waker(waker);
    let read_tx = repaired.transaction_with_str(PROJECTS)?;
    let read = read_tx.object_store(PROJECTS)?.get(&"cancelled".into())?;
    let mut pending = Box::pin(request(&read));
    if !pending.as_mut().poll(&mut context).is_pending() {
        return Err("read completed before event dispatch".into());
    }
    drop(pending);
    if read.onsuccess().is_some() || read.onerror().is_some() {
        return Err("cancelled read retained callbacks".into());
    }
    let tx = repaired.transaction_with_str_and_mode(PROJECTS, web_sys::IdbTransactionMode::Readwrite)?;
    tx.object_store(PROJECTS)?
        .put_with_key(&"must not commit".into(), &"cancelled".into())?;
    let mut pending = Box::pin(settled(&tx));
    if !pending.as_mut().poll(&mut context).is_pending() {
        return Err("write completed before event dispatch".into());
    }
    drop(pending);
    if tx.oncomplete().is_some() || tx.onabort().is_some() {
        return Err("cancelled transaction retained callbacks".into());
    }
    tick().await;
    let read_tx = repaired.transaction_with_str(PROJECTS)?;
    if !request(&read_tx.object_store(PROJECTS)?.get(&"cancelled".into())?)
        .await?
        .is_undefined()
    {
        return Err("cancelled transaction committed a late write".into());
    }
    repaired.close();
    Ok(())
}

/// Ownership for a request callback used inside a larger write transaction.
pub(super) struct SuccessHandler {
    request: web_sys::IdbRequest,
    _callback: Closure<dyn FnMut(web_sys::Event)>,
}
impl SuccessHandler {
    pub(super) fn new(
        request: web_sys::IdbRequest,
        callback: Closure<dyn FnMut(web_sys::Event)>,
    ) -> Self {
        request.set_onsuccess(Some(callback.as_ref().unchecked_ref()));
        Self {
            request,
            _callback: callback,
        }
    }
}
impl Drop for SuccessHandler {
    fn drop(&mut self) { self.request.set_onsuccess(None); }
}
