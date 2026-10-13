//! Persistent workspace adapters. Studio owns every lifetime and the only dock.
use dashboard_configurator::field_registry::{FieldKey, FieldTarget, Registry, TargetOwner};
use dashboard_configurator::simulator::ControlSimulator;
use dashboard_configurator::workspace::*;

use super::*;

pub const PREFERENCES_KEY: &str = "ea888-studio-workspaces-v1";
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PreviewMode {
    Appearance,
    Simulation,
}
#[derive(Clone, Copy)]
pub struct PreviewDockState {
    pub sim: RwSignal<ControlSimulator, LocalStorage>,
    pub timing: RwSignal<dashboard_configurator::preview::Preview, LocalStorage>,
    pub sequence: RwSignal<u64>,
    pub pending: RwSignal<Option<dashboard_core::display::pipeline::FrameToken>>,
    pub consumed: RwSignal<bool>,
    pub mode: RwSignal<PreviewMode>,
    pub displayed: RwSignal<PreviewMode>,
    pub invalid_pixels: RwSignal<bool>,
    pub pixels_key: RwSignal<Option<AcceptedConfigKey>>,
    pub notice: RwSignal<String>,
    pub channel: RwSignal<Channel>,
    accepted_key: StoredValue<AcceptedConfigKey>,
}
impl PreviewDockState {
    pub fn new(editor: &Editor) -> Self {
        Self {
            sim: RwSignal::new_local(ControlSimulator::new(editor.preview().clone())),
            timing: RwSignal::new_local(dashboard_configurator::preview::Preview::new()),
            sequence: RwSignal::new(0),
            pending: RwSignal::new(None),
            consumed: RwSignal::new(false),
            mode: RwSignal::new(PreviewMode::Appearance),
            displayed: RwSignal::new(PreviewMode::Appearance),
            invalid_pixels: RwSignal::new(false),
            pixels_key: RwSignal::new(None),
            notice: RwSignal::new("Ready · accepted configuration".into()),
            channel: RwSignal::new(Channel::Boost),
            accepted_key: StoredValue::new(editor.accepted_config_key()),
        }
    }

    /// Recheck accepted lineage at action time as well as on reactive updates.
    pub fn synchronize(
        self,
        editor: &Editor,
    ) {
        let key = editor.accepted_config_key();
        if self.accepted_key.get_value() != key {
            self.sim.update(|s| s.restart(editor.preview().clone()));
            self.accepted_key.set_value(key);
            self.pending.set(None);
            self.consumed.set(false);
            self.notice
                .set("Simulation restarted · accepted configuration changed; render a current frame".into());
        }
    }

    pub fn install(
        self,
        ui: Ui,
    ) {
        Effect::new(move |_| ui.observe_cache(self.sim.with(ControlSimulator::cache_exhaustion_status)));
        Effect::new(move |_| {
            ui.editor.with(|e| {
                self.synchronize(e);
                if e.has_invalid_draft() {
                    let map = e
                        .diagnostic()
                        .map_or(e.editing_config().controls, |d| d.config.controls);
                    self.sim.update(|s| s.inspect_invalid(map));
                } else if self.sim.with_untracked(ControlSimulator::invalid) {
                    self.sim.update(ControlSimulator::resume_accepted_bindings);
                    self.notice
                        .set("Accepted bindings restored · simulation retained".into());
                }
            });
        });
    }
}

pub fn restore() -> (WorkspaceState, String) {
    let read = web_sys::window()
        .ok_or(())
        .and_then(|w| w.local_storage().map_err(|_| ())?.ok_or(()))
        .and_then(|s| s.get_item(PREFERENCES_KEY).map_err(|_| ()));
    match read {
        Ok(text) => {
            let (state, error) = WorkspaceState::from_preferences(text.as_deref());
            (state, error.unwrap_or_default().into())
        }
        Err(()) => {
            (
                WorkspaceState::default(),
                "Workspace preferences are unavailable; navigation works in this session".into(),
            )
        }
    }
}
pub fn persist(ui: Ui) {
    let result = ui
        .workspace
        .with_untracked(WorkspaceState::preferences)
        .map_err(str::to_string)
        .and_then(|text| {
            web_sys::window()
                .and_then(|w| w.local_storage().ok().flatten())
                .ok_or_else(|| "Workspace preferences are unavailable".to_string())
                .and_then(|storage| {
                    storage
                        .set_item(PREFERENCES_KEY, &text)
                        .map_err(|_| "Workspace preferences could not be saved; navigation remains available".into())
                })
        });
    if let Err(message) = result {
        ui.notice.set(message);
    }
}
pub fn activate(
    ui: Ui,
    workspace: Workspace,
) {
    stage::cancel(ui);
    widget_menu::close(ui, false);
    ui.workspace.update(|s| s.activate(workspace));
    persist(ui);
    if let Some(element) = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id(&format!("studio-tab-{}", workspace.id())))
        .and_then(|e| e.dyn_into::<web_sys::HtmlElement>().ok())
    {
        let _ = element.focus();
    }
}
pub fn open(
    ui: Ui,
    workspace: Workspace,
    group: GroupId,
) {
    activate(ui, workspace);
    ui.workspace.update(|s| s.open(workspace, group));
    persist(ui);
}
pub fn inspect(
    ui: Ui,
    target: FieldTarget,
) {
    let resolved = ui.editor.with_untracked(|e| Registry::resolve(e, target));
    match resolved {
        Ok(field) => {
            ui.inspection.set(Some(target));
            if let TargetOwner::Channel(c) = target.owner() {
                ui.source_channel.set(c);
            }
            open(ui, field.workspace, field.group);
            leptos::leptos_dom::helpers::request_animation_frame(move || {
                let Ok(field) = ui.editor.with_untracked(|e| Registry::resolve(e, target)) else {
                    ui.notice
                        .set("That inspection target is stale. Find the field again.".into());
                    return;
                };
                if let Some(element) = web_sys::window()
                    .and_then(|w| w.document())
                    .and_then(|d| d.get_element_by_id(&field.dom_id))
                    .and_then(|e| e.dyn_into::<web_sys::HtmlElement>().ok())
                {
                    if element.has_attribute("disabled") {
                        if let Some(detail) = web_sys::window()
                            .and_then(|w| w.document())
                            .and_then(|d| d.get_element_by_id("studio-inspection-detail"))
                            .and_then(|e| e.dyn_into::<web_sys::HtmlElement>().ok())
                        {
                            let _ = detail.focus();
                        }
                        return;
                    }
                    let mut ancestor = element.parent_element();
                    while let Some(parent) = ancestor {
                        if parent.tag_name() == "DETAILS" {
                            let _ = parent.set_attribute("open", "");
                        }
                        ancestor = parent.parent_element();
                    }
                    let _ = element.focus();
                    element.scroll_into_view();
                } else if let Some(element) = web_sys::window()
                    .and_then(|w| w.document())
                    .and_then(|d| d.get_element_by_id("studio-inspection-detail"))
                    .and_then(|e| e.dyn_into::<web_sys::HtmlElement>().ok())
                {
                    let _ = element.focus();
                }
            });
        }
        Err(_) => {
            ui.notice
                .set("That inspection target is stale or unavailable. Find the field again.".into())
        }
    }
}

#[component]
pub fn WorkspaceTabs(ui: Ui) -> impl IntoView {
    view! { <nav class="workspace-tabs" role="tablist" aria-label="Studio workspaces">
        {Workspace::ALL.into_iter().map(move |workspace|view! {
            <button role="tab" id=format!("studio-tab-{}",workspace.id()) aria-controls=format!("studio-panel-{}",workspace.id())
                tabindex=move ||if ui.workspace.with(|s|s.focused()==workspace){0}else{-1}
                aria-selected=move ||ui.workspace.with(|s|s.active()==workspace).to_string()
                class:active=move ||ui.workspace.with(|s|s.active()==workspace)
                on:click=move |_|activate(ui,workspace)
                on:keydown=move |event| {
                    if event.alt_key()||event.ctrl_key()||event.meta_key()||event.shift_key(){return;}
                    let key=match event.key().as_str(){"ArrowLeft"=>Some(TabKey::Previous),"ArrowRight"=>Some(TabKey::Next),"Home"=>Some(TabKey::First),"End"=>Some(TabKey::Last),"Enter"|" "=>{event.prevent_default();let focused=ui.workspace.with_untracked(WorkspaceState::focused);activate(ui,focused);None},_=>None};
                    if let Some(key)=key { event.prevent_default();ui.workspace.update(|s|s.focus(key));let focused=ui.workspace.with_untracked(WorkspaceState::focused);if let Some(element)=web_sys::window().and_then(|w|w.document()).and_then(|d|d.get_element_by_id(&format!("studio-tab-{}",focused.id()))).and_then(|e|e.dyn_into::<web_sys::HtmlElement>().ok()){let _=element.focus();} }
                }>{workspace.label()}</button>
        }).collect_view()}
    </nav> }
}
#[component]
pub fn Group(
    ui: Ui,
    group: GroupId,
    children: Children,
) -> impl IntoView {
    view! { <details class="workspace-group" id=format!("studio-group-{}",group.id()) open=move ||ui.workspace.with(|s|s.expanded(group)) on:toggle=move |event|{let element=event_target::<web_sys::Element>(&event);let expanded=element.has_attribute("open");if ui.workspace.with_untracked(|s|s.expanded(group))!=expanded {ui.workspace.update(|s|s.set_expanded(group,expanded));persist(ui);}}><summary>{group.label()}</summary>{children()}</details> }
}
#[component]
pub fn ChannelHistory(ui: Ui) -> impl IntoView {
    view! { <p class="hint">"Each channel shares sixty timed buckets across all widgets. Duration changes clear that channel history, preserving extrema."</p><div class="channel-history">{Channel::ALL.into_iter().map(move |channel|view!{<TextField ui field=RawField::HistorySeconds(channel) label=channel.label() hint="10–900 seconds"/>}).collect_view()}</div> }
}

#[component]
pub fn RuleSummary(ui: Ui) -> impl IntoView {
    view! { <h3>"Configured warning rules"</h3><p class="hint">"Canonical thresholds and priorities. Source qualification, freshness and operating-context guards remain enforced."</p>
        <div class="workspace-scroll" role="region" aria-label="Configured warning rules" tabindex="0"><table><thead><tr><th>"Widget"</th><th>"Channel"</th><th>"Rule"</th><th>"Priority"</th><th>"Settings"</th></tr></thead><tbody>
        {move ||ui.editor.with(|e|Registry::with_candidate(e,|config|config.widgets.iter().enumerate().map(|(index,w)|{
            let target=Registry::target(e,FieldKey::WarningEnabled,TargetOwner::Widget(index)).ok();
            view! { <tr><td>{w.label.to_string()}</td><td>{w.channel.label()}</td><td>{if w.warning.enabled{format!("{:?}: {} / {}",w.warning.mode,w.warning.trigger,w.warning.clear)}else{"Disabled".into()}}</td><td>{format!("{:?}",w.warning.advanced.priority)}</td><td><button disabled=target.is_none() on:click=move |_|{if let Some(target)=target{inspect(ui,target);}}>"Inspect rule"</button></td></tr> }
        }).collect_view()))}
        </tbody></table></div>
    }
}
#[component]
pub fn BuildFlashEmpty(ui: Ui) -> impl IntoView {
    view! { <div class="settings-copy"><h1>"Build & Flash"</h1><p>"Native build and flashing tools will appear here when their integration is ready."</p><p>"Configure your onboard recovery and controls before exporting a configuration."</p><button on:click=move |_|open(ui,Workspace::Device,GroupId::Bindings)>"Open Device controls"</button></div> }
}
#[component]
pub fn InspectionDetail(ui: Ui) -> impl IntoView {
    view! { <div id="studio-inspection-detail" tabindex="-1" hidden=move ||ui.inspection.with(Option::is_none) class="inspection-detail">
        <p>{move ||ui.inspection.get().map(|target|ui.editor.with(|e|match Registry::resolve(e,target){Ok(field)=>format!("Inspecting {} · {}{}{}",Registry::descriptor(target.key()).label,field.inspection.json_path.unwrap_or_default(),if !field.inspection.id_lookup_unambiguous || e.has_pending_json(){" · read only candidate"}else{""},field.capability.reason().map(|r|format!(" · {r}")).unwrap_or_default()),Err(_)=>"Inspection target changed. Find the field again.".into()})).unwrap_or_default()}</p>
        <button on:click=move |_|ui.inspection.set(None)>"Return to selected widget"</button>
    </div> }
}

pub fn key_by_label(label: &str) -> Option<FieldKey> {
    Registry::descriptors().iter().find(|d| d.label == label).map(|d| d.key)
}
