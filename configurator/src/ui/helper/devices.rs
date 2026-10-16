//! Explicit selection and board confirmation; inspection never confirms capacity.
use dashboard_helper_protocol::DeviceStatus;

use super::*;

fn unavailable(ui: Ui) -> bool {
    let state = ui.helper;
    state.busy.get()
        || state.building.get()
        || !state.view.with(|v| v.connected)
        || state.view.with(|v| v.feed.active())
}

#[component]
pub fn Panel(ui: Ui) -> impl IntoView {
    let state = ui.helper;
    view! {
        {move ||state.view.with(|v|v.devices.clone()).map(|report|view! {
            <details class="build-disclosure" open><summary>{format!("BOOTSEL candidates · {}",report.devices.len())}</summary>
                <p>{report.reason.as_str().to_string()}</p>
                <p>"Hold BOOTSEL while connecting your Pico 2, then choose Find BOOTSEL devices. Inspection reads chip and security information. It does not write firmware or establish the flash capacity."</p>
                {report.devices.iter().map(|device| {
                    let selected=device.clone();
                    let unsupported=device.status!=DeviceStatus::DescriptorOnly;
                    view! {<div class="helper-device"><strong>{device.product.as_ref().map_or("RP2350 BOOTSEL",|p|p.as_str()).to_string()}</strong>
                        <span>{format!("{:04x}:{:04x} · {:?}",device.vid,device.pid,device.status)}</span>
                        <p>{device.reason.as_str().to_string()}</p>
                        <small>{format!("Serial: {}",device.serial.as_ref().map_or("unavailable",|s|s.as_str()))}</small>
                        <button disabled=move ||unsupported||unavailable(ui) on:click=move |_| {
                            let mut result=Ok(());state.device.update(|d|result=d.select(selected.clone()));
                            if let Err(message)=result {state.view.update(|v|v.message=message.into());return;}
                            state.inspecting.set_value(true);send(ui,Action::Inspect);
                        }>"Inspect this device"</button>
                    </div>}
                }).collect_view()}
            </details>
        })}
        {move ||state.device.with(|d|d.inspection().cloned()).map(|report|view! {
            <section class="helper-inspection" aria-label="Selected device inspection">
                <div class="helper-status"><strong>"Selected device"</strong><span class="chip">{if report.complete{"INSPECTED"}else{"INSPECTION FAILED"}}</span></div>
                <p>{report.reason.as_str().to_string()}</p>
                {report.facts.map(|facts|view! {<dl class="build-toolchain"><dt>"Chip"</dt><dd>{format!("RP2350 {} · {}",facts.revision.as_str(),facts.available_cpus.as_str())}</dd>
                    <dt>"Identity"</dt><dd><code>{facts.chip_id.as_str().to_string()}</code></dd>
                    <dt>"Security"</dt><dd>"Secure boot off · debug access enabled"</dd>
                    <dt>"Flash capacity"</dt><dd>"Unverified"<small>"A tool estimate cannot establish installed flash capacity."</small></dd></dl>})}
                <label class="build-toggle helper-confirmation"><input type="checkbox" disabled=move ||unavailable(ui)||!state.device.with(|d|d.inspection().is_some_and(|r|r.complete))
                    prop:checked=move ||state.device.with(|d|d.acknowledgement().is_some()) on:change=move |event| {
                        let mut result=Ok(());state.device.update(|d|result=d.acknowledge(event_target_checked(&event)));
                        if let Err(message)=result {state.view.update(|v|v.message=message.into());}
                    }/><span>"I confirm this selected device is a genuine Raspberry Pi Pico 2 with manufacturer-specified 4 MB flash."</span></label>
                <small class="build-note">"This is your confirmation, not a measurement. Scanning, reconnecting or inspecting again clears it. Firmware keeps the existing 2 MiB layout and settings protection."</small>
            </section>
        })}
    }
}
