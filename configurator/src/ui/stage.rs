use dashboard_configurator::geometry::{self, ContentRect, GrabOffset};
use dashboard_configurator::preview::{Preview, PreviewOptions};
use leptos::leptos_dom::helpers::request_animation_frame;
use wasm_bindgen::Clamped;

use super::*;

#[derive(Clone, Copy)]
pub struct Drag {
    token: PointerToken,
    span: Span,
    offset: GrabOffset,
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
    event.prevent_default();
    event.stop_propagation();
    widget_menu::close(ui, false);
    let (span, offset) = match target {
        PointerTarget::Library(_) => (Span::OneByOne, GrabOffset::center(Span::OneByOne)),
        PointerTarget::Widget(id) => {
            let Some(widget) = ui.widget(id) else {
                return;
            };
            let Some(rect) = content_rect(ui) else {
                return;
            };
            let Ok(offset) = geometry::grab_offset(
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
    // Selecting a widget is persistent even when its drag is cancelled or
    // commits the same placement. Replace any pending save at selection time.
    ui.run(|editor| {
        accepted = Some(editor.begin_pointer(event.pointer_id(), target)?);
        Ok(editor.revision())
    });
    match accepted {
        Some(token) => {
            if let Some(button) = element.dyn_ref::<web_sys::HtmlElement>() {
                let _ = button.focus();
            }
            ui.capture.set(Some((event.pointer_id(), element)));
            ui.drag.set(Some(Drag {
                token,
                span,
                offset,
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

pub fn cancel(ui: Ui) {
    if let Some(drag) = ui.drag.get_untracked() {
        ui.editor.update(|e| {
            let _ = e.cancel_pointer(drag.token);
        });
        ui.drag.set(None);
    }
    release_capture(ui);
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
    if ui.editor.with_untracked(Editor::active_pointer) != Some(drag.token) {
        cancel(ui);
        return;
    }
    event.prevent_default();
    drag.position = content_rect(ui).and_then(|rect| {
        if matches!(drag.token.target(), PointerTarget::Library(_)) {
            // A new compact widget belongs to the pointed cell. It has no
            // existing grab anchor: requiring its center made half of the
            // first cell unusable and was surprising near outer edges.
            return geometry::cell_at(rect, f64::from(event.client_x()), f64::from(event.client_y()))
                .ok()
                .map(|cell| {
                    Placement {
                        column: cell.column,
                        row: cell.row,
                        span: Span::OneByOne,
                    }
                });
        }
        geometry::drop_placement(
            rect,
            f64::from(event.client_x()),
            f64::from(event.client_y()),
            drag.span,
            drag.offset,
        )
        .ok()
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
        ui.run(move |e| e.commit_pointer(drag.token, event.pointer_id(), position));
    } else {
        ui.editor.update(|e| {
            let _ = e.cancel_pointer(drag.token);
        });
        ui.notice
            .set("Drop inside the display grid. Your layout is unchanged.".into());
    }
}

pub fn install_playback(ui: Ui) {
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
                    let _ = ui.sample_ms.try_set(c.sample(now()));
                });
            },
            std::time::Duration::from_millis(50),
        ) {
            Ok(handle) => {
                on_cleanup(move || {
                    handle.clear();
                    let _ = clock.try_update(|c| {
                        let _ = ui.sample_ms.try_set(c.pause(now()));
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
                <div class="library-item"><button class="sensor-drag" aria-label=format!("Drag {}",channel.label()) disabled=move ||ui.invalid()
                    on:pointerdown=move |e|start(ui,e,PointerTarget::Library(channel)) on:click=move |e|{if e.detail()==0{add(ui,channel);}}>
                    <span class="sensor-mark" aria-hidden="true">{match channel{Channel::Boost=>"↗",Channel::Lambda=>"λ",Channel::Battery=>"ϟ",_=>"°"}}</span>
                    <span><b>{channel.label()}</b><small>{match channel{Channel::Boost=>"Relative pressure",Channel::Lambda=>"Mixture + optional AFR",Channel::Battery=>"ECU module voltage",Channel::Water=>"Coolant temperature",Channel::Oil=>"Engine oil temperature",Channel::Dsg=>"DSG oil temperature",Channel::Iat=>"Intake air temperature",Channel::Egt=>"Exhaust temperature"}}</small></span>
                    <span class="grip" aria-hidden="true">"⠿"</span>
                </button><button class="library-add" aria-label=format!("Add {}",channel.label()) disabled=move ||ui.invalid() on:click=move |_|add(ui,channel)>"+"</button></div>
            }/></div>
            <div class="library-note"><span class="chip">"HARDWARE BOUNDS"</span><p>"Eight cells. Three sizes. Every view checked before export."</p><div class="mini-grid" aria-hidden="true">{(0..8).map(|_|view!{<i></i>}).collect_view()}</div><p class="hint">"Encoder: rotate for views, click for Full / Dim / Off. Debug and Logs keep their controls."</p></div>
        </aside>
    }
}

fn style(placement: Placement) -> String {
    placement.rect().map_or("display:none".into(), |r| {
        format!(
            "left:{}%;top:{}%;width:{}%;height:{}%",
            f64::from(r.x) / 3.2,
            f64::from(r.y) / 2.4,
            f64::from(r.width) / 3.2,
            f64::from(r.height) / 2.4
        )
    })
}

#[component]
pub fn Canvas(ui: Ui) -> impl IntoView {
    let preview = RwSignal::new(Preview::default());
    let queued = RwSignal::new(false);
    let revision = Memo::new(move |_| ui.editor.with(Editor::revision));
    Effect::new(move |_| {
        revision.get();
        ui.psi.get();
        ui.warnings.get();
        ui.sample_ms.get();
        ui.sample_set.get();
        // Mount/readiness is reactive too. Multiple input events share one draw.
        if ui.canvas.get().is_none() || queued.get_untracked() {
            return;
        }
        queued.set(true);
        request_animation_frame(move || {
            if queued.try_set(false).is_some() {
                return;
            }
            let Some(canvas) = ui.canvas.get_untracked() else {
                return;
            };
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
                        let draft = e.diagnostic();
                        let pixels = match draft.as_ref() {
                            Some(draft) => p.render_diagnostic(e.preview(), draft, e.selection().view, options),
                            None => p.render(e.preview(), e.selection().view, options),
                        }
                        .map_err(|e| format!("Preview failed: {e:?}"))?;
                        let image = web_sys::ImageData::new_with_u8_clamped_array_and_sh(Clamped(pixels), 320, 240)
                            .map_err(|_| "Canvas image allocation failed".to_string())?;
                        let context = canvas
                            .get_context("2d")
                            .map_err(|_| "Canvas is unavailable".to_string())?
                            .ok_or_else(|| "Canvas is unavailable".to_string())?
                            .dyn_into::<web_sys::CanvasRenderingContext2d>()
                            .map_err(|_| "Canvas context is unavailable".to_string())?;
                        context
                            .put_image_data(&image, 0.0, 0.0)
                            .map_err(|_| "Canvas draw failed".to_string())?;
                        Ok(())
                    })();
                });
                result
            });
            if let Err(error) = result {
                ui.preview_error.set(Some(error));
                return;
            }
            ui.preview_error.set(None);
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
                let trace_id = metrics::begin(ui, start);
                // Next animation boundary includes the browser's opportunity
                // to present the canvas, not just Rust rendering duration.
                request_animation_frame(move || {
                    metrics::record(ui, trace_id, start, submitted, render, (widgets, views), warnings);
                });
            }
        });
    });
    view! {
        <div class="invalid-preview-heading" hidden=move || !ui.invalid() role="status"><strong>"INVALID PREVIEW"</strong><span>"Rejected draft · save and export blocked"</span></div>
        <div class="display-stage" class:invalid-preview=move ||ui.invalid()><div class="display-rails" aria-hidden="true"><span>"A"</span><span>"B"</span></div>
            <div class="display-frame"><div class="screen">
                <canvas node_ref=ui.canvas width="320" height="240" aria-label=move ||if ui.invalid(){"INVALID PREVIEW. Rejected draft with safe rendering fallbacks. Save and export blocked."}else{"Exact Pico dashboard preview with synthetic values"}></canvas>
                <div class="preview-error" hidden=move ||ui.preview_error.get().is_none() role="alert"><strong>"PREVIEW ERROR"</strong><p>{move ||ui.preview_error.get().unwrap_or_default()}</p><p>"Last frame only. Change the test values or reload to retry."</p></div>
                <div id="widget-grid" class="canvas-hits" tabindex="-1" aria-label="Widget placement grid">
                    <For each=move ||ui.editor.with(|e|e.editing_config().widgets.iter().filter(|w|w.view==e.selection().view).map(|w|w.id).collect::<Vec<_>>()) key=|id|*id children=move |id|view!{
                        <button id=format!("widget-{id}") class="widget-hit" disabled=move ||ui.layout_locked() class:selected=move ||ui.editor.with(|e|e.selection().widget==Some(id))
                            style=move ||ui.widget(id).map_or("display:none".into(),|w|style(w.placement))
                            aria-label=move ||ui.widget(id).map_or(String::new(),|w|format!("Select {} widget",w.label))
                            title="Drag to move. Right-click or Shift+F10 for actions. Arrow keys move one cell. Delete removes."
                            aria-haspopup="menu" aria-expanded=move ||ui.menu.with(|menu|menu.as_ref().is_some_and(|m|m.actions.target()==id)).to_string()
                            on:contextmenu=move |e|widget_menu::open_pointer(ui,id,e)
                            on:pointerdown=move |e|start(ui,e,PointerTarget::Widget(id))
                            on:click=move |event|{if event.detail()==0 {ui.run(move |e|e.select(Selection{view:e.selection().view,widget:Some(id)}));}}
                            on:keydown=move |e|{
                                if e.key()=="ContextMenu" || (e.key()=="F10" && e.shift_key()) { e.prevent_default(); e.stop_propagation(); widget_menu::open_keyboard(ui,id); return; }
                                if e.key()=="Delete"{e.prevent_default();ui.apply(Edit::DeleteWidget(id));return;}
                                use dashboard_configurator::widget_actions::Direction;
                                let direction=match e.key().as_str(){"ArrowLeft"=>Direction::Left,"ArrowRight"=>Direction::Right,"ArrowUp"=>Direction::Up,"ArrowDown"=>Direction::Down,_=>return};
                                e.prevent_default();
                                if let Some(placement)=ui.widget(id).and_then(|w|direction.placement(w.placement)){ui.apply(Edit::PlaceWidget{id,placement});}
                            }><span class="selection-corner"></span></button>
                    }/>
                    <div class="drop-guide" class:invalid=move ||ui.drag.get().is_some_and(|d|!d.valid)
                        style=move ||ui.drag.get().and_then(|d|d.position).map_or("display:none".into(),style) aria-hidden="true"></div>
                    {move ||ui.editor.with(|e|e.diagnostic().map(|draft|{
                        let ids=draft.affected(e.preview(),e.selection().view);
                        draft.config.widgets.iter().filter(|w|w.view==e.selection().view && ids.contains(&w.id)).map(|w|view!{
                            <div class="invalid-widget" style=style(w.placement)><span>{format!("! INVALID #{}",w.id)}</span></div>
                        }).collect_view()
                    }))}
                </div>
            </div></div><div class="display-rails" aria-hidden="true"><span>"X"</span><span>"Y"</span></div>
        </div>
    }
}
