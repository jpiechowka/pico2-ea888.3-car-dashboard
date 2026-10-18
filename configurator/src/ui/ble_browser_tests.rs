//! Actual DOM event wiring, including invalid drafts and synthetic fixture controls.
use wasm_bindgen::JsValue;

use super::browser_tests::tick;
use super::*;

fn check(
    ok: bool,
    message: &str,
) -> Result<(), JsValue> {
    if ok { Ok(()) } else { Err(message.into()) }
}
fn doc() -> web_sys::Document { web_sys::window().unwrap().document().unwrap() }
fn by_id(id: &str) -> Result<web_sys::Element, JsValue> {
    doc()
        .get_element_by_id(id)
        .ok_or_else(|| format!("missing {id}").into())
}
fn click_id(id: &str) -> Result<(), JsValue> {
    by_id(id)?.dyn_into::<web_sys::HtmlElement>()?.click();
    Ok(())
}
fn button(text: &str) -> Result<web_sys::HtmlElement, JsValue> {
    let nodes = doc().query_selector_all("button")?;
    for index in 0..nodes.length() {
        let node = nodes.item(index).unwrap();
        if node.text_content().unwrap_or_default().trim() == text {
            return node.dyn_into::<web_sys::HtmlElement>().map_err(Into::into);
        }
    }
    Err(format!("missing button {text}").into())
}
fn input(key: FieldKey) -> Result<web_sys::HtmlInputElement, JsValue> {
    by_id(&Registry::dom_id(key, TargetOwner::Root))?
        .dyn_into()
        .map_err(Into::into)
}
async fn text(
    key: FieldKey,
    value: &str,
) -> Result<(), JsValue> {
    let input = input(key)?;
    check(!input.disabled(), "BLE repair field unexpectedly disabled")?;
    input.focus()?;
    input.set_value(value);
    input.dispatch_event(&web_sys::Event::new("input")?)?;
    input.blur()?;
    tick().await;
    Ok(())
}
async fn choice(
    key: FieldKey,
    value: &str,
) -> Result<(), JsValue> {
    let select = by_id(&Registry::dom_id(key, TargetOwner::Root))?.dyn_into::<web_sys::HtmlSelectElement>()?;
    check(!select.disabled(), "BLE choice unexpectedly disabled")?;
    select.set_value(value);
    select.dispatch_event(&web_sys::Event::new("change")?)?;
    tick().await;
    Ok(())
}
fn fixture_text() -> String {
    by_id("studio-ble-simulation")
        .unwrap()
        .text_content()
        .unwrap_or_default()
}
async fn click(text: &str) -> Result<(), JsValue> {
    let button = button(text)?;
    check(!button.has_attribute("disabled"), &format!("button disabled: {text}"))?;
    button.click();
    tick().await;
    Ok(())
}
fn fits_viewport(element: &web_sys::Element) -> Result<(), JsValue> {
    let width = doc().document_element().unwrap().client_width() as f64;
    let bounds = element.get_bounding_client_rect();
    check(
        bounds.width() > 0.0 && bounds.left() >= 0.0 && bounds.right() <= width + 1.0,
        &format!(
            "BLE control exceeds viewport: {}..{} of {}",
            bounds.left(),
            bounds.right(),
            width
        ),
    )
}
pub(super) async fn run(ui: Ui) -> Result<(), JsValue> {
    click_id("studio-tab-device")?;
    tick().await;
    fits_viewport(input(FieldKey::BleAddress)?.as_ref())?;
    let before = ui.editor.with_untracked(Editor::accepted_config_key);
    let enable = input(FieldKey::BleEnabled)?;
    enable.set_checked(true);
    enable.dispatch_event(&web_sys::Event::new("change")?)?;
    tick().await;
    check(
        ui.invalid() && ui.snapshot_blocked(),
        "zero peer enabled BLE or unlocked export",
    )?;
    check(
        ui.editor.with_untracked(Editor::accepted_config_key) == before,
        "invalid BLE changed accepted identity",
    )?;
    check(enable.checked(), "rejected enable selection not retained in DOM")?;
    check(
        doc()
            .query_selector(".error-navigation")?
            .is_some_and(|e| !e.has_attribute("hidden")),
        "BLE error navigation hidden",
    )?;
    text(FieldKey::BleAddress, "C1:02:03:04:05:06").await?;
    check(
        !ui.invalid() && !ui.snapshot_blocked(),
        "peer repair did not admit whole draft",
    )?;
    let bytes = ui.editor.with_untracked(|e| e.export_binary().unwrap());
    click("↶ Undo").await?;
    check(!enable.checked(), "Undo did not restore disabled BLE")?;
    click("↷ Redo").await?;
    check(
        ui.editor.with_untracked(|e| e.export_binary().unwrap()) == bytes,
        "Redo changed BLE bytes",
    )?;
    text(FieldKey::BleAddress, "broken").await?;
    text(FieldKey::BlePollGap, "999").await?;
    text(FieldKey::BleAddress, "C1:02:03:04:05:06").await?;
    check(
        ui.invalid() && ui.snapshot_blocked(),
        "MAC repair erased invalid timing",
    )?;
    text(FieldKey::BlePollGap, "20").await?;
    check(!ui.invalid(), "timing repair failed")?;
    text(FieldKey::BleAddress, "41:02:03:04:05:06").await?;
    choice(FieldKey::BleAddressKind, "RandomStatic").await?;
    check(ui.invalid(), "invalid random static address accepted")?;
    text(FieldKey::BleAddress, "C1:02:03:04:05:06").await?;
    check(!ui.invalid(), "static address repair failed")?;
    choice(FieldKey::BleProfile, "ObdLinkCx").await?;
    check(
        by_id("studio-panel-device")?
            .text_content()
            .unwrap()
            .contains("requires an encrypted link"),
        "CX security help absent",
    )?;
    choice(FieldKey::BleProfile, "TestPeer").await?;
    // A deferred shortcut must not steal focus after the user navigates away.
    button("Open synthetic BLE diagnostics")?.click();
    click_id("studio-tab-device")?;
    for _ in 0..4 {
        tick().await;
    }
    check(
        doc().active_element().is_some_and(|e| e.id() == "studio-tab-device"),
        "stale BLE shortcut stole focus",
    )?;
    click("Open synthetic BLE diagnostics").await?;
    for _ in 0..4 {
        tick().await;
    }
    let fixture = by_id("studio-ble-simulation")?;
    check(
        doc().active_element().is_some_and(|e| e.id() == fixture.id()),
        "BLE shortcut did not focus its diagnostics panel",
    )?;
    let top = fixture.get_bounding_client_rect().top();
    let height = web_sys::window().unwrap().inner_height()?.as_f64().unwrap();
    check(
        top >= 0.0 && top < height,
        "BLE shortcut left its panel outside the viewport",
    )?;
    check(
        ui.workspace.with_untracked(|s| s.active()) == Workspace::Diagnostics,
        "BLE diagnostics navigation failed",
    )?;
    fits_viewport(&by_id("studio-ble-simulation")?)?;
    click("Connect synthetic peer").await?;
    check(
        fixture_text().contains("SyntheticReady") && fixture_text().contains("13 completed"),
        "shared initialization fixture failed",
    )?;
    click("Begin fixture reply").await?;
    click("Inject old epoch reply").await?;
    check(
        fixture_text().contains("Err(Epoch)") && fixture_text().contains("pending true"),
        "old epoch mutated current fixture",
    )?;
    click("Deliver fragmented reply").await?;
    check(
        fixture_text().contains("14 completed")
            && fixture_text().contains("pending false")
            && fixture_text().contains("last fixture response 3 ms"),
        "fragment fixture failed",
    )?;
    click("Begin fixture reply").await?;
    click("Inject notification loss").await?;
    check(
        fixture_text().contains("NotificationLoss")
            && fixture_text().contains("pending false")
            && fixture_text().contains("last fixture response unavailable"),
        "notification loss did not retire fixture",
    )?;
    click("Connect synthetic peer").await?;
    click("Begin fixture reply").await?;
    click("Expire response deadline").await?;
    check(fixture_text().contains("Timeout"), "deadline fixture failed")?;
    click("Connect synthetic peer").await?;
    click("Disconnect fixture").await?;
    check(
        fixture_text().contains("Disabled"),
        "disconnect fixture stayed connected",
    )?;
    click("Connect synthetic peer").await?;
    click_id("studio-open-json")?;
    tick().await;
    let area = by_id("studio-json-text")?.dyn_into::<web_sys::HtmlTextAreaElement>()?;
    area.set_value(&(area.value() + " "));
    area.dispatch_event(&web_sys::Event::new("input")?)?;
    tick().await;
    check(
        ui.snapshot_blocked() && button("Connect synthetic peer")?.has_attribute("disabled"),
        "pending JSON leaves BLE fixture active",
    )?;
    click_id("studio-json-discard")?;
    tick().await;
    check(
        !button("Connect synthetic peer")?.has_attribute("disabled"),
        "discarded JSON leaves BLE fixture blocked",
    )?;
    click_id("studio-tab-device")?;
    tick().await;
    text(FieldKey::BlePollGap, "21").await?;
    check(
        fixture_text().contains("0 completed") && fixture_text().contains("reset for the accepted configuration"),
        "accepted edit did not reset BLE fixture",
    )?;
    click_id("studio-tab-build-flash")?;
    tick().await;
    let build = by_id("studio-panel-build-flash")?.text_content().unwrap();
    check(
        build.contains("BLE prototype · wireless adapter")
            && build.contains("Real MC+/CX profiles initialize and identify the adapter only"),
        "BLE build help drifted",
    )?;
    check(
        !build.contains("Longan CAN acquisition"),
        "old transport still selected in build label",
    )?;
    // All fixture state stays synthetic and separate from the accepted project.
    check(
        ui.editor.with_untracked(|e| e.preview().config().ble.profile) == BleProfile::TestPeer,
        "fixture changed configured profile",
    )?;
    Ok(())
}
