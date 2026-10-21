//! Immutable browser originals. Arc references pin bytes outside Wasm memory.
//! Every mutation uses one origin lock; tab lifetime locks make crash cleanup safe.
use std::cell::RefCell;
use std::collections::BTreeSet;
use std::future::Future;
use std::rc::Rc;
use std::sync::{Arc, Weak};

use dashboard_config_tools::project::{Artwork, OriginalReference, Project};
use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::{JsFuture, future_to_promise};

use super::idb::{CURRENT, ORIGINALS as BLOBS, PINS, PROJECTS, database, request, settled};
/// Origin-wide mutex for blob/pin/project read-modify-write operations.
const LOCK: &str = "ea888-original-store";
/// Prefix identifying tab-lifetime Web Locks during crash-safe pin collection.
const PREFIX: &str = "ea888-original-tab-";
// Current + 32 undo/redo entries + one captured export + one incoming import.
/// Distinct live original references per tab: current + 32 history + export + import.
const MAX_TAB_PINS: usize = 35;
/// Maximum active pin-owning tabs admitted by maintenance.
const MAX_TABS: usize = 8;
thread_local! {
    /// Cached lifetime-lock name and acquisition promise, including a failed acquisition.
    static OWNER: RefCell<Option<(String,js_sys::Promise)>> = const { RefCell::new(None) };
    /// Weak handles deduplicated by source reference; strong Rust owners pin browser bytes.
    static REFERENCES: RefCell<Vec<Weak<OriginalReference>>> = const { RefCell::new(Vec::new()) };
    /// Per-tab exclusion of concurrent source-byte materializations.
    static MATERIALIZING: RefCell<bool> = const { RefCell::new(false) };
}
/// Per-tab lease preventing simultaneous large source-byte copies into Wasm memory.
pub struct Materialization;
impl Materialization {
    /// Returns an exclusive materialization lease, or a JS error while another lease lives.
    pub fn acquire() -> Result<Self, JsValue> {
        MATERIALIZING.with(|busy| {
            let mut busy = busy.borrow_mut();
            if *busy {
                Err(JsValue::from_str("Another source operation is running"))
            } else {
                *busy = true;
                Ok(Self)
            }
        })
    }
}
impl Drop for Materialization {
    /// Releases per-tab source materialization ownership.
    fn drop(&mut self) { MATERIALIZING.with(|busy| *busy.borrow_mut() = false); }
}
/// Returns reflected property `key` from `value`, propagating JS getter/reflection errors.
fn get(
    value: &JsValue,
    key: &str,
) -> Result<JsValue, JsValue> {
    js_sys::Reflect::get(value, &key.into())
}
/// Returns the navigator Web Locks property, or a JS error without window/access.
/// Missing/unsupported managers fail when their methods are subsequently required.
fn locks() -> Result<JsValue, JsValue> { get(web_sys::window().ok_or(JsValue::NULL)?.navigator().as_ref(), "locks") }
/// Returns callable property `name` of `value`; getter errors or nonfunctions fail.
fn method(
    value: &JsValue,
    name: &str,
) -> Result<js_sys::Function, JsValue> {
    get(value, name)?.dyn_into()
}
/// Returns this tab's UUID-based lifetime-lock name after acquisition. Caches the
/// acquisition promise; crypto/locks/native rejection fails without inventing identity.
/// The held promise never resolves, so the browser releases the lock on tab destruction.
async fn owner() -> Result<String, JsValue> {
    let existing = OWNER.with(|o| o.borrow().clone());
    let (id, ready) = if let Some(owner) = existing {
        owner
    } else {
        let manager = locks()?;
        let crypto = get(web_sys::window().ok_or(JsValue::NULL)?.as_ref(), "crypto")?;
        let uuid = method(&crypto, "randomUUID")?
            .call0(&crypto)?
            .as_string()
            .ok_or(JsValue::NULL)?;
        let id = format!("{PREFIX}{uuid}");
        let mut resolver = None;
        let mut rejecter = None;
        let ready = js_sys::Promise::new(&mut |resolve, reject| {
            resolver = Some(resolve);
            rejecter = Some(reject);
        });
        let resolve = resolver.ok_or(JsValue::NULL)?;
        let callback = Closure::once_into_js(move |_: JsValue| -> js_sys::Promise {
            let _ = resolve.call0(&JsValue::NULL);
            // The browser releases this lock on tab destruction, including crashes.
            js_sys::Promise::new(&mut |_, _| {})
        });
        let held = method(&manager, "request")?
            .call2(&manager, &id.clone().into(), &callback)?
            .dyn_into::<js_sys::Promise>()?;
        let reject = rejecter.ok_or(JsValue::NULL)?;
        wasm_bindgen_futures::spawn_local(async move {
            if let Err(e) = JsFuture::from(held).await {
                let _ = reject.call1(&JsValue::NULL, &e);
            }
        });
        OWNER.with(|o| *o.borrow_mut() = Some((id.clone(), ready.clone())));
        (id, ready)
    };
    JsFuture::from(ready).await?;
    Ok(id)
}
/// Acquisition cancellation guard; an already acquired operation keeps its lock until settlement.
struct LockWait {
    /// Abort signal supplied to the queued Web Locks request.
    controller: web_sys::AbortController,
    /// Prevents an expired/cancelled queued callback from starting the operation.
    retired: Rc<std::cell::Cell<bool>>,
}
impl Drop for LockWait {
    /// Retires queued work and aborts acquisition; does not cancel work already started.
    fn drop(&mut self) {
        self.retired.set(true);
        self.controller.abort();
    }
}
/// Runs `operation` under the origin mutex and returns its result. Acquisition has
/// a 10-second browser timeout; queued cancellation prevents entry, while already
/// acquired work retains the lock until its future settles even if the caller drops.
/// API/acquisition failures return JS errors. See [Web Locks](https://www.w3.org/TR/web-locks/).
pub async fn locked<T: 'static>(operation: impl Future<Output = Result<T, JsValue>> + 'static) -> Result<T, JsValue> {
    locked_with_timeout(operation, super::idb::DEADLINE_MS).await
}
/// Runs `operation` under the origin mutex with acquisition delay `timeout_ms` in
/// milliseconds and returns its output. Deadline/cancellation retires queued work;
/// acquired operations have no execution deadline. Native/API failures return JS errors,
/// and a detached cleanup task retains the callback/timer until the lock request settles.
async fn locked_with_timeout<T: 'static>(
    operation: impl Future<Output = Result<T, JsValue>> + 'static,
    timeout_ms: i32,
) -> Result<T, JsValue> {
    let manager = locks()?;
    let controller = web_sys::AbortController::new()?;
    let retired = Rc::new(std::cell::Cell::new(false));
    let _wait = LockWait {
        controller: controller.clone(),
        retired: retired.clone(),
    };
    let options = js_sys::Object::new();
    js_sys::Reflect::set(&options, &"signal".into(), controller.signal().as_ref())?;
    let result = Rc::new(RefCell::new(None));
    let output = result.clone();
    let expired = retired.clone();
    let timer = Rc::new(RefCell::new(Some(super::idb::Deadline::new(timeout_ms, move || {
        expired.set(true);
        controller.abort();
    })?)));
    let acquired_timer = timer.clone();
    let mut operation = Some(operation);
    let callback = Closure::<dyn FnMut(JsValue) -> js_sys::Promise>::new(move |_| {
        // The deadline applies to acquisition only. An acquired operation keeps
        // the lock until its transaction reports completion or rollback.
        acquired_timer.borrow_mut().take();
        if retired.get() {
            return js_sys::Promise::resolve(&JsValue::UNDEFINED);
        }
        let operation = operation.take();
        let output = output.clone();
        future_to_promise(async move {
            if let Some(operation) = operation {
                *output.borrow_mut() = Some(operation.await);
                Ok(JsValue::UNDEFINED)
            } else {
                Err("Browser storage lock invoked twice".into())
            }
        })
    });
    let promise = method(&manager, "request")?
        .call3(&manager, &LOCK.into(), &options, callback.as_ref())?
        .dyn_into::<js_sys::Promise>()?;
    // Abort is processed on the browser lock queue. Keep the callback alive
    // until that request settles, even if the Rust waiter is dropped first.
    let cleanup = promise.clone();
    wasm_bindgen_futures::spawn_local(async move {
        let _callback = callback;
        let _timer = timer;
        let _ = JsFuture::from(cleanup).await;
    });
    JsFuture::from(promise).await.map_err(|_| {
        JsValue::from_str("Browser storage lock unavailable or timed out; close other Studio tabs and retry")
    })?;
    result.borrow_mut().take().ok_or(JsValue::NULL)?
}

/// Returns an existing equal live handle or registers `reference` weakly, removing
/// expired entries; retaining the returned Arc pins its browser blob during maintenance.
fn register(reference: Arc<OriginalReference>) -> Arc<OriginalReference> {
    REFERENCES.with(|refs| {
        let mut refs = refs.borrow_mut();
        refs.retain(|r| r.strong_count() > 0);
        if let Some(existing) = refs.iter().filter_map(Weak::upgrade).find(|r| **r == *reference) {
            existing
        } else {
            refs.push(Arc::downgrade(&reference));
            reference
        }
    })
}
/// Returns distinct keys of live registered references, pruning expired weak handles;
/// errors above 35 keys instead of silently dropping history/export/import pins.
fn current_pins() -> Result<BTreeSet<String>, JsValue> {
    let keys = REFERENCES.with(|refs| {
        let mut refs = refs.borrow_mut();
        refs.retain(|r| r.strong_count() > 0);
        refs.iter()
            .filter_map(Weak::upgrade)
            .map(|r| r.key())
            .collect::<BTreeSet<_>>()
    });
    if keys.len() > MAX_TAB_PINS {
        Err(JsValue::from_str("Original history pin limit reached"))
    } else {
        Ok(keys)
    }
}
/// Returns the JS value for `key` in `store` through `db`'s readonly transaction;
/// an absent record is undefined and native/request/timeout failures propagate.
async fn read(
    db: &web_sys::IdbDatabase,
    store: &str,
    key: &str,
) -> Result<JsValue, JsValue> {
    let tx = db.transaction_with_str(store)?;
    request(&tx.object_store(store)?.get(&key.into())?).await
}
/// Returns bounded keys from `store` in `db`, including overflow sentinels: at most
/// 10 pin-owner keys or 283 blob keys. Callers reject excess; native/request errors propagate.
async fn keys(
    db: &web_sys::IdbDatabase,
    store: &str,
) -> Result<js_sys::Array, JsValue> {
    let tx = db.transaction_with_str(store)?;
    let limit = if store == PINS {
        MAX_TABS + 2
    } else {
        MAX_TABS * MAX_TAB_PINS + 3
    };
    request(
        &tx.object_store(store)?
            .get_all_keys_with_key_and_limit(&JsValue::UNDEFINED, limit as u32)?,
    )
    .await?
    .dyn_into()
}
/// Returns `value` as an exact 64-byte lowercase hexadecimal source key; nonstrings,
/// wrong lengths, uppercase or nonhex bytes return a JS error.
fn key(value: JsValue) -> Result<String, JsValue> {
    let key = value.as_string().ok_or(JsValue::NULL)?;
    if key.len() != 64 || !key.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) {
        return Err(JsValue::NULL);
    }
    Ok(key)
}
/// Keeps maintenance rollback owned from transaction creation through settlement.
struct MaintenanceGuard<'a>(
    /// Transaction whose queued writes must not survive an early setup error.
    &'a web_sys::IdbTransaction,
);
impl Drop for MaintenanceGuard<'_> {
    /// Attempts rollback; completed/committing transactions can reject abort.
    fn drop(&mut self) { let _ = self.0.abort(); }
}
/// Maintains pins and blob GC through `db` for lifetime-lock owner `id`, optionally
/// publishing `incoming` key/blob. Caller must hold the origin mutation lock and keep
/// incoming source references strongly pinned. Retains live-tab pins and stored project
/// source; samples own Rust pins after all awaits. Returns errors on malformed keys,
/// owner/tab/pin/blob bounds, browser APIs or settlement. Writes share one transaction;
/// setup errors abort queued writes before dispatch. Settlement timeout/cancellation
/// attempts abort, but cannot prove rollback once native commit has started.
async fn maintain(
    db: &web_sys::IdbDatabase,
    id: &str,
    incoming: Option<(String, web_sys::Blob)>,
) -> Result<(), JsValue> {
    let manager = locks()?;
    let query = method(&manager, "query")?
        .call0(&manager)?
        .dyn_into::<js_sys::Promise>()?;
    let observation = JsFuture::from(query).await?;
    let held = get(&observation, "held")?.dyn_into::<js_sys::Array>()?;
    let mut alive = BTreeSet::new();
    for item in held.iter() {
        if let Some(name) = get(&item, "name")?.as_string().filter(|n| n.starts_with(PREFIX)) {
            alive.insert(name);
        }
    }
    if !alive.contains(id) {
        return Err(JsValue::from_str("Original storage supports up to eight active tabs"));
    }
    let pin_ids = keys(db, PINS).await?;
    if pin_ids.length() as usize > MAX_TABS + 1 {
        return Err(JsValue::NULL);
    }
    let existing_owners = pin_ids
        .iter()
        .filter_map(|v| v.as_string())
        .filter(|tab| tab != id && alive.contains(tab))
        .count();
    if existing_owners >= MAX_TABS {
        return Err(JsValue::from_str("Original storage supports up to eight active tabs"));
    }
    let mut retained = BTreeSet::new();
    let mut dead = Vec::new();
    for value in pin_ids.iter() {
        let tab = value.as_string().ok_or(JsValue::NULL)?;
        if tab == id {
            continue;
        }
        if !alive.contains(&tab) {
            dead.push(tab);
            continue;
        }
        let pins = read(db, PINS, &tab).await?.dyn_into::<js_sys::Array>()?;
        if pins.length() as usize > MAX_TAB_PINS {
            return Err(JsValue::NULL);
        }
        for value in pins.iter() {
            retained.insert(key(value)?);
        }
    }
    let record = read(db, PROJECTS, CURRENT).await?;
    if !record.is_undefined() {
        let source = get(&record, "original")?;
        if !source.is_undefined() && !source.is_null() {
            retained.insert(key(source)?);
        }
    }
    let stored = keys(db, BLOBS).await?;
    if stored.length() as usize > MAX_TABS * MAX_TAB_PINS + 2 {
        return Err(JsValue::NULL);
    }
    // Sample live Rust handles after all awaits; a newer import cannot lose its pin.
    let own = current_pins()?;
    retained.extend(own.iter().cloned());
    let stores = js_sys::Array::new();
    stores.push(&BLOBS.into());
    stores.push(&PINS.into());
    let tx = db.transaction_with_str_sequence_and_mode(&stores, web_sys::IdbTransactionMode::Readwrite)?;
    let _rollback = MaintenanceGuard(&tx);
    let pins = tx.object_store(PINS)?;
    let blobs = tx.object_store(BLOBS)?;
    let values = js_sys::Array::new();
    for k in own {
        values.push(&k.into());
    }
    pins.put_with_key(&values, &id.into())?;
    for tab in dead {
        pins.delete(&tab.into())?;
    }
    for value in stored.iter() {
        let k = key(value)?;
        if !retained.contains(&k) {
            blobs.delete(&k.into())?;
        }
    }
    if let Some((k, blob)) = incoming {
        blobs.put_with_key(&blob, &k.into())?;
    }
    #[cfg(feature = "browser-tests")]
    if ABORT_MAINTENANCE.with(std::cell::Cell::get) {
        tx.abort()?;
    }
    settled(&tx).await
}
/// Returns `art` with embedded originals externalized under the origin lock, or
/// unchanged without embedded bytes. Browser/pin/identity failures return JS errors;
/// accepted editor publication remains the caller's responsibility.
pub async fn externalize(art: Arc<Artwork>) -> Result<Arc<Artwork>, JsValue> {
    locked(async move { externalize_locked(art).await }).await
}
/// Returns `art` with a registered external reference after blob/pin commit, or
/// unchanged without embedded bytes. Caller holds the origin mutex; source/Blob/store
/// failures propagate without returning an uncommitted replacement artwork handle.
async fn externalize_locked(art: Arc<Artwork>) -> Result<Arc<Artwork>, JsValue> {
    let Some(source) = art.original() else { return Ok(art) };
    let reference = register(Arc::new(
        OriginalReference::new(art.recipe().source_sha256, source.len()).map_err(|_| JsValue::NULL)?,
    ));
    let parts = js_sys::Array::new();
    parts.push(&js_sys::Uint8Array::from(source.as_ref()));
    let blob = web_sys::Blob::new_with_u8_array_sequence(&parts)?;
    let external = Arc::new(art.externalized(reference.clone()).map_err(|_| JsValue::NULL)?);
    drop(art);
    let id = owner().await?;
    let k = reference.key();
    let db = database().await?;
    let result = maintain(&db, &id, Some((k, blob))).await;
    db.close();
    result?;
    Ok(external)
}
/// Returns `project` after externalizing any embedded original artwork; absent artwork
/// passes through. Storage/source/project-admission failures return JS errors.
pub async fn prepare(project: Project) -> Result<Project, JsValue> {
    if let Some(art) = project.artwork().cloned() {
        let art = externalize(art).await?;
        project.with_artwork(art).map_err(|_| JsValue::NULL)
    } else {
        Ok(project)
    }
}
/// Returns `project` with restored browser-original ownership described by `record`.
/// Caller holds the origin mutex from record read through pin publication. Validates
/// source key/recipe identity and stored Blob length, then commits pins; byte digest is
/// deferred to materialization/conversion. Missing external metadata externalizes any
/// embedded source. Missing/corrupt source or browser/project failures return JS errors.
pub async fn restore_locked(
    project: Project,
    record: &JsValue,
) -> Result<Project, JsValue> {
    let source = get(record, "original")?;
    if source.is_undefined() || source.is_null() {
        return if let Some(art) = project.artwork().cloned() {
            project
                .with_artwork(externalize_locked(art).await?)
                .map_err(|_| JsValue::NULL)
        } else {
            Ok(project)
        };
    }
    let k = key(source)?;
    let art = project.artwork().ok_or(JsValue::NULL)?;
    let reference = register(Arc::new(
        OriginalReference::new(art.recipe().source_sha256, art.recipe().source_length as usize)
            .map_err(|_| JsValue::NULL)?,
    ));
    if reference.key() != k {
        return Err(JsValue::NULL);
    }
    let id = owner().await?;
    let r = reference.clone();
    let db = database().await?;
    let result = async {
        let blob = read(&db, BLOBS, &k).await?.dyn_into::<web_sys::Blob>()?;
        if blob.size() != r.len() as f64 {
            return Err(JsValue::NULL);
        }
        maintain(&db, &id, None).await
    }
    .await;
    db.close();
    result?;
    project
        .with_artwork(Arc::new(art.externalized(reference).map_err(|_| JsValue::NULL)?))
        .map_err(|_| JsValue::NULL)
}
/// Returns the stored Blob for `art`'s external reference after locked pin maintenance
/// and length checking. Missing reference/blob, wrong size or browser failure errors;
/// this does not read bytes or verify their digest.
pub async fn blob(art: Arc<Artwork>) -> Result<web_sys::Blob, JsValue> {
    let reference = art.original_reference().ok_or(JsValue::NULL)?.clone();
    let id = owner().await?;
    locked(async move {
        let db = database().await?;
        let result = async {
            maintain(&db, &id, None).await?;
            let blob = read(&db, BLOBS, &reference.key()).await?.dyn_into::<web_sys::Blob>()?;
            if blob.size() != reference.len() as f64 {
                return Err(JsValue::NULL);
            }
            Ok(blob)
        }
        .await;
        db.close();
        result
    })
    .await
}
/// Returns the external original Blob for `art` after reading its bytes and checking
/// exact recipe length/SHA-256 under a materialization lease. Busy, missing/corrupt
/// backing data and browser failures return JS errors before conversion may start.
pub async fn verified_blob(art: Arc<Artwork>) -> Result<web_sys::Blob, JsValue> {
    let _lease = Materialization::acquire()?;
    let blob = blob(art.clone()).await?;
    let buffer = JsFuture::from(blob.array_buffer()).await?;
    let bytes = js_sys::Uint8Array::new(&buffer);
    if bytes.length() != art.recipe().source_length
        || dashboard_config_tools::project::digest(&bytes.to_vec()) != art.recipe().source_sha256
    {
        return Err(JsValue::NULL);
    }
    Ok(blob)
}
/// Returns `project` with its external original copied/verified into shared Rust bytes,
/// or unchanged without an external reference. Caller must hold a materialization lease
/// to bound concurrent byte copies. Missing/changed source or project validation errors
/// propagate; browser reads may complete after caller cancellation.
pub async fn materialize(project: Project) -> Result<Project, JsValue> {
    let Some(art) = project.artwork().cloned().filter(|a| a.original_reference().is_some()) else {
        return Ok(project);
    };
    let source = JsFuture::from(blob(art.clone()).await?.array_buffer()).await?;
    let bytes = js_sys::Uint8Array::new(&source);
    if bytes.length() as usize != art.recipe().source_length as usize {
        return Err(JsValue::NULL);
    }
    project
        .with_artwork(Arc::new(
            art.materialized(Arc::from(bytes.to_vec())).map_err(|_| JsValue::NULL)?,
        ))
        .map_err(|_| JsValue::NULL)
}

#[cfg(feature = "browser-tests")]
thread_local! {
/// Test-only abort injection after all maintenance writes are queued.
static ABORT_MAINTENANCE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) }; }

#[cfg(feature = "browser-tests")]
/// Returns success after same-origin cross-document lock timeout/cancellation and
/// real-IDB pin/GC rollback regressions. Mutates test storage and creates an iframe;
/// JS failures return errors and fixture assumptions may panic. Isolated origin only.
pub(super) async fn browser_cases() -> Result<(), JsValue> {
    // A second same-origin document holds the lock while production waits.
    let document = web_sys::window().unwrap().document().unwrap();
    let frame = document.create_element("iframe")?;
    document.body().unwrap().append_child(&frame)?;
    let other = get(&frame, "contentWindow")?;
    let manager = get(&get(&other, "navigator")?, "locks")?;
    let mut release = None;
    let held = js_sys::Promise::new(&mut |resolve, _| release = Some(resolve));
    let mut acquired = None;
    let ready = js_sys::Promise::new(&mut |resolve, _| acquired = Some(resolve));
    let acquired = acquired.unwrap();
    let callback = Closure::once_into_js(move |_: JsValue| -> js_sys::Promise {
        let _ = acquired.call0(&JsValue::NULL);
        held
    });
    // Native request returns the iframe realm's Promise; instanceof in our realm is false.
    let holding = method(&manager, "request")?
        .call2(&manager, &LOCK.into(), &callback)?
        .unchecked_into::<js_sys::Promise>();
    JsFuture::from(ready).await?;
    let expired_entered = Rc::new(std::cell::Cell::new(false));
    let expired_observed = expired_entered.clone();
    if locked_with_timeout(
        async move {
            expired_observed.set(true);
            Ok(())
        },
        30,
    )
    .await
    .is_ok()
    {
        return Err("queued origin lock did not expire".into());
    }
    let cancelled_entered = Rc::new(std::cell::Cell::new(false));
    let cancelled_observed = cancelled_entered.clone();
    {
        use std::future::Future;
        let mut cancelled = std::pin::pin!(locked(async move {
            cancelled_observed.set(true);
            Ok(())
        }));
        let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
        if !cancelled.as_mut().poll(&mut cx).is_pending() {
            return Err("blocked lock completed synchronously".into());
        }
    }
    let entered = Rc::new(std::cell::Cell::new(false));
    let observed = entered.clone();
    wasm_bindgen_futures::spawn_local(async move {
        locked(async move {
            observed.set(true);
            Ok(())
        })
        .await
        .unwrap();
    });
    for _ in 0..5 {
        super::browser_tests::tick().await;
    }
    if entered.get() {
        return Err("origin lock failed to serialize second document".into());
    }
    release.unwrap().call0(&JsValue::NULL)?;
    JsFuture::from(holding).await?;
    super::browser_tests::until(|| entered.get()).await;
    if expired_entered.get() || cancelled_entered.get() {
        return Err("expired origin lock ran after its owner released".into());
    }
    frame.remove();
    // Production origin lock and lifetime lock, real IDB pins and GC.
    locked(async {
        let db = database().await?;
        let id = owner().await?;
        let live = register(Arc::new(OriginalReference::new([0x12; 32], 3).unwrap()));
        let dead = "34".repeat(32);
        let parts = js_sys::Array::new();
        parts.push(&js_sys::Uint8Array::from(&[1u8, 2, 3][..]));
        let blob = web_sys::Blob::new_with_u8_array_sequence(&parts)?;
        maintain(&db, &id, Some((live.key(), blob.clone()))).await?;
        maintain(&db, &id, Some((dead.clone(), blob))).await?;
        maintain(&db, &id, None).await?;
        if read(&db, BLOBS, &live.key()).await?.is_undefined() || !read(&db, BLOBS, &dead).await?.is_undefined() {
            return Err("GC lost live original or retained unpinned original".into());
        }
        let old = live.key();
        drop(live);
        ABORT_MAINTENANCE.with(|flag| flag.set(true));
        let aborted = maintain(&db, &id, None).await;
        ABORT_MAINTENANCE.with(|flag| flag.set(false));
        if aborted.is_ok() || read(&db, BLOBS, &old).await?.is_undefined() {
            return Err("aborted GC deleted retained bytes".into());
        }
        maintain(&db, &id, None).await?;
        if !read(&db, BLOBS, &old).await?.is_undefined() {
            return Err("released original was not collected".into());
        }
        // A malformed later key must not commit earlier queued GC deletions or
        // the pin rewrite when maintenance returns an error before settled().
        let orphan = "00".repeat(32);
        let malformed = "zz-invalid-key";
        let tx = db.transaction_with_str_and_mode(BLOBS, web_sys::IdbTransactionMode::Readwrite)?;
        let store = tx.object_store(BLOBS)?;
        store.put_with_key(&"retained evidence".into(), &orphan.clone().into())?;
        store.put_with_key(&"malformed evidence".into(), &malformed.into())?;
        settled(&tx).await?;
        let rejected = maintain(&db, &id, None).await;
        let preserved = !read(&db, BLOBS, &orphan).await?.is_undefined();
        let tx = db.transaction_with_str_and_mode(BLOBS, web_sys::IdbTransactionMode::Readwrite)?;
        tx.object_store(BLOBS)?.delete(&orphan.into())?;
        tx.object_store(BLOBS)?.delete(&malformed.into())?;
        settled(&tx).await?;
        if rejected.is_ok() || !preserved {
            return Err("failed original maintenance committed partial garbage collection".into());
        }
        db.close();
        Ok(())
    })
    .await
}
