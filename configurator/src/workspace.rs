//! Browser preferences and pure manual workspace navigation, never firmware state.
use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

pub use crate::appearance_presets::AppearanceScope;
use crate::appearance_presets::{AppearanceOffer, AppearanceRequest, AppearanceSnapshot};
use crate::config::{MAX_VIEWS, MAX_WIDGETS, Placement};
use crate::editor::{ActionBlock, Edit, Editor, EditorError, EditorStamp, InspectionOwner, InspectionTarget};
use crate::field_registry::{FieldTarget, Registry};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportFormat {
    Json,
    Eav,
}
/// A closed action allowlist. Object authority is minted only by Editor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    Navigate(Workspace),
    Find(FieldTarget),
    DuplicateWidget(Option<InspectionTarget>),
    DuplicateView(Option<InspectionTarget>),
    CopyAppearance,
    ApplyAppearance(AppearanceScope),
    Undo,
    Redo,
    OpenJson,
    Export(ExportFormat),
    RecoverSimulator,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandEffect {
    OpenJson(crate::json_draft::JsonDraftHandle),
    CopiedAppearance(AppearanceSnapshot),
    PreviewAppearance(AppearanceRequest),
    Navigate(Workspace),
    Inspect(FieldTarget),
    Changed,
    Export(ExportFormat),
    RecoverSimulator,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandError {
    Unavailable(&'static str),
    Editor(EditorError),
}
impl CommandError {
    pub fn message(self) -> String {
        match self {
            Self::Unavailable(reason) => reason.into(),
            Self::Editor(error) => format!("Command rejected: {error:?}"),
        }
    }
}
impl Command {
    pub fn selected_widget(editor: &Editor) -> Self {
        Self::DuplicateWidget(editor.selection().widget.and_then(|id| {
            let index = editor.preview().config().widgets.iter().position(|w| w.id == id)?;
            editor.object_inspection_target(InspectionOwner::Widget { index }).ok()
        }))
    }

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

    /// Row state and dispatch use the identical synchronous admission policy.
    pub fn availability(
        self,
        editor: &Editor,
        captured_stamp: EditorStamp,
    ) -> Option<&'static str> {
        self.availability_with_appearance(editor, captured_stamp, None)
    }

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
        if let Some(block) = editor.action_readiness().block {
            return Some(match block {
                ActionBlock::InvalidDraft => "Repair or discard invalid edits first",
                ActionBlock::JsonPending => "Apply or Discard pending JSON first",
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
fn free_placement(
    editor: &Editor,
    widget: &crate::config::Widget,
) -> Option<Placement> {
    let occupied = editor
        .preview()
        .config()
        .widgets
        .iter()
        .filter(|w| w.view == widget.view)
        .fold(0u8, |mask, w| mask | w.placement.occupied().unwrap_or(u8::MAX));
    for row in 0..2 {
        for column in 0..4 {
            let p = Placement {
                row,
                column,
                span: widget.placement.span,
            };
            if p.occupied().is_some_and(|mask| mask & occupied == 0) {
                return Some(p);
            }
        }
    }
    None
}
pub fn dispatch(
    editor: &mut Editor,
    workspace: &mut WorkspaceState,
    command: Command,
    captured_stamp: EditorStamp,
) -> Result<CommandEffect, CommandError> {
    dispatch_with_appearance(editor, workspace, command, captured_stamp, None)
}

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
pub enum ShortcutAction {
    Palette,
    Undo,
    Redo,
}
/// The adapter supplies effective editable ancestry and actual IME state.
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
pub struct FocusRequest(u64);
#[derive(Default)]
pub struct FocusRequests {
    next: u64,
    active: Option<FocusRequest>,
}
impl FocusRequests {
    pub fn issue(&mut self) -> Option<FocusRequest> {
        self.cancel();
        self.next = self.next.checked_add(1)?;
        let token = FocusRequest(self.next);
        self.active = Some(token);
        Some(token)
    }

    pub fn cancel(&mut self) { self.active = None; }

    pub fn is_current(
        &self,
        token: FocusRequest,
    ) -> bool {
        self.active == Some(token)
    }
}
pub const MAX_PREFERENCES_BYTES: usize = 4096;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Workspace {
    #[default]
    Views,
    Device,
    DataWarnings,
    Diagnostics,
    BuildFlash,
}
impl Workspace {
    pub const ALL: [Self; 5] = [
        Self::Views,
        Self::Device,
        Self::DataWarnings,
        Self::Diagnostics,
        Self::BuildFlash,
    ];

    pub const fn index(self) -> usize {
        match self {
            Self::Views => 0,
            Self::Device => 1,
            Self::DataWarnings => 2,
            Self::Diagnostics => 3,
            Self::BuildFlash => 4,
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Views => "Views",
            Self::Device => "Device",
            Self::DataWarnings => "Data & warnings",
            Self::Diagnostics => "Diagnostics",
            Self::BuildFlash => "Build & Flash",
        }
    }

    pub const fn id(self) -> &'static str {
        match self {
            Self::Views => "views",
            Self::Device => "device",
            Self::DataWarnings => "data-warnings",
            Self::Diagnostics => "diagnostics",
            Self::BuildFlash => "build-flash",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum GroupId {
    Layout,
    Readout,
    Gauge,
    HistoryStatistics,
    Warnings,
    Appearance,
    Bindings,
    Indicator,
    Channels,
    Sources,
    Fuel,
    CombinedWarnings,
    Simulation,
    Files,
    Measurements,
}
impl GroupId {
    pub const fn id(self) -> &'static str {
        match self {
            Self::Layout => "layout",
            Self::Readout => "readout",
            Self::Gauge => "gauge",
            Self::HistoryStatistics => "history-statistics",
            Self::Warnings => "warnings",
            Self::Appearance => "appearance",
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

    pub const fn label(self) -> &'static str {
        match self {
            Self::Layout => "Layout",
            Self::Readout => "Readout",
            Self::Gauge => "Gauge",
            Self::HistoryStatistics => "History & statistics",
            Self::Warnings => "Warnings",
            Self::Appearance => "Appearance",
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
pub enum TabKey {
    Previous,
    Next,
    First,
    Last,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceState {
    active: Workspace,
    focused: Workspace,
    expanded: BTreeSet<GroupId>,
}
impl Default for WorkspaceState {
    fn default() -> Self {
        Self {
            active: Workspace::Views,
            focused: Workspace::Views,
            expanded: [
                GroupId::Layout,
                GroupId::Readout,
                GroupId::Bindings,
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
struct Preferences {
    version: u16,
    active: Workspace,
    expanded: Vec<GroupId>,
}
impl WorkspaceState {
    pub fn active(&self) -> Workspace { self.active }

    pub fn focused(&self) -> Workspace { self.focused }

    pub fn expanded(
        &self,
        group: GroupId,
    ) -> bool {
        self.expanded.contains(&group)
    }

    pub fn focus(
        &mut self,
        key: TabKey,
    ) {
        let index = self.focused.index();
        self.focused = Workspace::ALL[match key {
            TabKey::Previous => (index + 4) % 5,
            TabKey::Next => (index + 1) % 5,
            TabKey::First => 0,
            TabKey::Last => 4,
        }];
    }

    pub fn activate_focused(&mut self) { self.active = self.focused; }

    pub fn activate(
        &mut self,
        workspace: Workspace,
    ) {
        self.active = workspace;
        self.focused = workspace;
    }

    pub fn open(
        &mut self,
        workspace: Workspace,
        group: GroupId,
    ) {
        self.activate(workspace);
        self.expanded.insert(group);
    }

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
mod focus_tests {
    use super::*;
    #[test]
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
