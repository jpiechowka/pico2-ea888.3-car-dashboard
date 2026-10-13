//! Native modal and fixed typed command adapters; Studio owns focus lifetime.
use dashboard_configurator::field_registry::SearchQuery;
use dashboard_configurator::workspace::{
    Command,
    CommandEffect,
    ExportFormat,
    FocusRequests,
    ShortcutAction,
    dispatch_with_appearance,
    shortcut_action,
};
use leptos::leptos_dom::helpers::{AnimationFrameRequestHandle, request_animation_frame_with_handle};

use super::*;

#[derive(Default)]
pub struct FocusState {
    requests: FocusRequests,
    frame: Option<AnimationFrameRequestHandle>,
}
pub fn cancel_focus(ui: Ui) {
    let _ = ui.focus.try_update(|state| {
        state.requests.cancel();
        if let Some(frame) = state.frame.take() {
            frame.cancel();
        }
    });
}
fn element(id: &str) -> Option<web_sys::HtmlElement> {
    web_sys::window()?.document()?.get_element_by_id(id)?.dyn_into().ok()
}
fn usable(element: &web_sys::HtmlElement) -> bool {
    let rect = element.get_bounding_client_rect();
    element.is_connected()
        && !element.has_attribute("disabled")
        && (rect.width() > 0.0 || rect.height() > 0.0)
        && element
            .closest("[hidden], [inert], dialog:not([open])")
            .ok()
            .flatten()
            .is_none()
}
fn focus(element: &web_sys::HtmlElement) {
    let _ = element.focus_with_options(&{
        let options = web_sys::FocusOptions::new();
        options.set_prevent_scroll(true);
        options
    });
}
pub fn focus_tab(ui: Ui) {
    if let Some(active) = ui.workspace.try_with_untracked(WorkspaceState::active)
        && let Some(tab) = element(&format!("studio-tab-{}", active.id()))
    {
        focus(&tab);
    }
}
fn schedule_inspection(
    ui: Ui,
    target: InspectionTarget,
    field: Option<FieldTarget>,
    workspace: Workspace,
) {
    cancel_focus(ui);
    let mut issued = None;
    ui.focus.update(|state| issued = state.requests.issue());
    let Some(token) = issued else {
        ui.notice
            .set("Focus identity exhausted; reopen Studio to navigate".into());
        return;
    };
    let result = request_animation_frame_with_handle(move || {
        if ui.focus.try_with_untracked(|state| state.requests.is_current(token)) != Some(true)
            || ui.workspace.try_with_untracked(WorkspaceState::active) != Some(workspace)
            || ui.error_inspection.try_get_untracked() != Some(Some(target))
        {
            return;
        }
        let _ = ui.focus.try_update(|state| state.frame = None);
        let resolved = ui.editor.try_with_untracked(|e| e.inspect(target));
        let Some(Ok(inspection)) = resolved else {
            let _ = ui.notice.try_set("Field changed; refresh commands and results".into());
            return;
        };
        let id = field.and_then(|f| {
            ui.editor
                .try_with_untracked(|e| {
                    Registry::resolve(e, f)
                        .ok()
                        .filter(|_| inspection.id_lookup_unambiguous && !e.has_pending_json())
                        .map(|r| r.dom_id)
                })
                .flatten()
        });
        let destination = id.as_deref().and_then(element);
        let destination = destination.filter(|node| !node.has_attribute("disabled"));
        if let Some(node) = destination {
            let mut parent = node.parent_element();
            while let Some(ancestor) = parent {
                if ancestor.tag_name() == "DETAILS" {
                    let _ = ancestor.set_attribute("open", "");
                }
                parent = ancestor.parent_element();
            }
            if usable(&node) {
                focus(&node);
                node.scroll_into_view();
                return;
            }
        }
        if let Some(detail) = element("studio-inspection-detail").filter(usable) {
            focus(&detail);
        }
    });
    match result {
        Ok(frame) => ui.focus.update(|state| state.frame = Some(frame)),
        Err(_) => {
            cancel_focus(ui);
            ui.notice.set("The browser could not schedule field focus".into());
        }
    }
}
pub fn inspect(
    ui: Ui,
    target: FieldTarget,
) {
    let resolved = ui.editor.with_untracked(|e| Registry::resolve(e, target));
    match resolved {
        Ok(field) => {
            ui.inspection.set(Some(target));
            ui.error_inspection.set(Some(target.inspection()));
            if let TargetOwner::Channel(c) = target.owner() {
                ui.source_channel.set(c);
            }
            workspaces::open(ui, field.workspace, field.group);
            schedule_inspection(ui, target.inspection(), Some(target), field.workspace);
        }
        Err(_) => ui.notice.set("Field changed; refresh commands and results".into()),
    }
}
pub fn inspect_location(
    ui: Ui,
    target: InspectionTarget,
) {
    if ui.editor.with_untracked(|e| e.inspect(target).is_ok()) {
        ui.inspection.set(None);
        ui.error_inspection.set(Some(target));
        workspaces::open(ui, Workspace::Diagnostics, GroupId::Files);
        schedule_inspection(ui, target, None, Workspace::Diagnostics);
    } else {
        ui.notice.set("Field changed; refresh commands and results".into());
    }
}
fn schedule_appearance(
    ui: Ui,
    request: AppearanceRequest,
) {
    cancel_focus(ui);
    let mut issued = None;
    ui.focus.update(|state| issued = state.requests.issue());
    let Some(token) = issued else {
        ui.notice.set("Focus identity exhausted; reopen Studio".into());
        return;
    };
    match request_animation_frame_with_handle(move || {
        if ui.focus.try_with_untracked(|state| state.requests.is_current(token)) != Some(true)
            || ui.workspace.try_with_untracked(WorkspaceState::active) != Some(Workspace::Views)
            || ui.appearance.try_with_untracked(AppearanceSession::request) != Some(Some(request))
        {
            return;
        }
        let _ = ui.focus.try_update(|state| state.frame = None);
        if ui
            .editor
            .try_with_untracked(|editor| editor.appearance_preview(request).is_ok())
            != Some(true)
        {
            return;
        }
        if let Some(node) = element("studio-appearance-preview").filter(usable) {
            focus(&node);
            node.scroll_into_view();
        }
    }) {
        Ok(frame) => ui.focus.update(|state| state.frame = Some(frame)),
        Err(_) => {
            cancel_focus(ui);
            ui.notice.set("The browser could not schedule appearance focus".into());
        }
    }
}
pub(super) fn offer(
    ui: Ui,
    stamp: EditorStamp,
) -> AppearanceOffer {
    ui.appearance
        .with(|session| ui.editor.with(|editor| session.offer(editor, stamp)))
}
pub(super) fn availability(
    ui: Ui,
    command: Command,
    stamp: EditorStamp,
    offer: AppearanceOffer,
) -> Option<&'static str> {
    if matches!(command, Command::ApplyAppearance(_)) && !ui.appearance.with(|session| session.is_current(offer)) {
        return Some("Appearance source changed; refresh this action");
    }
    ui.editor
        .with(|editor| command.availability_with_appearance(editor, stamp, offer.snapshot()))
}
pub(super) fn execute(
    ui: Ui,
    command: Command,
    stamp: EditorStamp,
    offer: AppearanceOffer,
) -> bool {
    if let Some(reason) = availability(ui, command, stamp, offer) {
        ui.notice.set(reason.into());
        return false;
    }
    // Ui::run retains normal save/latency behavior. The pure dispatch is the
    // authority; a rejected command cannot cancel import/pointer/draft state.
    let mut workspace = ui.workspace.get_untracked();
    let mut outcome = None;
    ui.run(|editor| {
        let result = dispatch_with_appearance(editor, &mut workspace, command, stamp, offer.snapshot());
        let revision = editor.revision();
        let ok = result.is_ok();
        outcome = Some(result);
        if ok { Ok(revision) } else { Err(EditorError::StaleToken) }
    });
    match outcome.expect("synchronous command dispatch") {
        Ok(effect) => {
            match effect {
                CommandEffect::CopiedAppearance(snapshot) => {
                    let mut result = Ok(());
                    ui.appearance.update(|session| result = session.copy(snapshot));
                    ui.notice.set(match result {
                        Ok(()) => "Appearance copied. Choose Copied appearance explicitly to use it.".into(),
                        Err(error) => error.message().into(),
                    });
                }
                CommandEffect::PreviewAppearance(request) => {
                    let mut result = Ok(());
                    ui.appearance.update(|session| result = session.stage(request, offer));
                    if let Err(error) = result {
                        ui.notice.set(error.message().into());
                        return false;
                    }
                    workspaces::open(ui, Workspace::Views, GroupId::Appearance);
                    schedule_appearance(ui, request);
                }
                CommandEffect::Navigate(w) => workspaces::activate(ui, w),
                CommandEffect::Inspect(target) => inspect(ui, target),
                CommandEffect::Changed => {}
                CommandEffect::Export(format) => io::export(ui, format == ExportFormat::Json),
                CommandEffect::RecoverSimulator => controls::recover(ui),
            }
            true
        }
        Err(error) => {
            ui.notice.set(error.message());
            false
        }
    }
}
fn editable(event: &web_sys::KeyboardEvent) -> bool {
    let Some(target) = event.target().and_then(|e| e.dyn_into::<web_sys::Element>().ok()) else {
        return true;
    };
    if target.closest("input, textarea, select").ok().flatten().is_some() {
        return true;
    }
    // isContentEditable respects inherited true and explicit false boundaries.
    let mut node = Some(target);
    while let Some(element) = node {
        if let Some(html) = element.dyn_ref::<web_sys::HtmlElement>() {
            return html.is_content_editable();
        }
        node = element.parent_element();
    }
    false
}
fn search_text(input: &web_sys::HtmlInputElement) -> Option<String> {
    // Check JS string length before copying it into Rust. A paste of arbitrary
    // size is rejected without allocating that entire DOM value in the model.
    let value = js_sys::Reflect::get(input.as_ref(), &"value".into()).ok()?;
    if !value.is_string() {
        return None;
    }
    let text: js_sys::JsString = value.unchecked_into();
    if text.length() > 128 {
        return None;
    }
    text.as_string()
}

#[derive(Clone, Copy)]
struct Palette {
    dialog: NodeRef<leptos::html::Dialog>,
    search: NodeRef<leptos::html::Input>,
    active: RwSignal<bool>,
    sessions: StoredValue<FocusRequests>,
    invoker: StoredValue<Option<web_sys::HtmlElement>, LocalStorage>,
}
impl Palette {
    fn native(self) -> Option<web_sys::HtmlDialogElement> {
        self.dialog
            .try_get_untracked()
            .flatten()
            .and_then(|node| (*node).clone().dyn_into().ok())
    }

    fn open(
        self,
        ui: Ui,
    ) {
        if self.active.get_untracked() {
            return;
        }
        let Some(dialog) = self.native() else {
            ui.notice.set("Command dialog is unavailable".into());
            return;
        };
        let mut session = None;
        self.sessions.update_value(|s| session = s.issue());
        if session.is_none() {
            ui.notice.set("Command dialog identity exhausted".into());
            return;
        }
        cancel_focus(ui);
        self.invoker.set_value(
            web_sys::window()
                .and_then(|w| w.document())
                .and_then(|d| d.active_element())
                .and_then(|e| e.dyn_into().ok()),
        );
        match dialog.show_modal() {
            Ok(()) => {
                self.active.set(true);
                if let Some(search) = self.search.get_untracked() {
                    let _ = search.focus();
                }
            }
            Err(_) => {
                self.sessions.update_value(FocusRequests::cancel);
                self.invoker.set_value(None);
                ui.notice.set("The browser could not open the command dialog".into());
            }
        }
    }

    fn close(
        self,
        ui: Ui,
    ) {
        // Native close events are queued: a duplicate close after activation
        // must not cancel the new Studio-owned destination focus request.
        if !self.active.get_untracked() {
            return;
        }
        self.sessions.update_value(FocusRequests::cancel);
        cancel_focus(ui);
        self.active.set(false);
        if let Some(dialog) = self.native() {
            dialog.close();
        }
        let invoker = self.invoker.get_value();
        self.invoker.set_value(None);
        if let Some(invoker) = invoker.filter(usable) {
            focus(&invoker);
        } else {
            focus_tab(ui);
        }
    }
}

#[component]
pub fn CommandPalette(ui: Ui) -> impl IntoView {
    let palette = Palette {
        dialog: NodeRef::new(),
        search: NodeRef::new(),
        active: RwSignal::new(false),
        sessions: StoredValue::new(FocusRequests::default()),
        invoker: StoredValue::new_local(None),
    };
    let query = RwSignal::new(String::new());
    let bounded_query = StoredValue::new(SearchQuery::default());
    let query_notice = RwSignal::new(String::new());
    let listener = window_event_listener(leptos::ev::keydown, move |event| {
        if event.default_prevented() {
            return;
        }
        let Some(action) = shortcut_action(
            &event.key(),
            event.ctrl_key() || event.meta_key(),
            event.shift_key(),
            event.alt_key(),
            event.repeat(),
            event.is_composing(),
            editable(&event),
        ) else {
            return;
        };
        if action == ShortcutAction::Palette {
            event.prevent_default();
            palette.open(ui);
        } else if !palette.active.get_untracked() {
            let command = if action == ShortcutAction::Undo {
                Command::Undo
            } else {
                Command::Redo
            };
            let stamp = ui.editor.with_untracked(Editor::stamp);
            if ui.editor.with_untracked(|e| command.availability(e, stamp).is_none()) {
                event.prevent_default();
                execute(ui, command, stamp, offer(ui, stamp));
            }
        }
    });
    on_cleanup(move || {
        listener.remove();
        cancel_focus(ui);
        palette.sessions.update_value(FocusRequests::cancel);
        if let Some(dialog) = palette.native() {
            dialog.close();
        }
    });
    view! {
        <button id="studio-command-launcher" aria-haspopup="dialog" aria-controls="studio-command-dialog" on:click=move |_|palette.open(ui)>"Find / Commands"<small>"Ctrl / ⌘ K"</small></button>
        <dialog node_ref=palette.dialog id="studio-command-dialog" class="command-dialog" aria-labelledby="studio-command-title"
            on:cancel=move |event: web_sys::Event|{event.prevent_default();palette.close(ui);}
            on:close=move |_|{if palette.native().is_none_or(|d|!d.open()){palette.close(ui);}}
            on:keydown=move |event|{if event.key()=="Escape"{event.stop_propagation();}}>
            <div class="command-heading"><h2 id="studio-command-title">"Find settings and commands"</h2><button aria-label="Close command palette" on:click=move |_|palette.close(ui)>"× Close"</button></div>
            <label for="studio-command-search">"Search fields, labels, sensors or actions"</label>
            <input node_ref=palette.search id="studio-command-search" type="search" autocomplete="off" autofocus=true
                prop:value=move ||query.get() on:input=move |_| {
                    let text=palette.search.get_untracked().and_then(|input|search_text(&input));
                    let mut accepted=false;
                    if let Some(ref text)=text { bounded_query.update_value(|q|accepted=q.set(text).is_ok()); }
                    if accepted {query.set(text.expect("accepted bounded query"));query_notice.set(String::new());}
                    else {if let Some(search)=palette.search.get_untracked(){search.set_value(&query.get_untracked());}query_notice.set("Search is limited to 128 UTF-8 bytes; previous query retained".into());}
                }/>
            <p role="status" hidden=move ||query_notice.get().is_empty()>{move ||query_notice.get()}</p>
            <div class="command-results">
                <h3>"Commands"</h3>
                {move || {
                    let query=query.get().to_lowercase();
                    ui.editor.with(|editor| {
                        let stamp=editor.stamp();
                        Command::inventory(editor).into_iter().filter(|command|query.split_whitespace().all(|token|command.label().to_lowercase().contains(token)))
                            .map(|command| {let source=offer(ui,stamp); view!{<button class="command-result" disabled=move ||availability(ui,command,stamp,source).is_some()
                                on:click=move |_|{if availability(ui,command,stamp,source).is_none(){palette.close(ui);execute(ui,command,stamp,source);}else{ui.notice.set("Command changed; refresh commands and results".into());}}>
                                <span>{command.label()}</span><small>{move ||availability(ui,command,stamp,source).unwrap_or("Ready")}</small></button>}}).collect_view()
                    })
                }}
                <h3>"Settings"</h3>
                {move ||ui.editor.with(|editor| {
                    match Registry::search(editor,&query.get()) {
                        Ok(page)=> {
                            let stamp=editor.stamp();
                            let omitted=page.omitted;
                            let rows=page.results.into_iter().map(|target| {
                                let descriptor=Registry::descriptor(target.key());
                                let context=Registry::with_candidate(editor,|c|match target.owner(){
                                    TargetOwner::Widget(i)|TargetOwner::Band(i,_)=>format!("Widget [{i}] {} · view {}",c.widgets[i].label,c.widgets[i].view),
                                    TargetOwner::View(i)=>format!("View [{i}] {}",c.views[i].name),TargetOwner::Channel(c)=>c.label().into(),
                                    other=>format!("{other:?}"),
                                });
                                let reason=Registry::resolve(editor,target).ok().and_then(|r|r.capability.reason()).unwrap_or("Inspect setting");
                                view!{<button class="command-result" disabled=move ||ui.editor.with(|e|Command::Find(target).availability(e,stamp).is_some())
                                    on:click=move |_|{palette.close(ui);execute(ui,Command::Find(target),stamp,offer(ui,stamp));}>
                                    <span>{descriptor.label}</span><small>{format!("{} / {} · {context}",descriptor.workspace.label(),descriptor.group.label())}</small><small>{reason}</small></button>}
                            }).collect_view();
                            view!{<div>{rows}<p class="hint">{if omitted>0{format!("{omitted} more settings; refine the search")}else{"All matching settings shown".into()}}</p></div>}.into_any()
                        },
                        Err(_)=>view!{<p>"Fields changed; refresh the search"</p>}.into_any(),
                    }
                })}
            </div>
        </dialog>
    }
}
