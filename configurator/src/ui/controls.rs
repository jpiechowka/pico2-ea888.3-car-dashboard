use dashboard_configurator::simulator::ControlSimulator;

use super::*;

fn write_timing_canvas(
    canvas: NodeRef<leptos::html::Canvas>,
    pixels: &[u8],
) -> Result<(), String> {
    let image = web_sys::ImageData::new_with_u8_clamped_array_and_sh(wasm_bindgen::Clamped(pixels), 320, 240)
        .map_err(|_| "Timing image allocation failed")?;
    let canvas = canvas.get_untracked().ok_or("Timing canvas unavailable")?;
    let context = canvas
        .get_context("2d")
        .map_err(|_| "Timing canvas unavailable")?
        .ok_or("Timing canvas unavailable")?
        .dyn_into::<web_sys::CanvasRenderingContext2d>()
        .map_err(|_| "Timing context unavailable")?;
    context
        .put_image_data(&image, 0., 0.)
        .map_err(|_| "Timing canvas draw failed".into())
}

fn binding_id(
    page: ControlPage,
    button: ButtonId,
    hold: bool,
) -> String {
    format!(
        "control-{}-{}-{}",
        page.label().to_lowercase(),
        button.label().to_lowercase(),
        if hold { "hold" } else { "tap" }
    )
}

fn error_target(error: ControlError) -> String {
    match error {
        ControlError::HoldDuration => "control-hold-ms".into(),
        ControlError::ReservedRecovery { page } => binding_id(page, ButtonId::Y, true),
        ControlError::IncompatibleButton { page, button, hold } => binding_id(page, button, hold),
        ControlError::IncompatibleEncoderClick { page } => {
            format!("control-{}-encoder-click", page.label().to_lowercase())
        }
        ControlError::IncompatibleEncoderRotation { page } => {
            format!("control-{}-encoder-rotation", page.label().to_lowercase())
        }
        ControlError::UnreachableScreen { .. } | ControlError::MissingViewNavigation => "control-routes".into(),
    }
}

fn error_label(error: ControlError) -> String {
    match error {
        ControlError::HoldDuration => "Hold duration must be 400–1500 ms.".into(),
        ControlError::ReservedRecovery { page } => {
            format!(
                "{}: Y hold must retain fixed recovery. Repair its stored binding below.",
                page.label()
            )
        }
        ControlError::IncompatibleButton { page, button, hold } => {
            format!(
                "{} {} {}: select an action compatible with this screen.",
                page.label(),
                button.label(),
                if hold { "hold" } else { "tap" }
            )
        }
        ControlError::IncompatibleEncoderClick { page } => {
            format!("{} encoder click: select a compatible action.", page.label())
        }
        ControlError::IncompatibleEncoderRotation { page } => {
            format!("{} encoder rotation: select a compatible action.", page.label())
        }
        ControlError::UnreachableScreen { page } => {
            format!(
                "Add onboard button navigation from Dashboard to {}. Encoder routes do not count.",
                page.label()
            )
        }
        ControlError::MissingViewNavigation => {
            "Keep a Dashboard button bound to Next view or Previous view. Encoder routes do not count.".into()
        }
    }
}

fn change_map(
    ui: Ui,
    token: FieldToken,
    change: impl FnOnce(&mut ControlMap),
) {
    ui.run(move |editor| {
        let mut map = editor.editing_config().controls;
        change(&mut map);
        editor.apply_if_current(token, Edit::SetControls(map))
    });
}

#[component]
fn ActionPicker(
    ui: Ui,
    page: ControlPage,
    button: Option<ButtonId>,
    #[prop(default = false)] hold: bool,
) -> impl IntoView {
    let action = move || {
        ui.editor.with(|e| {
            let screen = e.editing_config().controls.screens[page.index()];
            button.map_or(screen.encoder_click, |b| {
                if hold {
                    screen.buttons[b.index()].hold
                } else {
                    screen.buttons[b.index()].tap
                }
            })
        })
    };
    // Rebuild the menu only when a rejected imported action changes compatibility.
    // Retain that value as a visibly invalid choice so the candidate remains inspectable.
    view! {
        <For each=move || vec![if action().allowed_on(page) {"compatible".to_string()} else {format!("{:?}",action())}] key=|action| action.clone() children=move |_| {
            let mut options: Vec<_> = ControlAction::ALL.into_iter().filter(|a| a.allowed_on(page))
                .map(|a| (format!("{a:?}"), a.label().to_string())).collect();
            if !action().allowed_on(page) {
                let rejected = action();
                options.push((format!("{rejected:?}"), format!("INVALID: {}", rejected.label())));
            }
            view! {
                <Choice ui repair_invalid_draft=true
                    accessible_label=button.map_or_else(||format!("{} encoder click",page.label()),|button|format!("{} {} {}",page.label(),button.label(),if hold{"hold"}else{"tap"}))
                    id=button.map_or_else(||format!("control-{}-encoder-click",page.label().to_lowercase()), |b|binding_id(page,b,hold))
                    label=if button.is_none() { "Encoder click" } else if hold { "Hold" } else { "Tap" }
                    value=Signal::derive(move ||format!("{:?}",action())) options
                    on_change=Callback::new(move |(token,value): (FieldToken,String)| {
                        if let Some(action)=ControlAction::ALL.into_iter().find(|a| format!("{a:?}")==value) {
                            change_map(ui,token,move |map| {
                                let screen=&mut map.screens[page.index()];
                                if let Some(button)=button {
                                    if hold {screen.buttons[button.index()].hold=action;} else {screen.buttons[button.index()].tap=action;}
                                } else {screen.encoder_click=action;}
                            });
                        }
                    })/>
            }
        }/>
    }
}

#[component]
fn RotationPicker(
    ui: Ui,
    page: ControlPage,
) -> impl IntoView {
    let action = move || {
        ui.editor
            .with(|e| e.editing_config().controls.screens[page.index()].encoder_rotation)
    };
    view! {
        <For each=move ||vec![if action().allowed_on(page) {"compatible".to_string()} else {format!("{:?}",action())}] key=|a|a.clone() children=move |_| {
            let mut options:Vec<_>=RotationAction::ALL.into_iter().filter(|a|a.allowed_on(page)).map(|a|(format!("{a:?}"),a.label().to_string())).collect();
            if !action().allowed_on(page) {let rejected=action();options.push((format!("{rejected:?}"),format!("INVALID: {}",rejected.label())));}
            view! {
                <Choice ui repair_invalid_draft=true label="Encoder rotation" id=format!("control-{}-encoder-rotation",page.label().to_lowercase())
                    accessible_label=format!("{} encoder rotation",page.label())
                    value=Signal::derive(move ||format!("{:?}",action())) options
                    on_change=Callback::new(move |(token,value):(FieldToken,String)| {
                        if let Some(action)=RotationAction::ALL.into_iter().find(|a|format!("{a:?}")==value){change_map(ui,token,move |map|map.screens[page.index()].encoder_rotation=action);}
                    })/>
            }
        }/>
    }
}

#[component]
pub fn DeviceControls(ui: Ui) -> impl IntoView {
    let sim = RwSignal::<ControlSimulator, LocalStorage>::new_local(
        ui.editor.with_untracked(|e| ControlSimulator::new(e.preview().clone())),
    );
    Effect::new(move |_| ui.observe_cache(sim.with(ControlSimulator::cache_exhaustion_status)));
    let previous = StoredValue::new(ui.editor.with_untracked(|e| e.preview().config().clone()));
    let simulation_notice = RwSignal::new("Ready · accepted configuration".to_string());
    let timing_preview = RwSignal::<dashboard_configurator::preview::Preview, LocalStorage>::new_local(
        dashboard_configurator::preview::Preview::new(),
    );
    let timing_canvas = NodeRef::<leptos::html::Canvas>::new();
    let sequence = RwSignal::new(0u64);
    let pending_frame = RwSignal::new(None::<dashboard_core::display::pipeline::FrameToken>);
    let canvas_accepted = RwSignal::new(false);
    let channel = RwSignal::new(Channel::Boost);
    Effect::new(move |_| {
        ui.editor.with(|e| {
            let accepted = e.preview().config();
            if previous.with_value(|old| old != accepted) {
                sim.update(|s| s.restart(e.preview().clone()));
                previous.set_value(accepted.clone());
                pending_frame.set(None);
                canvas_accepted.set(false);
                simulation_notice.set("Simulation restarted · accepted configuration changed".into());
            }
            if e.has_invalid_draft() {
                let map = e
                    .diagnostic()
                    .map_or(e.editing_config().controls, |draft| draft.config.controls);
                sim.update(|s| s.inspect_invalid(map));
            } else if sim.with_untracked(ControlSimulator::invalid) {
                sim.update(|s| s.restart(e.preview().clone()));
                simulation_notice.set("Simulation restarted · candidate repaired".into());
                pending_frame.set(None);
                canvas_accepted.set(false);
            }
        });
    });
    view! {
        <section class="device-controls" aria-label="Device controls">
            <h2>"Device controls"</h2>
            <p>"Map onboard buttons for each screen. Keep button routes to Debug, Logs and every dashboard view, even when an encoder is installed."</p>
            <TextField ui field=RawField::ControlHoldMilliseconds id="control-hold-ms".to_string() label="Ordinary hold duration" hint="400–1500 ms · Y recovery is always 2000 ms"/>
            <div role="alert" class="control-error">{move || ui.editor.with(|e| {
                if matches!(e.draft_error(),Some(EditorError::Parse(RawField::ControlHoldMilliseconds))) {
                    return view!{<a href="#control-hold-ms">"Hold duration: enter a complete number."</a>}.into_any();
                }
                let map=e.diagnostic().map_or(e.editing_config().controls,|draft|draft.config.controls);
                validate_controls(&map).err().map(|error| view! {
                    <a href=format!("#{}",error_target(error))>{error_label(error)}</a>
                }).into_any()
            })}</div>
            <div id="control-routes" tabindex="-1" class="control-routes">
                <strong>"Onboard fallback routes"</strong>
                <ul>{move || ui.editor.with(|e| ControlPage::ALL.into_iter().map(|page| {
                    let screen=e.editing_config().controls.screens[page.index()];
                    let routes=ButtonId::ALL.into_iter().flat_map(|button| {
                        let binding=screen.buttons[button.index()];
                        [(false,binding.tap),(true,binding.hold)].into_iter()
                            .filter(move |(_,action)|action.destination(page).is_some() || matches!(action,ControlAction::NextView|ControlAction::PreviousView))
                            .map(move |(hold,action)|format!("{} {} → {}",button.label(),if hold{"hold"}else{"tap"},action.label()))
                    }).collect::<Vec<_>>().join(" · ");
                    view!{<li>{format!("{}: {} · Y hold → Dashboard / Full",page.label(),if routes.is_empty(){"no configurable routes"}else{&routes})}</li>}
                }).collect_view())}</ul>
            </div>
            <div class="control-pages">{ControlPage::ALL.into_iter().map(|page| view! {
                <fieldset class="control-page"><legend>{page.label()}</legend>
                    {ButtonId::ALL.into_iter().map(move |button| view! {
                        <div class="control-binding"><strong class="keycap">{button.label()}</strong>
                            <ActionPicker ui page button=Some(button)/>
                            {if button==ButtonId::Y {view!{
                                <div class="locked-recovery" id=binding_id(page,button,true) tabindex="-1">
                                    <b>"Hold · locked recovery"</b><small>"2 seconds → Dashboard · last view · Full brightness"</small>
                                    <button hidden=move || ui.editor.with(|e|e.editing_config().controls.screens[page.index()].buttons[3].hold==ControlAction::None)
                                        on:click=move |_| {let token=ui.editor.with_untracked(Editor::field_token);change_map(ui,token,move |map|map.screens[page.index()].buttons[3].hold=ControlAction::None);}>
                                        "Repair reserved Y hold"
                                    </button>
                                </div>
                            }.into_any()} else {view!{<ActionPicker ui page button=Some(button) hold=true/>}.into_any()}}
                        </div>
                    }).collect_view()}
                    <div class="control-encoder">
                        <RotationPicker ui page/>
                        <ActionPicker ui page button=None/>
                    </div>
                    <p class="recovery-note">"Y hold always wakes Full, returns to Dashboard and cancels pending shutdown. Encoder bindings cannot replace onboard fallback routes."</p>
                </fieldset>
            }).collect_view()}</div>
            <section class="control-simulator" aria-label="Control simulator">
                <h3>"Try device controls"</h3>
                <strong class="control-error" hidden=move || !sim.with(ControlSimulator::invalid)>"INVALID SIMULATION · candidate bindings / accepted layout · inspection only"</strong>
                <p role="status">{move ||simulation_notice.get()}</p>
                <output id="control-simulator-state" aria-live="polite">{move ||sim.with(ControlSimulator::summary)}</output>
                <output aria-label="RGB indicator duty">{move ||sim.with(|s|{let d=s.led_duty();format!("LED light duty · R {} · G {} · B {} / 10000",d.red,d.green,d.blue)})}</output>
                <p class="hint">"Persistent timing simulation · real configured dwell · explicit sample, fault, stale and frame-completion steps. Appearance stress values above are separate."</p>
                <label class="field"><span>"Fault / history channel"</span><select aria-label="Simulation channel" on:change=move |event|{
                    if let Ok(i)=event_target_value(&event).parse::<usize>() && let Some(c)=Channel::ALL.get(i){channel.set(*c);}
                }>{Channel::ALL.into_iter().map(|c|view!{<option value=c.index().to_string()>{c.label()}</option>}).collect_view()}</select></label>
                <output aria-label="History gaps and duration">{move ||sim.with(|s|{
                    let snapshot=s.snapshot();let h=snapshot.sensor(dashboard_core::samples::SensorId::from_channel(channel.get())).history();
                    format!("{} shared history · {} s · {} / 60 samples · {} gaps",channel.get().label(),h.duration_seconds(),h.count(),60-h.count())
                })}</output>
                <ul aria-label="Warning episode state">{move ||sim.with(|s|{
                    let snap=s.snapshot();s.config().config().widgets.iter().enumerate().filter(|(_,w)|w.warning.enabled).map(|(i,w)|{
                        let e=snap.warning_eligibility(dashboard_core::alarms::RuleIndex::new(i).expect("bounded widget"),s.now_ms());
                        view!{<li>{format!("{} · {}{}{}",w.label,e.label(),if e.snoozed{" · SNOOZED"}else{""},if e.latched && !e.label().contains("LATCHED"){" · LATCHED"}else{""})}</li>}
                    }).collect_view()
                })}</ul>
                <output aria-label="Warning acknowledgment state">{move ||sim.with(|s| match s.session().popup(){
                    Some(dashboard_core::popup::Popup::Warnings(_, feedback))=>feedback.label().to_string(),
                    _ if s.pending_warning_command().is_some()=>"ACK PENDING".into(),
                    _=>"No pending warning acknowledgment".into(),
                })}</output>
                <div class="simulator-actions">
                    <button disabled=move ||ui.invalid() on:click=move |_|sim.update(|s|s.advance_to(s.now_ms().saturating_add(250),Some([1.,90.,95.,80.,30.,700.,13.8,1.])))>"Sample normal / recover +250 ms"</button>
                    <button disabled=move ||ui.invalid() on:click=move |_|sim.update(|s|s.advance_to(s.now_ms().saturating_add(250),Some([3.,150.,120.,150.,80.,1000.,11.,1.4])))>"Sample elevated +250 ms"</button>
                    <button disabled=move ||ui.invalid() on:click=move |_|sim.update(|s|s.inject_fault(dashboard_core::samples::SensorId::from_channel(channel.get_untracked()),dashboard_core::samples::Quality::TransportError,s.now_ms().saturating_add(1)))>"Inject source fault"</button>
                    <button disabled=move ||ui.invalid() on:click=move |_|sim.update(|s|s.advance_to(s.now_ms().saturating_add(10000),None))>"No samples +10 s (stale)"</button>
                    <button disabled=move ||ui.invalid() on:click=move |_|sim.update(|s|s.advance_to(s.now_ms().saturating_add(250),None))>"No samples +250 ms"</button>
                    <button disabled=move ||ui.invalid() on:click=move |_|sim.update(|s|s.producer_step(None))>"Process acknowledgment result"</button>
                </div>
                <canvas node_ref=timing_canvas width="320" height="240" aria-label="Persistent dashboard timing simulation"></canvas>
                <output aria-label="Simulation frame state">{move ||if pending_frame.get().is_some(){if canvas_accepted.get(){"Dashboard pixels accepted · awaiting completion"}else{"Dashboard candidate retained · retry the same pixels before completion"}}else{"Ready to submit dashboard frame"}}</output>
                <div class="simulator-actions">
                    <button disabled=move ||ui.invalid() || (pending_frame.get().is_some() && canvas_accepted.get()) || (pending_frame.get().is_none() && sim.with(|s|s.session().page()!=ControlPage::Dashboard)) on:click=move |_|{
                        if pending_frame.get_untracked().is_some(){
                            match timing_preview.with_untracked(|p|write_timing_canvas(timing_canvas,p.rgba())){
                                Ok(())=>{canvas_accepted.set(true);simulation_notice.set("Retained Dashboard pixels accepted. Complete the submitted frame.".into());},
                                Err(error)=>simulation_notice.set(format!("{error}. Candidate retained; retry the same pixels.")),
                            }
                            return;
                        }
                        let Some(next)=sequence.get_untracked().checked_add(1) else {simulation_notice.set("Frame counter exhausted. Reload to restart.".into());return;};
                        let token=dashboard_core::display::pipeline::FrameToken{buffer:dashboard_core::display::pipeline::BufferId::A,sequence:next};
                        let mut result=Ok(());
                        sim.update(|s| timing_preview.update(|p|{
                            result=(||{
                                let pixels=p.render_simulation_submitted(s,token).map_err(|e|format!("Timing preview: {e:?}"))?;
                                sequence.set(next);pending_frame.set(Some(token));
                                write_timing_canvas(timing_canvas,pixels)
                            })();
                        }));
                        match result { Ok(())=>{canvas_accepted.set(true);simulation_notice.set("Dashboard pixels submitted. Complete the frame to commit LED and acknowledgment identity.".into());},Err(error)=>simulation_notice.set(format!("{error}. Candidate retained when submitted; retry its same pixels.")), }
                    }>{move ||if pending_frame.get().is_some() && !canvas_accepted.get(){"Retry same submitted pixels"}else{"Render / submit dashboard frame"}}</button>
                    <button disabled=move ||ui.invalid() || pending_frame.get().is_none() || !canvas_accepted.get() on:click=move |_|{
                        if let Some(token)=pending_frame.get_untracked(){sim.update(|s|{if s.complete_frame(token){simulation_notice.set("Dashboard frame completed · LED and warning identity committed".into());pending_frame.set(None);canvas_accepted.set(false);}});}
                    }>"Complete submitted frame"</button>
                </div>
                <div class="simulator-buttons">{ButtonId::ALL.into_iter().map(|button| view! {
                    <div><strong class="keycap">{button.label()}</strong>
                        <button on:click=move |_|sim.update(|s|s.tap(button))>{format!("Tap {}",button.label())}</button>
                        <button on:click=move |_|sim.update(|s|s.hold(button))>{if button==ButtonId::Y {"Recover (2 s)".to_string()}else{format!("Hold {}",button.label())}}</button>
                    </div>
                }).collect_view()}</div>
                <div class="simulator-actions">
                    <button on:click=move |_|sim.update(|s|s.rotate(-1))>"Encoder −"</button>
                    <button on:click=move |_|sim.update(|s|s.rotate(1))>"Encoder +"</button>
                    <button on:click=move |_|sim.update(ControlSimulator::click)>"Encoder click"</button>
                    <button on:click=move |_|sim.update(|s|s.advance(2500))>"Advance 2.5 s"</button>
                    <button disabled=move || !sim.with(ControlSimulator::reset_pending) on:click=move |_|sim.update(ControlSimulator::acknowledge_reset)>"Acknowledge statistics reset"</button>
                </div>
                <small>"Onboard taps and holds use the stored map: Toggle boost units changes the physical-toggle state; Acknowledge warnings uses the last completed Dashboard page. Repeat it before processing to inspect Busy. Debug/Logs retain state readouts; their raster screens are a later workspace capability."</small>
            </section>
        </section>
    }
}
