//! Browser preferences and pure manual workspace navigation, never firmware state.
use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

pub use crate::appearance_presets::AppearanceScope;
use crate::appearance_presets::{AppearanceOffer, AppearanceRequest, AppearanceSnapshot};
use crate::config::{MAX_VIEWS, MAX_WIDGETS, Placement};
use crate::editor::{ActionBlock, Edit, Editor, EditorError, EditorStamp, InspectionOwner, InspectionTarget};
use crate::field_registry::{FieldTarget, Registry};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Dashboard-only export selection; complete project/startup artwork uses separate project actions.
pub enum ExportFormat {
    /// Editable configuration JSON.
    Json,
    /// Validated binary EAV configuration for firmware.
    Eav,
}
/// A closed action allowlist. Object authority is minted only by Editor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    /// Activate a browser workspace without editing firmware state.
    Navigate(
        /// Workspace to activate and focus.
        Workspace,
    ),
    /// Reveal one current registry-owned field.
    Find(
        /// Revision/owner-bound field inspection target.
        FieldTarget,
    ),
    /// Clone selected widget into available same-view geometry.
    DuplicateWidget(
        /// Captured selected object authority; None is an unavailable inventory row.
        Option<InspectionTarget>,
    ),
    /// Clone selected view and its widgets within shared limits.
    DuplicateView(
        /// Captured selected object authority; None is an unavailable inventory row.
        Option<InspectionTarget>,
    ),
    /// Capture portable appearance from a current clean selected widget.
    CopyAppearance,
    /// Open confirmation for a guarded appearance offer, without applying it yet.
    ApplyAppearance(
        /// Destination set captured by the appearance offer.
        AppearanceScope,
    ),
    /// Restore the previous accepted history entry when history admission permits.
    Undo,
    /// Reapply the next accepted history entry when history admission permits.
    Redo,
    /// Open the editor-owned JSON transaction in Diagnostics.
    OpenJson,
    /// Request an admitted dashboard-only export from the browser adapter.
    Export(
        /// Requested configuration encoding.
        ExportFormat,
    ),
    /// Request the simulator's reserved two-second Y recovery gesture.
    RecoverSimulator,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Reducer dispatch result; browser adapters perform deferred focus, download and simulator effects.
pub enum CommandEffect {
    /// JSON transaction opened and Files group activated.
    OpenJson(
        /// Editor-owned current JSON handle for adapter focus/parsing.
        crate::json_draft::JsonDraftHandle,
    ),
    /// Portable appearance captured, ready for local clipboard storage.
    CopiedAppearance(
        /// Value snapshot excluding source identity and unrelated settings.
        AppearanceSnapshot,
    ),
    /// Appearance target/source admitted for confirmation, without changing config.
    PreviewAppearance(
        /// Guarded request to retain until explicit apply.
        AppearanceRequest,
    ),
    /// Workspace activation already applied to workspace state.
    Navigate(
        /// Activated workspace for browser focus/persistence.
        Workspace,
    ),
    /// Target workspace/group opened, leaving browser field focus pending.
    Inspect(
        /// Still-guarded field target to resolve at focus time.
        FieldTarget,
    ),
    /// Editor transaction/history action completed; refresh rendered state.
    Changed,
    /// Export admitted; adapter must still snapshot/write the current project safely.
    Export(
        /// Dashboard-only output encoding.
        ExportFormat,
    ),
    /// Adapter should issue the reserved simulator recovery gesture.
    RecoverSimulator,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Command admission or editor transaction rejection, retaining their different state effects.
pub enum CommandError {
    /// No command mutation admitted.
    Unavailable(
        /// Stable user-facing admission reason.
        &'static str,
    ),
    /// Editor rejected dispatch; an invalid edit may remain inspectable according to editor transaction rules.
    Editor(
        /// Underlying typed reducer failure.
        EditorError,
    ),
}
impl CommandError {
    /// Returns a display reason, preserving an unavailable message or formatting the typed editor rejection.
    pub fn message(self) -> String {
        match self {
            Self::Unavailable(reason) => reason.into(),
            Self::Editor(error) => format!("Command rejected: {error:?}"),
        }
    }
}
impl Command {
    /// Returns a duplication command capturing `editor`'s selected preview widget, or a disabled None-target row if
    /// unresolved.
    pub fn selected_widget(editor: &Editor) -> Self {
        Self::DuplicateWidget(editor.selection().widget.and_then(|id| {
            let index = editor.preview().config().widgets.iter().position(|w| w.id == id)?;
            editor.object_inspection_target(InspectionOwner::Widget { index }).ok()
        }))
    }

    /// Returns a duplication command capturing `editor`'s selected preview view, or a disabled None-target row if
    /// unresolved.
    pub fn selected_view(editor: &Editor) -> Self {
        Self::DuplicateView(
            editor
                .preview()
                .config()
                .views
                .iter()
                .position(|v| v.id == editor.selection().view)
                .and_then(|index| editor.object_inspection_target(InspectionOwner::View { index }).ok()),
        )
    }

    /// Returns the fixed command palette inventory plus selected-object authorities minted from `editor` now.
    /// Availability is checked separately at display/dispatch time; this does not issue Find results.
    pub fn inventory(editor: &Editor) -> Vec<Self> {
        let mut commands = Workspace::ALL.into_iter().map(Self::Navigate).collect::<Vec<_>>();
        commands.extend([
            Self::selected_widget(editor),
            Self::selected_view(editor),
            Self::CopyAppearance,
            Self::ApplyAppearance(AppearanceScope::Selected),
            Self::ApplyAppearance(AppearanceScope::ThisView),
            Self::ApplyAppearance(AppearanceScope::AllWidgets),
            Self::Undo,
            Self::Redo,
            Self::OpenJson,
            Self::Export(ExportFormat::Json),
            Self::Export(ExportFormat::Eav),
            Self::RecoverSimulator,
        ]);
        commands
    }

    /// Returns the fixed palette label for this command; no object value or admission reason is included.
    pub fn label(self) -> &'static str {
        match self {
            Self::Navigate(w) => w.label(),
            Self::Find(_) => "Find setting",
            Self::DuplicateWidget(_) => "Duplicate selected widget",
            Self::DuplicateView(_) => "Duplicate selected view",
            Self::CopyAppearance => "Copy Appearance",
            Self::ApplyAppearance(AppearanceScope::Selected) => "Apply Appearance to selected",
            Self::ApplyAppearance(AppearanceScope::ThisView) => "Apply Appearance to this view",
            Self::ApplyAppearance(AppearanceScope::AllWidgets) => "Apply Appearance to all widgets",
            Self::Undo => "Undo",
            Self::Redo => "Redo",
            Self::OpenJson => "Open JSON",
            Self::Export(ExportFormat::Json) => "Export editable JSON",
            Self::Export(ExportFormat::Eav) => "Export for Pico (.eav)",
            Self::RecoverSimulator => "Recover simulator (Y hold, 2 s)",
        }
    }

    /// Returns an unavailable reason for `editor` against `captured_stamp`, or None when currently admitted.
    /// No appearance offer is supplied, so copy/apply actions remain unavailable; dispatch uses this same synchronous
    /// policy.
    pub fn availability(
        self,
        editor: &Editor,
        captured_stamp: EditorStamp,
    ) -> Option<&'static str> {
        self.availability_with_appearance(editor, captured_stamp, None)
    }

    /// Returns None when `editor` admits this command at `captured_stamp` using optional captured `appearance`, else a
    /// reason. Even navigation/recovery reject stale stamps; Undo/Redo use history readiness, other mutating/export
    /// actions use snapshot readiness. Selection targets and geometry are rechecked; no state is changed or
    /// appearance confirmation applied.
    pub fn availability_with_appearance(
        self,
        editor: &Editor,
        captured_stamp: EditorStamp,
        appearance: Option<AppearanceOffer>,
    ) -> Option<&'static str> {
        if captured_stamp != editor.stamp() {
            return Some("Field changed; refresh commands and results");
        }
        match self {
            Self::Navigate(_) | Self::RecoverSimulator => return None,
            Self::Find(target) => {
                return Registry::resolve(editor, target)
                    .err()
                    .map(|_| "Field changed; refresh commands and results");
            }
            Self::OpenJson => return None,
            _ => {}
        }
        let readiness = if matches!(self, Self::Undo | Self::Redo) {
            editor.history_readiness()
        } else {
            editor.action_readiness()
        };
        if let Some(block) = readiness.block {
            return Some(match block {
                ActionBlock::InvalidDraft => "Repair or discard invalid edits first",
                ActionBlock::JsonPending => "Apply or Discard pending JSON first",
                ActionBlock::StartupPending => "Apply or discard startup timing first",
                ActionBlock::PointerActive => "Finish or cancel the active pointer gesture",
                ActionBlock::ImportPending => "Wait for or cancel the current import",
                ActionBlock::IdentityExhausted => "Editor identity exhausted; no new action can be accepted",
            });
        }
        match self {
            Self::CopyAppearance => {
                match appearance.and_then(AppearanceOffer::copy_request) {
                    Some(request) => {
                        editor
                            .check_appearance_request(request)
                            .err()
                            .map(|_| "Appearance target changed; refresh this action")
                    }
                    None => Some("Select a current clean widget to copy"),
                }
            }
            Self::ApplyAppearance(scope) => {
                match appearance {
                    None => Some("Choose a usable appearance source in Views"),
                    Some(offer) => {
                        match offer.request(scope) {
                            Some(request) => {
                                editor
                                    .check_appearance_request(request)
                                    .err()
                                    .map(|_| "Appearance target changed; refresh this action")
                            }
                            None => Some("Appearance source or destination is unavailable"),
                        }
                    }
                }
            }
            Self::Undo if editor.history_len().0 == 0 => Some("No accepted edit to undo"),
            Self::Redo if editor.history_len().1 == 0 => Some("No accepted edit to redo"),
            Self::DuplicateWidget(target) => {
                let Some(widget) = target.and_then(|t| selected_widget(editor, t)) else {
                    return Some("Select a current unambiguous widget");
                };
                if editor.preview().config().widgets.len() >= MAX_WIDGETS {
                    Some("The 64-widget limit is reached")
                } else if free_placement(editor, widget).is_none() {
                    Some("No free space for this widget size in its view")
                } else {
                    None
                }
            }
            Self::DuplicateView(target) => {
                let Some(id) = target.and_then(|t| selected_view(editor, t)) else {
                    return Some("Select a current unambiguous view");
                };
                let config = editor.preview().config();
                if config.views.len() >= MAX_VIEWS {
                    Some("The 8-view limit is reached")
                } else if config.widgets.len() + config.widgets.iter().filter(|w| w.view == id).count() > MAX_WIDGETS {
                    Some("Duplicating this view exceeds the 64-widget limit")
                } else {
                    None
                }
            }
            _ => None,
        }
    }
}
/// Returns `editor`'s selected preview widget only if `target` resolves unambiguously to that widget/view; otherwise
/// None.
fn selected_widget(
    editor: &Editor,
    target: InspectionTarget,
) -> Option<&crate::config::Widget> {
    let resolved = editor.inspect(target).ok()?;
    if !resolved.id_lookup_unambiguous {
        return None;
    }
    let widget = editor.preview().config().widgets.get(resolved.widget_index?)?;
    (editor.selection().widget == Some(widget.id) && editor.selection().view == widget.view).then_some(widget)
}
/// Returns the selected view ID only if `target` resolves unambiguously in `editor` to that current view; otherwise
/// None.
fn selected_view(
    editor: &Editor,
    target: InspectionTarget,
) -> Option<u8> {
    let resolved = editor.inspect(target).ok()?;
    if !resolved.id_lookup_unambiguous {
        return None;
    }
    let id = editor.preview().config().views.get(resolved.view_index?)?.id;
    (editor.selection().view == id).then_some(id)
}
/// Returns the first free placement of `widget`'s span in its view within `editor`'s accepted preview, or None when
/// full/invalid.
fn free_placement(
    editor: &Editor,
    widget: &crate::config::Widget,
) -> Option<Placement> {
    crate::geometry::first_free_placement(editor.preview().config(), widget.view, widget.placement.span, None)
}
/// Dispatches `command` against `editor`/`workspace` using original `captured_stamp` and no appearance offer.
/// Returns its immediate/deferred effect, or admission/editor rejection; same state effects as
/// dispatch_with_appearance.
pub fn dispatch(
    editor: &mut Editor,
    workspace: &mut WorkspaceState,
    command: Command,
    captured_stamp: EditorStamp,
) -> Result<CommandEffect, CommandError> {
    dispatch_with_appearance(editor, workspace, command, captured_stamp, None)
}

/// Dispatches `command` against `editor` and browser `workspace`, rechecking `captured_stamp` and optional `appearance`
/// offer. Returns the effect or unavailable/editor error. Duplication/history/JSON may mutate editor; navigation/Find
/// only workspace. Appearance apply returns confirmation authority, export/recovery return adapter work, and no files
/// are written here. Admission errors do not mutate; editor errors follow reducer rules, including retained invalid
/// drafts.
pub fn dispatch_with_appearance(
    editor: &mut Editor,
    workspace: &mut WorkspaceState,
    command: Command,
    captured_stamp: EditorStamp,
    appearance: Option<AppearanceOffer>,
) -> Result<CommandEffect, CommandError> {
    if let Some(reason) = command.availability_with_appearance(editor, captured_stamp, appearance) {
        return Err(CommandError::Unavailable(reason));
    }
    let changed = |result: Result<_, EditorError>| result.map(|_| CommandEffect::Changed).map_err(CommandError::Editor);
    match command {
        Command::CopyAppearance => {
            let request = appearance
                .and_then(AppearanceOffer::copy_request)
                .ok_or(CommandError::Unavailable("Select a current clean widget to copy"))?;
            editor.check_appearance_request(request).map_err(CommandError::Editor)?;
            Ok(CommandEffect::CopiedAppearance(request.snapshot()))
        }
        Command::ApplyAppearance(scope) => {
            let request = appearance
                .and_then(|offer| offer.request(scope))
                .ok_or(CommandError::Unavailable("Choose an appearance source"))?;
            editor.check_appearance_request(request).map_err(CommandError::Editor)?;
            Ok(CommandEffect::PreviewAppearance(request))
        }
        Command::Navigate(w) => {
            workspace.activate(w);
            Ok(CommandEffect::Navigate(w))
        }
        Command::Find(target) => {
            let field = Registry::resolve(editor, target)
                .map_err(|_| CommandError::Unavailable("Field changed; refresh commands and results"))?;
            workspace.open(field.workspace, field.group);
            Ok(CommandEffect::Inspect(target))
        }
        Command::DuplicateWidget(Some(target)) => {
            let widget = selected_widget(editor, target)
                .ok_or(CommandError::Unavailable("Field changed; refresh commands and results"))?;
            let placement = free_placement(editor, widget).ok_or(CommandError::Unavailable(
                "No free space for this widget size in its view",
            ))?;
            let edit = Edit::DuplicateWidget {
                id: widget.id,
                view: widget.view,
                placement,
            };
            changed(editor.apply(edit))
        }
        Command::DuplicateView(Some(target)) => {
            let id = selected_view(editor, target)
                .ok_or(CommandError::Unavailable("Field changed; refresh commands and results"))?;
            changed(editor.apply(Edit::DuplicateView {
                id,
                name: "Copy".into(),
            }))
        }
        Command::Undo => changed(editor.undo()),
        Command::OpenJson => {
            let handle = editor.open_json().map_err(CommandError::Editor)?;
            workspace.open(Workspace::Diagnostics, GroupId::Files);
            Ok(CommandEffect::OpenJson(handle))
        }
        Command::Redo => changed(editor.redo()),
        Command::Export(format) => Ok(CommandEffect::Export(format)),
        Command::RecoverSimulator => Ok(CommandEffect::RecoverSimulator),
        _ => Err(CommandError::Unavailable("Command integration is unavailable")),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Global shortcut intent after browser adapter supplies effective focus/IME state.
pub enum ShortcutAction {
    /// Open command palette, including from editable controls.
    Palette,
    /// Undo accepted editor history outside text-editing controls.
    Undo,
    /// Redo accepted editor history outside text-editing controls.
    Redo,
}
/// Returns shortcut intent for `key` with platform `primary`, `shift` and `alt` modifiers, or None when not admitted.
/// Rejects `repeat`/`composing` events; `editable` effective ancestry blocks Z history shortcuts but permits primary-K
/// palette. The adapter supplies actual IME state and performs preventDefault only for an admitted intent.
#[allow(clippy::too_many_arguments)]
pub fn shortcut_action(
    key: &str,
    primary: bool,
    shift: bool,
    alt: bool,
    repeat: bool,
    composing: bool,
    editable: bool,
) -> Option<ShortcutAction> {
    if !primary || alt || repeat || composing {
        return None;
    }
    match key.to_ascii_lowercase().as_str() {
        "k" if !shift => Some(ShortcutAction::Palette),
        "z" if !editable => {
            Some(if shift {
                ShortcutAction::Redo
            } else {
                ShortcutAction::Undo
            })
        }
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Monotonic deferred-focus identity local to one FocusRequests owner.
pub struct FocusRequest(
    /// Nonzero issued counter; never wraps.
    u64,
);
#[derive(Default)]
/// Single outstanding deferred-focus request with permanent exhaustion refusal.
pub struct FocusRequests {
    /// Last issued value, initially zero and checked before increment.
    next: u64,
    /// Sole admitted focus callback token, or None after cancellation/exhaustion.
    active: Option<FocusRequest>,
}
impl FocusRequests {
    /// Retires the prior request and returns a fresh token, or None at counter exhaustion; never reuses authority.
    pub fn issue(&mut self) -> Option<FocusRequest> {
        self.cancel();
        self.next = self.next.checked_add(1)?;
        let token = FocusRequest(self.next);
        self.active = Some(token);
        Some(token)
    }

    /// Retires any active request without resetting the counter.
    pub fn cancel(&mut self) { self.active = None; }

    /// Returns whether `token` is this owner's sole active request; does not consume it.
    pub fn is_current(
        &self,
        token: FocusRequest,
    ) -> bool {
        self.active == Some(token)
    }
}
/// Maximum UTF-8 JSON bytes accepted/emitted for browser workspace preferences.
pub const MAX_PREFERENCES_BYTES: usize = 4096;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
/// Browser workspace identity, unrelated to selected firmware screen or dashboard view.
pub enum Workspace {
    #[default]
    /// Dashboard views, widgets and appearance editing; default workspace.
    Views,
    /// Device orientation, buttons, startup, RGB and connection configuration.
    Device,
    /// Data sources, history, fuel assumptions and warning rules.
    DataWarnings,
    /// JSON/files, simulation evidence and event diagnostics.
    Diagnostics,
    /// Explicit helper checks/build/inspection/flash controls.
    BuildFlash,
    /// Read-only ECU research/reference evidence.
    EcuReference,
}
impl Workspace {
    /// Stable visible tab order used by keyboard navigation.
    pub const ALL: [Self; 6] = [
        Self::Views,
        Self::Device,
        Self::DataWarnings,
        Self::Diagnostics,
        Self::BuildFlash,
        Self::EcuReference,
    ];

    /// Returns this workspace's index in ALL, always within 0..6.
    pub const fn index(self) -> usize {
        match self {
            Self::Views => 0,
            Self::Device => 1,
            Self::DataWarnings => 2,
            Self::Diagnostics => 3,
            Self::BuildFlash => 4,
            Self::EcuReference => 5,
        }
    }

    /// Returns the visible tab label.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Views => "Views",
            Self::Device => "Device",
            Self::DataWarnings => "Data & warnings",
            Self::Diagnostics => "Diagnostics",
            Self::BuildFlash => "Build & Flash",
            Self::EcuReference => "ECU Reference",
        }
    }

    /// Returns the stable DOM identifier suffix; distinct from serialized enum spelling.
    pub const fn id(self) -> &'static str {
        match self {
            Self::Views => "views",
            Self::Device => "device",
            Self::DataWarnings => "data-warnings",
            Self::Diagnostics => "diagnostics",
            Self::BuildFlash => "build-flash",
            Self::EcuReference => "ecu-reference",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
/// Persisted collapsible inspector group identity, shared by registry navigation and DOM anchors.
pub enum GroupId {
    /// BLE connection configuration and prototype limits.
    Ble,
    /// Widget size, placement and view membership.
    Layout,
    /// Numeric readout settings and units.
    Readout,
    /// Gauge shape, range and appearance.
    Gauge,
    /// Per-widget history and extrema display.
    HistoryStatistics,
    /// Per-widget warning thresholds/behavior.
    Warnings,
    /// Colour/font and portable appearance presets.
    Appearance,
    /// Button and encoder assignments.
    Bindings,
    /// RGB status policy.
    Indicator,
    /// Shared channel history duration.
    Channels,
    /// Qualified channel-source configuration.
    Sources,
    /// Fuel assumption and equivalent AFR reference.
    Fuel,
    /// Combined warning rules and ordering.
    CombinedWarnings,
    /// Timing/control simulation settings.
    Simulation,
    /// JSON import/export and project files.
    Files,
    /// Resource/performance observations with provenance.
    Measurements,
}
impl GroupId {
    /// Returns the stable DOM group-anchor suffix used by field navigation.
    pub const fn id(self) -> &'static str {
        match self {
            Self::Layout => "layout",
            Self::Readout => "readout",
            Self::Gauge => "gauge",
            Self::HistoryStatistics => "history-statistics",
            Self::Warnings => "warnings",
            Self::Appearance => "appearance",
            Self::Ble => "ble",
            Self::Bindings => "bindings",
            Self::Indicator => "indicator",
            Self::Channels => "channels",
            Self::Sources => "sources",
            Self::Fuel => "fuel",
            Self::CombinedWarnings => "combined-warnings",
            Self::Simulation => "simulation",
            Self::Files => "files",
            Self::Measurements => "measurements",
        }
    }

    /// Returns the visible collapsible-group heading.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Layout => "Layout",
            Self::Readout => "Readout",
            Self::Gauge => "Gauge",
            Self::HistoryStatistics => "History & statistics",
            Self::Warnings => "Warnings",
            Self::Appearance => "Appearance",
            Self::Ble => "Bluetooth connection",
            Self::Bindings => "Buttons & encoder",
            Self::Indicator => "RGB indicator",
            Self::Channels => "Channel history",
            Self::Sources => "Channel sources",
            Self::Fuel => "Fuel & AFR",
            Self::CombinedWarnings => "Combined warnings",
            Self::Simulation => "Timing simulation",
            Self::Files => "Configuration files",
            Self::Measurements => "Measurements",
        }
    }
}
#[derive(Clone, Copy, Debug)]
/// Manual tab-focus movement; changing focus does not activate the workspace.
pub enum TabKey {
    /// Move left, wrapping from first to last.
    Previous,
    /// Move right, wrapping from last to first.
    Next,
    /// Focus the first tab.
    First,
    /// Focus the last tab.
    Last,
}
#[derive(Clone, Debug, PartialEq, Eq)]
/// Browser-only active tab, roving focus and persistent expanded groups.
pub struct WorkspaceState {
    /// Workspace whose panel is visible.
    active: Workspace,
    /// Roving tab stop, possibly different from visible panel during arrow navigation.
    focused: Workspace,
    /// Expanded groups across all workspaces, stored without duplicates.
    expanded: BTreeSet<GroupId>,
}
impl Default for WorkspaceState {
    /// Returns Views active/focused with Layout, Readout, Bindings, BLE, Channels and Files expanded.
    fn default() -> Self {
        Self {
            active: Workspace::Views,
            focused: Workspace::Views,
            expanded: [
                GroupId::Layout,
                GroupId::Readout,
                GroupId::Bindings,
                GroupId::Ble,
                GroupId::Channels,
                GroupId::Files,
            ]
            .into_iter()
            .collect(),
        }
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// Strict versioned browser JSON preferences; contains no editor/project values.
struct Preferences {
    /// Only schema version one is accepted.
    version: u16,
    /// Workspace restored as both active and focused.
    active: Workspace,
    /// Serialized expanded groups; duplicate entries are rejected on restore.
    expanded: Vec<GroupId>,
}
impl WorkspaceState {
    /// Returns the visible workspace.
    pub fn active(&self) -> Workspace { self.active }

    /// Returns the roving tab-focus workspace, which need not be active.
    pub fn focused(&self) -> Workspace { self.focused }

    /// Returns whether `group` is currently expanded, regardless of active workspace.
    pub fn expanded(
        &self,
        group: GroupId,
    ) -> bool {
        self.expanded.contains(&group)
    }

    /// Moves roving focus according to `key`, wrapping adjacent navigation without changing the active panel.
    pub fn focus(
        &mut self,
        key: TabKey,
    ) {
        let index = self.focused.index();
        self.focused = Workspace::ALL[match key {
            TabKey::Previous => (index + Workspace::ALL.len() - 1) % Workspace::ALL.len(),
            TabKey::Next => (index + 1) % Workspace::ALL.len(),
            TabKey::First => 0,
            TabKey::Last => Workspace::ALL.len() - 1,
        }];
    }

    /// Activates the currently focused tab without changing expanded groups.
    pub fn activate_focused(&mut self) { self.active = self.focused; }

    /// Sets `workspace` active and focused, retaining expanded-group state.
    pub fn activate(
        &mut self,
        workspace: Workspace,
    ) {
        self.active = workspace;
        self.focused = workspace;
    }

    /// Activates/focuses `workspace` and expands `group`; no group/workspace compatibility inference is performed.
    pub fn open(
        &mut self,
        workspace: Workspace,
        group: GroupId,
    ) {
        self.activate(workspace);
        self.expanded.insert(group);
    }

    /// Sets `group` membership to `expanded` without changing active workspace or focus.
    pub fn set_expanded(
        &mut self,
        group: GroupId,
        expanded: bool,
    ) {
        if expanded {
            self.expanded.insert(group);
        } else {
            self.expanded.remove(&group);
        }
    }

    /// Returns version-one JSON for active workspace and sorted expanded groups, or encoding/4-KiB-bound error.
    /// Does not persist it and omits transient focused-tab state.
    pub fn preferences(&self) -> Result<String, &'static str> {
        let text = serde_json::to_string(&Preferences {
            version: 1,
            active: self.active,
            expanded: self.expanded.iter().copied().collect(),
        })
        .map_err(|_| "Workspace preferences could not be encoded")?;
        if text.len() > MAX_PREFERENCES_BYTES {
            return Err("Workspace preferences exceed 4 KiB");
        }
        Ok(text)
    }

    /// Returns restored preferences from optional `text`, plus a warning on invalid/oversize/version/duplicate input.
    /// None returns defaults without warning; invalid input returns defaults with warning and never partially restores.
    pub fn from_preferences(text: Option<&str>) -> (Self, Option<&'static str>) {
        let Some(text) = text else {
            return (Self::default(), None);
        };
        let fail = || (Self::default(), Some("Workspace preferences are invalid; using Views"));
        if text.len() > MAX_PREFERENCES_BYTES {
            return fail();
        }
        let Ok(pref) = serde_json::from_str::<Preferences>(text) else {
            return fail();
        };
        let expanded = pref.expanded.iter().copied().collect::<BTreeSet<_>>();
        if pref.version != 1 || expanded.len() != pref.expanded.len() {
            return fail();
        }
        (
            Self {
                active: pref.active,
                focused: pref.active,
                expanded,
            },
            None,
        )
    }
}

#[cfg(test)]
/// Deferred focus identity exhaustion must retire the previous callback without wrapping.
mod focus_tests {
    use super::*;
    #[test]
    /// Issuing at MAX returns None and invalidates the prior MAX token.
    fn exhausted_request_retires_previous_without_wrapping() {
        let old = FocusRequest(u64::MAX);
        let mut state = FocusRequests {
            next: u64::MAX,
            active: Some(old),
        };
        assert_eq!(state.issue(), None);
        assert!(!state.is_current(old));
    }
}
