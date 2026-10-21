//! Separate best-effort relay lease. No polling, job slots, raw text or recursive notices.
use std::time::Duration;

use dashboard_configurator::diagnostic_relay::Queue;
use dashboard_helper_protocol::decode;
use dashboard_helper_protocol::diagnostics::Response;
use leptos::leptos_dom::helpers::{TimeoutHandle, set_timeout_with_handle};
use wasm_bindgen_futures::spawn_local;

use super::*;
#[derive(Clone, Copy)]
/// Mount-owned optional helper relay resources; one abortable request and deadline at a time.
pub struct State {
    /// Session-only explicit relay opt-in.
    enabled: RwSignal<bool>,
    /// Pure bounded queue and dispatched-lease accounting.
    queue: StoredValue<Queue>,
    /// Saturating reactive invalidation tick for nonreactive queue mutations.
    changed: RwSignal<u64>,
    /// Abort controller for the current network request, retained until settlement.
    abort: StoredValue<Option<web_sys::AbortController>, LocalStorage>,
    /// Two-second deadline for the current request.
    timer: StoredValue<Option<TimeoutHandle>>,
    /// Bounded static relay status text, never recursively added to the event store.
    message: RwSignal<&'static str>,
}
impl State {
    /// Returns disabled relay state in the current Leptos owner; installation attaches lifecycle cleanup separately.
    pub fn new() -> Self {
        Self {
            enabled: RwSignal::new(false),
            queue: StoredValue::new(Queue::default()),
            changed: RwSignal::new(0),
            abort: StoredValue::new_local(None),
            timer: StoredValue::new(None),
            message: RwSignal::new("Off; browser diagnostics stay local."),
        }
    }

    /// Notifies reactive displays of queue changes, ignoring disposed owners and saturating the tick.
    fn changed(self) { let _ = self.changed.try_update(|n| *n = n.saturating_add(1)); }

    /// Applies helper `epoch` and requested `enabled` state, aborting an in-flight network request on change.
    /// Queue ownership remains until settlement; changed state updates bounded status text.
    fn synchronize(
        self,
        epoch: Option<u64>,
        enabled: bool,
    ) {
        let mut changed = false;
        self.queue.update_value(|q| changed = q.synchronize(epoch, enabled));
        if changed {
            if let Some(abort) = self.abort.get_value() {
                abort.abort();
            }
            self.message.set(if !enabled {
                "Off; browser diagnostics stay local."
            } else if epoch.is_none() {
                "Unavailable: connect the local helper to forward context."
            } else {
                "Ready; only newly admitted Warn/Error context is forwarded."
            });
            self.changed();
        }
    }

    /// Attaches `ui` helper/opt-in effects and mount cleanup; changes flush admitted queued work.
    /// Cleanup aborts fetch, clears deadline and disables/discards queued events without claiming delivery.
    pub fn install(
        self,
        ui: Ui,
    ) {
        Effect::new(move |_| {
            let epoch = helper::relay_epoch(ui);
            let enabled = self.enabled.get();
            self.synchronize(epoch, enabled);
            flush(ui);
        });
        on_cleanup(move || {
            if let Some(Some(abort)) = self.abort.try_get_value() {
                abort.abort();
            }
            if let Some(Some(timer)) = self.timer.try_get_value() {
                timer.clear();
            }
            let _ = self.queue.try_update_value(|q| {
                q.synchronize(None, false);
            });
        });
    }
}
/// Offers local `event` using `ui`'s accepted revision/workspace and current helper epoch, then attempts one flush.
/// Queue policy admits only bounded allowlisted Warn/Error context; local message/subject text is not sent.
pub fn offer(
    ui: Ui,
    event: &dashboard_configurator::events::Event,
) {
    let state = ui.relay;
    let epoch = helper::relay_epoch(ui);
    let enabled = state.enabled.get_untracked();
    let revision = ui.editor.with_untracked(|e| e.accepted_revision().get());
    let workspace = ui.workspace.with_untracked(|s| s.active());
    let tick = now().max(0.).min(u64::MAX as f64) as u64;
    state.synchronize(epoch, enabled);
    state.queue.update_value(|q| q.offer(event, revision, workspace, tick));
    state.changed();
    flush(ui);
}
/// Starts at most one queued batch for `ui` with a two-second abort deadline.
/// Setup/network/decode/timeout failures settle as dropped without retry. Completion updates current-generation
/// status only, releases request resources, and starts the next batch; disposed owners stop publication.
fn flush(ui: Ui) {
    let state = ui.relay;
    let mut prepared = None;
    let _ = state.queue.try_update_value(|q| prepared = q.begin());
    let Some((lease, body)) = prepared else {
        return;
    };
    let Ok(abort) = web_sys::AbortController::new() else {
        state.queue.update_value(|q| {
            q.settle(lease, None);
        });
        state
            .message
            .set("Relay unavailable; context dropped, browser history retained.");
        state.changed();
        return;
    };
    let cancel = abort.clone();
    let Ok(timer) = set_timeout_with_handle(move || cancel.abort(), Duration::from_secs(2)) else {
        state.queue.update_value(|q| {
            q.settle(lease, None);
        });
        state
            .message
            .set("Relay timeout could not be scheduled; context dropped.");
        state.changed();
        return;
    };
    state.abort.set_value(Some(abort.clone()));
    state.timer.set_value(Some(timer));
    state.message.set("Sending bounded diagnostic context…");
    state.changed();
    spawn_local(async move {
        let result = helper::relay_post(ui, lease.epoch, &body, &abort)
            .await
            .ok()
            .and_then(|bytes| decode::<Response>(&bytes).ok())
            .filter(|r| u16::from(r.admitted) + u16::from(r.dropped) == u16::from(lease.count));
        timer.clear();
        let mut current = false;
        if state
            .queue
            .try_update_value(|q| current = q.settle(lease, result))
            .is_none()
        {
            return;
        }
        let _ = state.abort.try_set_value(None);
        let _ = state.timer.try_set_value(None);
        if current {
            state.message.set(if result.is_some() {
                "Helper acknowledged context admission; terminal display is best effort."
            } else {
                "Relay request failed or timed out; batch dropped without replay. Browser history retained."
            });
        }
        state.changed();
        flush(ui);
    });
}
#[component]
/// Returns relay opt-in, disclosure and queue/admission/drop status controls for `ui`.
pub fn Controls(
    /// Persistent Studio owner containing relay state and helper connection.
    ui: Ui
) -> impl IntoView {
    let state = ui.relay;
    view! {<div class="event-console">
        <label><input type="checkbox" prop:checked=move||state.enabled.get() on:change=move|event|state.enabled.set(event_target_checked(&event))/><span>"Forward Warn/Error context to local helper terminal (this session)"</span></label>
        <p class="hint">"Sends only severity, category, approved code, workspace, event sequence, client time and accepted revision. Messages, subjects, configuration and artwork stay in the browser. The Trunk terminal cannot receive browser events directly."</p>
        <p class="hint" role="status">{move||{state.changed.get();state.queue.with_value(|q|format!("{} · {} queued · {} admitted · {} dropped{}",state.message.get(),q.queued(),q.admitted,q.dropped,if q.busy(){" · one request pending"}else{""}))}}</p>
    </div>}
}
