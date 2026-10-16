//! Recovery records are observations, never permission to resume programming.
use dashboard_helper_protocol::{FlashPhase, WriteOutcome};

use super::*;

#[component]
pub fn Panel(ui: Ui) -> impl IntoView {
    view! { {move ||ui.helper.view.with(|v|v.history.clone()).map(|history|view! {
        <details class="build-disclosure"><summary>{format!("Flash history · {} recorded attempts",history.attempts.len())}</summary>
            <p>"These records survive a helper restart. They never resume a write or reboot automatically. A past verification describes that attempt; it does not prove what is connected or running now."</p>
            {history.attempts.iter().map(|attempt| {
                let phase=match attempt.phase {FlashPhase::Unpublished=>"Preparation incomplete",FlashPhase::IntentArmed=>"Write intent recorded",FlashPhase::LoadCompleted=>"Transfer finished",FlashPhase::VerificationConfirmed=>"Payload verification recorded",FlashPhase::RebootArmed=>"Reboot intent recorded",FlashPhase::RebootCompleted=>"Reboot request completed",FlashPhase::Interrupted=>"Attempt interrupted"};
                let outcome=match attempt.recovery_write_outcome {WriteOutcome::NotStarted=>"No write intent published",WriteOutcome::Verified=>"Previously verified payload",_=>"Write result unknown"};
                view! {<div class="helper-artifact"><strong>{phase}</strong><span class="chip">{outcome}</span>
                    <dl><dt>"Attempt"</dt><dd><code>{attempt.job_id.as_str().to_string()}</code></dd>
                        {attempt.device_id.as_ref().map(|id|view!{<dt>"Device selection"</dt><dd><code>{id.as_str().to_string()}</code></dd>})}
                        {attempt.exit.map(|exit|view!{<dt>"Process exit"</dt><dd>{format!("{} (0x{:08X})",exit.interpreted_i32,exit.windows_u32)}</dd>})}
                    </dl>
                    <small>"To try again: reconnect in BOOTSEL, scan and inspect the device, confirm its board type, and explicitly choose a matching firmware file."</small>
                </div>}
            }).collect_view()}
        </details>
    })} }
}
