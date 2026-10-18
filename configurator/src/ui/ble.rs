//! Configuration shares the normal transactional editor and field navigation.
use super::*;
fn change(
    ui: Ui,
    token: FieldToken,
    f: impl FnOnce(&mut BleSettings),
) {
    let mut settings = ui.editor.with_untracked(|e| e.editing_config().ble);
    f(&mut settings);
    ui.typed(token, Edit::SetBle(settings));
}
#[component]
pub fn BlePanel(ui: Ui) -> impl IntoView {
    let settings = move || ui.editor.with(|e| Registry::with_candidate(e, |c| c.ble));
    view! {
        <div class="settings-content"><div class="settings-copy">
            <h3>"USB power. Bluetooth data."</h3>
            <p>"Keep the Pico 2 W, PIM715 display and encoder. The BLE prototype replaces the wired Longan connection. USB supplies the dashboard; switching USB off does not switch off an OBD adapter."</p>
            <p class="hint">"MC+ and CX profiles currently initialize and identify the adapter only. ECU polling remains blocked until the exact ECU and diagnostic route are qualified. Selecting a profile does not establish hardware compatibility."</p>
            <p class="hint">"Test peer uses a dedicated synthetic GATT service and never sends vehicle requests. Browser preview and simulation use synthetic data; they cannot report a real BLE connection."</p>
            <button on:click=move |_|workspaces::open(ui,Workspace::Diagnostics,GroupId::Simulation)>"Open synthetic BLE diagnostics"</button>
            <p role="status" class="hint">"Physical BLE status: unavailable in Studio. Inspect the Pico diagnostics during an authorized device trial."</p>
            <p class="hint">{move || if settings().profile == BleProfile::ObdLinkCx {"OBDLink CX requires an encrypted link. Prototype bonds are volatile: re-pair after reboot. No PIN or bond key is stored in this project."} else {"Peer selection is explicit. Automatic discovery and rotating private addresses are unsupported in this prototype. No PIN or bond key is stored in this project."}}</p>
        </div><div class="settings-form">
            <Toggle ui repair_invalid_draft=true label="Enable BLE prototype" id=Registry::dom_id(FieldKey::BleEnabled,TargetOwner::Root) value=Signal::derive(move ||settings().enabled) on_change=Callback::new(move |(token,value)|change(ui,token,|s|s.enabled=value))/>
            <Choice ui repair_invalid_draft=true label="BLE profile" id=Registry::dom_id(FieldKey::BleProfile,TargetOwner::Root) value=Signal::derive(move ||format!("{:?}",settings().profile)) options=choices(&[("VlinkerMcPlus","vLinker MC+ · adapter identity only"),("ObdLinkCx","OBDLink CX · encrypted, identity only"),("TestPeer","Synthetic test peer · no car traffic")]) on_change=Callback::new(move |(token,value):(FieldToken,String)|change(ui,token,|s|s.profile=match value.as_str(){"ObdLinkCx"=>BleProfile::ObdLinkCx,"TestPeer"=>BleProfile::TestPeer,_=>BleProfile::VlinkerMcPlus}))/>
            <TextField ui field=RawField::Ble(BleField::Address) label="BLE peer address" hint="Six hexadecimal pairs in display order, e.g. C1:02:03:04:05:06. Use the actual peer address. All zero is allowed only while disabled."/>
            <Choice ui repair_invalid_draft=true label="BLE address kind" id=Registry::dom_id(FieldKey::BleAddressKind,TargetOwner::Root) value=Signal::derive(move ||format!("{:?}",settings().peer.kind)) options=choices(&[("Public","Public"),("RandomStatic","Random static")]) on_change=Callback::new(move |(token,value):(FieldToken,String)|change(ui,token,|s|s.peer.kind=if value=="RandomStatic"{BleAddressKind::RandomStatic}else{BleAddressKind::Public}))/>
            <Toggle ui repair_invalid_draft=true label="Reconnect automatically" id=Registry::dom_id(FieldKey::BleReconnect,TargetOwner::Root) value=Signal::derive(move ||settings().auto_reconnect) on_change=Callback::new(move |(token,value)|change(ui,token,|s|s.auto_reconnect=value))/>
            <fieldset><legend>"Advanced connection timing"</legend>
                <TextField ui field=RawField::Ble(BleField::ConnectTimeout) label="Connection timeout (ms)" hint="See the shared bounds below."/><p class="hint">{format!("{}–{}; connection attempt deadline",BLE_CONNECT_TIMEOUT_MIN_MS,BLE_CONNECT_TIMEOUT_MAX_MS)}</p>
                <TextField ui field=RawField::Ble(BleField::ResponseTimeout) label="Response timeout (ms)" hint="See the shared bounds below."/><p class="hint">{format!("{}–{}; whole response deadline, including fragments",BLE_RESPONSE_TIMEOUT_MIN_MS,BLE_RESPONSE_TIMEOUT_MAX_MS)}</p>
                <TextField ui field=RawField::Ble(BleField::PollGap) label="Minimum command gap (ms)" hint="See the shared bounds below."/><p class="hint">{format!("{}–{}; minimum spacing, not a requested sensor sample rate",BLE_POLL_GAP_MIN_MS,BLE_POLL_GAP_MAX_MS)}</p>
                <p class="hint">"Inactive settings remain stored and validated. A shorter gap cannot override source qualification, freshness limits or an outstanding response."</p>
            </fieldset>
        </div></div>
    }
}

#[component]
pub fn BleSimulationPanel(ui: Ui) -> impl IntoView {
    use dashboard_configurator::ble_simulator::BleSimulator;
    let sim = RwSignal::new_local(
        ui.editor
            .with_untracked(|e| BleSimulator::new(e.preview().config().ble)),
    );
    let key = StoredValue::new(ui.editor.with_untracked(Editor::accepted_config_key));
    let notice =
        RwSignal::new("Enable BLE and select an explicit peer in Device to exercise this synthetic model.".to_owned());
    Effect::new(move |_| {
        ui.editor.with(|e| {
            if key.get_value() != e.accepted_config_key() {
                key.set_value(e.accepted_config_key());
                sim.set(BleSimulator::new(e.preview().config().ble));
                notice.set("Synthetic BLE session reset for the accepted configuration.".into());
            }
        });
    });
    let blocked = move || {
        ui.invalid()
            || ui
                .editor
                .with(|e| e.has_pending_json() || !e.preview().config().ble.enabled)
    };
    let act = move |action: fn(&mut BleSimulator) -> Result<(), dashboard_core::ble::session::Error>| {
        if blocked() || key.get_value() != ui.editor.with_untracked(Editor::accepted_config_key) {
            return;
        }
        sim.update(|s| {
            let result = action(s);
            let snapshot = s.snapshot();
            let message = format!(
                "Synthetic BLE: {:?}; epoch {}; elapsed {} ms; {:?}",
                snapshot.state,
                snapshot.epoch,
                s.now_ms(),
                result
            );
            notice.set(message.clone());
            ui.event(Notice::info(Category::Simulation, "simulation.ble", None, message));
        });
    };
    view! { <fieldset><legend>"Synthetic BLE protocol diagnostics"</legend>
        <p class="hint">"No browser Bluetooth connection is opened. These explicit fixture actions exercise the shared firmware parser, deadlines and epoch checks. Fixture milliseconds are simulated, not measured radio performance. MC+/CX fixture success does not prove pairing or device compatibility."</p>
        <p role="status">{move ||notice.get()}</p>
        <p>{move ||sim.with(|s|format!("SYNTHETIC · {:?} · {} completed · {} lost notifications · last fixture response {:?} ms · pending {}",s.snapshot().state,s.snapshot().completed_transactions,s.snapshot().lost_notifications,s.snapshot().last_latency_ms,s.busy()))}</p>
        <div class="button-row">
            <button disabled=blocked on:click=move |_|act(BleSimulator::connect)>"Connect synthetic peer"</button>
            <button disabled=move ||blocked() ||sim.with(|s|!s.connected() ||s.busy()) on:click=move |_|act(BleSimulator::begin_reply)>"Begin fixture reply"</button>
            <button disabled=move ||blocked() ||!sim.with(BleSimulator::busy) on:click=move |_|act(BleSimulator::complete_fragmented)>"Deliver fragmented reply"</button>
            <button disabled=move ||blocked() ||!sim.with(BleSimulator::busy) on:click=move |_|act(BleSimulator::lose_notification)>"Inject notification loss"</button>
            <button disabled=move ||blocked() ||!sim.with(BleSimulator::busy) on:click=move |_|act(BleSimulator::expire_reply)>"Expire response deadline"</button>
            <button disabled=move ||blocked() ||!sim.with(BleSimulator::busy) on:click=move |_|act(BleSimulator::stale_reply)>"Inject old epoch reply"</button>
            <button disabled=move ||blocked() ||!sim.with(BleSimulator::connected) on:click=move |_|act(|s|{s.disconnect();Ok(())})>"Disconnect fixture"</button>
        </div>
        <p class="hint">"After a fault, reconnect creates a new epoch and repeats initialization. No fixture samples are published as vehicle measurements. Automatic radio reconnect and encryption require separate device testing."</p>
    </fieldset> }
}
