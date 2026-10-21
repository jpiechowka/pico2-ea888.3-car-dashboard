//! Canvas-current context-menu adapter with snapshot authority and deferred focus restoration.
use dashboard_configurator::geometry::{ContentRect, panel_point_in};
use dashboard_configurator::widget_actions::{Direction, WidgetMenu, menu_position};
use leptos::leptos_dom::helpers::request_animation_frame;

use super::*;

#[derive(Clone)]
/// Open browser popup with reducer-owned actions and viewport-relative anchor.
pub struct Menu {
    /// Revision/selection-bound move/delete/overlap-selection authority.
    pub actions: WidgetMenu,
    /// Client CSS-pixel origin, measured from pointer or widget centre.
    anchor: (f64, f64),
}

/// Returns the mounted HTML widget node for `id`, or None without window/document/matching HTML element.
fn widget_element(id: u16) -> Option<web_sys::HtmlElement> {
    web_sys::window()?
        .document()?
        .get_element_by_id(&format!("widget-{id}"))?
        .dyn_into()
        .ok()
}

/// Clears `ui`'s menu; when `restore_focus` is true, best-effort focuses its former target if still mounted.
/// Missing nodes/focus failures are ignored; does nothing when no menu is open.
pub fn close(
    ui: Ui,
    restore_focus: bool,
) {
    let menu = ui.menu.get_untracked();
    if let Some(menu) = menu {
        ui.menu.set(None);
        if restore_focus && let Some(element) = widget_element(menu.actions.target()) {
            let _ = element.focus();
        }
    }
}

/// Opens `ui`'s menu for widget `id` at client CSS-pixel `anchor` only while canvas identity is current.
/// Cancels pointer state/old menu, maps overlap point through displayed geometry, then selects through the reducer.
/// Selection errors use normal UI reporting and leave the menu closed; coordinate failure retains target-only actions.
fn open(
    ui: Ui,
    id: u16,
    anchor: (f64, f64),
) {
    if !ui.canvas_current() {
        return;
    }
    let pixels_before = ui.canvas_identity();
    stage::cancel(ui);
    close(ui, false);
    let point = ui.canvas.get_untracked().and_then(|canvas| {
        let r = canvas.get_bounding_client_rect();
        panel_point_in(
            ui.pixel_geometry.get_untracked(),
            ContentRect {
                left: r.left(),
                top: r.top(),
                width: r.width(),
                height: r.height(),
            },
            anchor.0,
            anchor.1,
        )
        .ok()
    });
    let mut actions = None;
    ui.run(|editor| {
        actions = Some(WidgetMenu::open(editor, id, point)?);
        Ok(editor.revision())
    });
    if let Some(actions) = actions {
        ui.canvas_selection(pixels_before);
        ui.menu.set(Some(Menu { actions, anchor }));
    }
}

/// Consumes context-menu `event` defaults/propagation and opens `ui` widget `id` at its client coordinates if
/// canvas-current.
pub fn open_pointer(
    ui: Ui,
    id: u16,
    event: web_sys::MouseEvent,
) {
    event.prevent_default();
    event.stop_propagation();
    open(ui, id, (f64::from(event.client_x()), f64::from(event.client_y())));
}

/// Opens `ui` widget `id`'s menu at its mounted element centre; missing elements or stale canvas leave it closed.
pub fn open_keyboard(
    ui: Ui,
    id: u16,
) {
    if let Some(element) = widget_element(id) {
        let r = element.get_bounding_client_rect();
        open(ui, id, (r.left() + r.width() / 2.0, r.top() + r.height() / 2.0));
    }
}

/// Installs `ui` menu invalidation on stale editor/canvas, resize, scroll and blur, with owner-cleanup listener
/// removal. Requires an active reactive owner; invalidation closes without stealing focus.
pub fn install(ui: Ui) {
    // A menu is a snapshot of one editor revision. File import, undo, a field
    // edit or another selection must never redirect an old menu action.
    Effect::new(move |_| {
        let current = ui.editor.with(|editor| {
            ui.menu
                .with(|menu| menu.as_ref().is_none_or(|m| m.actions.is_current(editor)))
        });
        if !current || !ui.canvas_current() {
            close(ui, false);
        }
    });
    let resize = window_event_listener(leptos::ev::resize, move |_| close(ui, false));
    let scroll = window_event_listener(leptos::ev::scroll, move |_| close(ui, false));
    let blur = window_event_listener(leptos::ev::blur, move |_| close(ui, false));
    on_cleanup(move || {
        resize.remove();
        scroll.remove();
        blur.remove();
    });
}

/// Schedules best-effort focus in `ui` for optional widget `id`, falling back to the grid after reactive DOM updates.
/// Callback checks original field token, closed-menu state and signal lifetime; stale/disposed requests do nothing.
fn focus_target(
    ui: Ui,
    id: Option<u16>,
) {
    // Deletion and selection update the DOM at the next reactive boundary.
    let token = ui.editor.with_untracked(Editor::field_token);
    request_animation_frame(move || {
        if ui.editor.try_with_untracked(|e| e.field_token() == token) != Some(true)
            || ui.menu.try_with_untracked(Option::is_none) != Some(true)
        {
            return;
        }
        let element = id.and_then(widget_element).or_else(|| {
            web_sys::window()?
                .document()?
                .get_element_by_id("widget-grid")?
                .dyn_into::<web_sys::HtmlElement>()
                .ok()
        });
        if let Some(element) = element {
            let _ = element.focus();
        }
    });
}

/// Executes `menu`'s `direction` move through `ui` only if canvas-current and reducer movement remains admissible.
/// Closes the menu, reports reducer results and schedules guarded target focus, including retained invalid repair
/// outcomes.
fn move_widget(
    ui: Ui,
    menu: &WidgetMenu,
    direction: Direction,
) {
    if !ui.canvas_current() || !ui.editor.with_untracked(|e| menu.can_move(e, direction)) {
        return;
    }
    close(ui, false);
    ui.run(|editor| menu.move_widget(editor, direction));
    focus_target(ui, Some(menu.target()));
}

/// Handles menu `event` for `ui` and mounted root `element`: Escape/Tab close, arrows/Home/End move menu-item focus.
/// Arrow navigation wraps and includes aria-disabled items; query/focus failures are ignored. Tab's native default
/// remains enabled.
fn keyboard(
    ui: Ui,
    element: web_sys::HtmlElement,
    event: web_sys::KeyboardEvent,
) {
    match event.key().as_str() {
        "Escape" => {
            event.prevent_default();
            event.stop_propagation();
            close(ui, true);
        }
        "Tab" => close(ui, true),
        "ArrowDown" | "ArrowUp" | "Home" | "End" => {
            event.prevent_default();
            event.stop_propagation();
            let Ok(items) = element.query_selector_all("[role=menuitem]") else {
                return;
            };
            let len = items.length();
            if len == 0 {
                return;
            }
            let active = web_sys::window()
                .and_then(|w| w.document())
                .and_then(|d| d.active_element());
            let current = (0..len)
                .find(|&i| {
                    items
                        .item(i)
                        .is_some_and(|item| active.as_ref().is_some_and(|active| item.is_same_node(Some(active))))
                })
                .unwrap_or(0);
            let next = match event.key().as_str() {
                "Home" => 0,
                "End" => len - 1,
                "ArrowUp" => {
                    if current == 0 {
                        len - 1
                    } else {
                        current - 1
                    }
                }
                _ => (current + 1) % len,
            };
            if let Some(item) = items
                .item(next)
                .and_then(|item| item.dyn_into::<web_sys::HtmlElement>().ok())
            {
                let _ = item.focus();
            }
        }
        _ => {}
    }
}

#[component]
/// Returns the reactive popup for `ui`'s current menu, or no menu content when closed.
pub fn Popup(
    /// Shared popup identity, pointer state and guarded widget-action reducer.
    ui: Ui
) -> impl IntoView {
    view! { {move || ui.menu.get().map(|menu| view! { <MenuContents ui menu/> })} }
}

#[component]
/// Returns measured popup content for captured `menu` in `ui`, with guarded deferred positioning/focus and reducer
/// actions. Invalid viewport/menu measurements close it; disposed/stale callbacks do not restore focus into a
/// replacement menu.
fn MenuContents(
    /// Shared popup identity, pointer state and guarded widget-action reducer.
    ui: Ui,
    /// Captured menu identity, target/action authority and viewport anchor.
    menu: Menu,
) -> impl IntoView {
    let node = NodeRef::<leptos::html::Div>::new();
    let position = RwSignal::new(None::<(f64, f64)>);
    let target = menu.actions.target();
    let name = menu
        .actions
        .widgets()
        .iter()
        .find(|w| w.0 == target)
        .map(|w| w.1.clone())
        .unwrap_or_default();
    let actions = StoredValue::new(menu.actions);
    let accessible_name = format!("Actions for {name} widget");
    Effect::new(move |_| {
        if node.get().is_none() {
            return;
        }
        request_animation_frame(move || {
            let Some(Some(element)) = node.try_get_untracked() else {
                return;
            };
            let Some(window) = web_sys::window() else {
                return;
            };
            let r = element.get_bounding_client_rect();
            // Root client dimensions exclude scrollbars; innerWidth includes
            // them and can position the right edge underneath the scrollbar.
            let viewport = window
                .document()
                .and_then(|d| d.document_element())
                .map(|root| (f64::from(root.client_width()), f64::from(root.client_height())));
            let fitted = viewport.and_then(|viewport| menu_position(menu.anchor, (r.width(), r.height()), viewport));
            if position.try_set(fitted).is_some() {
                return;
            }
            if fitted.is_none() {
                close(ui, true);
                return;
            }
            // Reactive visibility applies after this callback. Focusing a
            // still-hidden child is ignored by browsers. The next boundary
            // also checks mount/lifetime so a closed menu cannot steal focus.
            request_animation_frame(move || {
                let Some(Some(element)) = node.try_get_untracked() else {
                    return;
                };
                if ui
                    .menu
                    .try_with_untracked(|m| m.as_ref().is_some_and(|m| m.actions.target() == target))
                    != Some(true)
                {
                    return;
                }
                // ARIA menu items with aria-disabled remain focusable.
                if let Ok(Some(first)) = element.query_selector("[role=menuitem]")
                    && let Some(first) = first.dyn_ref::<web_sys::HtmlElement>()
                {
                    let _ = first.focus();
                }
            });
        });
    });
    view! {
        <div class="widget-menu-backdrop" on:pointerdown=move |e| { e.prevent_default(); e.stop_propagation(); close(ui, true); }
            on:contextmenu=move |e| { e.prevent_default(); close(ui, true); }></div>
        <div node_ref=node class="widget-menu" role="menu" aria-label=accessible_name
            style=move ||position.get().map_or("visibility:hidden;left:0;top:0".into(), |(x,y)|format!("left:{x}px;top:{y}px"))
            on:keydown=move |e| { if let Some(node)=node.get_untracked() { keyboard(ui, (*node).clone(), e); } }
            on:contextmenu=move |e|e.prevent_default()>
            <div class="widget-menu-title" role="presentation"><span>{name}</span><small>{format!("#{target}")}</small></div>
            {[(Direction::Left,"←","Move left"),(Direction::Right,"→","Move right"),(Direction::Up,"↑","Move up"),(Direction::Down,"↓","Move down")].into_iter().map(move |(direction,icon,label)|view!{
                <button role="menuitem" tabindex="-1" aria-disabled=move ||ui.editor.with(|e|!actions.with_value(|m|m.can_move(e,direction))).to_string()
                    on:click=move |_|actions.with_value(|m|move_widget(ui,m,direction))><span aria-hidden="true">{icon}</span>{label}</button>
            }).collect_view()}
            <div role="separator"></div>
            <button class="danger-text" role="menuitem" tabindex="-1" on:click=move |_|{
                if !ui.canvas_current() {return;}
                close(ui,false);
                actions.with_value(|m|{ui.run(|e|m.delete(e));});
                let next=ui.editor.with_untracked(|e|e.editing_config().widgets.iter().find(|w|w.view==e.selection().view).map(|w|w.id));
                focus_target(ui,next);
            }><span aria-hidden="true">"×"</span>"Delete widget"</button>
            {actions.with_value(|m| (m.widgets().len()>1).then(||view!{
                <div role="separator"></div><div class="widget-menu-caption" role="presentation">"WIDGETS AT THIS POINT"</div>
                {m.widgets().iter().map(|(id,label)|{let id=*id;let text=format!("Select {label} #{id}");view!{
                    <button role="menuitem" tabindex="-1" class:current=id==target on:click=move |_|{
                        if !ui.canvas_current() {return;}
                        close(ui,false);
                        actions.with_value(|m|{let mut selection=m.clone();ui.run(|e|selection.select(e,id));});
                        focus_target(ui,Some(id));
                    }><span aria-hidden="true">{if id==target{"✓"}else{"▧"}}</span>{text}</button>
                }}).collect_view()}
            }))}
        </div>
    }
}
