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
const LOCK: &str = "ea888-original-store";
const PREFIX: &str = "ea888-original-tab-";
// Current + 32 undo/redo entries + one captured export + one incoming import.
const MAX_TAB_PINS: usize = 35;
const MAX_TABS: usize = 8;
thread_local! {
    static OWNER: RefCell<Option<(String,js_sys::Promise)>> = const { RefCell::new(None) };
    static REFERENCES: RefCell<Vec<Weak<OriginalReference>>> = const { RefCell::new(Vec::new()) };
    static MATERIALIZING: RefCell<bool> = const { RefCell::new(false) };
}
pub struct Materialization;
impl Materialization {
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
    fn drop(&mut self) { MATERIALIZING.with(|busy| *busy.borrow_mut() = false); }
}
fn get(
    value: &JsValue,
    key: &str,
) -> Result<JsValue, JsValue> {
    js_sys::Reflect::get(value, &key.into())
}
fn locks() -> Result<JsValue, JsValue> { get(web_sys::window().ok_or(JsValue::NULL)?.navigator().as_ref(), "locks") }
fn method(
    value: &JsValue,
    name: &str,
) -> Result<js_sys::Function, JsValue> {
    get(value, name)?.dyn_into()
}
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
pub async fn locked<T: 'static>(operation: impl Future<Output = Result<T, JsValue>> + 'static) -> Result<T, JsValue> {
    let manager = locks()?;
    let result = Rc::new(RefCell::new(None));
    let output = result.clone();
    let callback = Closure::once_into_js(move |_: JsValue| -> js_sys::Promise {
        future_to_promise(async move {
            *output.borrow_mut() = Some(operation.await);
            Ok(JsValue::UNDEFINED)
        })
    });
    let promise = method(&manager, "request")?
        .call2(&manager, &LOCK.into(), &callback)?
        .dyn_into::<js_sys::Promise>()?;
    JsFuture::from(promise).await?;
    result.borrow_mut().take().ok_or(JsValue::NULL)?
}
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
async fn read(
    db: &web_sys::IdbDatabase,
    store: &str,
    key: &str,
) -> Result<JsValue, JsValue> {
    let tx = db.transaction_with_str(store)?;
    request(&tx.object_store(store)?.get(&key.into())?).await
}
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
fn key(value: JsValue) -> Result<String, JsValue> {
    let key = value.as_string().ok_or(JsValue::NULL)?;
    if key.len() != 64 || !key.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) {
        return Err(JsValue::NULL);
    }
    Ok(key)
}
/// Called only while the origin mutation lock is held. Pin writes and blob GC commit together.
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
pub async fn externalize(art: Arc<Artwork>) -> Result<Arc<Artwork>, JsValue> {
    locked(async move { externalize_locked(art).await }).await
}
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
pub async fn prepare(project: Project) -> Result<Project, JsValue> {
    if let Some(art) = project.artwork().cloned() {
        let art = externalize(art).await?;
        project.with_artwork(art).map_err(|_| JsValue::NULL)
    } else {
        Ok(project)
    }
}
/// Caller holds the origin lock from the current-record read through pin publication.
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
/// Verify backing bytes before they become a new conversion source.
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
thread_local! { static ABORT_MAINTENANCE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) }; }

#[cfg(feature = "browser-tests")]
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
        db.close();
        Ok(())
    })
    .await
}
