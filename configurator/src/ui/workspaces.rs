//! Persistent workspace adapters. Studio owns every lifetime and the only dock.
use dashboard_configurator::field_registry::{FieldKey, FieldTarget, Registry, TargetOwner};
use dashboard_configurator::simulator::ControlSimulator;
use dashboard_configurator::workspace::*;

use super::*;

pub const PREFERENCES_KEY: &str = "ea888-studio-workspaces-v1";
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PreviewMode {
    Startup,
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
    pub running: RwSignal<bool>,
    pub held: RwSignal<[bool; 4]>,
    pub release_until: RwSignal<Option<u64>>,
    pub inputs: RwSignal<[dashboard_configurator::simulator::BoundedText<64>; 8]>,
    pub ack_text: RwSignal<dashboard_configurator::simulator::BoundedText<20>>,
    pub callbacks: StoredValue<
        dashboard_configurator::frame_callbacks::FrameCallbacks<
            leptos::leptos_dom::helpers::AnimationFrameRequestHandle,
        >,
    >,
    pub playback_clock: StoredValue<
        Option<(
            dashboard_configurator::timing::Playback,
            dashboard_configurator::simulator::SourceSessionKey,
        )>,
    >,
    pub inspection_stamp: RwSignal<Option<(EditorStamp, Option<dashboard_configurator::json_draft::JsonProjection>)>>,
    accepted_key: StoredValue<AcceptedConfigKey>,
    reported_key: StoredValue<AcceptedConfigKey>,
}
impl PreviewDockState {
    pub fn new(editor: &Editor) -> Self {
        let mut simulator = ControlSimulator::new(editor.preview().clone());
        simulator.require_frame_completion();
        Self {
            sim: RwSignal::new_local(simulator),
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
            running: RwSignal::new(false),
            held: RwSignal::new([false; 4]),
            release_until: RwSignal::new(None),
            inputs: RwSignal::new(std::array::from_fn(|_| {
                dashboard_configurator::simulator::BoundedText::default()
            })),
            ack_text: RwSignal::new(dashboard_configurator::simulator::BoundedText::default()),
            callbacks: StoredValue::new(dashboard_configurator::frame_callbacks::FrameCallbacks::new()),
            playback_clock: StoredValue::new(None),
            inspection_stamp: RwSignal::new(None),
            accepted_key: StoredValue::new(editor.accepted_config_key()),
            reported_key: StoredValue::new(editor.accepted_config_key()),
        }
    }

    /// Recheck accepted lineage at action time as well as on reactive updates.
    pub fn synchronize(
        self,
        _ui: Ui,
        editor: &Editor,
    ) -> bool {
        let key = editor.accepted_config_key();
        let Some(accepted_key) = self.accepted_key.try_get_value() else {
            return false;
        };
        if self.sim.try_with_untracked(|_| ()).is_none() || self.inspection_stamp.try_get_untracked().is_none() {
            return false;
        }
        if accepted_key != key {
            super::simulation::cancel(_ui);
            let _ = self.sim.try_update(|s| s.restart(editor.preview().clone()));
            let _ = self.accepted_key.try_set_value(key);
            let _ = self.pending.try_set(None);
            let _ = self.consumed.try_set(false);
            let _ = self
                .notice
                .try_set("Simulation restarted · accepted configuration changed; render a current frame".into());
        }
        if editor.has_invalid_draft() || editor.has_pending_json() {
            let projection = editor.json_draft().map(|draft| draft.projection());
            let stamp = Some((editor.stamp(), projection));
            if self.inspection_stamp.get_untracked() != stamp {
                let map = if editor.has_pending_json() {
                    match editor.json_draft().map(|draft| draft.state()) {
                        Some(dashboard_configurator::json_draft::JsonState::Candidate { config, .. }) => {
                            config.controls
                        }
                        _ => editor.preview().config().controls,
                    }
                } else {
                    editor
                        .diagnostic()
                        .map_or(editor.editing_config().controls, |d| d.config.controls)
                };
                // Install inspection before freezing the clock, so cancellation
                // cannot sample accepted state after JSON becomes pending.
                let proof = if editor.has_pending_json() {
                    _ui.json.with_candidate(editor, Clone::clone).ok()
                } else {
                    None
                };
                let _ = self.sim.try_update(|s| {
                    match proof {
                        Some(proof) => s.inspect_candidate(proof),
                        None => s.inspect_invalid(map),
                    }
                });
                super::simulation::cancel(_ui);
                let _ = self.inspection_stamp.try_set(stamp);
            }
        } else if self.inspection_stamp.get_untracked().is_some() || self.sim.with_untracked(ControlSimulator::invalid)
        {
            super::simulation::cancel(_ui);
            let _ = self.sim.try_update(ControlSimulator::resume_accepted_bindings);
            let _ = self.inspection_stamp.try_set(None);
            let _ = self
                .notice
                .try_set("Accepted bindings restored · simulation retained".into());
        }
        true
    }

    /// Report at an explicit committed action, not each raw-input restart.
    pub fn report_restart(
        self,
        ui: Ui,
    ) {
        let current = self.accepted_key.get_value();
        if self.reported_key.get_value() != current {
            self.reported_key.set_value(current);
            let revision = ui.editor.with_untracked(|e| e.accepted_revision().get());
            ui.event(Notice::info(
                Category::Simulation,
                "simulation.restarted",
                None,
                format!("Simulation restarted at accepted revision {revision}"),
            ));
        }
    }

    pub fn install(
        self,
        ui: Ui,
    ) {
        Effect::new(move |_| ui.observe_cache(self.sim.with(ControlSimulator::cache_exhaustion_status)));
        Effect::new(move |_| {
            ui.editor.with(|e| {
                self.synchronize(ui, e);
            });
        });
        super::simulation::install(ui);
    }
}

pub fn restore() -> (WorkspaceState, String, Result<(), StorageIssue>) {
    let read = io::storage().and_then(|s| s.get_item(PREFERENCES_KEY).map_err(io::storage_error));
    match read {
        Ok(text) => {
            let (state, error) = WorkspaceState::from_preferences(text.as_deref());
            (
                state,
                error.unwrap_or_default().into(),
                if error.is_some() {
                    Err(StorageIssue::Corrupt)
                } else {
                    Ok(())
                },
            )
        }
        Err(failure) => {
            (
                WorkspaceState::default(),
                "Workspace preferences are unavailable; navigation works in this session".into(),
                Err(StorageIssue::Failure(failure)),
            )
        }
    }
}
pub fn persist(ui: Ui) {
    let result = ui
        .workspace
        .with_untracked(WorkspaceState::preferences)
        .map_err(|_| StorageIssue::Corrupt)
        .and_then(|text| {
            io::storage().map_err(StorageIssue::Failure).and_then(|storage| {
                storage
                    .set_item(PREFERENCES_KEY, &text)
                    .map_err(|error| StorageIssue::Failure(io::storage_error(error)))
            })
        });
    if let Err(issue) = result {
        ui.storage_outcome(StorageArea::Preferences, Err(issue));
        ui.notice
            .set("Workspace preferences could not be saved; navigation remains available".into());
    } else {
        ui.storage_outcome(StorageArea::Preferences, Ok(()));
        ui.storage_read_outcome(StorageArea::Preferences, Ok(()));
    }
}
pub fn activate(
    ui: Ui,
    workspace: Workspace,
) {
    command_palette::cancel_focus(ui);
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
    command_palette::inspect(ui, target);
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
pub fn InspectionDetail(ui: Ui) -> impl IntoView {
    view! { <div id="studio-inspection-detail" tabindex="-1" hidden=move ||ui.inspection.with(Option::is_none)&&ui.error_inspection.with(Option::is_none) class="inspection-detail">
        <p>{move ||ui.inspection.get().map(|target|ui.editor.with(|e|match Registry::resolve(e,target){Ok(field)=>format!("Inspecting {} · {}{}{}",Registry::descriptor(target.key()).label,field.inspection.display_location(),if !field.inspection.id_lookup_unambiguous || e.has_pending_json(){" · read only candidate"}else{""},field.capability.reason().map(|r|format!(" · {r}")).unwrap_or_default()),Err(_)=>"Field changed; find the field again.".into()})).or_else(||ui.error_inspection.get().map(|target|ui.editor.with(|e|match e.inspect(target){Ok(location)=>format!("Read-only JSON location · {}",location.display_location()),Err(_)=>"Field changed; find the field again.".into()}))).unwrap_or_default()}</p>
        <button on:click=move |_|{command_palette::cancel_focus(ui);ui.inspection.set(None);ui.error_inspection.set(None);}>"Return to selected widget"</button>
    </div> }
}

pub fn key_by_label(label: &str) -> Option<FieldKey> {
    Registry::descriptors().iter().find(|d| d.label == label).map(|d| d.key)
}
