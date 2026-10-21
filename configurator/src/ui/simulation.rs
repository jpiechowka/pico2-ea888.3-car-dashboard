//! Persistent simulation adapter; one source, one canvas and exact retained retries.
use dashboard_configurator::event_adapters::{complete_simulation_pixels, consume_simulation_pixels};
use dashboard_configurator::simulator::{CallbackToken, ControlSimulator, Scenario, SimulationInput, WarningFixture};
use dashboard_core::input_model::EncoderBatch;
use leptos::leptos_dom::helpers::{
    request_animation_frame_with_handle,
    set_interval_with_handle,
    window_event_listener_untyped,
};

use super::*;

/// Returns callback authority bound to `ui`'s current gate/source/accepted config lineage, or None after
/// disposal/exhaustion. Capture alone does not synchronize a pending editor change; callbacks must recheck
/// current_callback before effects.
pub(super) fn capture_callback(ui: Ui) -> Option<CallbackToken> {
    let editor = ui.editor.try_with_untracked(Editor::accepted_config_key)?;
    let source = ui.dock.sim.try_with_untracked(ControlSimulator::source_key)?;
    ui.dock
        .callbacks
        .try_with_value(|callbacks| callbacks.gate.capture_for_editor(source, editor))
        .flatten()
}

/// Synchronizes `ui` against actual editor state, then returns whether `callback` still matches source/gate/accepted
/// lineage. Fallible signal access makes disposed callbacks inert; synchronization can cancel old playback/restart
/// source before rejecting.
pub(super) fn current_callback(
    ui: Ui,
    callback: CallbackToken,
) -> bool {
    if ui.dock.callbacks.try_with_value(|_| ()).is_none() {
        return false;
    }
    if !ui
        .editor
        .try_with_untracked(|e| ui.dock.synchronize(ui, e))
        .unwrap_or(false)
    {
        return false;
    }
    let Some(editor) = ui.editor.try_with_untracked(Editor::accepted_config_key) else {
        return false;
    };
    let Some(source) = ui.dock.sim.try_with_untracked(ControlSimulator::source_key) else {
        return false;
    };
    ui.dock
        .callbacks
        .try_with_value(|callbacks| callbacks.gate.is_current_for_editor(callback, source, editor))
        .unwrap_or(false)
}

/// Freezes `ui`'s active elapsed clock, stops playback, clears held/release tracking and retires scheduled callbacks.
/// Resets gesture trackers without synthesizing release taps; disposed signals are ignored and accepted pending pixels
/// remain retained.
pub(super) fn cancel(ui: Ui) {
    freeze_clock(ui);
    let _ = ui.dock.running.try_set(false);
    let _ = ui.dock.held.try_set([false; 4]);
    let _ = ui.dock.release_until.try_set(None);
    let _ = ui.dock.sim.try_update(ControlSimulator::cancel_inputs);
    retire_callbacks(ui);
}

/// Retires `ui`'s callback gate and cancels held animation-frame handles; disposal is a no-op.
fn retire_callbacks(ui: Ui) {
    let _ = ui
        .dock
        .callbacks
        .try_update_value(|callbacks| callbacks.retire(|handle| handle.cancel()));
}

/// Removes `ui`'s playback clock and samples its final active elapsed boundary only for the same source.
/// Cancels gestures before sampling and refuses accepted sampling while pending JSON has not yet installed inspection.
fn freeze_clock(ui: Ui) {
    let mut frozen = None;
    let _ = ui.dock.playback_clock.try_update_value(|clock| {
        if let Some((mut clock, source)) = clock.take() {
            frozen = Some((clock.pause(now()), source));
        }
    });
    if let Some((at, source)) = frozen {
        // Dropping elapsed playback while entering a pending draft must not
        // sample the accepted session before the inspection effect runs.
        if ui.editor.try_with_untracked(Editor::has_pending_json) == Some(true)
            && ui.dock.sim.try_with_untracked(ControlSimulator::invalid) != Some(true)
        {
            return;
        }
        let _ = ui.dock.sim.try_update(|sim| {
            if sim.source_key() == Some(source) {
                // Cancel trackers before observing the final active clock boundary.
                sim.cancel_inputs();
                sim.physical_at([false; 4], EncoderBatch::default(), at);
            }
        });
    }
}

/// Installs `ui` blur/visibility/cleanup cancellation and a source-bound 50-ms playback/held-release timer.
/// Timer advances from elapsed clock, not callback count, and rechecks editor/source authority; creation failure leaves
/// manual steps usable. Requires a reactive owner; cleanup clears timers/listeners and freezes time without fabricating
/// input edges.
pub(super) fn install(ui: Ui) {
    let blur = window_event_listener_untyped("blur", move |_| {
        cancel(ui);
        ui.playing.set(false);
        stage::cancel(ui);
    });
    // visibilitychange bubbles from Document; inspect the actual document state.
    let hidden = window_event_listener_untyped("visibilitychange", move |_| {
        if web_sys::window().and_then(|w| w.document()).is_some_and(|d| d.hidden()) {
            cancel(ui);
            ui.playing.set(false);
            stage::cancel(ui);
        }
    });
    on_cleanup(move || {
        blur.remove();
        hidden.remove();
        cancel(ui);
    });
    let active = Memo::new(move |_| {
        let running = ui.dock.running.get()
            || ui.dock.held.with(|pressed| pressed.iter().any(|b| *b))
            || ui.dock.release_until.get().is_some();
        // Replay can cancel and resume in one event; the boolean alone would
        // coalesce and leave the old, retired timer installed.
        (running, ui.dock.sim.with(ControlSimulator::source_key))
    });
    Effect::new(move |_| {
        let (active, source) = active.get();
        if !active {
            return;
        }
        let Some(token) = capture_callback(ui) else {
            ui.dock.notice.set("Simulation callback identity exhausted".into());
            return;
        };
        let mut clock =
            dashboard_configurator::timing::Playback::new(ui.dock.sim.with_untracked(ControlSimulator::display_now_ms));
        clock.start(now());
        let Some(source) = source else {
            return;
        };
        ui.dock.playback_clock.set_value(Some((clock, source)));
        match set_interval_with_handle(
            move || {
                if !current_callback(ui, token) {
                    return;
                }
                let running = ui.dock.running.get_untracked();
                let held = ui.dock.held.get_untracked();
                let mut at = 0;
                ui.dock.playback_clock.update_value(|clock| {
                    if let Some((clock, _)) = clock {
                        at = clock.sample(now());
                    }
                });
                ui.simulate(|sim| {
                    if running && !sim.invalid() {
                        if let Err(error) = sim.scenario_step(at) {
                            ui.dock.notice.set(format!("Simulation: {error:?}"));
                        }
                    } else {
                        sim.physical_at(held, EncoderBatch::default(), at);
                    }
                });
                if ui.dock.release_until.get_untracked().is_some_and(|until| at >= until) {
                    ui.dock.release_until.set(None);
                }
                if running && ui.dock.mode.get_untracked() == PreviewMode::Simulation && !ui.inspection_active() {
                    submit(ui, false);
                }
            },
            std::time::Duration::from_millis(50),
        ) {
            Ok(handle) => {
                on_cleanup(move || {
                    handle.clear();
                    freeze_clock(ui);
                    retire_callbacks(ui);
                })
            }
            Err(_) => {
                ui.dock.running.set(false);
                ui.dock
                    .notice
                    .set("Simulation timer unavailable; use manual Next sample".into());
            }
        }
    });
}

/// Records `result` for `ui`'s simulation canvas: success publishes consumed/displayed lineage, failure retains exact
/// pending bytes. Updates bounded render diagnostics and notice; does not acknowledge simulator frame completion.
fn canvas_result(
    ui: Ui,
    result: Result<(), RenderIssue>,
) {
    ui.render_issue(RenderLane::Simulation, result);
    match result {
        Ok(()) => {
            ui.dock.consumed.set(true);
            ui.dock.displayed.set(PreviewMode::Simulation);
            ui.dock.invalid_pixels.set(false);
            ui.dock
                .pixels_key
                .set(Some(ui.editor.with_untracked(Editor::accepted_config_key)));
            ui.dock
                .notice
                .set("Synthetic pixels accepted; completion is separate".into());
        }
        Err(error) => {
            ui.dock.consumed.set(false);
            ui.dock
                .notice
                .set(format!("{error}; exact pending pixels retained for Retry"));
        }
    }
}

/// Renders/submits or retries exact pending bytes for `ui`; `manual` selects Simulation mode and suppresses automatic
/// completion. Synchronizes editor first and refuses inspection; consumed pending frames are retained, with automatic
/// completion scheduled unless the dropped-completion fixture is selected. Failures report without issuing a
/// replacement token for retained bytes. Frame-sequence exhaustion refuses new work; successful canvas write remains
/// separate from completed presentation metadata.
pub(super) fn submit(
    ui: Ui,
    manual: bool,
) {
    if manual {
        ui.dock.mode.set(PreviewMode::Simulation);
    }
    ui.editor.with_untracked(|e| ui.dock.synchronize(ui, e));
    if ui.inspection_active() {
        return;
    }
    let mut result = Ok(());
    if let Some(token) = ui.dock.pending.get_untracked() {
        if ui.dock.consumed.get_untracked() {
            if !manual
                && ui
                    .dock
                    .sim
                    .with_untracked(|s| s.projection().scenario != Scenario::DroppedCompletion)
            {
                schedule_completion(ui);
            }
            return;
        }
        ui.dock.sim.with_untracked(|sim| {
            ui.dock.timing.with_untracked(|p| {
                let geometry = p.geometry();
                let fit_issues = p.fit_issues().to_vec();
                result = consume_simulation_pixels(sim, p, token, |pixels| {
                    ui.write_canvas(pixels, geometry, PreviewMode::Simulation).map(|()| {
                        ui.pixel_fit_issues.set(fit_issues.clone());
                    })
                });
            })
        });
    } else {
        let Some(next) = ui.dock.sequence.get_untracked().checked_add(1) else {
            ui.dock.notice.set("Frame identity exhausted".into());
            return;
        };
        let token = dashboard_core::display::pipeline::FrameToken {
            buffer: dashboard_core::display::pipeline::BufferId::A,
            sequence: next,
        };
        ui.simulate(|sim| {
            ui.dock.timing.update(|p| {
                result = (|| {
                    p.render_simulation_submitted(sim, token)
                        .map_err(RenderIssue::Preview)?;
                    ui.dock.sequence.set(next);
                    ui.dock.pending.set(Some(token));
                    let geometry = p.geometry();
                    let fit_issues = p.fit_issues().to_vec();
                    consume_simulation_pixels(sim, p, token, |pixels| {
                        ui.write_canvas(pixels, geometry, PreviewMode::Simulation).map(|()| {
                            ui.pixel_fit_issues.set(fit_issues.clone());
                        })
                    })
                })();
            })
        });
    }
    canvas_result(ui, result);
    if !manual
        && result.is_ok()
        && ui
            .dock
            .sim
            .with_untracked(|s| s.projection().scenario != Scenario::DroppedCompletion)
    {
        schedule_completion(ui);
    }
}

/// Schedules at most one `ui` animation-frame completion bound to callback/source/editor and captured pending token.
/// Stale/mode/inspection changes suppress execution; scheduling failure leaves manual completion available with
/// retained pixels.
fn schedule_completion(ui: Ui) {
    if ui.dock.callbacks.with_value(|callbacks| callbacks.completion.is_some()) {
        return;
    }
    let Some(callback) = capture_callback(ui) else {
        return;
    };
    let pending = ui.dock.pending.get_untracked();
    match request_animation_frame_with_handle(move || {
        if !current_callback(ui, callback) {
            return;
        }
        ui.dock.callbacks.update_value(|callbacks| callbacks.completion = None);
        if pending == ui.dock.pending.get_untracked()
            && ui.dock.mode.get_untracked() == PreviewMode::Simulation
            && !ui.inspection_active()
        {
            complete(ui);
        }
    }) {
        Ok(handle) => {
            ui.dock
                .callbacks
                .update_value(|callbacks| callbacks.completion = Some(handle))
        }
        Err(_) => {
            ui.dock
                .notice
                .set("Completion scheduling failed; use Complete submitted frame".into())
        }
    }
}

/// Restores exact pending pixels in `ui` before acknowledging their simulator token and releasing preview ownership.
/// Inspection/no pending state is a no-op. Canvas failure keeps authority pending; after successful Off completion a
/// separate draw projects black, whose failure is reported without undoing already completed metadata.
pub(super) fn complete(ui: Ui) {
    ui.editor.with_untracked(|e| ui.dock.synchronize(ui, e));
    if ui.inspection_active() {
        return;
    }
    let Some(token) = ui.dock.pending.get_untracked() else {
        return;
    };
    let mut result = Ok(());
    ui.simulate(|sim| {
        ui.dock.timing.update(|p| {
            let geometry = p.geometry();
            let fit_issues = p.fit_issues().to_vec();
            result = complete_simulation_pixels(sim, p, token, |pixels| {
                ui.write_canvas(pixels, geometry, PreviewMode::Simulation).map(|()| {
                    ui.pixel_fit_issues.set(fit_issues.clone());
                })
            });
            if result.is_ok() && sim.session().backlight().compare() == 0 {
                // Completed shutdown projects black; this read-only draw cannot commit new metadata.
                result = p
                    .render_simulation(sim)
                    .map(|_| ())
                    .map_err(RenderIssue::Preview)
                    .and_then(|()| {
                        let geometry = p.geometry();
                        let fit_issues = p.fit_issues().to_vec();
                        ui.write_canvas(p.rgba(), geometry, PreviewMode::Simulation).map(|()| {
                            ui.pixel_fit_issues.set(fit_issues.clone());
                        })
                    });
            }
        })
    });
    if ui.dock.sim.with_untracked(|sim| sim.completed_frame() == Some(token)) {
        ui.dock.pending.set(None);
        ui.dock.consumed.set(false);
        ui.dock.displayed.set(PreviewMode::Simulation);
        ui.dock.invalid_pixels.set(false);
        ui.dock
            .pixels_key
            .set(Some(ui.editor.with_untracked(Editor::accepted_config_key)));
        ui.dock
            .notice
            .set("Synthetic frame completed; captured LED/warning/backlight metadata committed".into());
    } else {
        canvas_result(ui, result);
    }
    ui.render_issue(RenderLane::Simulation, result);
}

/// Captures an admitted project from `ui`, cancels playback/pointer state and starts full startup replay with matching
/// frame completions. Rejection preserves accepted acquisition and reports why; success captures project stamp so later
/// edits cancel the replay.
pub(super) fn replay_startup(ui: Ui) {
    let captured = ui
        .editor
        .with_untracked(|e| e.accepted_project().map(|p| (e.stamp(), p)));
    let Ok((stamp, project)) = captured else {
        ui.dock
            .notice
            .set("Finish the current edit or import before startup replay.".into());
        return;
    };
    ui.editor.with_untracked(|e| ui.dock.synchronize(ui, e));
    cancel(ui);
    stage::cancel(ui);
    ui.playing.set(false);
    let mut result = Ok(());
    ui.dock.sim.update(|sim| result = sim.restart_project(&project));
    if let Err(error) = result {
        ui.dock.notice.set(format!("Startup replay rejected: {error:?}"));
        return;
    }
    ui.dock.startup_stamp.set_value(Some(stamp));
    ui.dock.pending.set(None);
    ui.dock.consumed.set(false);
    ui.dock.mode.set(PreviewMode::Simulation);
    ui.dock.notice.set(
        "Full startup replay: synthetic acquisition and matching frame completions; controls and recovery are active."
            .into(),
    );
    ui.dock.running.set(true);
    submit(ui, false);
}

/// Requests fresh `scenario` source in synchronized `ui`, refusing inspection and cancelling old callbacks first.
/// Success clears local pending flags and logs selection; failure retains old evidence and reports exhaustion/admission
/// refusal.
fn source(
    ui: Ui,
    scenario: Scenario,
) {
    ui.editor.with_untracked(|e| ui.dock.synchronize(ui, e));
    if ui.inspection_active() {
        return;
    }
    cancel(ui);
    let mut result = Ok(());
    ui.simulate(|sim| result = sim.set_scenario(scenario));
    if result.is_ok() {
        ui.dock.pending.set(None);
        ui.dock.consumed.set(false);
        ui.dock
            .notice
            .set("Scenario changed; explicit new synthetic source session".into());
        ui.event(Notice::info(
            Category::Simulation,
            "simulation.scenario",
            None,
            scenario.label(),
        ));
    } else {
        ui.dock.notice.set(format!("Scenario change rejected: {result:?}"));
    }
}

/// Updates `ui`'s held level for `button` to `down` at current elapsed playback time, then dispatches all levels
/// together. A real held-to-released edge schedules 50 ms of settling when representable; lifecycle cancellation uses a
/// different path.
fn held(
    ui: Ui,
    button: ButtonId,
    down: bool,
) {
    let was_held = ui.dock.held.with_untracked(|pressed| pressed[button.index()]);
    let mut at = ui.dock.sim.with_untracked(ControlSimulator::display_now_ms);
    ui.dock.playback_clock.update_value(|clock| {
        if let Some((clock, _)) = clock {
            at = clock.sample(now());
        }
    });
    if !down && was_held {
        ui.dock.release_until.set(at.checked_add(50));
    }
    ui.dock.held.update(|pressed| pressed[button.index()] = down);
    let pressed = ui.dock.held.get_untracked();
    ui.simulate(|sim| sim.physical_at(pressed, EncoderBatch::default(), at));
}

/// Parses `ui`'s currently admitted bounded reset text and delivers exact u64 token through accepted-only simulation
/// admission. Oversize/nondecimal text does nothing; stale/duplicate token reports refusal without resetting
/// statistics.
fn deliver_reset_token(ui: Ui) {
    let text = ui
        .dock
        .ack_text
        .with_untracked(|value| value.admitted().map(str::to_owned));
    if let Some(text) = text
        && let Ok(token) = text.parse::<u64>()
    {
        ui.simulate_accepted(|sim| {
            let accepted = sim.acknowledge_reset_token(token);
            ui.dock.notice.set(
                if accepted {
                    "Exact reset token acknowledged"
                } else {
                    "Stale, duplicate or unmatched reset acknowledgment; no change"
                }
                .into(),
            );
        });
    }
}

/// Submits admitted raw text for `channel` in `ui` after cancelling playback; `diagnostic` bypasses only UI test-domain
/// filtering. Oversize text is blocked, inspection is refused by accepted-only dispatch, and shared sample gates remain
/// authoritative.
fn submit_channel_input(
    ui: Ui,
    channel: Channel,
    diagnostic: bool,
) {
    let text = ui
        .dock
        .inputs
        .with_untracked(|values| values[channel.index()].admitted().map(str::to_owned));
    if let Some(text) = text {
        cancel(ui);
        ui.simulate_accepted(|sim| {
            if diagnostic {
                sim.diagnostic_input(channel, &text);
            } else {
                sim.custom_input(channel, &text);
            }
        });
    }
}

#[component]
/// Returns `ui`'s persistent synthetic control/data/presentation test panel with explicit delayed reset and completion
/// actions. Candidate controls remain inspectable while accepted sample/reset/completion actions stay guarded; all
/// observations are synthetic.
pub(super) fn Panel(
    /// Shared accepted/candidate simulation sessions and pending presentation ownership.
    ui: Ui
) -> impl IntoView {
    let sim = ui.dock.sim;
    view! {
        <section class="control-simulator" aria-label="All-screen simulator">
            <a class="simulation-preview-link" href="#studio-preview-dock" on:click=move |_|ui.dock.mode.set(PreviewMode::Simulation)>"Show live display preview ↓"</a>
            <h3>"Synthetic device simulation"</h3>
            <strong class="control-error" hidden=move ||!sim.with(ControlSimulator::invalid)>{move||if ui.json_pending(){json_editor::label(ui)}else{"INVALID SIMULATION · candidate controls/presentation; accepted acquisition retained"}}</strong>
            <p class="hint">"Every screen is synthetic. Values and source catalogue entries are not ECU evidence. Automatic playback follows active elapsed time; manual steps use virtual time."</p>
            <label class="field"><span>"Scenario (starts a new source session)"</span><select aria-label="Simulation scenario" disabled=move||ui.inspection_active() prop:value=move||sim.with(|s|Scenario::ALL.iter().position(|v|*v==s.projection().scenario).unwrap_or(0).to_string()) on:change=move|e|{
                if let Ok(index)=event_target_value(&e).parse::<usize>() && let Some(s)=Scenario::ALL.get(index){source(ui,*s);}
            }>{Scenario::ALL.into_iter().enumerate().map(|(i,s)|view!{<option value=i.to_string()>{s.label()}</option>}).collect_view()}</select></label>
            <div class="simulator-actions">
                <button disabled=move||ui.inspection_active() on:click=move |_|{if ui.dock.running.get_untracked(){cancel(ui);}else{ui.dock.running.set(true);ui.dock.mode.set(PreviewMode::Simulation);}}>{move||if ui.dock.running.get(){"Pause playback"}else{"Play active elapsed time"}}</button>
                <button disabled=move||ui.inspection_active() on:click=move |_|{cancel(ui);ui.simulate_accepted(|s|{if let Some(at)=s.now_ms().checked_add(50){let _=s.scenario_step(at);}});}>"Next sample (manual +50 ms)"</button>
                <button disabled=move||ui.inspection_active() on:click=move |_|source(ui,sim.with_untracked(|s|s.projection().scenario))>"Restart source"</button>
            </div>
            <output id="control-simulator-state" aria-live="polite">{move||sim.with(ControlSimulator::summary)}</output>
            <p role="status">{move||ui.dock.notice.get()}</p>
            <output aria-label="Inspection control outcome">{move||sim.with(|s|s.inspection_outcome().unwrap_or(""))}</output>
            <output aria-label="Rejected control outcome">{move||sim.with(|s|s.control_outcome().unwrap_or(""))}</output>
            <output aria-label="Simulation frame state">{move||format!("Source {:?} · current {:?} · pending {:?} · canvas accepted {}",sim.with(|s|s.source_epoch()),sim.with(|s|s.completed_frame()),ui.dock.pending.get(),ui.dock.consumed.get())}</output>
            <div class="simulator-actions">
                <button disabled=move||ui.inspection_active() ||(ui.dock.pending.get().is_some()&&ui.dock.consumed.get()) on:click=move |_|submit(ui,true)>{move||if ui.dock.pending.get().is_some(){"Retry exact pending pixels"}else{"Render / submit current screen"}}</button>
                <button disabled=move||ui.inspection_active() ||ui.dock.pending.get().is_none() on:click=move |_|complete(ui)>"Complete submitted frame"</button>
            </div>
            <output aria-label="RGB indicator duty">{move||sim.with(|s|{let d=s.led_duty();format!("Completed scene LED duty: R {} G {} B {} / 10000",d.red,d.green,d.blue)})}</output>
            <div class="simulator-buttons">{ButtonId::ALL.into_iter().map(|button|view!{
                <div><strong class="keycap">{button.label()}</strong>
                    <button on:click=move |_|{cancel(ui);ui.simulate(|s|s.tap(button));}>{format!("Tap {}",button.label())}</button>
                    <button on:click=move |_|{cancel(ui);ui.simulate(|s|s.hold(button));}>{format!("Hold {} (manual elapsed)",button.label())}</button>
                    <button class:active=move||ui.dock.held.with(|p|p[button.index()]) on:click=move |_|held(ui,button,true)>{format!("Press / keep {} held",button.label())}</button>
                    <button on:click=move |_|held(ui,button,false)>{format!("Release {}",button.label())}</button>
                </div>
            }).collect_view()}</div>
            <div class="simulator-actions">
                <button on:click=move |_|{cancel(ui);ui.simulate(|s|s.rotate(-1));}>"Encoder −"</button>
                <button on:click=move |_|{cancel(ui);ui.simulate(|s|s.rotate(1));}>"Encoder +"</button>
                <button on:click=move |_|{cancel(ui);ui.simulate(ControlSimulator::click);}>"Encoder click"</button>
                <button disabled=move||ui.json_pending() on:click=move |_|ui.simulate_accepted(ControlSimulator::acknowledge_warnings)>"Acknowledge completed warning page"</button>
                <button on:click=move |_|super::controls::recover(ui)>"Protected Y recovery (manual 2 s)"</button>
                <button on:click=move |_|cancel(ui)>"Cancel physical inputs"</button>
            </div>
            <p class="hint">"Hold multiple keys with Press; Release samples the real edge. Cancel, pointer cancellation, blur and hiding the tab release trackers without a tap or hold. Y hold always recovers Dashboard Full."</p>
            <output aria-label="Statistics reset tokens">{move||sim.with(|s|format!("Reset requested {:?} · producer generation {}",s.pending_reset_token(),s.snapshot().reset_generation))}</output>
            <label class="field"><span>"Exact reset acknowledgment token"</span><input aria-label="Reset acknowledgment token" type="text" inputmode="numeric" maxlength="20" prop:value=move||ui.dock.ack_text.with(|v|v.text().to_owned()) on:input=move|e|{let value=event_target_value(&e);ui.dock.ack_text.update(|v|{v.observe(&value);});}/></label>
            <output role="status">{move||if ui.dock.ack_text.with(|v|v.is_blocked()){"Reset input exceeds 20 UTF-8 bytes; delivery blocked until corrected"}else{""}}</output>
            <div class="simulator-actions"><button disabled=move||ui.inspection_active()||ui.dock.ack_text.with(|v|v.is_blocked()) on:click=move |_|deliver_reset_token(ui)>"Deliver exact reset acknowledgment"</button><button disabled=move||ui.inspection_active() on:click=move |_|ui.simulate_accepted(ControlSimulator::acknowledge_reset)>"Acknowledge current reset"</button><button disabled=move||ui.inspection_active() on:click=move |_|ui.simulate_accepted(|s|s.producer_step(None))>"Process warning acknowledgment result"</button></div>
            <p class="hint">"Delayed reset: trigger Reset statistics using the configured button, then deliver its token. Dropped completion: playback retains one frame until explicit Complete. Warning acknowledgment uses the last completed accepted Dashboard only."</p>
            <h4>"Per-channel synthetic inputs"</h4>
            <div class="simulation-inputs">{Channel::ALL.into_iter().map(|channel|view!{
                <details class="simulation-channel"><summary>{channel.label()}</summary>
                    <label class="field"><span>{channel.label()}</span><input aria-label=format!("Synthetic {} input",channel.label()) type="text" maxlength="64" prop:value=move||ui.dock.inputs.with(|v|v[channel.index()].text().to_owned()) on:input=move|e|{let text=event_target_value(&e);ui.dock.inputs.update(|v|{v[channel.index()].observe(&text);});}/></label>
                    <output role="status">{move||if ui.dock.inputs.with(|v|v[channel.index()].is_blocked()){"Input exceeds 64 UTF-8 bytes; submission blocked until corrected"}else{""}}</output>
                    <small>{format!("Synthetic testing range {}..{}; canonical units",channel.domain().0,channel.domain().1)}</small>
                    <button disabled=move||ui.inspection_active()||ui.dock.inputs.with(|v|v[channel.index()].is_blocked()) on:click=move |_|submit_channel_input(ui,channel,false)>"Submit channel"</button>
                    <button disabled=move||ui.inspection_active()||ui.dock.inputs.with(|v|v[channel.index()].is_blocked()) on:click=move |_|submit_channel_input(ui,channel,true)>"Inject diagnostic value (bypass UI range)"</button>
                    <output>{move||sim.with(|s|{let p=s.projection();let m=p.snapshot.metadata[channel.index()];format!("{} · last good {:?} · {:?} · sample time {:?}",p.outcomes[channel.index()].map_or("No custom outcome",|o|o.label()),p.snapshot.sensor(dashboard_core::samples::SensorId::from_channel(channel)).value(),p.snapshot.quality(dashboard_core::samples::SensorId::from_channel(channel),p.now_ms),m.received_at_ms)})}</output>
                    <div class="simulator-actions">{[("Reject NaN",f32::NAN),("Reject Infinity",f32::INFINITY),("Reject magnitude",1_000_001.)].into_iter().map(|(label,value)|view!{<button disabled=move||ui.inspection_active() on:click=move |_|ui.simulate_accepted(|s|{if let Some(at_ms)=s.now_ms().checked_add(1){s.inject(SimulationInput::Channel{channel,value,at_ms});}})>{label}</button>}).collect_view()}<button disabled=move||ui.inspection_active() on:click=move |_|ui.simulate_accepted(|s|{let at_ms=s.now_ms().saturating_sub(1);s.inject(SimulationInput::Channel{channel,value:1.,at_ms});})>"Inject backdated"</button><button disabled=move||ui.inspection_active() on:click=move |_|ui.simulate_accepted(|s|{if let Some(at_ms)=s.now_ms().checked_add(1){s.inject(SimulationInput::Fault{sensor:dashboard_core::samples::SensorId::from_channel(channel),quality:dashboard_core::samples::Quality::TransportError,at_ms});}})>"Source fault"</button></div>
                    <small>{move||ui.editor.with(|e|{let d=dashboard_configurator::config::definition(e.preview().config().sources[channel.index()].definition);format!("Configured catalogue: {} · {} · {:?}; synthetic values are not decoded ECU replies",d.name,d.interpretation,d.unavailable)})}</small>
                </details>
            }).collect_view()}</div>
            <label class="field"><span>"Inspect shared history"</span><select aria-label="Simulation channel" on:change=move|e|{if let Ok(i)=event_target_value(&e).parse::<usize>()&&let Some(c)=Channel::ALL.get(i){ui.dock.channel.set(*c);}}>{Channel::ALL.into_iter().map(|c|view!{<option value=c.index().to_string()>{c.label()}</option>}).collect_view()}</select></label>
            <output aria-label="History gaps and duration">{move||sim.with(|s|{let c=ui.dock.channel.get();let snapshot=s.snapshot();let h=snapshot.sensor(dashboard_core::samples::SensorId::from_channel(c)).history();format!("{} · {} s · {} / 60 filled buckets; gaps remain blank",c.label(),h.duration_seconds(),h.count())})}</output>
            <ul aria-label="Accepted warning episode state">{move||sim.with(|s|{let snap=s.snapshot();s.config().config().widgets.iter().enumerate().filter(|(_,w)|w.warning.enabled).map(|(i,w)|{let e=snap.warning_eligibility(dashboard_core::alarms::RuleIndex::new(i).unwrap(),s.now_ms());let witness=match s.warning_fixture(i){WarningFixture::Available(v)=>format!("witness {v}"),WarningFixture::Unavailable(reason)=>format!("Unavailable: {reason}"),WarningFixture::Disabled=>"Disabled".into()};view!{<li>{format!("Accepted {} · {} · {}{}{}",w.label,e.label(),witness,if e.snoozed{" · SNOOZED"}else{""},if e.latched{" · LATCHED"}else{""})}</li>}}).collect_view()})}</ul>
            <output aria-label="Warning acknowledgment state">{move||sim.with(|s|if let Some(dashboard_core::popup::Popup::Warnings(_,feedback))=s.session().popup(){feedback.label()}else if s.pending_warning_command().is_some(){"ACK PENDING"}else{"No accepted warning acknowledgment pending"})}</output>
        </section>
    }
}
