//! Explicit, selected cleanup; protected and incomplete attempts stay read-only.
use dashboard_helper_protocol::BuildAttemptState;

use super::*;

/// Requests cleanup of attempt `id` in `ui` only when operations are available and
/// the latest admitted storage view still grants a cleanup ID. Otherwise does nothing;
/// the helper rechecks ownership/protection before any filesystem removal.
fn remove(
    ui: Ui,
    id: &str,
) {
    if operation_unavailable(ui) {
        return;
    }
    let Some(id) = ui.helper.view.with_untracked(|v| v.storage.as_ref()?.cleanup_id(id)) else {
        return;
    };
    ui.helper.removing_build.set_value(Some(id));
    send(ui, Action::CleanupBuild);
}

/// Returns `ui`'s retained-build inventory and per-attempt confirmation controls.
/// Protected/incomplete attempts stay read-only; deletion requires explicit confirmation
/// and a fresh cleanup check. Rendering the panel does not fetch inventory or remove files.
#[component]
pub fn Panel(
    /// Shared helper storage inspection, cleanup selection and confirmation state.
    ui: Ui
) -> impl IntoView {
    view! {<details class="build-disclosure"><summary>"Build storage"</summary>
        <p>"Review the files retained by local builds. Removing an attempt frees its compiler output and saved inputs; your open Studio project and the Pico are unchanged."</p>
        <div class="helper-actions"><button disabled=move ||operation_unavailable(ui) on:click=move |_|send(ui,Action::Storage)>"Refresh build storage"</button></div>
        {move ||ui.helper.view.with(|v|v.storage.clone()).map(|storage| {
            let message=storage.explanation().to_string();
            view! {<p class="build-note" role="status">{message}</p>{if let BuildStorageView::Ready(report)=storage {
                let bytes=report.attempts.iter().map(|a|a.bytes).sum::<u64>();
                Some(view! {<p>{format!("{} attempts · {:.1} MiB retained",report.attempts.len(),bytes as f64/1048576.0)}</p>
                    {report.attempts.iter().map(|attempt| {
                        let confirming=RwSignal::new(false);
                        let id=attempt.attempt_id.as_str().to_string();
                        let closed=attempt.state!=BuildAttemptState::Incomplete;
                        let eligible=!attempt.protected&&closed;
                        let label=match attempt.state {BuildAttemptState::Artifact=>"Successful build",BuildAttemptState::Failed=>"Failed build",BuildAttemptState::Incomplete=>"Interrupted · review required"};
                        view! {<div class="helper-artifact"><strong>{label}</strong><span class="chip">{format!("{:.1} MiB",attempt.bytes as f64/1048576.0)}</span>
                            <code>{attempt.attempt_id.as_str().to_string()}</code>
                            {(attempt.protected&&attempt.state==BuildAttemptState::Artifact).then(||view!{<small>"In use by a retained firmware file. Download what you need, then restart the helper to release its session. Rebuild before flashing afterward."</small>})}
                            {(!closed).then(||view!{<small>"Process closure was not confirmed. Studio will not delete this attempt automatically."</small>})}
                            <div class="helper-actions" hidden=move ||confirming.get()><button disabled=move ||!eligible||operation_unavailable(ui) on:click=move |_|confirming.set(true)>"Remove build files"</button></div>
                            <div class="build-caution" hidden=move ||!confirming.get()><p>"Permanently remove this attempt's local build files and diagnostic evidence? Download any logs or firmware you want to keep first."</p>
                                <div class="helper-actions"><button disabled=move ||!eligible||operation_unavailable(ui) on:click=move |_|{confirming.set(false);remove(ui,&id);}>"Confirm removal"</button><button on:click=move |_|confirming.set(false)>"Keep files"</button></div>
                            </div>
                        </div>}
                    }).collect_view()}
                })
            }else{None}}
            }
        })}
        <small class="build-note">"Limits: 16 successful attempts / 8 GiB and four failed records. A new build reserves up to 4 GiB of scratch space plus free-space headroom, so admission can stop before the retained limit is full. Unknown files are kept for manual review."</small>
    </details>}
}
