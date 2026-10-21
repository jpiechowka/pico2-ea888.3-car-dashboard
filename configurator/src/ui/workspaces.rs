//! Persistent workspace adapters. Studio owns every lifetime and the only dock.
use dashboard_configurator::field_registry::{FieldKey, FieldTarget, Registry, TargetOwner};
use dashboard_configurator::simulator::ControlSimulator;
use dashboard_configurator::workspace::*;

use super::*;

/// Origin-local storage key for version-one workspace navigation preferences, separate from projects.
pub const PREFERENCES_KEY: &str = "ea888-studio-workspaces-v1";
pub use dashboard_configurator::canvas_frame::PreviewMode;
#[derive(Clone, Copy)]
/// Studio-lifetime preview/simulation owner shared across mounted workspaces; switching tabs does not replace
/// acquisition.
pub struct PreviewDockState {
    /// Persistent accepted acquisition and isolated candidate-control simulator, local to the browser thread.
    pub sim: RwSignal<ControlSimulator, LocalStorage>,
    /// Shared renderer scratch retaining submitted simulation pixels until explicit consumption.
    pub timing: RwSignal<dashboard_configurator::preview::Preview, LocalStorage>,
    /// Checked frame-token sequence retained across source restarts.
    pub sequence: RwSignal<u64>,
    /// Submitted frame awaiting browser consumption/completion, or None.
    pub pending: RwSignal<Option<dashboard_core::display::pipeline::FrameToken>>,
    /// Pending frame bytes were accepted by canvas; completion can still be intentionally delayed.
    pub consumed: RwSignal<bool>,
    /// Requested dock rendering mode.
    pub mode: RwSignal<PreviewMode>,
    /// Mode of currently displayed pixels, updated only after successful canvas publication.
    pub displayed: RwSignal<PreviewMode>,
    /// Displayed pixels belong to invalid/unapplied inspection rather than accepted presentation.
    pub invalid_pixels: RwSignal<bool>,
    /// Accepted configuration lineage associated with displayed pixels, absent until published.
    pub pixels_key: RwSignal<Option<AcceptedConfigKey>>,
    /// Human-readable latest simulation/render status.
    pub notice: RwSignal<String>,
    /// Selected channel for manual sample input controls.
    pub channel: RwSignal<Channel>,
    /// Playback requested; clock/callback identity still gates actual advancement.
    pub running: RwSignal<bool>,
    /// Browser-held A/B/X/Y levels forwarded to debounced simulator gestures.
    pub held: RwSignal<[bool; 4]>,
    /// Simulation-millisecond release settling deadline, or None outside settling.
    pub release_until: RwSignal<Option<u64>>,
    /// Bounded per-channel raw sample text, independently blocking oversize edits.
    pub inputs: RwSignal<[dashboard_configurator::simulator::BoundedText<64>; 8]>,
    /// Bounded raw decimal reset-token input for delayed acknowledgement tests.
    pub ack_text: RwSignal<dashboard_configurator::simulator::BoundedText<20>>,
    /// Sole owned animation-frame reservation with checked scheduling/cancellation identity.
    pub callbacks: StoredValue<
        dashboard_configurator::frame_callbacks::FrameCallbacks<
            leptos::leptos_dom::helpers::AnimationFrameRequestHandle,
        >,
    >,
    /// Active playback anchor plus exact simulator source identity; None after cancellation.
    pub playback_clock: StoredValue<
        Option<(
            dashboard_configurator::timing::Playback,
            dashboard_configurator::simulator::SourceSessionKey,
        )>,
    >,
    /// Last installed invalid/JSON inspection provenance, including candidate parse epoch when available.
    pub inspection_stamp: RwSignal<Option<(EditorStamp, Option<dashboard_configurator::json_draft::JsonProjection>)>>,
    /// Project stamp captured for startup replay; edits cancel replay without discarding acquisition.
    pub startup_stamp: StoredValue<Option<EditorStamp>>,
    /// Accepted config lineage currently installed in the persistent simulator.
    accepted_key: StoredValue<AcceptedConfigKey>,
    /// Last lineage reported as a committed simulation restart, suppressing raw-input event spam.
    reported_key: StoredValue<AcceptedConfigKey>,
}
impl PreviewDockState {
    /// Returns owner-scoped dock signals seeded from accepted `editor` config, requiring explicit frame completion.
    /// Starts stopped in Appearance with no published pixels; requires a live Leptos owner and performs no
    /// render/device I/O.
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
            startup_stamp: StoredValue::new(None),
            accepted_key: StoredValue::new(editor.accepted_config_key()),
            reported_key: StoredValue::new(editor.accepted_config_key()),
        }
    }

    /// Synchronizes captured dock against `editor` and browser `_ui`; returns false if required signals are disposed,
    /// true otherwise. Accepted-lineage changes restart simulation; stale project stamps cancel startup only.
    /// Invalid/pending JSON installs isolated inspection before cancelling playback, while repair restores accepted
    /// controls without replacing acquisition. Called both reactively and at action time so stale callbacks cannot
    /// borrow new authority.
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
        let stale_startup = self
            .startup_stamp
            .try_get_value()
            .flatten()
            .is_some_and(|stamp| stamp != editor.stamp());
        if accepted_key != key || stale_startup {
            let _ = self.startup_stamp.try_set_value(None);
            super::simulation::cancel(_ui);
            let _ = self.sim.try_update(|s| {
                if accepted_key != key {
                    s.restart(editor.preview().clone());
                } else {
                    s.cancel_startup_replay();
                }
            });
            let _ = self.accepted_key.try_set_value(key);
            let _ = self.pending.try_set(None);
            let _ = self.consumed.try_set(false);
            let _ = self.notice.try_set(
                if accepted_key != key {
                    "Simulation restarted · accepted configuration changed; render a current frame"
                } else {
                    "Startup replay cancelled · project changed; acquisition retained"
                }
                .into(),
            );
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

    /// Emits a bounded restart event through `ui` only when installed accepted lineage differs from the last reported
    /// lineage. Call at committed action boundaries; does not restart simulation or log every raw-input update.
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

    /// Installs owner-scoped cache/existing-editor synchronization effects and playback lifecycle handling for `ui`.
    /// Requires the Studio reactive owner; repeated installation would create duplicate observers/listeners.
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

/// Returns origin-local preferences, display notice and read outcome; missing key uses defaults without error.
/// Invalid JSON yields defaults/Corrupt, storage failure defaults/Failure; never writes or blocks in-session
/// navigation.
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
/// Serializes `ui` workspace preferences and writes the fixed origin-local key; reports storage outcome and failure
/// notice. Failure preserves active in-memory navigation; success clears read/write failure latches. Does not save
/// project data.
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
/// Activates/focuses `workspace` in `ui`, cancelling pending field focus, pointer interaction and context menu, then
/// persists preferences. DOM focus and storage are best-effort; accepted project/acquisition remain intact.
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
/// Activates `workspace` in `ui`, expands `group`, and persists resulting navigation state; storage failure leaves
/// memory usable.
pub fn open(
    ui: Ui,
    workspace: Workspace,
    group: GroupId,
) {
    activate(ui, workspace);
    ui.workspace.update(|s| s.open(workspace, group));
    persist(ui);
}
/// Delegates guarded field `target` inspection/focus to `ui`'s command-palette adapter, retaining its captured revision
/// authority.
pub fn inspect(
    ui: Ui,
    target: FieldTarget,
) {
    command_palette::inspect(ui, target);
}

#[component]
/// Returns manual-activation workspace tabs for `ui`: unmodified arrows/Home/End move focus, Enter/Space/click activate
/// and persist.
pub fn WorkspaceTabs(
    /// Shared workspace preferences, registry inspection and editor authority.
    ui: Ui
) -> impl IntoView {
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
/// Returns persistent collapsible `group` for `ui` around `children`; native toggle changes update preferences only
/// when state differs.
pub fn Group(
    /// Shared workspace preferences, registry inspection and editor authority.
    ui: Ui,
    /// Stable preference key and visible title for this collapsible section.
    group: GroupId,
    /// One-shot factory for the section body, invoked when the component is created.
    children: Children,
) -> impl IntoView {
    view! { <details class="workspace-group" id=format!("studio-group-{}",group.id()) open=move ||ui.workspace.with(|s|s.expanded(group)) on:toggle=move |event|{let element=event_target::<web_sys::Element>(&event);let expanded=element.has_attribute("open");if ui.workspace.with_untracked(|s|s.expanded(group))!=expanded {ui.workspace.update(|s|s.set_expanded(group,expanded));persist(ui);}}><summary>{group.label()}</summary>{children()}</details> }
}
#[component]
/// Returns shared per-channel history-duration raw editors for `ui`, preserving exact invalid text and shared timing
/// bounds.
pub fn ChannelHistory(
    /// Shared workspace preferences, registry inspection and editor authority.
    ui: Ui
) -> impl IntoView {
    view! { <p class="hint">"Each channel shares sixty timed buckets across all widgets. Duration changes clear that channel history, preserving extrema."</p><div class="channel-history">{Channel::ALL.into_iter().map(move |channel|view!{<TextField ui field=RawField::HistorySeconds(channel) label=channel.label() hint="10–900 seconds"/>}).collect_view()}</div> }
}

#[component]
/// Returns candidate-aware warning summary for `ui`, with guarded per-row registry inspection targets and canonical
/// threshold text.
pub fn RuleSummary(
    /// Shared workspace preferences, registry inspection and editor authority.
    ui: Ui
) -> impl IntoView {
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
/// Returns current field/error inspection details for `ui`, re-resolving target authority and labelling stale/read-only
/// candidates. Return-to-selection cancels pending focus and clears inspection without editing the project.
pub fn InspectionDetail(
    /// Shared workspace preferences, registry inspection and editor authority.
    ui: Ui
) -> impl IntoView {
    view! { <div id="studio-inspection-detail" tabindex="-1" hidden=move ||ui.inspection.with(Option::is_none)&&ui.error_inspection.with(Option::is_none) class="inspection-detail">
        <p>{move ||ui.inspection.get().map(|target|ui.editor.with(|e|match Registry::resolve(e,target){Ok(field)=>format!("Inspecting {} · {}{}{}",Registry::descriptor(target.key()).label,field.inspection.display_location(),if !field.inspection.id_lookup_unambiguous || e.has_pending_json(){" · read only candidate"}else{""},field.capability.reason().map(|r|format!(" · {r}")).unwrap_or_default()),Err(_)=>"Field changed; find the field again.".into()})).or_else(||ui.error_inspection.get().map(|target|ui.editor.with(|e|match e.inspect(target){Ok(location)=>format!("Read-only JSON location · {}",location.display_location()),Err(_)=>"Field changed; find the field again.".into()}))).unwrap_or_default()}</p>
        <button on:click=move |_|{command_palette::cancel_focus(ui);ui.inspection.set(None);ui.error_inspection.set(None);}>"Return to selected widget"</button>
    </div> }
}

/// Returns the first registry key whose exact visible label equals `label`, or None; no fuzzy or localized matching.
pub fn key_by_label(label: &str) -> Option<FieldKey> {
    Registry::descriptors().iter().find(|d| d.label == label).map(|d| d.key)
}
