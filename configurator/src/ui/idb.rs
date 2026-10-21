//! IndexedDB event ownership. Request success is not a committed write.
//! See the [IndexedDB lifecycle](https://www.w3.org/TR/IndexedDB-3/#transaction-lifetime)
//! working draft; complete acknowledges browser commit, not strict durability without a hint.
use std::cell::{Cell, RefCell};
use std::ops::Deref;
use std::rc::Rc;

use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;

/// Origin-local database name; schema version is independently opened as version 2.
const DATABASE: &str = "ea888-studio-project-v1";
/// Store containing the current project record.
pub(super) const PROJECTS: &str = "projects";
/// Immutable original-artwork blob store.
pub(super) const ORIGINALS: &str = "originals";
/// Per-tab original-reference pin records.
pub(super) const PINS: &str = "original-pins";
/// Fixed key of the current project.
pub(super) const CURRENT: &str = "current";
/// Storage timeout delay in milliseconds; browser task scheduling can delay its callback.
pub(super) const DEADLINE_MS: i32 = 10_000;
thread_local! {
    // An IDB open cannot be cancelled. Retain at most one retired observer until
    // its terminal event, so retries cannot accumulate closures or late upgrades.
    /// One unresolved native open, including a retired caller awaiting terminal cleanup.
    static OPEN_PENDING: Cell<bool> = const { Cell::new(false) };
}

/// Owned browser timeout retaining its callback until drop.
pub(super) struct Deadline {
    /// Window whose timer namespace owns the handle.
    window: web_sys::Window,
    /// Opaque timeout handle, not a duration.
    handle: i32,
    /// Rust callback kept alive while the timeout is installed.
    _callback: Closure<dyn FnMut()>,
}
impl Deadline {
    /// Installs `callback` after browser delay `ms` milliseconds and returns its guard.
    /// Missing window or timer installation failure returns a JS error; normal browser
    /// clamping/scheduling applies, so this is not a real-time deadline.
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

    /// Clears the timeout; repeated cancellation is harmless and retains the closure until drop.
    pub(super) fn cancel(&self) { self.window.clear_timeout_with_handle(self.handle); }
}
impl Drop for Deadline {
    /// Cancels before releasing the owned Rust callback.
    fn drop(&mut self) { self.cancel(); }
}

// JsFuture retains its callback state until settlement, even after its Rust
// waiter is dropped. Rejecting an already settled Promise has no effect.
/// Reject-on-drop guard releases Promise waiter state when its Rust future is cancelled.
struct CallerPromise(
    /// Reject function; repeated rejection after settlement has no effect.
    js_sys::Function,
);
impl Drop for CallerPromise {
    /// Rejects the pending waiter as cancelled, ignoring JS invocation errors.
    fn drop(&mut self) { let _ = self.0.call1(&JsValue::NULL, &"Browser storage wait cancelled".into()); }
}
/// Owns both callback properties of one request; callers must not replace them concurrently.
struct RequestEvents {
    /// Native request with exclusively owned success/error properties.
    request: web_sys::IdbRequest,
    /// Request result resolver retained until listener removal.
    _success: Closure<dyn FnMut(web_sys::Event)>,
    /// Request error resolver retained until listener removal.
    _error: Closure<dyn FnMut(web_sys::Event)>,
}
impl Drop for RequestEvents {
    /// Detaches listeners before releasing Rust closures.
    fn drop(&mut self) {
        self.request.set_onsuccess(None);
        self.request.set_onerror(None);
    }
}
/// Returns `request`'s JS result or native/scheduling/timeout error. Poll before its
/// terminal event; this replaces success/error properties. A 10-second timer attempts
/// transaction abort before rejection. Dropping the future detaches callbacks and
/// rejects its waiter, but does not itself abort the request. Intended for reads;
/// request success does not acknowledge transaction commit.
pub(super) async fn request(request: &web_sys::IdbRequest) -> Result<JsValue, JsValue> {
    let mut events = None;
    let mut deadline = None;
    let mut cancellation = None;
    let promise = js_sys::Promise::new(&mut |resolve, reject| {
        cancellation = Some(CallerPromise(reject.clone()));
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
    let promise = JsFuture::from(promise);
    let _cancellation = cancellation;
    let _deadline = deadline.ok_or(JsValue::NULL)??;
    let _events = events;
    promise.await
}

/// Connections close on every return/cancellation path and yield to schema upgrades.
pub(super) struct Database {
    /// Connection closed on drop or version-change notification.
    db: web_sys::IdbDatabase,
    /// Callback retained while registered on the connection.
    _version_change: Closure<dyn FnMut(web_sys::Event)>,
}
impl Database {
    /// Returns ownership of `db` after installing close-on-version-change behavior.
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
    /// Native interface borrowed for transaction creation.
    type Target = web_sys::IdbDatabase;

    /// Returns the guarded connection without transferring callback ownership.
    fn deref(&self) -> &Self::Target { &self.db }
}
impl Drop for Database {
    /// Removes the version-change listener and requests connection closure.
    fn drop(&mut self) {
        self.db.set_onversionchange(None);
        self.db.close();
    }
}
/// Late-open publication state independent of caller future lifetime.
#[derive(Default)]
struct OpenState {
    /// Retired callers cannot publish connections or perform late schema upgrades.
    retired: Cell<bool>,
    /// Successful connection awaiting handoff; retirement closes any retained result.
    result: RefCell<Option<web_sys::IdbDatabase>>,
}
impl OpenState {
    /// Permanently prevents publication and closes an already received connection.
    fn retire(&self) {
        self.retired.set(true);
        if let Some(db) = self.result.borrow_mut().take() {
            db.close();
        }
    }
}
/// Caller lifetime guard for an open request that cannot itself be cancelled.
struct OpenCaller(
    /// Shared publication/retirement state.
    Rc<OpenState>,
    /// Native request whose upgrade transaction may be aborted.
    web_sys::IdbOpenDbRequest,
);
impl Drop for OpenCaller {
    /// Retires publication and attempts to abort an active upgrade.
    fn drop(&mut self) {
        self.0.retire();
        if let Some(tx) = self.1.transaction() {
            let _ = tx.abort();
        }
    }
}
/// Terminal observer retained after caller timeout/cancellation to dispose late results.
struct OpenEvents {
    /// Native open with owned upgrade/blocked properties.
    open: web_sys::IdbOpenDbRequest,
    /// Success/error callbacks retained until a terminal event.
    _request: RequestEvents,
    /// Creates missing stores only for live callers; retired upgrades abort.
    _upgrade: Closure<dyn FnMut(web_sys::Event)>,
    /// Rejects blocked upgrades while keeping the terminal observer alive.
    _blocked: Closure<dyn FnMut(web_sys::Event)>,
}
impl Drop for OpenEvents {
    /// Detaches upgrade/blocked callbacks and releases the one-open slot.
    fn drop(&mut self) {
        self.open.set_onupgradeneeded(None);
        self.open.set_onblocked(None);
        OPEN_PENDING.with(|pending| pending.set(false));
    }
}
/// Returns a version-2 connection guard, creating missing stores during upgrade.
/// Errors cover unavailable IDB, pending opens, blocked upgrades, native failure and
/// a 10-second timeout. Cancellation retires publication; a detached observer closes
/// late success/aborts late upgrade and holds the single-open slot until a terminal
/// event. Existing stores/data are never deleted.
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
    let mut cancellation = None;
    let response = js_sys::Promise::new(&mut |resolve, reject| {
        cancellation = Some(CallerPromise(reject.clone()));
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
    let response = JsFuture::from(response);
    let _cancellation = cancellation;
    // Open requests have no cancel API. The cleanup task owns its listeners until
    // a terminal event even when the caller times out or drops its future.
    wasm_bindgen_futures::spawn_local(async move {
        let _events = events;
        let _ = JsFuture::from(terminal).await;
    });
    let _deadline = deadline.ok_or(JsValue::NULL)??;
    response.await?;
    let db = state.result.borrow_mut().take().ok_or(JsValue::NULL)?;
    drop(caller);
    Ok(Database::new(db))
}

/// Settlement listeners that attempt rollback when dropped without terminal acknowledgement.
struct TransactionEvents {
    /// Native transaction with exclusively owned complete/abort properties.
    tx: web_sys::IdbTransaction,
    /// Set by complete/abort; deadline rejection alone is not terminal evidence.
    finished: Rc<Cell<bool>>,
    /// Resolves the browser commit-complete event.
    _complete: Closure<dyn FnMut(web_sys::Event)>,
    /// Rejects native transaction abort.
    _abort: Closure<dyn FnMut(web_sys::Event)>,
}
impl Drop for TransactionEvents {
    /// Attempts abort before terminal acknowledgement, then detaches both listeners.
    fn drop(&mut self) {
        if !self.finished.get() {
            let _ = self.tx.abort();
        }
        self.tx.set_oncomplete(None);
        self.tx.set_onabort(None);
    }
}
/// Register in the task that enqueues writes; only complete acknowledges a commit.
/// Returns success on `tx` complete, or a JS error on abort/scheduling/timeout failure.
/// Poll before native dispatch; this replaces complete/abort properties. A 10-second
/// timeout attempts abort and reports unknown outcome if it fails. Cancellation also
/// attempts abort and detaches listeners; an already committing transaction may finish,
/// so errors alone never prove rollback.
pub(super) async fn settled(tx: &web_sys::IdbTransaction) -> Result<(), JsValue> {
    let finished = Rc::new(Cell::new(false));
    let mut events = None;
    let mut deadline = None;
    let mut cancellation = None;
    let promise = js_sys::Promise::new(&mut |resolve, reject| {
        cancellation = Some(CallerPromise(reject.clone()));
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
    let promise = JsFuture::from(promise);
    let _cancellation = cancellation;
    let _events = events;
    let _deadline = deadline.ok_or(JsValue::NULL)??;
    promise.await.map(|_| ())
}

#[cfg(feature = "browser-tests")]
/// Returns success after blocked-upgrade, cancellation/listener-release and rollback
/// regressions; JS failures return errors and setup assumptions may panic. Destructively
/// resets the current origin's Studio database; use only an isolated browser test origin.
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
    /// Owned no-op wake target; Weak references detect leaked Promise waiter state.
    struct CancelWake;
    #[allow(
        clippy::manual_noop_waker,
        reason = "The test needs an owning Waker to detect leaked Promise callbacks"
    )]
    impl std::task::Wake for CancelWake {
        /// Consumes the owning wake notification without scheduling test work.
        fn wake(self: std::sync::Arc<Self>) {}
    }
    /// Polls `future` once and drops it pending, returning a weak wake-owner witness;
    /// errors if it completed before native event dispatch.
    fn cancel_pending(future: impl Future) -> Result<std::sync::Weak<CancelWake>, JsValue> {
        let owner = std::sync::Arc::new(CancelWake);
        let released = std::sync::Arc::downgrade(&owner);
        let waker = std::task::Waker::from(owner);
        let mut context = std::task::Context::from_waker(&waker);
        let mut pending = Box::pin(future);
        if !pending.as_mut().poll(&mut context).is_pending() {
            return Err("storage wait completed before event dispatch".into());
        }
        drop(pending);
        Ok(released)
    }
    tick().await;
    let released_open = cancel_pending(database())?;
    tick().await;
    let open_retained = released_open.upgrade().is_some();
    until(|| !OPEN_PENDING.with(Cell::get)).await;
    let read_tx = repaired.transaction_with_str(PROJECTS)?;
    let read = read_tx.object_store(PROJECTS)?.get(&"cancelled".into())?;
    let released_read = cancel_pending(request(&read))?;
    if read.onsuccess().is_some() || read.onerror().is_some() {
        return Err("cancelled read retained callbacks".into());
    }
    tick().await;
    let read_retained = released_read.upgrade().is_some();
    let tx = repaired.transaction_with_str_and_mode(PROJECTS, web_sys::IdbTransactionMode::Readwrite)?;
    tx.object_store(PROJECTS)?
        .put_with_key(&"must not commit".into(), &"cancelled".into())?;
    let released_write = cancel_pending(settled(&tx))?;
    if tx.oncomplete().is_some() || tx.onabort().is_some() {
        return Err("cancelled transaction retained callbacks".into());
    }
    tick().await;
    let write_retained = released_write.upgrade().is_some();
    if open_retained || read_retained || write_retained {
        return Err(format!(
            "cancelled promise waiter retained: open={open_retained}, read={read_retained}, write={write_retained}"
        )
        .into());
    }
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
    /// Native request whose success property is exclusively owned.
    request: web_sys::IdbRequest,
    /// Rust callback retained while installed.
    _callback: Closure<dyn FnMut(web_sys::Event)>,
}
impl SuccessHandler {
    /// Installs `callback` on `request` and returns its owner. The callback may enqueue
    /// writes in the active transaction; callers separately await transaction settlement
    /// and must not replace the success property while this guard exists.
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
    /// Detaches the success property before freeing its Rust callback.
    fn drop(&mut self) { self.request.set_onsuccess(None); }
}
