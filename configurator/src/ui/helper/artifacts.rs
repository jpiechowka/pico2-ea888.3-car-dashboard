//! Downloads preserve the identity of a completed native build across later edits.
use super::*;

#[component]
pub fn Panel(ui: Ui) -> impl IntoView {
    let state = ui.helper;
    let cache = StoredValue::new(dashboard_configurator::helper_client::ArtifactDisplayCache::default());
    let current = Memo::new(move |_| {
        ui.editor.with(|editor| {
            ui.build.with(|selection| {
                let mut identity = None;
                cache.update_value(|cache| identity = cache.inspect(editor, selection).ok());
                identity
            })
        })
    });
    view! {
        <details class="build-disclosure" open><summary>"Admitted firmware files"</summary>
            <p>"A successful build appears here only after its image and UF2 checks pass. The helper retains the latest two files for this session. Editing your project leaves older files available and marks them as different."</p>
            <p hidden=move ||state.view.with(|v|!v.artifacts.is_empty())>"No admitted firmware file in this helper session."</p>
            {move ||state.view.with(|v|v.artifacts.clone()).into_iter().map(|artifact| {
                let status=artifact.clone(); let selected=artifact.clone(); let flash=artifact.clone(); let eligible=artifact.clone();
                view! {<div class="helper-artifact"><strong>{format!("{:?} · {} bytes",artifact.options.requested.mode,artifact.uf2_bytes)}</strong>
                    <span class="chip">{move ||if current.with(|p|p.as_ref().is_some_and(|p|p.matches_artifact(&status))){"MATCHES ACCEPTED PROJECT"}else{"DIFFERENT PROJECT / OPTIONS"}}</span>
                    <small>"Image checks passed · physical hardware remains unverified"</small>
                    <dl><dt>"Project SHA256"</dt><dd><code>{artifact.project_sha256.as_str().to_string()}</code></dd><dt>"UF2 SHA256"</dt><dd><code>{artifact.uf2_sha256.as_str().to_string()}</code></dd></dl>
                    <button disabled=move ||operation_unavailable(ui) on:click=move |_|{state.download.set_value(Some(selected.clone()));send(ui,Action::Download);}>"Download verified UF2"</button>
                    <button class="primary" disabled=move ||operation_unavailable(ui)||!current.with(|p|p.as_ref().is_some_and(|p|p.matches_artifact(&eligible)))||state.device.with(|d|d.acknowledgement().is_none())
                        on:click=move |_|{state.flashing.set_value(Some(flash.clone()));send(ui,Action::Flash);}>"Flash selected Pico 2 W"</button>
                    <small>"Flashing requires a matching accepted project and build options, a fresh device inspection, and your board confirmation. It writes, verifies, then requests reboot. Cancellation after writing starts can leave an unknown result."</small>
                </div>}
            }).collect_view()}
        </details>
    }
}
