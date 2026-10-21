//! Browser-only validation driver. Excluded from ordinary production builds.
use wasm_bindgen::JsValue;
use wasm_bindgen::closure::Closure;
use wasm_bindgen_futures::JsFuture;

use super::*;

/// Yields through a browser timer requesting a 10-ms delay; panics if timer setup or promise completion fails. Browser
/// scheduling may take longer.
pub(super) async fn tick() {
    let promise = js_sys::Promise::new(&mut |resolve, _| {
        web_sys::window()
            .unwrap()
            .set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, 10)
            .unwrap();
    });
    JsFuture::from(promise).await.unwrap();
}
/// Polls `ready` at most 500 times with timer yields; panics on exhaustion. The nominal five-second budget excludes
/// browser scheduling delays.
pub(super) async fn until(mut ready: impl FnMut() -> bool) {
    for _ in 0..500 {
        if ready() {
            return;
        }
        tick().await;
    }
    panic!("browser condition exceeded five seconds");
}
/// Returns success when `value` is true, otherwise a JavaScript error containing `label`.
fn check(
    value: bool,
    label: &str,
) -> Result<(), JsValue> {
    if value { Ok(()) } else { Err(label.into()) }
}
/// Starts the asynchronous suite against mounted `ui` only with the exact test query and isolated-server handshake.
/// Installs a panic hook and posts results; handshake failure leaves the suite inert.
pub(super) fn launch(ui: Ui) {
    // A test-feature build accidentally served on the user's origin must remain inert.
    if web_sys::window().unwrap().location().search().ok().as_deref() != Some("?browser-tests=1") {
        return;
    }
    std::panic::set_hook(Box::new(|info| {
        web_sys::console::error_1(&format!("browser test panic: {info}").into())
    }));
    wasm_bindgen_futures::spawn_local(async move {
        let handshake = async {
            let response = JsFuture::from(web_sys::window().unwrap().fetch_with_str("/__test_environment"))
                .await?
                .dyn_into::<web_sys::Response>()?;
            if !response.ok() {
                return Err(JsValue::NULL);
            }
            let marker = JsFuture::from(response.text()?).await?;
            check(
                marker.as_string().as_deref() == Some("dashboard-studio-isolated-browser-tests-v1"),
                "test server handshake rejected",
            )
        }
        .await;
        if handshake.is_err() {
            return;
        }
        until(|| ui.project_store.ready.get_untracked() && ui.canvas.get_untracked().is_some()).await;
        let result = run(ui).await;
        let text = match result {
            Ok(()) => "PASS browser failure suite".to_owned(),
            Err(e) => format!("FAIL {e:?}"),
        };
        web_sys::console::log_1(&text.clone().into());
        let init = web_sys::RequestInit::new();
        init.set_method("POST");
        init.set_body(&JsValue::from_str(&text));
        let _ = JsFuture::from(web_sys::window().unwrap().fetch_with_str_and_init("/__result", &init)).await;
    });
}
/// Posts `label` to the isolated server progress endpoint; ignores fetch failures and returns after the request
/// settles. Panics without a browser window.
pub(super) async fn progress(label: &str) {
    let init = web_sys::RequestInit::new();
    init.set_method("POST");
    init.set_body(&label.into());
    let _ = JsFuture::from(web_sys::window().unwrap().fetch_with_str_and_init("/__progress", &init)).await;
}
/// Runs storage, worker, helper fixture, canvas-failure and real DOM regressions against `ui`; returns the first
/// JavaScript/assertion failure. Mutates isolated test storage and editor state, with no physical device operations.
async fn run(ui: Ui) -> Result<(), JsValue> {
    progress("RUN IDB blocked upgrade and retry").await;
    idb::browser_cases().await?;
    progress("RUN IDB abort").await;
    // A successful request followed by abort must never masquerade as a commit.
    let db = idb::database().await?;
    let tx = db.transaction_with_str_and_mode(idb::PROJECTS, web_sys::IdbTransactionMode::Readwrite)?;
    tx.object_store(idb::PROJECTS)?
        .put_with_key(&"old".into(), &"abort-case".into())?;
    idb::settled(&tx).await?;
    let tx = db.transaction_with_str_and_mode(idb::PROJECTS, web_sys::IdbTransactionMode::Readwrite)?;
    let put = tx
        .object_store(idb::PROJECTS)?
        .put_with_key(&"new".into(), &"abort-case".into())?;
    let abort_tx = tx.clone();
    let abort = Closure::<dyn FnMut(web_sys::Event)>::new(move |_| abort_tx.abort().unwrap());
    put.set_onsuccess(Some(abort.as_ref().unchecked_ref()));
    check(
        idb::settled(&tx).await.is_err(),
        "aborted transaction reported committed",
    )?;
    put.set_onsuccess(None);
    let tx = db.transaction_with_str(idb::PROJECTS)?;
    let value = idb::request(&tx.object_store(idb::PROJECTS)?.get(&"abort-case".into())?).await?;
    check(
        value.as_string().as_deref() == Some("old"),
        "abort replaced committed bytes",
    )?;
    db.close();
    progress("RUN project_io").await;
    project_io::browser_cases(ui).await?;
    progress("RUN originals").await;
    originals::browser_cases().await?;
    progress("RUN helper").await;
    helper::browser_cases().await?;
    progress("RUN startup_artwork").await;
    startup_artwork::browser_cases(ui).await?;
    progress("RUN canvas failure").await;
    // Actual canvas method throws; the production adapter must revoke hit targets.
    ui.write_canvas(&vec![0; 320 * 240 * 4], Geometry::Landscape, PreviewMode::Appearance)
        .map_err(|_| JsValue::from_str("healthy canvas write failed"))?;
    check(
        ui.canvas_current(),
        "healthy canvas did not publish interaction targets",
    )?;
    let canvas = ui.canvas.get_untracked().unwrap();
    let context = canvas.get_context("2d")?.unwrap();
    let previous = js_sys::Reflect::get(&context, &"putImageData".into())?;
    let failure =
        Closure::<dyn FnMut() -> Result<(), JsValue>>::new(|| Err(JsValue::from_str("injected canvas write failure")));
    js_sys::Reflect::set(&context, &"putImageData".into(), failure.as_ref())?;
    let result = ui.write_canvas(&vec![0; 320 * 240 * 4], Geometry::Landscape, PreviewMode::Appearance);
    js_sys::Reflect::set(&context, &"putImageData".into(), &previous)?;
    check(result == Err(RenderIssue::CanvasDraw), "canvas failure was swallowed")?;
    check(!ui.canvas_current(), "failed pixels published interactive targets")?;
    progress("RUN DOM repair").await;
    dom_cases(ui).await?;
    progress("RUN BLE DOM and protocol fixtures").await;
    super::ble_browser_tests::run(ui).await?;
    progress("RUN occupancy geometry").await;
    occupancy_cases(ui).await?;
    Ok(())
}
/// Exercises actual input/change wiring on `ui`, preserving accepted state during invalid text and repairing export
/// admission; returns DOM/assertion errors. Panics when expected startup fixtures are missing.
async fn dom_cases(ui: Ui) -> Result<(), JsValue> {
    let doc = web_sys::window().unwrap().document().unwrap();
    let input = doc
        .query_selector(".view-controls label.field input")?
        .ok_or(JsValue::from_str("view name input missing"))?
        .dyn_into::<web_sys::HtmlInputElement>()?;
    let accepted = ui.editor.with_untracked(Editor::accepted_config_key);
    input.focus()?;
    check(
        doc.active_element()
            .is_some_and(|active| active.is_same_node(Some(input.as_ref()))),
        "view field did not receive focus",
    )?;
    input.set_value("THIS VIEW NAME IS FAR TOO LONG TO BE VALID");
    input.dispatch_event(&web_sys::Event::new("input")?)?;
    tick().await;
    check(
        ui.editor.with_untracked(Editor::has_invalid_draft),
        "DOM invalid input did not retain draft",
    )?;
    check(
        ui.editor.with_untracked(Editor::accepted_config_key) == accepted,
        "invalid input changed accepted configuration",
    )?;
    check(ui.snapshot_blocked(), "invalid DOM input enabled export")?;
    input.set_value("Repaired");
    input.dispatch_event(&web_sys::Event::new("input")?)?;
    input.blur()?;
    tick().await;
    check(
        !ui.editor.with_untracked(Editor::has_invalid_draft),
        "DOM repair did not clear invalid draft",
    )?;
    check(!ui.snapshot_blocked(), "DOM repair left export blocked")?;
    let picker = doc
        .query_selector("select[aria-label='Startup animation']")?
        .unwrap()
        .dyn_into::<web_sys::HtmlSelectElement>()?;
    for tag in [6u8, 7, 8, 9, 10, 0] {
        picker.set_value(&tag.to_string());
        picker.dispatch_event(&web_sys::Event::new("change")?)?;
        tick().await;
        check(
            ui.editor.with_untracked(|e| e.startup_profile().theme.wire_tag()) == tag,
            "startup picker mapped a wire tag to the wrong theme",
        )?;
    }
    Ok(())
}

/// Changes `ui` orientation and checks the rendered occupancy grid after each edit; returns rejected-edit,
/// missing-table or DOM/assertion errors.
async fn occupancy_cases(ui: Ui) -> Result<(), JsValue> {
    let doc = web_sys::window().unwrap().document().unwrap();
    for (orientation, grid) in [(Orientation::Portrait, "2 × 4"), (Orientation::Landscape, "4 × 2")] {
        check(
            ui.run(|editor| editor.apply(Edit::SetOrientation(orientation))),
            "orientation edit rejected",
        )?;
        tick().await;
        let table = doc
            .query_selector("[aria-label='View occupancy']")?
            .ok_or(JsValue::from_str("occupancy table missing"))?;
        check(
            table.text_content().unwrap_or_default().contains(grid),
            "occupancy table reported wrong grid",
        )?;
    }
    Ok(())
}
