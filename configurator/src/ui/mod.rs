//! Browser composition and adapters between editor authority, rendered pixels, persistence
//! and explicit helper actions. Reactive handles are owned by the mounted Studio scope.
//! [Component props](https://book.leptos.dev/view/03_components.html) carry static values
//! or reactive handles; prop rustdoc also describes the macro-generated builder fields.

/// Widget bands, readouts, smoothing and warning acknowledgment controls.
mod advanced;
/// Configuration and synthetic BLE diagnostic panels.
mod ble;
/// Build options and current-image status presentation.
mod build_flash;
/// Revision-bound command offers, search and deferred focus.
mod command_palette;
/// Device bindings and synthetic timing/canvas controls.
mod controls;
/// Offline documentary ECU evidence and catalogue references.
mod ecu_reference;
/// Bounded contextual browser diagnostics and filtering.
mod event_log;
/// Raw and typed controls with retained invalid input and target ownership.
mod fields;
/// Explicit loopback helper authentication and one request/poll chain.
mod helper;
/// IndexedDB request/open/transaction lifetime and deadline ownership.
mod idb;
/// Widget, source, fuel, warning and appearance editors.
mod inspector;
/// Bounded file imports/downloads and legacy/preset storage.
mod io;
/// Retained textarea drafts, debounce parsing and explicit apply/discard.
mod json_editor;
/// Shared LED policy controls and previews.
mod led;
/// Browser render/interaction timing without physical scanout claims.
mod metrics;
/// Device/screen/view orientation and artwork-mismatch repair.
mod orientation;
/// Origin-locked original blobs, live ownership pins and materialization leases.
mod originals;
/// Complete-project persistence, conflict checks and captured exports.
mod project_io;
/// Optional bounded diagnostic relay with separate cancellation ownership.
mod relay;
/// Accepted configuration resource and image admission summaries.
mod resources;
/// Owned timing callbacks and synthetic display completion.
mod simulation;
/// Captured pointer placement and appearance/inspection canvas publication.
mod stage;
/// Startup profile, preview and project-level artwork admission.
mod startup;
/// Replaceable bounded Rust worker conversion of original artwork.
mod startup_artwork;
/// Revision-bound widget actions and context-menu interaction.
mod widget_menu;
/// Workspace navigation, registry focus and persistent preview dock.
mod workspaces;

use dashboard_configurator::appearance_presets::*;
use dashboard_configurator::config::*;
use dashboard_configurator::editor::*;
use dashboard_configurator::event_adapters::{
    AdapterState,
    Notice,
    RenderIssue,
    RenderLane,
    SimulationObservation,
    StorageArea,
    StorageIssue,
    simulation_changes,
    validation_transition,
};
use dashboard_configurator::events::{Category, EventStore};
use dashboard_configurator::field_registry::{FieldId, FieldKey, FieldTarget, Registry, TargetOwner};
use dashboard_configurator::resources::{ResourceCache, ResourceReport};
use dashboard_configurator::workspace::{Command, GroupId, Workspace, WorkspaceState};
use fields::{Choice, ColorField, TextField, Toggle, choices, error_text};
use inspector::{FuelSettings, Inspector, SourceSettings, WarningSettingsPanel};
use leptos::leptos_dom::helpers::{event_target_checked, event_target_value};
use leptos::prelude::*;
use stage::Library;
use wasm_bindgen::JsCast;
use workspaces::{Group, PreviewDockState, PreviewMode};

/// Copyable handles to one mounted editor/session; copying does not clone project assets.
#[derive(Clone, Copy)]
pub struct Ui {
    /// Performance timestamp in milliseconds captured before initial restore.
    mount_started: f64,
    /// Deduplicates each cache-identity exhaustion cause for this mount.
    cache_seen: RwSignal<dashboard_core::cache_diagnostics::CacheExhaustionSeen>,
    /// Accepted project, revisioned drafts, selection and undo/redo authority.
    editor: RwSignal<Editor>,
    /// Current build options, independently revisioned by the helper state.
    build: RwSignal<dashboard_configurator::build_flash::BuildSelection>,
    /// Ephemeral capability, native operation observation and explicit selection.
    helper: helper::State,
    /// Optional diagnostic queue with independent cancellation.
    relay: relay::State,
    /// Memoized accepted-project resource analysis.
    resources: StoredValue<ResourceCache>,
    /// Textarea draft projection, parse timer and inspection pixels identity.
    json: json_editor::State,
    /// Current user-facing transient message, never a project payload log.
    notice: RwSignal<String>,
    /// Bounded local event history and filters.
    events: RwSignal<EventStore>,
    /// Transition deduplication for storage/render diagnostic events.
    event_adapters: StoredValue<AdapterState>,
    /// Whether bounded events are also mirrored to browser console.
    console_enabled: RwSignal<bool>,
    /// At most 16 object URLs retained until delayed revocation.
    downloads: StoredValue<io::Downloads, LocalStorage>,
    /// Stored draft differs from the observed persistence base.
    conflict: RwSignal<bool>,
    /// Observed legacy JSON storage record, None when absent.
    saved_bytes: RwSignal<Option<String>>,
    /// Copied/preset appearance snapshot and current preview offer.
    appearance: RwSignal<AppearanceSession>,
    /// Bounded named-preset list and explicit persistence state.
    presets: RwSignal<PresetStore>,
    /// Preset write lock acquisition/operation is active.
    preset_lock_busy: RwSignal<bool>,
    /// One bounded preset file read is in flight.
    preset_import_busy: RwSignal<bool>,
    /// Current browser-lock AbortController retained as a JS object.
    preset_abort: StoredValue<Option<js_sys::Object>, LocalStorage>,
    /// Synthetic appearance boost toggle; fixed configured units remain fixed.
    psi: RwSignal<bool>,
    /// Whether appearance rendering shows synthetic warnings.
    warnings: RwSignal<bool>,
    /// Synthetic sample position in milliseconds, independent of frame count.
    sample_ms: RwSignal<u64>,
    /// Demo/minimum/maximum/width/zero synthetic value selection.
    sample_set: RwSignal<dashboard_configurator::preview::SampleSet>,
    /// Last appearance/canvas failure until a successful current render.
    preview_error: RwSignal<Option<String>>,
    /// First pending interaction performance timestamp in milliseconds.
    pending_at: RwSignal<Option<f64>>,
    /// Last appearance render duration in browser milliseconds.
    render_ms: RwSignal<f64>,
    /// Mounted shared-renderer canvas node, absent before mount/disposal.
    canvas: NodeRef<leptos::html::Canvas>,
    /// Geometry of the last successfully written canvas.
    pixel_geometry: RwSignal<Geometry>,
    /// Exact editor/mode/geometry identity permitted for interaction.
    published_canvas: RwSignal<dashboard_configurator::canvas_frame::PublishedCanvas>,
    /// Widget IDs whose current rendered sample exceeds reserved lanes.
    pixel_fit_issues: RwSignal<Vec<u16>>,
    /// One pointer gesture tied to canvas geometry and editor authority.
    drag: RwSignal<Option<stage::Drag>>,
    /// Native pointer ID and element whose capture must be released.
    capture: RwSignal<Option<(i32, web_sys::Element)>>,
    /// Current widget context menu with captured action identity.
    menu: RwSignal<Option<widget_menu::Menu>>,
    /// Appearance/startup elapsed-time playback enabled.
    playing: RwSignal<bool>,
    /// Startup timing drafts, accepted profile and feedback.
    startup: startup::State,
    /// IndexedDB restore/save/conflict operation state.
    project_store: project_io::State,
    /// Active workspace and stored group preferences.
    workspace: RwSignal<WorkspaceState>,
    /// Registry field target, including revision and candidate position.
    inspection: RwSignal<Option<FieldTarget>>,
    /// Validation diagnostic target offered for repair navigation.
    error_inspection: RwSignal<Option<InspectionTarget>>,
    /// Cancelable deferred focus request identity and handle.
    focus: RwSignal<command_palette::FocusState>,
    /// Channel shown in shared source controls.
    source_channel: RwSignal<Channel>,
    /// Offline documentary reference search/filter/selection state.
    reference: RwSignal<dashboard_configurator::ecu_reference::ReferenceState>,
    /// Persistent synthetic simulator and frame ownership across workspaces.
    dock: PreviewDockState,
    /// Bounded browser interaction timing measurements.
    trace: RwSignal<metrics::Trace>,
    /// Optional browser performance observer detached on cleanup.
    observer: RwSignal<Option<web_sys::PerformanceObserver>>,
}

/// Returns performance.now() milliseconds, or zero when Window/Performance is absent;
/// consumers handle unavailable/time-reversal observations without physical timing claims.
fn now() -> f64 { web_sys::window().and_then(|w| w.performance()).map_or(0.0, |p| p.now()) }

impl Ui {
    /// Returns the current candidate selection/mode/geometry identity, or None when
    /// its selected view has no resolvable orientation.
    fn canvas_identity(self) -> Option<dashboard_configurator::canvas_frame::CanvasFrame> {
        self.editor.with(|editor| {
            let geometry = editor
                .editing_config()
                .view_orientation(editor.selection().view)?
                .geometry();
            Some(dashboard_configurator::canvas_frame::CanvasFrame::capture(
                editor,
                self.dock.mode.get(),
                geometry,
            ))
        })
    }

    /// Returns whether Views permits layout editing and its exact current identity
    /// matches successfully published pixels; failures/pending drafts deny hit targets.
    fn canvas_current(self) -> bool {
        self.workspace.with(|workspace| workspace.active() == Workspace::Views)
            && self.editor.with(Editor::can_edit_layout)
            && self
                .canvas_identity()
                .is_some_and(|identity| self.published_canvas.with(|pixels| pixels.permits(identity)))
    }

    /// Advances published selection from `before` only when current identity differs
    /// in permitted selection metadata; absent/mismatched identities remain untrusted.
    fn canvas_selection(
        self,
        before: Option<dashboard_configurator::canvas_frame::CanvasFrame>,
    ) {
        if let Some((before, after)) = before.zip(self.canvas_identity()) {
            self.published_canvas.update(|pixels| pixels.selected(before, after));
        }
    }

    /// Writes RGBA `pixels` for `geometry`/`mode`, returning canvas errors without
    /// publishing new geometry. Records success/failure against the captured editor
    /// identity; browser submission does not prove physical display completion.
    fn write_canvas(
        self,
        pixels: &[u8],
        geometry: Geometry,
        mode: PreviewMode,
    ) -> Result<(), RenderIssue> {
        let identity = self.editor.with_untracked(|editor| {
            dashboard_configurator::canvas_frame::CanvasFrame::capture(editor, mode, geometry)
        });
        let result = controls::write_timing_canvas(self.canvas, pixels, geometry);
        self.published_canvas.update(|published| {
            let _ = published.publish(identity, result);
        });
        if result.is_ok() {
            self.pixel_geometry.set(geometry);
        }
        result
    }

    /// Returns cached accepted-project resource analysis or editor admission error;
    /// panics only if the synchronous cache callback unexpectedly fails to run.
    fn resource_report(self) -> Result<ResourceReport, EditorError> {
        self.editor.with(|editor| {
            let mut report = None;
            self.resources
                .update_value(|cache| report = Some(cache.inspect(editor)));
            report.expect("resource cache callback ran")
        })
    }

    /// Records bounded contextual `notice` locally and through enabled diagnostic sinks.
    fn event(
        self,
        notice: Notice,
    ) {
        event_log::record(self, &notice);
    }

    /// Reports changed write outcome `result` for `area`, suppressing repeated
    /// identical states and safely ignoring a disposed adapter.
    fn storage_outcome(
        self,
        area: StorageArea,
        result: Result<(), StorageIssue>,
    ) {
        let mut event = None;
        let _ = self
            .event_adapters
            .try_update_value(|state| event = state.storage(area, result));
        if let Some(event) = event {
            self.event(event);
        }
    }

    /// Reports changed restore/read `result` for `area` independently of write
    /// outcomes; disposed adapters emit nothing.
    fn storage_read_outcome(
        self,
        area: StorageArea,
        result: Result<(), StorageIssue>,
    ) {
        let mut event = None;
        let _ = self
            .event_adapters
            .try_update_value(|state| event = state.storage_read(area, result));
        if let Some(event) = event {
            self.event(event);
        }
    }

    /// Records boolean `success` for rendering `lane`, emitting only adapter
    /// transitions instead of per-frame diagnostic spam.
    fn render_outcome(
        self,
        lane: RenderLane,
        success: bool,
    ) {
        let mut event = None;
        let _ = self
            .event_adapters
            .try_update_value(|state| event = state.render(lane, success));
        if let Some(event) = event {
            self.event(event);
        }
    }

    /// Records detailed rendering `result` for `lane`, emitting bounded changes
    /// while retaining failure classification for recovery.
    fn render_issue(
        self,
        lane: RenderLane,
        result: Result<(), RenderIssue>,
    ) {
        let mut event = None;
        let _ = self
            .event_adapters
            .try_update_value(|state| event = state.render_result(lane, result));
        if let Some(event) = event {
            self.event(event);
        }
    }

    /// Compares accepted identity with `before`; a new valid configuration emits
    /// `subject` feedback and synchronizes/reports simulator restart. Draft-only changes
    /// do not become accepted-configuration events.
    fn accepted_field(
        self,
        before: AcceptedConfigKey,
        subject: &'static str,
    ) {
        if let Some(revision) = self
            .editor
            .try_with_untracked(|e| {
                (e.accepted_config_key() != before && !e.has_invalid_draft()).then(|| e.accepted_revision().get())
            })
            .flatten()
        {
            self.event(Notice::info(
                Category::Editor,
                "editor.accepted",
                Some(subject),
                format!("Accepted configuration revision {revision}"),
            ));
            self.editor.with_untracked(|e| self.dock.synchronize(self, e));
            self.dock.report_restart(self);
        }
    }

    /// Synchronizes simulator ownership, applies `action` once, then records bounded
    /// before/after state transitions; callers decide whether inspection permits action.
    fn simulate(
        self,
        action: impl FnOnce(&mut dashboard_configurator::simulator::ControlSimulator),
    ) {
        self.editor.with_untracked(|e| self.dock.synchronize(self, e));
        self.dock.report_restart(self);
        let mut events = Vec::new();
        self.dock.sim.update(|sim| {
            let before = SimulationObservation::capture(sim);
            action(sim);
            events = simulation_changes(&before, &SimulationObservation::capture(sim));
        });
        for event in events {
            self.event(event);
        }
    }

    /// Applies sampling/producer `action` only after synchronizing and rechecking
    /// pending inspection; blocked actions are no-ops. Physical controls use their
    /// separate bounded inspection route.
    fn simulate_accepted(
        self,
        action: impl FnOnce(&mut dashboard_configurator::simulator::ControlSimulator),
    ) {
        self.editor.with_untracked(|e| self.dock.synchronize(self, e));
        if self.inspection_active() {
            return;
        }
        self.simulate(action);
    }

    /// Returns whether current action readiness blocks save/export/build snapshots.
    fn snapshot_blocked(self) -> bool { self.editor.with(|e| e.action_readiness().block.is_some()) }

    /// Returns the capability explanation for widget-scoped registry `label`, or
    /// None for unknown/non-widget labels, missing widget, or a usable capability.
    fn control_reason(
        self,
        label: &str,
    ) -> Option<&'static str> {
        let key = workspaces::key_by_label(label)?;
        match Registry::descriptor(key).scope {
            dashboard_configurator::field_registry::Scope::Widget => {
                self.inspected_widget()
                    .and_then(|(id, _)| self.widget(id))
                    .and_then(|w| Registry::widget_capability(key, &w).reason())
            }
            _ => None,
        }
    }

    /// Returns inspected candidate ID/channel only when registry identity is current
    /// and unambiguous, otherwise None; absent inspection uses the editing selection.
    fn inspected_widget(self) -> Option<(u16, Channel)> {
        self.editor.with(|e| {
            if let Some(target) = self.inspection.get() {
                let resolved = Registry::resolve(e, target).ok()?;
                if !resolved.inspection.id_lookup_unambiguous {
                    return None;
                }
                let index = resolved.inspection.widget_index?;
                return Registry::candidate_widget(e, index).map(|w| (w.id, w.channel));
            }
            e.selection().widget.and_then(|id| {
                e.editing_config()
                    .widgets
                    .iter()
                    .find(|w| w.id == id)
                    .map(|w| (id, w.channel))
            })
        })
    }

    /// Returns the unambiguous current inspected candidate view ID, falling back to
    /// editor selection if the target is absent, stale or unresolved.
    fn inspected_view(self) -> u8 {
        self.editor.with(|e| {
            self.inspection
                .get()
                .and_then(|target| Registry::resolve(e, target).ok())
                .filter(|r| r.inspection.id_lookup_unambiguous)
                .and_then(|r| r.inspection.view_index)
                .and_then(|index| Registry::candidate_view_id(e, index))
                .unwrap_or(e.selection().view)
        })
    }

    /// Returns registry-resolved DOM identity for raw `field`, or its legacy field ID
    /// fallback when the current editor cannot resolve that target.
    fn raw_dom(
        self,
        field: RawField,
    ) -> String {
        self.editor.with(|e| {
            FieldId::Raw(field)
                .resolve(e)
                .unwrap_or_else(|_| fields::field_id(field))
        })
    }

    /// Returns the registry DOM ID for `label` at its current root/widget/channel
    /// owner; unsupported scope, unknown label or missing owner returns an empty string.
    fn control_id(
        self,
        label: &str,
    ) -> String {
        let Some(key) = workspaces::key_by_label(label) else {
            return String::new();
        };
        let scope = Registry::descriptor(key).scope;
        let owner = match scope {
            dashboard_configurator::field_registry::Scope::Widget => {
                return self
                    .inspected_widget()
                    .map(|(id, _)| {
                        self.editor
                            .with(|e| FieldId::Widget { id, key }.resolve(e).unwrap_or_default())
                    })
                    .unwrap_or_default();
            }
            dashboard_configurator::field_registry::Scope::Channel => {
                Some(TargetOwner::Channel(self.source_channel.get()))
            }
            dashboard_configurator::field_registry::Scope::Root => Some(TargetOwner::Root),
            _ => None,
        };
        owner.map(|owner| Registry::dom_id(key, owner)).unwrap_or_default()
    }

    /// Returns whether pending JSON or a stale/ambiguous inspected target forbids
    /// typed repair; ordinary invalid raw drafts can remain editable.
    fn inspect_read_only(self) -> bool {
        self.editor.with(|e| {
            e.has_pending_json()
                || self.inspection.get().is_some_and(|target| {
                    Registry::resolve(e, target).map_or(true, |r| !r.inspection.id_lookup_unambiguous)
                })
        })
    }

    /// Rebinds `target`'s key/owner to current editor identity when resolvable;
    /// failure leaves the old target for downstream stale-target guards.
    fn refresh_inspection(
        self,
        target: FieldTarget,
    ) {
        if let Ok(next) = self
            .editor
            .with_untracked(|e| Registry::target(e, target.key(), target.owner()))
        {
            self.inspection.set(Some(next));
        }
    }

    /// Reports each newly exhausted cache identity in `status` once for this mount,
    /// updating notice and a warning event before later observations can repeat it.
    fn observe_cache(
        self,
        status: dashboard_core::cache_diagnostics::CacheExhaustionStatus,
    ) {
        self.cache_seen.update(|seen| {
            dashboard_configurator::cache_reporting::report(
                seen,
                status,
                |cause| self.notice.set(cause.to_string()),
                |cause| {
                    self.event(Notice {
                        category: Category::Rendering,
                        severity: dashboard_configurator::events::Severity::Warn,
                        code: "cache.exhausted",
                        subject: None,
                        message: cause.to_string(),
                    })
                },
            )
        });
    }

    /// Returns whether the editor retains a rejected raw/structural draft.
    fn invalid(self) -> bool { self.editor.with(Editor::has_invalid_draft) }

    /// Returns whether unapplied JSON text blocks accepted-state actions.
    fn json_pending(self) -> bool { self.editor.with(Editor::has_pending_json) }

    /// Returns whether invalid editor draft or pending JSON requires inspection
    /// routing instead of accepted simulation acquisition.
    fn inspection_active(self) -> bool { self.invalid() || self.json_pending() }

    /// Returns whether editor readiness currently denies layout changes.
    fn layout_locked(self) -> bool { self.editor.with(|e| !e.can_edit_layout()) }

    /// Runs `action` through normal feedback/persistence and returns true on success;
    /// newly accepted configuration identity also triggers acceptance/restart events.
    fn run(
        self,
        action: impl FnOnce(&mut Editor) -> Result<Revision, EditorError>,
    ) -> bool {
        let before = self.editor.with_untracked(Editor::accepted_config_key);
        let success = self.run_quiet(action);
        if success {
            self.accepted_field(before, "Configuration");
        }
        success
    }

    /// Applies `action` once and returns whether it succeeded. Tracks revision/selection
    /// and validation transitions, clears stale focus, schedules conflict-free valid saves
    /// and retains rejected drafts with error feedback. Success need not imply a change.
    fn run_quiet(
        self,
        action: impl FnOnce(&mut Editor) -> Result<Revision, EditorError>,
    ) -> bool {
        let started = now();
        let mut result = Ok(());
        let mut changed = false;
        let mut selection_changed = false;
        let mut validation = None;
        self.editor.update(|editor| {
            let before = editor.revision();
            let selection = editor.selection();
            let invalid = editor.has_invalid_draft();
            result = action(editor).map(|_| ());
            changed = before != editor.revision();
            selection_changed = selection != editor.selection();
            validation = validation_transition(invalid, editor.has_invalid_draft());
        });
        if let Some(event) = validation {
            self.event(event);
        }
        if selection_changed || (changed && !self.editor.with_untracked(Editor::has_invalid_draft)) {
            self.inspection.set(None);
            self.error_inspection.set(None);
            command_palette::cancel_focus(self);
        }
        if changed && self.pending_at.get_untracked().is_none() {
            self.pending_at.set(Some(started));
        }
        match result {
            Ok(()) => {
                self.notice.set(String::new());
                if changed && !self.editor.with_untracked(Editor::has_invalid_draft) && !self.conflict.get_untracked() {
                    io::save(self, false);
                }
                true
            }
            Err(error) => {
                self.notice.set(error_text(error));
                false
            }
        }
    }

    /// Captures the first pending input timestamp until a successful render consumes it.
    fn input_started(self) {
        if self.pending_at.get_untracked().is_none() {
            self.pending_at.set(Some(now()));
        }
    }

    /// Submits `edit` through normal editor admission, feedback and eligible persistence.
    fn apply(
        self,
        edit: Edit,
    ) {
        self.run(move |editor| editor.apply(edit));
    }

    /// Submits `edit` only under current `token` and unambiguous inspection authority;
    /// successful inspected edits refresh the target identity. Rejected values remain
    /// subject to editor draft semantics and are reported through quiet feedback.
    fn typed(
        self,
        token: FieldToken,
        edit: Edit,
    ) {
        let inspection = self.inspection.get_untracked();
        let success = self.run_quiet(move |editor| {
            if let Some(target) = inspection {
                let resolved = Registry::resolve(editor, target).map_err(|_| EditorError::StaleToken)?;
                if !resolved.inspection.id_lookup_unambiguous {
                    return Err(EditorError::AmbiguousTarget);
                }
            }
            editor.apply_if_current(token, edit)
        });
        if success && let Some(target) = inspection {
            self.refresh_inspection(target);
        }
    }

    /// Returns a layout-safe unique widget `id` for canvas interactions, or None
    /// when missing/ambiguous/unavailable in the editor layout projection.
    fn canvas_widget(
        self,
        id: u16,
    ) -> Option<Widget> {
        self.editor.with(|e| Registry::layout_widget(e, id))
    }

    /// Returns widget `id` from the resolved inspected candidate position when present,
    /// otherwise the editing configuration; stale inspection or mismatched ID returns None.
    fn widget(
        self,
        id: u16,
    ) -> Option<Widget> {
        self.editor.with(|e| {
            if let Some(target) = self.inspection.get() {
                let resolved = Registry::resolve(e, target).ok()?;
                if let Some(index) = resolved.inspection.widget_index {
                    return Registry::candidate_widget(e, index).filter(|w| w.id == id);
                }
            }
            e.editing_config().widgets.iter().find(|w| w.id == id).cloned()
        })
    }

    /// Applies `f` once to existing widget `id`'s placement, submitting it under
    /// `token`; absent widget is a no-op and shared admission checks geometry/overlap.
    fn place_widget(
        self,
        token: FieldToken,
        id: u16,
        f: impl FnOnce(&mut Placement),
    ) {
        let Some(widget) = self.widget(id) else {
            return;
        };
        let mut placement = widget.placement;
        f(&mut placement);
        self.typed(token, Edit::PlaceWidget { id, placement });
    }

    /// Applies `f` once to a copy of widget `id`, submitting it under `token`;
    /// missing widget is a no-op and rejected complete edits remain repairable drafts.
    fn update_widget(
        self,
        token: FieldToken,
        id: u16,
        f: impl FnOnce(&mut Widget),
    ) {
        if let Some(mut widget) = self.widget(id) {
            f(&mut widget);
            self.typed(token, Edit::UpdateWidget(widget));
        }
    }

    /// Returns current save/conflict status, distinguishing pending JSON, artwork
    /// shape mismatch, quota failure and unavailable browser storage.
    fn save_label(self) -> String {
        if self.conflict.get() {
            return "Browser draft changed in another tab".into();
        }
        self.editor.with(|e| {
            match e.save_status() {
                SaveStatus::Saved => "Saved in this browser".into(),
                SaveStatus::Dirty => "Unsaved changes".into(),
                SaveStatus::InvalidDraft if e.has_pending_json() => "Pending JSON · not saved".into(),
                SaveStatus::InvalidDraft if e.artwork_geometry_mismatch() => {
                    "Splash needs conversion · not saved".into()
                }
                SaveStatus::InvalidDraft => "Invalid edits · not saved".into(),
                SaveStatus::Saving => "Saving…".into(),
                SaveStatus::Failed(StorageFailure::Quota) => "Storage full · export to keep changes".into(),
                SaveStatus::Failed(_) => "Browser storage unavailable · export to keep changes".into(),
            }
        })
    }

    /// Returns first accepted-layout placement for `span` in the selected view,
    /// ignoring widget `except` when supplied; None means no admissible free footprint.
    fn free(
        self,
        span: Span,
        except: Option<u16>,
    ) -> Option<Placement> {
        self.editor.with_untracked(|e| {
            dashboard_configurator::geometry::first_free_placement(
                e.preview().config(),
                e.selection().view,
                span,
                except,
            )
        })
    }
}

/// Returns the mounted Studio editor with restored browser preferences/projects,
/// persistent preview and explicit helper controls. Registers owned listeners/timers
/// and cleanup; native helper connection, device operations and flashing require actions.
#[component]
pub fn Studio() -> impl IntoView {
    let mount_started = now();
    let (editor, initial_notice, saved_bytes, draft_storage) = io::restore();
    let (workspace, preference_notice, preference_storage) = workspaces::restore();
    let (presets, preset_storage) = io::restore_presets();
    let dock = PreviewDockState::new(&editor);
    let editor = RwSignal::new(editor);
    let ui = Ui {
        cache_seen: RwSignal::new(dashboard_core::cache_diagnostics::CacheExhaustionSeen::new()),
        mount_started,
        editor,
        build: RwSignal::new(dashboard_configurator::build_flash::BuildSelection::default()),
        helper: helper::State::new(),
        relay: relay::State::new(),
        resources: StoredValue::new(ResourceCache::default()),
        json: json_editor::State::new(),
        events: RwSignal::new(EventStore::default()),
        event_adapters: StoredValue::new(AdapterState::default()),
        console_enabled: RwSignal::new(true),
        downloads: StoredValue::new_local(io::Downloads::default()),
        notice: RwSignal::new(if preference_notice.is_empty() {
            initial_notice
        } else {
            preference_notice
        }),
        conflict: RwSignal::new(false),
        saved_bytes: RwSignal::new(saved_bytes),
        appearance: RwSignal::new(AppearanceSession::default()),
        presets: RwSignal::new(presets),
        preset_lock_busy: RwSignal::new(false),
        preset_import_busy: RwSignal::new(false),
        preset_abort: StoredValue::new_local(None),
        psi: RwSignal::new(false),
        warnings: RwSignal::new(false),
        sample_ms: RwSignal::new(118_000),
        sample_set: RwSignal::new(dashboard_configurator::preview::SampleSet::Demo),
        preview_error: RwSignal::new(None),
        pending_at: RwSignal::new(None),
        render_ms: RwSignal::new(0.0),
        canvas: NodeRef::new(),
        pixel_geometry: RwSignal::new(Geometry::Landscape),
        published_canvas: RwSignal::new(Default::default()),
        pixel_fit_issues: RwSignal::new(Vec::new()),
        drag: RwSignal::new(None),
        capture: RwSignal::new(None),
        menu: RwSignal::new(None),
        playing: RwSignal::new(false),
        startup: startup::State::new(editor),
        project_store: project_io::State::new(),
        workspace: RwSignal::new(workspace),
        inspection: RwSignal::new(None),
        error_inspection: RwSignal::new(None),
        focus: RwSignal::new(command_palette::FocusState::default()),
        source_channel: RwSignal::new(Channel::Oil),
        reference: RwSignal::new(dashboard_configurator::ecu_reference::ReferenceState::default()),
        dock,
        trace: RwSignal::new(metrics::Trace::default()),
        observer: RwSignal::new(None),
    };
    #[cfg(feature = "browser-tests")]
    browser_tests::launch(ui);
    project_io::restore(ui);
    ui.event(Notice::info(
        Category::Editor,
        "studio.initialized",
        None,
        "Studio initialized; diagnostics retained locally",
    ));
    ui.storage_read_outcome(StorageArea::Draft, draft_storage);
    ui.storage_read_outcome(StorageArea::Preferences, preference_storage);
    ui.storage_read_outcome(StorageArea::Presets, preset_storage);
    dock.install(ui);
    ui.helper.install();
    ui.relay.install(ui);
    stage::install_playback(ui);
    widget_menu::install(ui);
    metrics::install(ui);
    let storage_listener = window_event_listener(leptos::ev::storage, move |event| {
        if event.key().as_deref().is_none_or(|key| key == io::STORAGE_KEY) {
            io::storage_changed(ui);
        }
        if event.key().as_deref().is_none_or(|key| key == PRESET_STORAGE_KEY) {
            io::presets_changed(ui);
        }
    });
    on_cleanup(move || {
        storage_listener.remove();
        io::cancel_presets(ui);
    });
    view! {
        <div class="studio" on:pointermove=move |e| stage::drag_move(ui, e)
            on:pointerup=move |e| stage::drag_end(ui,e) on:pointercancel=move |e| {stage::cancel_event(ui,e);simulation::cancel(ui);}
            on:lostpointercapture=move |e| {stage::cancel_event(ui,e);simulation::cancel(ui);}
            on:keydown=move |e| { if e.key() == "Escape" { stage::cancel(ui); widget_menu::close(ui, true); } }>
            <header class="masthead">
                <a class="brand" href="./" aria-label="EA888 View Studio"><b>"EA"<span>"888"</span></b><div>"VIEW STUDIO"<small>"PICO 2 W / DNUC"</small></div></a>
                <div class="device-spec"><span class="dot"></span>{move ||format!("{} × {}",ui.pixel_geometry.get().width(),ui.pixel_geometry.get().height())}<span>{move ||format!("{} × {} GRID",ui.pixel_geometry.get().columns(),ui.pixel_geometry.get().rows())}</span></div>
                <div class="header-actions">
                    <command_palette::CommandPalette ui/>
                    <button title="Undo accepted edit" disabled=move || ui.editor.with(|e| Command::Undo.availability(e, e.stamp()).is_some()) on:click=move |_| { command_palette::execute_current(ui, Command::Undo); }>"↶ Undo"</button>
                    <button title="Redo accepted edit" disabled=move || ui.editor.with(|e| Command::Redo.availability(e, e.stamp()).is_some()) on:click=move |_| { command_palette::execute_current(ui, Command::Redo); }>"↷ Redo"</button>
                    <button class="primary" disabled=move || ui.snapshot_blocked() on:click=move |_| project_io::export(ui,dashboard_config_tools::project::ExportMode::Complete)>"Export project"<span>"↗"</span></button>
                </div>
            </header>
            <workspaces::WorkspaceTabs ui/>
            <fields::DiagnosticFocus ui/>
            <div class="workspace-layout" data-workspace=move ||ui.workspace.with(|s|s.active().id())>
                <section class="workspace-panel views-panel" id="studio-panel-views" role="tabpanel" aria-labelledby="studio-tab-views" tabindex="0" hidden=move ||ui.workspace.with(|s|s.active()!=Workspace::Views)>
                    <Library ui/>
                    <main class="view-settings">
                        <div class="section-heading"><div><p class="eyebrow">"YOUR DASHBOARD"</p><h1>"Build your view."</h1></div><span class="chip">{move ||ui.editor.with(|e|format!("{} / 8 views",e.preview().config().views.len()))}</span></div>
                        <Views ui/>
                    <div class="draft-alert" hidden=move || !ui.invalid() role="alert">
                        <strong>"INVALID PREVIEW · inspection only"</strong>
                        <p>{move || ui.editor.with(|e| e.draft_error().map(error_text).unwrap_or_default())}</p>
                        <p>"Rejected labels, styles and sizes are shown, including overlaps. Outside-grid positions project to the nearest valid position. Invalid numbers use safe bounds; source and fuel use the last valid settings. Parsed widget and band colours are shown even when unreadable. Edited warning colours show a sample popup. Live warning simulation is suspended. Unsupported glyphs appear as ? and long text is clipped. Exact rejected inputs and fallbacks are listed below."</p>
                        <p>{move ||if ui.layout_locked(){"Repair highlighted values or change the affected option, reference mode, band or control binding. Unrelated raw text stays visible; saving and export unlock only when every error is resolved. Discard returns to the accepted configuration."}else{"Repair fields or options, drag, resize or remove widgets. Right-click an overlap to select a covered widget. Save and export unlock once every error is resolved."}}</p>
                        <ul class="draft-details">{move ||ui.editor.with(|e|e.diagnostic().map(|draft|draft.descriptions(e.preview()).into_iter().map(|line|view!{<li>{line}</li>}).collect_view()))}</ul>
                        <button on:click=move |_| { ui.run(Editor::discard_raw); }>"Discard invalid edits"</button>
                    </div>
                    </main>
                </section>
                <section class="workspace-panel settings-panel" id="studio-panel-device" role="tabpanel" aria-labelledby="studio-tab-device" tabindex="0" hidden=move ||ui.workspace.with(|s|s.active()!=Workspace::Device)>
                    <Group ui group=GroupId::Bindings><orientation::Picker ui scope=orientation::Scope::Device/><controls::DeviceControls ui/></Group>
                    <Group ui group=GroupId::Ble><ble::BlePanel ui/></Group>
                    <Group ui group=GroupId::Indicator><led::LedPanel ui/></Group>
                    <startup::Panel ui/>
                    <div class="resource-navigation"><p>"Review configuration resources in Diagnostics and firmware artifacts in Build & Flash."</p><button on:click=move |_|workspaces::activate(ui,Workspace::BuildFlash)>"Open Build & Flash"</button></div>
                </section>
                <section class="workspace-panel settings-panel" id="studio-panel-data-warnings" role="tabpanel" aria-labelledby="studio-tab-data-warnings" tabindex="0" hidden=move ||ui.workspace.with(|s|s.active()!=Workspace::DataWarnings)>
                    <h1>"Data & warnings"</h1>
                    <Group ui group=GroupId::Channels><workspaces::ChannelHistory ui/></Group>
                    <Group ui group=GroupId::Sources><SourceSettings ui/></Group>
                    <Group ui group=GroupId::Fuel><FuelSettings ui/></Group>
                    <Group ui group=GroupId::CombinedWarnings><WarningSettingsPanel ui/><workspaces::RuleSummary ui/></Group>
                </section>
                <section class="workspace-panel settings-panel" id="studio-panel-diagnostics" role="tabpanel" aria-labelledby="studio-tab-diagnostics" tabindex="0" hidden=move ||ui.workspace.with(|s|s.active()!=Workspace::Diagnostics)>
                    <h1>"Diagnostics"</h1>
                    <event_log::EventLog ui/>
                    <Group ui group=GroupId::Simulation><controls::SimulationControls ui/><ble::BleSimulationPanel ui/></Group>
                    <Group ui group=GroupId::Files><io::Files ui/></Group>
                    <Group ui group=GroupId::Measurements><resources::Resources ui/></Group>
                </section>
                <section class="workspace-panel settings-panel" id="studio-panel-ecu-reference" role="tabpanel" aria-labelledby="studio-tab-ecu-reference" tabindex="0" hidden=move ||ui.workspace.with(|s|s.active()!=Workspace::EcuReference)><ecu_reference::ReferenceWorkspace ui/></section>
                <section class="workspace-panel settings-panel" id="studio-panel-build-flash" role="tabpanel" aria-labelledby="studio-tab-build-flash" tabindex="0" hidden=move ||ui.workspace.with(|s|s.active()!=Workspace::BuildFlash)><build_flash::BuildFlash ui/></section>
                <section class="preview-dock" id="studio-preview-dock" aria-label="Persistent display preview">
                    <workspaces::InspectionDetail ui/>
                    <div class="preview-mode"><button id="studio-mode-appearance" class:active=move ||ui.dock.mode.get()==PreviewMode::Appearance on:click=move |_|{stage::cancel(ui);simulation::cancel(ui);ui.dock.mode.set(PreviewMode::Appearance);}>"Appearance"</button><button id="studio-mode-simulation" class:active=move ||ui.dock.mode.get()==PreviewMode::Simulation on:click=move |_|{stage::cancel(ui);simulation::cancel(ui);ui.dock.mode.set(PreviewMode::Simulation);workspaces::open(ui,Workspace::Diagnostics,GroupId::Simulation);}>"Simulation"</button></div>
                    <div hidden=move ||ui.dock.mode.get()!=PreviewMode::Appearance>
                    <div class="preview-toolbar"><span><i class="status-light"></i>"SYNTHETIC PREVIEW"</span><div>
                        <button class:active=move || ui.psi.get() on:click=move |_| {ui.input_started();ui.psi.update(|v| *v = !*v);}>{move || if ui.psi.get() { "PSI / bar" } else { "bar / PSI" }}</button>
                        <button class:active=move || ui.warnings.get() on:click=move |_| {ui.input_started();ui.warnings.update(|v| *v = !*v);}>"Warnings"</button>
                        <button class:active=move ||ui.playing.get() on:click=move |_|ui.playing.update(|v|*v = !*v)>{move ||if ui.playing.get(){"Pause"}else{"Play"}}</button>
                        <button title="Advance synthetic demo values by one second" disabled=move ||ui.playing.get() on:click=move |_| {ui.input_started();ui.sample_ms.update(|t| *t=t.saturating_add(1000));}>"Next sample →"</button>
                    </div></div>
                    <label class="preview-samples">"Test values"
                        <select aria-label="Preview test values" on:change=move |event|{
                            use dashboard_configurator::preview::SampleSet;
                            ui.input_started();
                            ui.sample_set.set(match event_target_value(&event).as_str(){"minimum"=>SampleSet::Minimum,"maximum"=>SampleSet::Maximum,"width"=>SampleSet::WidthStress,"zero"=>SampleSet::Zero,_=>SampleSet::Demo});
                        }>
                            <option value="demo">"Animated demo"</option><option value="minimum">"Channel minima"</option><option value="maximum">"Channel maxima"</option><option value="width">"Extreme digit widths"</option><option value="zero">"Zero / unavailable AFR"</option>
                        </select><span>"Synthetic only · not operating limits"</span>
                    </label>
                    </div>
                    <startup::Playback ui/>
                    <stage::Canvas ui/>
                    <div class="canvas-caption"><span>{move ||if ui.dock.mode.get()==PreviewMode::Appearance && ui.workspace.with(|s|s.active()==Workspace::Views){"Drag to place · right-click for actions · arrows to move · Esc to cancel"}else{"Synthetic device display · controls in Diagnostics"}}</span><span>{move ||if ui.dock.mode.get()==PreviewMode::Appearance{format!("Appearance preview render {:.1} ms",ui.render_ms.get())}else{"Synthetic frame · no physical timing measurement".into()}}</span></div>
                </section>
                <Inspector ui/>
            </div>
            <footer class="statusbar"><div><span class="dot" class:error=move || ui.inspection_active()></span><span>{move || ui.save_label()}</span><span>{move ||json_editor::label(ui)}</span>
                <button disabled=move || ui.snapshot_blocked() on:click=move |_| io::save(ui,true)>"Save here"</button></div>
                <span>{move || match ui.resource_report() {
                    Ok(report) => resources::footer(&report),
                    Err(_) => "configuration resources unavailable".into(),
                }}</span>
            </footer>
            <div class="notice" role="status" hidden=move || ui.notice.get().is_empty()><span>{move || ui.notice.get()}</span><button aria-label="Dismiss notice" on:click=move |_| ui.notice.set(String::new())>"×"</button></div>
            <widget_menu::Popup ui/>
        </div>
    }
}

/// Returns accepted view navigation and inspected-view fields for `ui`; selection,
/// duplication, ordering and deletion use shared editor admission and preserve repair paths.
#[component]
fn Views(
    /// Shared view selection, layout candidate and accepted editor history.
    ui: Ui
) -> impl IntoView {
    view! {
        <div class="view-tabs" role="navigation" aria-label="Dashboard views">
            <For each=move || ui.editor.with(|e| e.preview().config().views.iter().map(|v| v.id).collect::<Vec<_>>()) key=|id| *id children=move |id| {
                view! { <button class:active=move || ui.editor.with(|e| e.selection().view==id) disabled=move || ui.inspection_active()
                    on:click=move |_| { ui.run(move |e| e.select(Selection { view:id,widget:None })); }>
                    {move || ui.editor.with(|e| e.preview().config().views.iter().find(|v|v.id==id).map(|v|v.name.to_string()).unwrap_or_default())}
                    <small>{move || ui.editor.with(|e| if e.preview().config().boot_view==id { "BOOT" } else { "" })}</small>
                </button> }
            }/>
            <button class="add-view" title="Add empty view" disabled=move || ui.inspection_active() || ui.editor.with(|e|e.preview().config().views.len()>=8)
                on:click=move |_| ui.apply(Edit::AddView { name:"New view".into() })>"+ View"</button>
        </div>
        <For each=move || vec![ui.inspected_view()] key=|id| *id children=move |id| view! {
            <div class="view-controls">
                <TextField ui field=RawField::ViewName(id) label="View name" hint="1–14 ASCII characters"/><orientation::Picker ui scope=orientation::Scope::View(id)/>
                <div class="view-tools">
                    <button id=Registry::dom_id(FieldKey::BootView,TargetOwner::Root) disabled=move || ui.inspection_active() on:click=move |_| ui.apply(Edit::SetBootView(id))>"Set as boot"</button>
                    <button disabled=move || ui.inspection_active() on:click=move |_| ui.apply(Edit::DuplicateView { id,name:"Copy".into() })>"Duplicate"</button>
                    <button title="Move view earlier in encoder order" disabled=move || ui.inspection_active() on:click=move |_| {
                        let index=ui.editor.with_untracked(|e|e.preview().config().views.iter().position(|v|v.id==id).unwrap_or(0));
                        ui.apply(Edit::ReorderView { id,index:index.saturating_sub(1) });
                    }>"←"</button>
                    <button title="Move view later in encoder order" disabled=move || ui.inspection_active() on:click=move |_| {
                        let index=ui.editor.with_untracked(|e| {let c=e.preview().config();(c.views.iter().position(|v|v.id==id).unwrap_or(0)+1).min(c.views.len()-1)});
                        ui.apply(Edit::ReorderView { id,index });
                    }>"→"</button>
                    <button class="danger-text" disabled=move || ui.inspection_active() || ui.editor.with(|e|e.preview().config().views.len()==1) on:click=move |_| ui.apply(Edit::DeleteView(id))>"Delete"</button>
                </div>
            </div>
        }/>
    }
}
#[cfg(feature = "browser-tests")]
/// Isolated browser integration/failure suite gated by origin handshake.
mod browser_tests;

#[cfg(feature = "browser-tests")]
/// Real DOM BLE field and synthetic-fixture regressions.
mod ble_browser_tests;
