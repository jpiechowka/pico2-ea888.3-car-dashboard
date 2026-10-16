use dashboard_configurator::geometry::{self, ContentRect, DragGesture, GrabOffset};
use dashboard_configurator::preview::{Preview, PreviewOptions};
use leptos::leptos_dom::helpers::request_animation_frame_with_handle;

use super::*;

#[derive(Clone, Copy)]
pub struct Drag {
    token: PointerToken,
    geometry: Geometry,
    span: Span,
    offset: GrabOffset,
    gesture: DragGesture,
    selection_changed: bool,
    position: Option<Placement>,
    valid: bool,
}

fn content_rect(ui: Ui) -> Option<ContentRect> {
    let canvas = ui.canvas.get_untracked()?;
    let r = canvas.get_bounding_client_rect();
    Some(ContentRect {
        left: r.left(),
        top: r.top(),
        width: r.width(),
        height: r.height(),
    })
}

fn start(
    ui: Ui,
    event: web_sys::PointerEvent,
    target: PointerTarget,
) {
    if event.button() != 0 || !event.is_primary() {
        return;
    }
    // Primary is per pointer type. One mouse/touch/pen owns a drag at a time.
    if ui.drag.get_untracked().is_some() {
        return;
    }
    let Ok(gesture) = DragGesture::new(f64::from(event.client_x()), f64::from(event.client_y())) else {
        return;
    };
    event.prevent_default();
    event.stop_propagation();
    widget_menu::close(ui, false);
    let geometry = ui.pixel_geometry.get_untracked();
    if ui.editor.with_untracked(|e| {
        e.editing_config()
            .view_orientation(e.selection().view)
            .map(|o| o.geometry())
    }) != Some(geometry)
    {
        return;
    }
    let (span, offset) = match target {
        PointerTarget::Library(_) => (Span::OneByOne, GrabOffset::center_in(geometry, Span::OneByOne)),
        PointerTarget::Widget(id) => {
            let Some(widget) = ui.canvas_widget(id) else {
                return;
            };
            let Some(rect) = content_rect(ui) else {
                return;
            };
            let Ok(offset) = geometry::grab_offset_in(
                geometry,
                rect,
                f64::from(event.client_x()),
                f64::from(event.client_y()),
                widget.placement,
            ) else {
                return;
            };
            (widget.placement.span, offset)
        }
    };
    let Some(element) = event
        .current_target()
        .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
    else {
        return;
    };
    if element.set_pointer_capture(event.pointer_id()).is_err() {
        ui.notice
            .set("Pointer capture is unavailable. Use the + button or position fields.".into());
        return;
    }
    let mut accepted = None;
    let selection_before = ui.editor.with_untracked(Editor::selection);
    // Selecting a widget is persistent even when its drag is cancelled or
    // commits the same placement. Replace any pending save at selection time.
    ui.run(|editor| {
        accepted = Some(editor.begin_pointer(event.pointer_id(), target)?);
        Ok(editor.revision())
    });
    match accepted {
        Some(token) => {
            if matches!(target, PointerTarget::Widget(_)) {
                ui.inspection.set(None);
            }
            if let Some(button) = element.dyn_ref::<web_sys::HtmlElement>() {
                let options = web_sys::FocusOptions::new();
                options.set_prevent_scroll(true);
                if button.focus_with_options(&options).is_err() {
                    ui.notice.set("Widget selected; browser focus was unavailable.".into());
                }
            }
            ui.capture.set(Some((event.pointer_id(), element)));
            ui.drag.set(Some(Drag {
                token,
                geometry,
                span,
                offset,
                gesture,
                selection_changed: ui.editor.with_untracked(Editor::selection) != selection_before,
                position: None,
                valid: false,
            }));
            drag_move(ui, event);
        }
        None => {
            let _ = element.release_pointer_capture(event.pointer_id());
        }
    }
}

fn save_selection(
    ui: Ui,
    drag: Drag,
) {
    // Start cannot serialize while its pointer is active. After a legitimate
    // changed selection resolves without a layout edit, schedule that save now.
    if drag.selection_changed
        && !ui.conflict.get_untracked()
        && ui
            .editor
            .with_untracked(|e| e.save_status() == SaveStatus::Dirty && e.action_readiness().block.is_none())
    {
        io::save(ui, false);
    }
}

pub fn cancel(ui: Ui) {
    let drag = ui.drag.get_untracked();
    let mut cancelled = false;
    if let Some(drag) = drag {
        ui.editor.update(|e| {
            cancelled = e.cancel_pointer(drag.token).is_ok();
        });
        ui.drag.set(None);
    }
    release_capture(ui);
    if cancelled && let Some(drag) = drag {
        save_selection(ui, drag);
    }
}

fn release_capture(ui: Ui) {
    let owner = ui.capture.get_untracked();
    ui.capture.set(None);
    if let Some((id, element)) = owner {
        let _ = element.release_pointer_capture(id);
    }
}

pub fn cancel_event(
    ui: Ui,
    event: web_sys::PointerEvent,
) {
    if ui
        .drag
        .get_untracked()
        .is_some_and(|d| d.token.pointer_id() == event.pointer_id())
    {
        cancel(ui);
    }
}

pub fn drag_move(
    ui: Ui,
    event: web_sys::PointerEvent,
) {
    let Some(mut drag) = ui.drag.get_untracked() else {
        return;
    };
    if event.pointer_id() != drag.token.pointer_id() {
        return;
    }
    if ui.editor.with_untracked(Editor::active_pointer) != Some(drag.token)
        || ui.pixel_geometry.get_untracked() != drag.geometry
    {
        cancel(ui);
        return;
    }
    event.prevent_default();
    drag.position = content_rect(ui).and_then(|rect| {
        drag.gesture
            .placement_in(
                drag.geometry,
                rect,
                (f64::from(event.client_x()), f64::from(event.client_y())),
                drag.token.target(),
                drag.span,
                drag.offset,
            )
            .ok()
            .flatten()
    });
    drag.valid = drag.position.is_some_and(|p| {
        ui.editor.with_untracked(|e| {
            geometry::validate_draft_drop(e.editing_config(), e.selection().view, drag.token.target(), p).is_ok()
        })
    });
    ui.drag.set(Some(drag));
}

pub fn drag_end(
    ui: Ui,
    event: web_sys::PointerEvent,
) {
    let Some(drag) = ui.drag.get_untracked() else {
        return;
    };
    if event.pointer_id() != drag.token.pointer_id() {
        return;
    }
    drag_move(ui, event.clone());
    let Some(drag) = ui.drag.get_untracked() else {
        return;
    };
    ui.drag.set(None);
    release_capture(ui);
    if let Some(position) = drag.position {
        if ui.run(move |e| e.commit_pointer(drag.token, event.pointer_id(), position)) {
            save_selection(ui, drag);
        }
    } else {
        let mut cancelled = false;
        ui.editor.update(|e| {
            cancelled = e.cancel_pointer(drag.token).is_ok();
        });
        if cancelled {
            save_selection(ui, drag);
        }
        if drag.gesture.is_dragging() {
            ui.notice
                .set("Drop inside the display grid. Your layout is unchanged.".into());
        }
    }
}

pub fn install_playback(ui: Ui) {
    Effect::new(move |_| {
        let pointer = ui.editor.with(Editor::active_pointer);
        let geometry = ui.pixel_geometry.get();
        if ui
            .drag
            .get_untracked()
            .is_some_and(|drag| Some(drag.token) != pointer || drag.geometry != geometry)
        {
            cancel(ui);
        }
    });
    use dashboard_configurator::timing::Playback;
    use leptos::leptos_dom::helpers::set_interval_with_handle;
    Effect::new(move |_| {
        if !ui.playing.get() {
            return;
        }
        let mut clock = Playback::new(ui.sample_ms.get_untracked());
        clock.start(now());
        let clock = RwSignal::new(clock);
        // The interval only schedules draws. Samples come from monotonic time,
        // so stalls/background throttling do not slow the simulated engine.
        match set_interval_with_handle(
            move || {
                let _ = clock.try_update(|c| {
                    let mut sample = c.sample(now());
                    if ui.dock.mode.get_untracked() == PreviewMode::Startup {
                        let end = ui.startup.profile.get_untracked().total_duration();
                        sample = sample.min(end);
                        if sample == end {
                            let _ = ui.playing.try_set(false);
                        }
                    }
                    let _ = ui.sample_ms.try_set(sample);
                });
            },
            std::time::Duration::from_millis(50),
        ) {
            Ok(handle) => {
                on_cleanup(move || {
                    handle.clear();
                    let _ = clock.try_update(|c| {
                        let sample = c.pause(now());
                        let sample = if ui.dock.mode.get_untracked() == PreviewMode::Startup {
                            sample.min(ui.startup.profile.get_untracked().total_duration())
                        } else {
                            sample
                        };
                        let _ = ui.sample_ms.try_set(sample);
                    });
                })
            }
            Err(_) => {
                ui.playing.set(false);
                ui.notice.set("Playback timer unavailable. Use Next sample.".into());
            }
        }
    });
}

fn add(
    ui: Ui,
    channel: Channel,
) {
    if let Some(placement) = ui.free(Span::OneByOne, None) {
        let view = ui.editor.with_untracked(|e| e.selection().view);
        ui.apply(Edit::AddWidget {
            view,
            channel,
            placement,
        });
    } else {
        ui.notice
            .set("This view is full. Remove a widget or add a new view first.".into());
    }
}

#[component]
pub fn Library(ui: Ui) -> impl IntoView {
    let search = RwSignal::new(String::new());
    view! {
        <aside class="library panel"><div class="panel-heading"><p class="eyebrow">"SENSOR LIBRARY"</p><h2>"Start with a signal."</h2><p>"Drag onto the grid, or use + to add."</p></div>
            <label class="search"><span aria-hidden="true">"⌕"</span><input type="search" aria-label="Find a sensor" placeholder="Find a sensor" on:input=move |e|search.set(event_target_value(&e))/></label>
            <div class="library-list"><For each=move || {Channel::ALL.into_iter().filter(|c|format!("{} {:?}",c.label(),c).to_lowercase().contains(&search.get().to_lowercase())).collect::<Vec<_>>()} key=|c|c.index() children=move |channel|view!{
                <div class="library-item"><button class="sensor-drag" aria-label=format!("Drag {}",channel.label()) disabled=move ||ui.inspection_active()
                    on:pointerdown=move |e|start(ui,e,PointerTarget::Library(channel)) on:click=move |e|{if e.detail()==0{add(ui,channel);}}>
                    <span class="sensor-mark" aria-hidden="true">{match channel{Channel::Boost=>"↗",Channel::Lambda=>"λ",Channel::Battery=>"ϟ",_=>"°"}}</span>
                    <span><b>{channel.label()}</b><small>{match channel{Channel::Boost=>"Relative pressure",Channel::Lambda=>"Mixture + optional AFR",Channel::Battery=>"ECU module voltage",Channel::Water=>"Coolant temperature",Channel::Oil=>"Engine oil temperature",Channel::Dsg=>"DSG oil temperature",Channel::Iat=>"Intake air temperature",Channel::Egt=>"Exhaust temperature"}}</small></span>
                    <span class="grip" aria-hidden="true">"⠿"</span>
                </button><button class="library-add" aria-label=format!("Add {}",channel.label()) disabled=move ||ui.inspection_active() on:click=move |_|add(ui,channel)>"+"</button></div>
            }/></div>
            <div class="library-note"><span class="chip">"HARDWARE BOUNDS"</span><p>"Eight cells. Three sizes. Every view checked before export."</p><div class="mini-grid" style=move ||format!("grid-template-columns:repeat({},1fr)",ui.pixel_geometry.get().columns()) aria-hidden="true">{(0..8).map(|_|view!{<i></i>}).collect_view()}</div><p class="hint">"Encoder: rotate for views, click for Full / Dim / Off. Debug and Logs keep their controls."</p></div>
        </aside>
    }
}

fn style(
    geometry: Geometry,
    placement: Placement,
) -> String {
    placement.rect_in(geometry).map_or("display:none".into(), |r| {
        format!(
            "left:{}%;top:{}%;width:{}%;height:{}%",
            f64::from(r.x) * 100. / f64::from(geometry.width()),
            f64::from(r.y) * 100. / f64::from(geometry.height()),
            f64::from(r.width) * 100. / f64::from(geometry.width()),
            f64::from(r.height) * 100. / f64::from(geometry.height())
        )
    })
}

#[component]
pub fn Canvas(ui: Ui) -> impl IntoView {
    let preview = RwSignal::new(Preview::default());
    let revision = Memo::new(move |_| ui.editor.with(Editor::revision));
    let json = Memo::new(move |_| json_editor::projection(ui));
    let source = Memo::new(move |_| {
        ui.dock
            .sim
            .with(dashboard_configurator::simulator::ControlSimulator::source_key)
    });
    Effect::new(move |_| {
        revision.get();
        json.get();
        source.get();
        ui.dock.inspection_stamp.get();
        ui.psi.get();
        ui.warnings.get();
        ui.sample_ms.get();
        ui.sample_set.get();
        ui.startup.profile.get();
        ui.startup.reduced.get();
        ui.dock.mode.get();
        if ui.dock.mode.get_untracked() == PreviewMode::Simulation && ui.inspection_active() {
            ui.dock.sim.with(|s| {
                s.display_session().page();
                s.display_now_ms();
            });
        }
        if ui.dock.mode.get_untracked() == PreviewMode::Simulation
            && !ui.inspection_active()
            && ui.json.pixels.get_untracked().is_none()
        {
            return;
        }
        // JSON changes have their own identity; enter the bounded controls
        // projection before capturing a source callback, without new accepted state.
        ui.editor.with_untracked(|e| ui.dock.synchronize(ui, e));
        // Mount/readiness is reactive too. Multiple input events share one draw.
        if ui.canvas.get().is_none() {
            return;
        }
        let Some(callback) = super::simulation::capture_callback(ui) else {
            return;
        };
        let mut request = None;
        ui.dock
            .callbacks
            .update_value(|callbacks| request = callbacks.queue_appearance());
        let Some(request) = request else {
            return;
        };
        match request_animation_frame_with_handle(move || {
            if !super::simulation::current_callback(ui, callback) {
                return;
            }
            let mut admitted = false;
            let _ = ui
                .dock
                .callbacks
                .try_update_value(|callbacks| admitted = callbacks.finish_appearance(request));
            if !admitted {
                return;
            }
            let Some(_canvas) = ui.canvas.get_untracked() else {
                return;
            };
            if ui.dock.mode.get_untracked() == PreviewMode::Simulation
                && !ui.inspection_active()
                && ui.json.pixels.get_untracked().is_none()
            {
                return;
            }
            // Appearance/invalid inspection pixels never consume the retained timing candidate.
            ui.dock.consumed.set(false);
            let started = now();
            let result = ui.editor.with_untracked(|e| {
                let options = PreviewOptions {
                    sample_ms: ui.sample_ms.get_untracked(),
                    boost_psi: ui.psi.get_untracked(),
                    warnings: ui.warnings.get_untracked(),
                    values: ui.sample_set.get_untracked().values(),
                };
                let mut result = Ok(());
                preview.update(|p| {
                    result = (|| {
                        let simulation = ui.dock.mode.get_untracked() == PreviewMode::Simulation;
                        let draft = e.diagnostic();
                        if ui.dock.mode.get_untracked() == PreviewMode::Startup {
                            p.render_startup_asset(
                                e.preview(),
                                e.selection().view,
                                ui.startup.profile.get_untracked(),
                                ui.sample_ms.get_untracked(),
                                ui.startup.reduced.get_untracked() || e.preview().config().reduced_motion,
                                e.startup_artwork().map(|a| a.converted()),
                            )
                            .map(|_| ())
                        } else if e.has_pending_json() {
                            use dashboard_configurator::json_draft::{JsonState, inspection_view};
                            match e.json_draft().map(|draft| draft.state()) {
                                Some(JsonState::Candidate { config, .. }) => {
                                    let preferred = if simulation {
                                        ui.dock.sim.with_untracked(|sim| sim.display_session().view_id())
                                    } else {
                                        e.selection().view
                                    };
                                    let view = inspection_view(config, preferred).unwrap_or(e.selection().view);
                                    match ui.json.with_candidate(e, |candidate| {
                                        if simulation {
                                            ui.dock.sim.with_untracked(|sim| {
                                                p.render_candidate_simulation(sim, candidate).map(|_| ())
                                            })
                                        } else {
                                            p.render(candidate.config(), view, options).map(|_| ())
                                        }
                                    }) {
                                        Ok(rendered) => rendered,
                                        Err(EditorError::Validation(error)) => {
                                            let candidate = dashboard_configurator::diagnostic::Draft {
                                                config: config.as_ref().clone(),
                                                raw: Vec::new(),
                                                error: EditorError::Validation(error),
                                            };
                                            if simulation {
                                                ui.dock.sim.with_untracked(|sim| {
                                                    p.render_inspection(sim, Some(&candidate)).map(|_| ())
                                                })
                                            } else {
                                                let safe_view = inspection_view(e.preview().config(), view)
                                                    .unwrap_or(e.preview().config().boot_view);
                                                p.render_diagnostic(e.preview(), &candidate, safe_view, options)
                                                    .map(|_| ())
                                            }
                                        }
                                        Err(_) => Err(dashboard_configurator::preview::PreviewError::InspectionOnly),
                                    }
                                }
                                // No former candidate can stand in for current unparsed/invalid text.
                                _ if simulation => {
                                    ui.dock.sim.with_untracked(|sim| p.render_simulation(sim).map(|_| ()))
                                }
                                _ => p.render(e.preview(), e.selection().view, options).map(|_| ()),
                            }
                        } else if simulation && ui.invalid() {
                            ui.dock
                                .sim
                                .with_untracked(|sim| p.render_inspection(sim, draft.as_ref()).map(|_| ()))
                        } else if simulation && ui.json.pixels.get_untracked().is_some() {
                            // Discard / identical Apply restores read-only accepted pixels;
                            // timing pending bytes and completion metadata remain untouched.
                            ui.dock.sim.with_untracked(|sim| p.render_simulation(sim).map(|_| ()))
                        } else {
                            match draft.as_ref() {
                                Some(draft) => p.render_diagnostic(e.preview(), draft, e.selection().view, options),
                                None => p.render(e.preview(), e.selection().view, options),
                            }
                            .map(|_| ())
                        }
                        .map_err(RenderIssue::Preview)?;
                        super::controls::write_timing_canvas(ui.canvas, p.rgba(), p.geometry())?;
                        ui.pixel_geometry.set(p.geometry());
                        ui.pixel_fit_issues.set(p.fit_issues().to_vec());
                        Ok(())
                    })();
                    ui.observe_cache(p.cache_exhaustion_status());
                });
                result
            });
            if let Err(error) = result {
                ui.render_issue(RenderLane::Appearance, Err(error));
                ui.preview_error.set(Some(error.to_string()));
                return;
            }
            ui.render_outcome(RenderLane::Appearance, true);
            ui.preview_error.set(None);
            ui.dock
                .pixels_key
                .set(Some(ui.editor.with_untracked(Editor::accepted_config_key)));
            ui.json
                .pixels
                .set(json_editor::projection(ui).filter(|_| ui.json_pending()));
            ui.dock
                .displayed
                .set(if ui.dock.mode.get_untracked() == PreviewMode::Startup {
                    PreviewMode::Startup
                } else if ui.dock.mode.get_untracked() == PreviewMode::Simulation && !ui.inspection_active() {
                    PreviewMode::Simulation
                } else {
                    PreviewMode::Appearance
                });
            ui.dock.invalid_pixels.set(
                ui.invalid()
                    || json_editor::projection(ui).is_some_and(|p| {
                        matches!(
                            p.phase(),
                            dashboard_configurator::json_draft::JsonPhase::SemanticInvalid
                        )
                    }),
            );
            metrics::first_frame(ui);
            let submitted = now();
            let render = submitted - started;
            ui.render_ms.set(render);
            if let Some(start) = ui.pending_at.get_untracked() {
                ui.pending_at.set(None);
                let (widgets, views) = ui
                    .editor
                    .with_untracked(|e| (e.preview().config().widgets.len(), e.preview().config().views.len()));
                let warnings = ui.warnings.get_untracked();
                let trace_permit = metrics::begin(ui, start);
                // Next animation boundary includes the browser's opportunity
                // to present the canvas, not just Rust rendering duration.
                metrics::schedule_record(ui, trace_permit, start, submitted, render, (widgets, views), warnings);
            }
        }) {
            Ok(handle) => {
                ui.dock.callbacks.update_value(|callbacks| {
                    if let Err(handle) = callbacks.set_appearance_handle(request, handle) {
                        handle.cancel();
                    }
                })
            }
            Err(_) => {
                ui.dock.callbacks.update_value(|callbacks| {
                    callbacks.finish_appearance(request);
                });
                ui.render_issue(RenderLane::Appearance, Err(RenderIssue::Generic));
            }
        }
    });
    view! {
        <div class="invalid-preview-heading" hidden=move ||!ui.editor.with(Editor::artwork_geometry_mismatch) role="alert"><strong>"SPLASH SHAPE MISMATCH"</strong><span>"Original artwork retained. Reconvert it in Device → Startup, reimport its source, or Undo. Save, export and build are blocked."</span><button on:click=move |_|workspaces::open(ui,Workspace::Device,GroupId::Bindings)>"Open Device settings"</button></div>
        <div class="json-preview-status" hidden=move ||json.get().is_none() role="status"><strong>{move ||json_editor::label(ui)}</strong><span hidden=move ||!ui.json_pending()>"Apply or Discard to resume accepted edits, export and timing completion."</span><button hidden=move ||!ui.json_pending() on:click=move |_|json_editor::open(ui)>"Resolve JSON edits"</button></div>
        <div class="invalid-preview-heading" hidden=move ||!ui.editor.with(Editor::has_startup_draft) role="status"><strong>{move ||ui.editor.with(Editor::startup_draft_label).unwrap_or_default()}</strong><span>"Apply or discard timing in Device → Startup. Save, export and build blocked."</span></div>
        <div class="invalid-preview-heading" hidden=move || !ui.invalid() && !ui.dock.invalid_pixels.get() role="status"><strong>"INVALID PREVIEW"</strong><span>{move ||if ui.invalid(){"Rejected draft · save and export blocked"}else{"Last inspection pixels · restore timing pixels in Diagnostics"}}</span></div>
        <p class="hint">{move ||if ui.dock.displayed.get()==PreviewMode::Startup{"Startup preview · shared Rust pixels · synthetic OBD status"}else if ui.invalid() ||ui.dock.invalid_pixels.get(){"INVALID SIMULATION / PREVIEW · candidate only; accepted acquisition and pending pixels retained"}else if ui.dock.pixels_key.get()!=Some(ui.editor.with(Editor::accepted_config_key)){"Last frame · configuration changed · submit a current frame in Diagnostics"}else if ui.dock.displayed.get()==PreviewMode::Simulation {"Synthetic Dashboard / Debug / Logs · browser completion is not physical scanout"}else if ui.dock.mode.get()==PreviewMode::Simulation {"Last appearance frame · submit or restore timing pixels in Diagnostics"}else{"Appearance preview · generated history"}}</p>
        <p class="draft-alert" role="status" hidden=move ||ui.pixel_fit_issues.get().is_empty()>{move ||format!("Content fit needs review in widgets {:?}. The current sample with reserved warning space exceeds a compact lane. FIT! marks an overflowing value; try a larger tile or fewer detail lanes. Other runtime values can have different widths.",ui.pixel_fit_issues.get())}</p>
        <div class="display-stage" class:portrait=move ||ui.pixel_geometry.get()==Geometry::Portrait class:invalid-preview=move ||ui.invalid() ||ui.dock.invalid_pixels.get()><div class="display-rails" aria-hidden="true"><span>"A"</span><span>"B"</span></div>
            <div class="display-frame"><div class="screen">
                <canvas id="studio-preview-canvas" node_ref=ui.canvas width="320" height="240" aria-label=move ||if let Some(label)=ui.editor.with(Editor::startup_draft_label){label}else if ui.json_pending(){json_editor::label(ui)}else if ui.invalid() ||ui.dock.invalid_pixels.get(){"INVALID SIMULATION / PREVIEW. Inspection pixels; retained timing pixels require explicit restoration."}else if ui.dock.displayed.get()==PreviewMode::Simulation{"Persistent all-screen synthetic timing simulation"}else{"Exact Pico dashboard appearance with synthetic values"}></canvas>
                <div class="preview-error" hidden=move ||ui.preview_error.get().is_none() role="alert"><strong>"PREVIEW ERROR"</strong><p>{move ||ui.preview_error.get().unwrap_or_default()}</p><p>"Last frame only. Change the test values or reload to retry."</p></div>
                <div id="widget-grid" class="canvas-hits" tabindex="-1" aria-label="Widget placement grid" hidden=move ||ui.workspace.with(|s|s.active()!=Workspace::Views) ||ui.dock.mode.get()!=PreviewMode::Appearance ||ui.editor.with(|e|e.editing_config().view_orientation(e.selection().view).map(|o|o.geometry()))!=Some(ui.pixel_geometry.get())>
                    <For each=move ||ui.editor.with(|e|e.editing_config().widgets.iter().filter(|w|w.view==e.selection().view).map(|w|w.id).collect::<Vec<_>>()) key=|id|*id children=move |id|view!{
                        <button id=format!("widget-{id}") class="widget-hit" disabled=move ||ui.layout_locked() class:selected=move ||ui.editor.with(|e|e.selection().widget==Some(id))
                            style=move ||ui.canvas_widget(id).map_or("display:none".into(),|w|style(ui.pixel_geometry.get(),w.placement))
                            aria-label=move ||ui.canvas_widget(id).map_or(String::new(),|w|format!("Select {} widget",w.label))
                            title="Drag to move. Right-click or Shift+F10 for actions. Arrow keys move one cell. Delete removes."
                            aria-haspopup="menu" aria-expanded=move ||ui.menu.with(|menu|menu.as_ref().is_some_and(|m|m.actions.target()==id)).to_string()
                            on:contextmenu=move |e|{ui.inspection.set(None);widget_menu::open_pointer(ui,id,e)}
                            on:pointerdown=move |e|start(ui,e,PointerTarget::Widget(id))
                            on:click=move |event|{if event.detail()==0 && ui.run(move |e|e.select(Selection{view:e.selection().view,widget:Some(id)})) {ui.inspection.set(None);}}
                            on:keydown=move |e|{
                                if e.key()=="ContextMenu" || (e.key()=="F10" && e.shift_key()) { e.prevent_default(); e.stop_propagation(); ui.inspection.set(None); widget_menu::open_keyboard(ui,id); return; }
                                if e.key()=="Delete"{e.prevent_default();ui.apply(Edit::DeleteWidget(id));return;}
                                use dashboard_configurator::widget_actions::Direction;
                                let direction=match e.key().as_str(){"ArrowLeft"=>Direction::Left,"ArrowRight"=>Direction::Right,"ArrowUp"=>Direction::Up,"ArrowDown"=>Direction::Down,_=>return};
                                e.prevent_default();
                                if let Some(placement)=ui.canvas_widget(id).and_then(|w|direction.placement_in(ui.pixel_geometry.get(),w.placement)){ui.apply(Edit::PlaceWidget{id,placement});}
                            }><span class="selection-corner"></span></button>
                    }/>
                    <div class="drop-guide" class:invalid=move ||ui.drag.get().is_some_and(|d|!d.valid)
                        style=move ||ui.drag.get().and_then(|d|d.position).map_or("display:none".into(),|p|style(ui.pixel_geometry.get(),p)) aria-hidden="true"></div>
                    {move ||ui.editor.with(|e|e.diagnostic().map(|draft|{
                        let ids=draft.affected(e.preview(),e.selection().view);
                        draft.config.widgets.iter().filter(|w|w.view==e.selection().view && ids.contains(&w.id)).map(|w|view!{
                            <div class="invalid-widget" style=style(ui.pixel_geometry.get(),w.placement)><span>{format!("! INVALID #{}",w.id)}</span></div>
                        }).collect_view()
                    }))}
                </div>
            </div></div><div class="display-rails" aria-hidden="true"><span>"X"</span><span>"Y"</span></div>
        </div>
    }
}
