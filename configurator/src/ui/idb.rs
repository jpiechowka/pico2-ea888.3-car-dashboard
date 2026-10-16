//! Shared IndexedDB schema and event ownership. A successful request is not a committed write.
use std::cell::Cell;
use std::rc::Rc;

use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;

const DATABASE: &str = "ea888-studio-project-v1";
pub(super) const PROJECTS: &str = "projects";
pub(super) const ORIGINALS: &str = "originals";
pub(super) const PINS: &str = "original-pins";
pub(super) const CURRENT: &str = "current";

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
                for name in [PROJECTS, ORIGINALS, PINS] {
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
/// Register in the same task that enqueues requests; an abort never reports a committed write.
pub(super) async fn settled(tx: &web_sys::IdbTransaction) -> Result<(), JsValue> {
    let mut complete = None;
    let mut abort = None;
    let promise = js_sys::Promise::new(&mut |resolve, reject| {
        complete = Some(Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
            let _ = resolve.call0(&JsValue::NULL);
        }));
        abort = Some(Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
            let _ = reject.call0(&JsValue::NULL);
        }));
        tx.set_oncomplete(complete.as_ref().map(|c| c.as_ref().unchecked_ref()));
        tx.set_onabort(abort.as_ref().map(|c| c.as_ref().unchecked_ref()));
    });
    let result = JsFuture::from(promise).await;
    tx.set_oncomplete(None);
    tx.set_onabort(None);
    drop((complete, abort));
    result.map(|_| ())
}
