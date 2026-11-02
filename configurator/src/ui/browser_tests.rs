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
    progress("RUN inspected widget geometry").await;
    inspected_geometry_cases(ui).await?;
    Ok(())
}

/// Exercises Find and actual axis-change DOM handlers for a widget whose view may differ from selection.
/// Both shapes, current selection and retained invalid placement must expose their own candidate grid;
/// valid repairs and duplication use the widget's view, while orphan JSON stays read-only.
/// Replaces `ui` with isolated in-memory fixtures and returns the first failure.
async fn inspected_geometry_cases(ui: Ui) -> Result<(), JsValue> {
    use dashboard_configurator::config::View;
    use dashboard_configurator::workspace::Command;

    let doc = web_sys::window().unwrap().document().unwrap();
    for (selected_shape, widget_shape, columns, rows) in [
        (Orientation::Landscape, Orientation::Portrait, 2_u8, 4_u8),
        (Orientation::Portrait, Orientation::Landscape, 4_u8, 2_u8),
    ] {
        for state in 0..3 {
            let mut raw = default_config();
            raw.orientation = selected_shape;
            raw.widgets.clear();
            raw.views
                .push(View {
                    id: 2,
                    name: "Inspected".try_into().unwrap(),
                    orientation: Some(widget_shape),
                })
                .unwrap();
            raw.widgets
                .push(default_widget(
                    Channel::Oil,
                    41,
                    2,
                    Placement {
                        column: 0,
                        row: 0,
                        span: Span::OneByOne,
                    },
                ))
                .unwrap();
            let mut editor = Editor::new(raw).unwrap();
            if state == 1 {
                editor
                    .select(Selection {
                        view: 2,
                        widget: Some(41),
                    })
                    .unwrap();
            } else if state == 2 {
                check(
                    editor
                        .apply(Edit::MoveWidget {
                            id: 41,
                            view: 2,
                            column: 0,
                            row: 99,
                        })
                        .is_err(),
                    "invalid placement fixture unexpectedly admitted",
                )?;
            }
            let selected = editor.selection();
            ui.inspection.set(None);
            ui.error_inspection.set(None);
            ui.editor.set(editor);
            tick().await;
            let target = ui
                .editor
                .with_untracked(|e| Registry::target(e, FieldKey::Row, TargetOwner::Widget(0)))
                .map_err(|_| JsValue::from_str("inspection target unavailable"))?;
            check(
                command_palette::execute_current(ui, Command::Find(target)),
                "Find widget failed",
            )?;
            tick().await;
            check(
                ui.editor.with_untracked(Editor::selection) == selected,
                "Find changed editor selection",
            )?;
            if selected_shape == Orientation::Landscape && state == 0 {
                progress("CHECK inspected portrait widget").await;
            }
            for (key, count) in [(FieldKey::Row, rows), (FieldKey::Column, columns)] {
                let select = doc
                    .get_element_by_id(&Registry::dom_id(key, TargetOwner::Widget(0)))
                    .ok_or(JsValue::from_str("inspected axis selector missing"))?
                    .dyn_into::<web_sys::HtmlSelectElement>()?;
                check(!select.disabled(), "inspected axis repair is disabled")?;
                let options = select.query_selector_all("option")?;
                let retained = u32::from(state == 2 && key == FieldKey::Row);
                check(
                    options.length() == u32::from(count) + retained,
                    "inspected axis options used selected view geometry",
                )?;
                for index in 0..u32::from(count) {
                    let option = options.item(index).unwrap().dyn_into::<web_sys::Element>()?;
                    check(
                        option.get_attribute("value").as_deref() == Some(index.to_string().as_str()),
                        "inspected axis omitted a valid coordinate",
                    )?;
                    check(
                        !option.text_content().unwrap_or_default().contains("outside grid"),
                        "valid inspected coordinate marked outside grid",
                    )?;
                }
                select.set_value(&(count - 1).to_string());
                select.dispatch_event(&web_sys::Event::new("change")?)?;
                tick().await;
            }
            check(
                ui.editor.with_untracked(|e| {
                    !e.has_invalid_draft()
                        && e.preview().config().widgets[0].placement
                            == Placement {
                                column: columns - 1,
                                row: rows - 1,
                                span: Span::OneByOne,
                            }
                }),
                "inspected valid move or retained repair did not commit",
            )?;
        }
    }
    for (selected_full, target_full) in [(true, false), (false, false), (true, true), (false, true)] {
        let mut raw = default_config();
        if !selected_full {
            raw.widgets.clear();
        }
        raw.views
            .push(View {
                id: 2,
                name: "Duplicate".try_into().unwrap(),
                orientation: Some(Orientation::Portrait),
            })
            .unwrap();
        raw.widgets
            .push(default_widget(
                Channel::Oil,
                41,
                2,
                Placement {
                    column: 0,
                    row: 0,
                    span: Span::OneByOne,
                },
            ))
            .unwrap();
        let target_index = raw.widgets.len() - 1;
        if target_full {
            for slot in 1..8_u8 {
                raw.widgets
                    .push(default_widget(
                        Channel::Oil,
                        41 + u16::from(slot),
                        2,
                        Placement {
                            column: slot % 2,
                            row: slot / 2,
                            span: Span::OneByOne,
                        },
                    ))
                    .unwrap();
            }
        }
        let before = raw.widgets.len();
        ui.inspection.set(None);
        ui.error_inspection.set(None);
        ui.editor.set(Editor::new(raw).unwrap());
        tick().await;
        let target = ui
            .editor
            .with_untracked(|e| Registry::target(e, FieldKey::Row, TargetOwner::Widget(target_index)))
            .map_err(|_| JsValue::from_str("duplicate inspection target unavailable"))?;
        check(
            command_palette::execute_current(ui, Command::Find(target)),
            "Find duplicate target failed",
        )?;
        tick().await;
        let button = doc
            .query_selector("#studio-widget-inspector .widget-actions button")?
            .ok_or(JsValue::from_str("visible duplicate button missing"))?
            .dyn_into::<web_sys::HtmlElement>()?;
        check(
            button.text_content().as_deref() == Some("Duplicate widget"),
            "wrong duplicate action target",
        )?;
        let before_key = ui.editor.with_untracked(Editor::accepted_config_key);
        let before_selection = ui.editor.with_untracked(Editor::selection);
        button.click();
        tick().await;
        check(
            ui.editor.with_untracked(|e| {
                !e.has_invalid_draft()
                    && if target_full {
                        e.accepted_config_key() == before_key && e.selection() == before_selection
                    } else {
                        e.preview().config().widgets.len() == before + 1
                            && e.preview().config().widgets.iter().filter(|w| w.view == 2).count() == 2
                    }
            }),
            "inspected duplication searched the selected view",
        )?;
    }
    // An orphan in an unapplied JSON candidate stays inspectable without borrowing
    // the selected view's otherwise valid grid or gaining typed edit authority.
    let mut editor = Editor::default();
    let mut orphan = default_config();
    orphan.widgets.truncate(1);
    orphan.widgets[0].view = 99;
    let handle = editor.open_json().unwrap();
    let parse = editor
        .set_json_text(handle, serde_json::to_string(&orphan).unwrap())
        .unwrap();
    editor.finish_json_parse(parse).unwrap();
    ui.inspection.set(None);
    ui.error_inspection.set(None);
    ui.editor.set(editor);
    tick().await;
    let target = ui
        .editor
        .with_untracked(|e| Registry::target(e, FieldKey::Row, TargetOwner::Widget(0)))
        .map_err(|_| JsValue::from_str("orphan inspection target unavailable"))?;
    check(
        command_palette::execute_current(ui, Command::Find(target)),
        "Find orphan failed",
    )?;
    tick().await;
    check(ui.inspect_read_only(), "unapplied orphan gained typed edit authority")?;
    check(
        ui.inspected_widget().is_none(),
        "orphan gained unambiguous widget authority",
    )?;
    check(
        ui.editor.with_untracked(Editor::has_pending_json),
        "orphan JSON was discarded",
    )?;
    for key in [FieldKey::Row, FieldKey::Column] {
        check(
            doc.get_element_by_id(&Registry::dom_id(key, TargetOwner::Widget(0)))
                .is_none(),
            "orphan borrowed another view's editable grid",
        )?;
    }
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
