use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};

use dashboard_config::*;
use serde::{Deserialize, Serialize};

pub const HISTORY_LIMIT: usize = 32;
pub const MAX_RAW_TEXT_BYTES: usize = 1024;
pub const MAX_RAW_FIELDS: usize = 64;
pub const SAVED_STATE_VERSION: u16 = 1;
pub const MAX_SAVED_BYTES: usize = MAX_JSON_BYTES;

// A replacement Editor must never accidentally accept an earlier instance's
// callbacks, even if both happen to have the same local revision/request.
static NEXT_EDITOR: AtomicUsize = AtomicUsize::new(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Revision(u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Selection {
    pub view: u8,
    pub widget: Option<u16>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FieldToken {
    owner: usize,
    revision: Revision,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointerTarget {
    Widget(u16),
    Library(Channel),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PointerToken {
    owner: usize,
    request: u64,
    revision: Revision,
    pointer_id: i32,
    target: PointerTarget,
    view: u8,
}

impl PointerToken {
    pub fn pointer_id(self) -> i32 { self.pointer_id }

    pub fn target(self) -> PointerTarget { self.target }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImportToken {
    owner: usize,
    request: u64,
    revision: Revision,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImportFormat {
    Json,
    Binary,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SaveToken {
    owner: usize,
    request: u64,
    revision: Revision,
}

pub struct SaveRequest {
    token: SaveToken,
    json: String,
}

impl SaveRequest {
    pub fn token(&self) -> SaveToken { self.token }

    pub fn json(&self) -> &str { &self.json }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StorageFailure {
    Denied,
    Quota,
    Unavailable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SaveStatus {
    Dirty,
    InvalidDraft,
    Saving,
    Saved,
    Failed(StorageFailure),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WidgetField {
    Label,
    Decimals,
    GraphMinimum,
    GraphMaximum,
    GaugeMinimum,
    GaugeMaximum,
    Color(ColorRole),
    WarningMessage,
    WarningTrigger,
    WarningClear,
    WarningLoad,
    WarningOn,
    WarningOff,
    SmoothingMilliseconds,
    PeakHoldMilliseconds,
    Reference,
    BandMinimum(u8),
    BandMaximum(u8),
    BandLabel(u8),
    BandColor(u8),
    WarningTriggerHigh,
    WarningClearHigh,
    WarningSnoozeSeconds,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LedField {
    Color,
    Brightness,
    Period,
    OnPercent,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FuelField {
    Ratio,
    Percent,
    GasolineRatio,
    EthanolRatio,
    GasolineDensity,
    EthanolDensity,
    ReferenceCelsius,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceField {
    Request,
    Response,
    PollMilliseconds,
    FreshnessMilliseconds,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RawField {
    HistorySeconds(Channel),
    Led { warning: bool, field: LedField },
    ControlHoldMilliseconds,
    ViewName(u8),
    Widget(u16, WidgetField),
    Fuel(FuelField),
    Source(Channel, SourceField),
    WarningPageMilliseconds,
    WarningColor { foreground: bool },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RawInput {
    pub field: RawField,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Edit {
    SetHistory {
        channel: Channel,
        seconds: u16,
    },
    SetReducedMotion(bool),
    SetLed(LedSettings),
    RemoveBand {
        id: u16,
        index: u8,
    },
    SetControls(ControlMap),
    AddView {
        name: String,
    },
    RenameView {
        id: u8,
        name: String,
    },
    DeleteView(u8),
    DuplicateView {
        id: u8,
        name: String,
    },
    ReorderView {
        id: u8,
        index: usize,
    },
    SetBootView(u8),
    AddWidget {
        view: u8,
        channel: Channel,
        placement: Placement,
    },
    DeleteWidget(u16),
    DuplicateWidget {
        id: u16,
        view: u8,
        placement: Placement,
    },
    MoveWidget {
        id: u16,
        view: u8,
        column: u8,
        row: u8,
    },
    ResizeWidget {
        id: u16,
        span: Span,
    },
    PlaceWidget {
        id: u16,
        placement: Placement,
    },
    ReorderWidget {
        id: u16,
        index: usize,
    },
    UpdateWidget(Widget),
    SetWarningMode {
        id: u16,
        mode: LimitMode,
    },
    SetSource {
        channel: Channel,
        source: ChannelSource,
    },
    SetFuel(Fuel),
    SetWarningPresentation(WarningPresentation),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditorError {
    Validation(ValidationError),
    Codec(ConfigError),
    MissingView(u8),
    MissingWidget(u16),
    LastView,
    ViewCapacity,
    WidgetCapacity,
    TextCapacity,
    InvalidIndex,
    SelectionMismatch,
    NoUndo,
    NoRedo,
    CounterExhausted,
    InvalidDraft,
    StaleToken,
    RawTooLong,
    RawCapacity,
    FieldUnavailable,
    RepairFieldFirst(RawField),
    Parse(RawField),
    SavedVersion,
    Storage(StorageFailure),
}

#[derive(Clone)]
struct Accepted {
    preview: ValidatedConfig,
    selection: Selection,
}

struct Rejected {
    config: Config,
    selection: Selection,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SavedEnvelope<C> {
    editor_version: u16,
    selected_view: u8,
    selected_widget: Option<u16>,
    config: C,
}

pub struct Editor {
    owner: usize,
    accepted: Box<Accepted>,
    revision: Revision,
    undo: VecDeque<Box<Accepted>>,
    redo: VecDeque<Box<Accepted>>,
    raw: Vec<RawInput>,
    rejected: Option<Box<Rejected>>,
    draft_error: Option<EditorError>,
    request: u64,
    pointer: Option<PointerToken>,
    import: Option<ImportToken>,
    save: Option<SaveToken>,
    saved_revision: Option<Revision>,
    save_failure: Option<StorageFailure>,
}

impl Default for Editor {
    fn default() -> Self { Self::new(default_config()).expect("reviewed default") }
}

impl Editor {
    pub fn new(config: Config) -> Result<Self, EditorError> {
        let preview = validate(config).map_err(EditorError::Validation)?;
        let selection = Selection {
            view: preview.config().boot_view,
            widget: None,
        };
        let owner = NEXT_EDITOR
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
            .map_err(|_| EditorError::CounterExhausted)?;
        Ok(Self {
            owner,
            accepted: Box::new(Accepted { preview, selection }),
            revision: Revision(0),
            undo: VecDeque::new(),
            redo: VecDeque::new(),
            raw: Vec::new(),
            rejected: None,
            draft_error: None,
            request: 0,
            pointer: None,
            import: None,
            save: None,
            saved_revision: None,
            save_failure: None,
        })
    }

    pub fn preview(&self) -> &ValidatedConfig { &self.accepted.preview }

    pub fn selection(&self) -> Selection {
        self.rejected
            .as_ref()
            .map_or(self.accepted.selection, |draft| draft.selection)
    }

    /// Editable geometry, including rejected structural changes. This is not a
    /// validation proof and must never be used for saving or firmware export.
    pub fn editing_config(&self) -> &Config {
        self.rejected
            .as_ref()
            .map_or(self.preview().config(), |draft| &draft.config)
    }

    pub fn can_edit_layout(&self) -> bool {
        if self.raw.is_empty() {
            return true;
        }
        let mut candidate = self.editing_config().clone();
        self.raw.iter().all(|input| apply_raw(&mut candidate, input).is_ok())
            && validate(candidate).is_err_and(|e| matches!(e.field, Field::Placement))
    }

    pub fn revision(&self) -> Revision { self.revision }

    pub fn history_len(&self) -> (usize, usize) { (self.undo.len(), self.redo.len()) }

    pub fn field_token(&self) -> FieldToken {
        FieldToken {
            owner: self.owner,
            revision: self.revision,
        }
    }

    pub fn raw_fields(&self) -> &[RawInput] { &self.raw }

    pub fn draft_error(&self) -> Option<EditorError> { self.draft_error }

    pub fn has_invalid_draft(&self) -> bool { !self.raw.is_empty() || self.rejected.is_some() }

    /// Inspection-only candidate. It never replaces the accepted document or
    /// enters serialization, acquisition, history, or firmware configuration.
    pub fn diagnostic(&self) -> Option<crate::diagnostic::Draft> {
        if !self.has_invalid_draft() {
            return None;
        }
        let mut config = self.editing_config().clone();
        for input in &self.raw {
            // Keep applying independent fields after a parse/capacity failure;
            // their original strings are retained and shown in diagnostics.
            let _ = apply_raw(&mut config, input);
        }
        Some(crate::diagnostic::Draft {
            config,
            raw: self.raw.clone(),
            error: self.draft_error.expect("invalid draft has a validation or parse error"),
        })
    }

    pub fn active_pointer(&self) -> Option<PointerToken> { self.pointer }

    pub fn import_is_current(
        &self,
        token: ImportToken,
    ) -> bool {
        self.import == Some(token) && token.revision == self.revision && !self.has_invalid_draft()
    }

    pub fn last_saved_revision(&self) -> Option<Revision> { self.saved_revision }

    /// Check immediately before a prepared write. Serialize browser writes;
    /// never interpret a cancelled timer as a successful storage completion.
    pub fn save_is_current(
        &self,
        token: SaveToken,
    ) -> bool {
        self.save == Some(token) && token.revision == self.revision && !self.has_invalid_draft()
    }

    pub fn save_status(&self) -> SaveStatus {
        if self.has_invalid_draft() {
            SaveStatus::InvalidDraft
        } else if self.save.is_some() {
            SaveStatus::Saving
        } else if let Some(failure) = self.save_failure {
            SaveStatus::Failed(failure)
        } else if self.saved_revision == Some(self.revision) {
            SaveStatus::Saved
        } else {
            SaveStatus::Dirty
        }
    }

    /// Return the exact versioned bytes to write. This does not claim that any
    /// storage operation succeeded; the adapter must acknowledge its result.
    pub fn begin_save(&mut self) -> Result<SaveRequest, EditorError> {
        self.ensure_clean()?;
        let request = self.next_request()?;
        let envelope = SavedEnvelope {
            editor_version: SAVED_STATE_VERSION,
            selected_view: self.selection().view,
            selected_widget: self.selection().widget,
            config: self.preview().config(),
        };
        let json = serde_json::to_string(&envelope).map_err(json_error)?;
        if json.len() > MAX_SAVED_BYTES {
            return Err(EditorError::Codec(ConfigError::Length));
        }
        self.request = request;
        let token = SaveToken {
            owner: self.owner,
            request,
            revision: self.revision,
        };
        self.save = Some(token);
        self.save_failure = None;
        Ok(SaveRequest { token, json })
    }

    /// `Ok(())` means these bytes were actually written. If an older write
    /// completed late, remember its revision without accepting its stale token
    /// as current. This prevents a newer document falsely remaining "Saved".
    pub fn acknowledge_save(
        &mut self,
        token: SaveToken,
        result: Result<(), StorageFailure>,
    ) -> Result<(), EditorError> {
        if self.save != Some(token) || token.revision != self.revision {
            if token.owner == self.owner && result.is_ok() {
                self.saved_revision = Some(token.revision);
                self.save_failure = None;
            }
            return Err(EditorError::StaleToken);
        }
        self.ensure_clean()?;
        self.save = None;
        match result {
            Ok(()) => {
                self.saved_revision = Some(token.revision);
                self.save_failure = None;
                Ok(())
            }
            Err(failure) => {
                self.save_failure = Some(failure);
                Err(EditorError::Storage(failure))
            }
        }
    }

    /// Restore at application initialization. Imported files use the tokened
    /// import methods so a delayed read cannot replace an already edited app.
    pub fn from_saved_state(bytes: &[u8]) -> Result<Self, EditorError> {
        if bytes.len() > MAX_SAVED_BYTES {
            return Err(EditorError::Codec(ConfigError::Length));
        }
        let envelope: SavedEnvelope<Box<serde_json::value::RawValue>> =
            serde_json::from_slice(bytes).map_err(json_error)?;
        if envelope.editor_version != SAVED_STATE_VERSION {
            return Err(EditorError::SavedVersion);
        }
        let selection = Selection {
            view: envelope.selected_view,
            widget: envelope.selected_widget,
        };
        let restored =
            dashboard_config::from_json_bytes(envelope.config.get().as_bytes()).map_err(EditorError::Codec)?;
        let mut editor = Self::new(restored.config().clone())?;
        check_selection(editor.preview().config(), selection)?;
        editor.accepted.selection = selection;
        editor.saved_revision = Some(editor.revision);
        Ok(editor)
    }

    pub fn begin_pointer(
        &mut self,
        pointer_id: i32,
        target: PointerTarget,
    ) -> Result<PointerToken, EditorError> {
        if matches!(target, PointerTarget::Library(_)) {
            self.ensure_clean()?;
        } else if !self.can_edit_layout() {
            return Err(EditorError::InvalidDraft);
        }
        let request = self.next_request()?;
        if let PointerTarget::Widget(id) = target {
            let config = self.editing_config();
            if config.widgets[widget_index(config, id)?].view != self.selection().view {
                return Err(EditorError::SelectionMismatch);
            }
            self.select(Selection {
                view: self.selection().view,
                widget: Some(id),
            })?;
        }
        self.request = request;
        let token = PointerToken {
            owner: self.owner,
            request,
            revision: self.revision,
            pointer_id,
            target,
            view: self.selection().view,
        };
        self.pointer = Some(token);
        Ok(token)
    }

    pub fn commit_pointer(
        &mut self,
        token: PointerToken,
        pointer_id: i32,
        placement: Placement,
    ) -> Result<Revision, EditorError> {
        if self.pointer != Some(token) || token.revision != self.revision || token.pointer_id != pointer_id {
            return Err(EditorError::StaleToken);
        }
        self.pointer = None;
        let edit = match token.target {
            PointerTarget::Widget(id) => Edit::PlaceWidget { id, placement },
            PointerTarget::Library(channel) => {
                Edit::AddWidget {
                    view: token.view,
                    channel,
                    placement,
                }
            }
        };
        self.apply(edit)
    }

    pub fn cancel_pointer(
        &mut self,
        token: PointerToken,
    ) -> Result<(), EditorError> {
        if self.pointer != Some(token) {
            return Err(EditorError::StaleToken);
        }
        self.pointer = None;
        Ok(())
    }

    pub fn begin_import(&mut self) -> Result<ImportToken, EditorError> {
        self.ensure_clean()?;
        self.request = self.next_request()?;
        let token = ImportToken {
            owner: self.owner,
            request: self.request,
            revision: self.revision,
        };
        self.import = Some(token);
        self.pointer = None;
        Ok(token)
    }

    /// Check the token before parsing. A failed import consumes its request but
    /// leaves the accepted document, history, selection, and revision intact.
    pub fn complete_import(
        &mut self,
        token: ImportToken,
        format: ImportFormat,
        bytes: &[u8],
    ) -> Result<Revision, EditorError> {
        if self.import != Some(token) || token.revision != self.revision {
            return Err(EditorError::StaleToken);
        }
        self.ensure_clean()?;
        self.import = None;
        let preview = match format {
            ImportFormat::Json => from_json_bytes(bytes),
            ImportFormat::Binary => decode(bytes),
        }
        .map_err(EditorError::Codec)?;
        let selection = Selection {
            view: preview.config().boot_view,
            widget: None,
        };
        let revision = self.next_revision()?;
        self.accept_validated(preview, selection)?;
        self.changed(revision);
        Ok(self.revision)
    }

    pub fn cancel_import(
        &mut self,
        token: ImportToken,
    ) -> Result<(), EditorError> {
        if self.import != Some(token) {
            return Err(EditorError::StaleToken);
        }
        self.import = None;
        Ok(())
    }

    /// A successful call accepts the raw keystroke, which can still leave an
    /// invalid draft. Render `diagnostic` with a prominent invalid label.
    /// Inputs beyond the explicit editor bound are rejected without accepting
    /// the keystroke; controls should display state read back from this reducer.
    pub fn set_raw(
        &mut self,
        token: FieldToken,
        field: RawField,
        text: String,
    ) -> Result<Revision, EditorError> {
        if token != self.field_token() {
            return Err(EditorError::StaleToken);
        }
        if text.len() > MAX_RAW_TEXT_BYTES {
            return Err(EditorError::RawTooLong);
        }
        // A retained raw field remains repairable after a mapping operation
        // creates a rejected candidate. It must still exist and apply to the
        // current candidate; the accepted document cannot prove that target.
        check_raw_target(self.editing_config(), self.selection(), field)?;
        let revision = self.next_revision()?;
        let mut fields = self.raw.clone();
        if let Some(input) = fields.iter_mut().find(|input| input.field == field) {
            input.text = text;
        } else {
            if fields.len() == MAX_RAW_FIELDS {
                return Err(EditorError::RawCapacity);
            }
            fields.push(RawInput { field, text });
        }
        let mut config = self.editing_config().clone();
        let result = fields
            .iter()
            .try_for_each(|input| apply_raw(&mut config, input))
            .and_then(|()| validate(config.clone()).map_err(EditorError::Validation));
        match result {
            Ok(preview) => {
                self.accept_validated(preview, self.selection())?;
                self.raw.clear();
                self.rejected = None;
                self.draft_error = None;
                // Correcting an invalid draft back to its original value is
                // still a new interaction and must retire earlier callbacks.
                self.changed(revision);
            }
            Err(error) => {
                if let Some(rejected) = &mut self.rejected {
                    rejected.config = config;
                }
                self.raw = fields;
                self.draft_error = Some(error);
                self.changed(revision);
            }
        }
        Ok(self.revision)
    }

    pub fn discard_raw(&mut self) -> Result<Revision, EditorError> {
        if self.has_invalid_draft() {
            let revision = self.next_revision()?;
            self.raw.clear();
            self.rejected = None;
            self.draft_error = None;
            self.changed(revision);
        }
        Ok(self.revision)
    }

    pub fn export_json(&self) -> Result<String, EditorError> {
        self.ensure_clean()?;
        to_json(self.preview()).map_err(EditorError::Codec)
    }

    pub fn export_binary(&self) -> Result<Vec<u8>, EditorError> {
        self.ensure_clean()?;
        let mut bytes = vec![0; MAX_FILE_BYTES];
        let len = encode(self.preview(), &mut bytes).map_err(EditorError::Codec)?.len();
        bytes.truncate(len);
        Ok(bytes)
    }

    pub fn apply(
        &mut self,
        edit: Edit,
    ) -> Result<Revision, EditorError> {
        let field_repair = matches!(
            edit,
            Edit::SetControls(_)
                | Edit::UpdateWidget(_)
                | Edit::SetWarningMode { .. }
                | Edit::SetHistory { .. }
                | Edit::SetReducedMotion(_)
                | Edit::SetLed(_)
                | Edit::RemoveBand { .. }
                | Edit::SetWarningPresentation(_)
                | Edit::SetFuel(_)
                | Edit::SetSource { .. }
        );
        if !field_repair
            && self.has_invalid_draft()
            && (!self.can_edit_layout()
                || !matches!(
                    edit,
                    Edit::MoveWidget { .. }
                        | Edit::ResizeWidget { .. }
                        | Edit::PlaceWidget { .. }
                        | Edit::DeleteWidget(_)
                ))
        {
            return Err(EditorError::InvalidDraft);
        }
        let recovering = self.has_invalid_draft();
        let mut config = self.editing_config().clone();
        let mut selection = self.selection();
        match edit {
            Edit::SetHistory { channel, seconds } => config.history_seconds[channel.index()] = seconds,
            Edit::SetReducedMotion(value) => config.reduced_motion = value,
            Edit::SetLed(value) => config.led = value,
            Edit::RemoveBand { id, index } => {
                let slot = widget_index(&config, id)?;
                let widget = &mut config.widgets[slot];
                let mut bands = widget.advanced.bands().to_vec();
                if usize::from(index) >= bands.len() {
                    return Err(EditorError::InvalidIndex);
                }
                bands.remove(usize::from(index));
                widget.advanced.set_bands(&bands).map_err(EditorError::Validation)?;
                // Removing a band deliberately removes its own draft fields;
                // following bands keep their identity and unrelated raw text.
                self.raw.retain_mut(|raw| {
                    let RawField::Widget(wid, field) = &mut raw.field else {
                        return true;
                    };
                    if *wid != id {
                        return true;
                    }
                    let band = match field {
                        WidgetField::BandMinimum(i)
                        | WidgetField::BandMaximum(i)
                        | WidgetField::BandLabel(i)
                        | WidgetField::BandColor(i) => i,
                        _ => return true,
                    };
                    if *band == index {
                        return false;
                    }
                    if *band > index {
                        *band -= 1;
                    }
                    true
                });
            }
            Edit::SetControls(mut controls) => {
                // The current hold text wins over a map captured by a picker.
                if self.raw.iter().any(|r| r.field == RawField::ControlHoldMilliseconds) {
                    controls.hold_ms = config.controls.hold_ms;
                }
                config.controls = controls;
            }
            Edit::AddView { name } => {
                let id = fresh_view(&config)?;
                config
                    .views
                    .push(View {
                        id,
                        name: bounded(&name)?,
                    })
                    .map_err(|_| EditorError::ViewCapacity)?;
                selection = Selection { view: id, widget: None };
            }
            Edit::RenameView { id, name } => {
                let index = view_index(&config, id)?;
                config.views[index].name = bounded(&name)?;
            }
            Edit::DeleteView(id) => {
                let index = view_index(&config, id)?;
                if config.views.len() == 1 {
                    return Err(EditorError::LastView);
                }
                config.views.remove(index);
                config.widgets.retain(|widget| widget.view != id);
                if config.boot_view == id {
                    config.boot_view = config.views[0].id;
                }
                if selection.view == id {
                    selection = Selection {
                        view: config.views[index % config.views.len()].id,
                        widget: None,
                    };
                }
            }
            Edit::DuplicateView { id, name } => {
                let index = view_index(&config, id)?;
                let new_id = fresh_view(&config)?;
                let copies: Vec<_> = config
                    .widgets
                    .iter()
                    .filter(|widget| widget.view == id)
                    .cloned()
                    .collect();
                if config.widgets.len() + copies.len() > MAX_WIDGETS {
                    return Err(EditorError::WidgetCapacity);
                }
                config
                    .views
                    .insert(
                        index + 1,
                        View {
                            id: new_id,
                            name: bounded(&name)?,
                        },
                    )
                    .map_err(|_| EditorError::ViewCapacity)?;
                for mut widget in copies {
                    widget.id = fresh_widget(&config)?;
                    widget.view = new_id;
                    config.widgets.push(widget).map_err(|_| EditorError::WidgetCapacity)?;
                }
                selection = Selection {
                    view: new_id,
                    widget: None,
                };
            }
            Edit::ReorderView { id, index } => {
                let old = view_index(&config, id)?;
                if index >= config.views.len() {
                    return Err(EditorError::InvalidIndex);
                }
                let view = config.views.remove(old);
                config
                    .views
                    .insert(index, view)
                    .map_err(|_| EditorError::ViewCapacity)?;
            }
            Edit::SetBootView(id) => config.boot_view = id,
            Edit::AddWidget {
                view,
                channel,
                placement,
            } => {
                let id = fresh_widget(&config)?;
                config
                    .widgets
                    .push(default_widget(channel, id, view, placement))
                    .map_err(|_| EditorError::WidgetCapacity)?;
                selection = Selection { view, widget: Some(id) };
            }
            Edit::DeleteWidget(id) => {
                let index = widget_index(&config, id)?;
                config.widgets.remove(index);
                if selection.widget == Some(id) {
                    selection.widget = None;
                }
            }
            Edit::DuplicateWidget { id, view, placement } => {
                let mut widget = config.widgets[widget_index(&config, id)?].clone();
                let new_id = fresh_widget(&config)?;
                widget.id = new_id;
                widget.view = view;
                widget.placement = placement;
                config.widgets.push(widget).map_err(|_| EditorError::WidgetCapacity)?;
                selection = Selection {
                    view,
                    widget: Some(new_id),
                };
            }
            Edit::MoveWidget { id, view, column, row } => {
                let index = widget_index(&config, id)?;
                let widget = &mut config.widgets[index];
                widget.view = view;
                widget.placement.column = column;
                widget.placement.row = row;
                selection = Selection { view, widget: Some(id) };
            }
            Edit::ResizeWidget { id, span } => {
                let index = widget_index(&config, id)?;
                config.widgets[index].placement.span = span;
            }
            Edit::PlaceWidget { id, placement } => {
                let index = widget_index(&config, id)?;
                config.widgets[index].placement = placement;
                selection = Selection {
                    view: config.widgets[index].view,
                    widget: Some(id),
                };
            }
            Edit::ReorderWidget { id, index } => {
                let old = widget_index(&config, id)?;
                if index >= config.widgets.len() {
                    return Err(EditorError::InvalidIndex);
                }
                let widget = config.widgets.remove(old);
                config
                    .widgets
                    .insert(index, widget)
                    .map_err(|_| EditorError::WidgetCapacity)?;
            }
            Edit::UpdateWidget(widget) => {
                let index = widget_index(&config, widget.id)?;
                if selection.widget == Some(widget.id) {
                    selection.view = widget.view;
                }
                config.widgets[index] = widget;
            }
            Edit::SetWarningMode { id, mode } => {
                let index = widget_index(&config, id)?;
                let (low, high) = config.widgets[index].channel.domain();
                let defaults = default_widget(
                    config.widgets[index].channel,
                    id,
                    config.widgets[index].view,
                    config.widgets[index].placement,
                )
                .warning;
                let warning = &mut config.widgets[index].warning;
                let window = |m| matches!(m, LimitMode::OutsideWindow | LimitMode::InsideWindow);
                if window(mode) && warning.mode != mode {
                    let step = (high - low) / 5.;
                    let endpoints = [low + step, low + 2. * step, low + 3. * step, low + 4. * step];
                    let [a, b, c, d] = endpoints;
                    (
                        warning.trigger,
                        warning.clear,
                        warning.advanced.clear_high,
                        warning.advanced.trigger_high,
                    ) = if mode == LimitMode::OutsideWindow {
                        (a, b, c, d)
                    } else {
                        (b, a, d, c)
                    };
                } else if window(warning.mode) && !window(mode) {
                    (warning.trigger, warning.clear) = if mode == defaults.mode {
                        (defaults.trigger, defaults.clear)
                    } else {
                        (defaults.clear, defaults.trigger)
                    };
                    warning.advanced.trigger_high = 0.;
                    warning.advanced.clear_high = 0.;
                }
                if matches!(
                    (warning.mode, mode),
                    (LimitMode::High, LimitMode::Low) | (LimitMode::Low, LimitMode::High)
                ) {
                    core::mem::swap(&mut warning.trigger, &mut warning.clear);
                }
                warning.mode = mode;
            }
            Edit::SetSource { channel, source } => config.sources[channel.index()] = source,
            Edit::SetFuel(fuel) => config.fuel = fuel,
            Edit::SetWarningPresentation(warnings) => config.warnings = warnings,
        }
        // A variant change cannot remove the parent of a retained raw field.
        // Reject before storing the candidate so its original input, token and
        // diagnostics remain addressable until the user repairs that field.
        for input in &self.raw {
            check_raw_available(&config, input.field).map_err(|_| EditorError::RepairFieldFirst(input.field))?;
        }
        let mut raw_error = None;
        for input in &self.raw {
            if let Err(error) = apply_raw(&mut config, input) {
                raw_error.get_or_insert(error);
            }
        }
        let result = raw_error.map_or_else(|| validate(config.clone()).map_err(EditorError::Validation), Err);
        match result {
            Ok(preview) => {
                let revision = if recovering { Some(self.next_revision()?) } else { None };
                self.accept_validated(preview, selection)?;
                self.rejected = None;
                self.raw.clear();
                self.draft_error = None;
                // Even a repair back to the original accepted bytes retires
                // callbacks created for the rejected layout.
                if let Some(revision) = revision {
                    self.changed(revision);
                }
                Ok(self.revision)
            }
            Err(error) => {
                let revision = self.next_revision()?;
                if check_selection(&config, selection).is_err() {
                    selection = self.accepted.selection;
                    if check_selection(&config, selection).is_err() {
                        selection = Selection {
                            view: config.views[0].id,
                            widget: None,
                        };
                    }
                }
                self.rejected = Some(Box::new(Rejected { config, selection }));
                self.draft_error = Some(error);
                self.changed(revision);
                Err(error)
            }
        }
    }

    /// Inspector callbacks capture a field token. Synchronous commands that
    /// deliberately operate on current state can use `apply` directly.
    pub fn apply_if_current(
        &mut self,
        token: FieldToken,
        edit: Edit,
    ) -> Result<Revision, EditorError> {
        if token != self.field_token() {
            return Err(EditorError::StaleToken);
        }
        self.apply(edit)
    }

    pub fn select(
        &mut self,
        selection: Selection,
    ) -> Result<Revision, EditorError> {
        if !self.can_edit_layout() {
            return Err(EditorError::InvalidDraft);
        }
        check_selection(self.editing_config(), selection)?;
        if selection != self.selection() {
            self.changed(self.next_revision()?);
            match &mut self.rejected {
                Some(draft) => draft.selection = selection,
                None => self.accepted.selection = selection,
            }
        }
        Ok(self.revision)
    }

    pub fn undo(&mut self) -> Result<Revision, EditorError> {
        self.ensure_clean()?;
        if self.undo.is_empty() {
            return Err(EditorError::NoUndo);
        }
        let revision = self.next_revision()?;
        let previous = self.undo.pop_back().ok_or(EditorError::NoUndo)?;
        let current = std::mem::replace(&mut self.accepted, previous);
        push_history(&mut self.redo, current);
        self.changed(revision);
        Ok(revision)
    }

    pub fn redo(&mut self) -> Result<Revision, EditorError> {
        self.ensure_clean()?;
        if self.redo.is_empty() {
            return Err(EditorError::NoRedo);
        }
        let revision = self.next_revision()?;
        let next = self.redo.pop_back().ok_or(EditorError::NoRedo)?;
        let current = std::mem::replace(&mut self.accepted, next);
        push_history(&mut self.undo, current);
        self.changed(revision);
        Ok(revision)
    }

    fn next_revision(&self) -> Result<Revision, EditorError> {
        self.revision
            .0
            .checked_add(1)
            .map(Revision)
            .ok_or(EditorError::CounterExhausted)
    }

    fn next_request(&self) -> Result<u64, EditorError> {
        self.request.checked_add(1).ok_or(EditorError::CounterExhausted)
    }

    fn accept_validated(
        &mut self,
        preview: ValidatedConfig,
        selection: Selection,
    ) -> Result<Revision, EditorError> {
        check_selection(preview.config(), selection)?;
        if preview == *self.preview() && selection == self.accepted.selection {
            // f32 equality treats negative zero as positive zero. Those have
            // different canonical payloads and can produce different labels;
            // avoid silently discarding them during an otherwise equal import.
            let mut before = vec![0; MAX_FILE_BYTES];
            let mut after = vec![0; MAX_FILE_BYTES];
            if encode(self.preview(), &mut before).map_err(EditorError::Codec)?
                == encode(&preview, &mut after).map_err(EditorError::Codec)?
            {
                return Ok(self.revision);
            }
        }
        let revision = self.next_revision()?;
        let current = std::mem::replace(&mut self.accepted, Box::new(Accepted { preview, selection }));
        push_history(&mut self.undo, current);
        self.redo.clear();
        self.changed(revision);
        Ok(revision)
    }

    fn changed(
        &mut self,
        revision: Revision,
    ) {
        self.revision = revision;
        self.pointer = None;
        self.import = None;
        self.save = None;
        self.save_failure = None;
    }

    fn ensure_clean(&self) -> Result<(), EditorError> {
        if self.has_invalid_draft() {
            Err(EditorError::InvalidDraft)
        } else {
            Ok(())
        }
    }
}

fn json_error(error: serde_json::Error) -> EditorError {
    EditorError::Codec(ConfigError::Json {
        line: error.line(),
        column: error.column(),
    })
}

fn check_raw_available(
    config: &Config,
    field: RawField,
) -> Result<(), EditorError> {
    // Availability is independent of current selection. Reuse the same target
    // checks as text entry without treating navigation as a variant change.
    let selection = match field {
        RawField::Widget(id, _) => {
            Selection {
                view: config.widgets[widget_index(config, id)?].view,
                widget: Some(id),
            }
        }
        RawField::ViewName(view) => Selection { view, widget: None },
        _ => {
            Selection {
                view: config.boot_view,
                widget: None,
            }
        }
    };
    check_raw_target(config, selection, field)
}

fn check_raw_target(
    config: &Config,
    selection: Selection,
    field: RawField,
) -> Result<(), EditorError> {
    match field {
        RawField::ViewName(id) => {
            view_index(config, id)?;
            if selection.view != id {
                return Err(EditorError::SelectionMismatch);
            }
        }
        RawField::Widget(id, field) => {
            let widget = &config.widgets[widget_index(config, id)?];
            if selection.widget != Some(id) || selection.view != widget.view {
                return Err(EditorError::SelectionMismatch);
            }
            if let WidgetField::BandMinimum(i)
            | WidgetField::BandMaximum(i)
            | WidgetField::BandLabel(i)
            | WidgetField::BandColor(i) = field
                && usize::from(i) >= widget.advanced.bands().len()
            {
                return Err(EditorError::FieldUnavailable);
            }
            if matches!(field, WidgetField::GraphMinimum | WidgetField::GraphMaximum)
                && !matches!(widget.display.graph_range, GraphRange::Fixed { .. })
            {
                return Err(EditorError::FieldUnavailable);
            }
        }
        RawField::Fuel(field) => {
            match (field, config.fuel) {
                (FuelField::Ratio, Fuel::Stoichiometric { .. }) => {}
                (
                    FuelField::ReferenceCelsius,
                    Fuel::EthanolBlend(Blend {
                        density_reference: DensityReference::CommonCelsius(_),
                        ..
                    }),
                ) => {}
                (
                    FuelField::Percent
                    | FuelField::GasolineRatio
                    | FuelField::EthanolRatio
                    | FuelField::GasolineDensity
                    | FuelField::EthanolDensity,
                    Fuel::EthanolBlend(_),
                ) => {}
                _ => return Err(EditorError::FieldUnavailable),
            }
        }
        RawField::Source(channel, field) => {
            let source = config.sources[channel.index()];
            match field {
                SourceField::Request | SourceField::Response if !matches!(source.route, Route::Physical11 { .. }) => {
                    return Err(EditorError::FieldUnavailable);
                }
                SourceField::PollMilliseconds if matches!(source.poll, Poll::Off) => {
                    return Err(EditorError::FieldUnavailable);
                }
                _ => {}
            }
        }
        RawField::ControlHoldMilliseconds
        | RawField::WarningPageMilliseconds
        | RawField::WarningColor { .. }
        | RawField::HistorySeconds(_)
        | RawField::Led { .. } => {}
    }
    Ok(())
}

fn apply_raw(
    config: &mut Config,
    input: &RawInput,
) -> Result<(), EditorError> {
    let RawInput { field, text } = input;
    let parse_error = || EditorError::Parse(*field);
    let number = || text.trim().parse::<f32>().map_err(|_| parse_error());
    let integer = || text.trim().parse::<u16>().map_err(|_| parse_error());
    match *field {
        RawField::HistorySeconds(channel) => config.history_seconds[channel.index()] = integer()?,
        RawField::Led { warning, field } => {
            let pattern = if warning {
                &mut config.led.warning
            } else {
                &mut config.led.normal
            };
            match field {
                LedField::Color => pattern.color = Color::parse(text).ok_or_else(parse_error)?,
                LedField::Brightness => pattern.brightness_percent = text.trim().parse().map_err(|_| parse_error())?,
                LedField::Period => pattern.period_ms = integer()?,
                LedField::OnPercent => pattern.on_percent = text.trim().parse().map_err(|_| parse_error())?,
            }
        }
        RawField::ControlHoldMilliseconds => config.controls.hold_ms = integer()?,
        RawField::ViewName(id) => {
            let index = view_index(config, id)?;
            config.views[index].name = bounded(text)?;
        }
        RawField::Widget(id, field) => {
            let index = widget_index(config, id)?;
            let widget = &mut config.widgets[index];
            match field {
                WidgetField::SmoothingMilliseconds => widget.advanced.smoothing_ms = integer()?,
                WidgetField::PeakHoldMilliseconds => widget.advanced.peak_hold_ms = integer()?,
                WidgetField::Reference => widget.advanced.reference = number()?,
                WidgetField::WarningTriggerHigh => widget.warning.advanced.trigger_high = number()?,
                WidgetField::WarningClearHigh => widget.warning.advanced.clear_high = number()?,
                WidgetField::WarningSnoozeSeconds => widget.warning.advanced.snooze_s = integer()?,
                WidgetField::BandMinimum(i)
                | WidgetField::BandMaximum(i)
                | WidgetField::BandColor(i)
                | WidgetField::BandLabel(i) => {
                    let mut bands = widget.advanced.bands().to_vec();
                    let band = bands.get_mut(usize::from(i)).ok_or(EditorError::FieldUnavailable)?;
                    match field {
                        WidgetField::BandMinimum(_) => band.min = number()?,
                        WidgetField::BandMaximum(_) => band.max = number()?,
                        WidgetField::BandColor(_) => band.color = Color::parse(text).ok_or_else(parse_error)?,
                        WidgetField::BandLabel(_) => {
                            band.set_label(text).map_err(|code| {
                                EditorError::Validation(ValidationError {
                                    code,
                                    field: Field::BandLabel(i),
                                    widget: Some(id),
                                    view: Some(widget.view),
                                    channel: Some(widget.channel),
                                })
                            })?
                        }
                        _ => unreachable!(),
                    }
                    widget.advanced.set_bands(&bands).map_err(EditorError::Validation)?;
                }
                WidgetField::Label => widget.label = bounded(text)?,
                WidgetField::Decimals => widget.display.decimals = text.trim().parse().map_err(|_| parse_error())?,
                WidgetField::Color(role) => role.set(&mut widget.colors, Color::parse(text).ok_or_else(parse_error)?),
                WidgetField::GaugeMinimum => widget.gauge.min = number()?,
                WidgetField::GaugeMaximum => widget.gauge.max = number()?,
                WidgetField::GraphMinimum | WidgetField::GraphMaximum => {
                    let GraphRange::Fixed { min, max } = &mut widget.display.graph_range else {
                        return Err(EditorError::FieldUnavailable);
                    };
                    if field == WidgetField::GraphMinimum {
                        *min = number()?;
                    } else {
                        *max = number()?;
                    }
                }
                WidgetField::WarningMessage => widget.warning.message = bounded(text)?,
                WidgetField::WarningTrigger => widget.warning.trigger = number()?,
                WidgetField::WarningClear => widget.warning.clear = number()?,
                WidgetField::WarningLoad => widget.warning.min_load_percent = number()?,
                WidgetField::WarningOn => widget.warning.on_ms = integer()?,
                WidgetField::WarningOff => widget.warning.off_ms = integer()?,
            }
        }
        RawField::Fuel(field) => {
            match &mut config.fuel {
                Fuel::Stoichiometric { ratio } if field == FuelField::Ratio => *ratio = number()?,
                Fuel::EthanolBlend(blend) => {
                    match field {
                        FuelField::Percent => blend.percent = number()?,
                        FuelField::GasolineRatio => blend.gasoline_stoich = number()?,
                        FuelField::EthanolRatio => blend.ethanol_stoich = number()?,
                        FuelField::GasolineDensity => blend.gasoline_density = number()?,
                        FuelField::EthanolDensity => blend.ethanol_density = number()?,
                        FuelField::ReferenceCelsius => {
                            blend.density_reference = DensityReference::CommonCelsius(number()?)
                        }
                        FuelField::Ratio => return Err(EditorError::FieldUnavailable),
                    }
                }
                _ => return Err(EditorError::FieldUnavailable),
            }
        }
        RawField::Source(channel, field) => {
            let source = &mut config.sources[channel.index()];
            match field {
                SourceField::Request | SourceField::Response => {
                    let Route::Physical11 { request, response } = &mut source.route else {
                        return Err(EditorError::FieldUnavailable);
                    };
                    let trimmed = text.trim();
                    let value = if let Some(hex) = trimmed.strip_prefix("0x").or_else(|| trimmed.strip_prefix("0X")) {
                        u16::from_str_radix(hex, 16).map_err(|_| parse_error())?
                    } else {
                        integer()?
                    };
                    if field == SourceField::Request {
                        *request = value;
                    } else {
                        *response = value;
                    }
                }
                SourceField::PollMilliseconds => source.poll = Poll::EveryMs(integer()?),
                SourceField::FreshnessMilliseconds => source.freshness_ms = integer()?,
            }
        }
        RawField::WarningPageMilliseconds => config.warnings.page_ms = integer()?,
        RawField::WarningColor { foreground } => {
            let color = Color::parse(text).ok_or_else(parse_error)?;
            if foreground {
                config.warning_colors.foreground = color;
            } else {
                config.warning_colors.background = color;
            }
        }
    }
    Ok(())
}

fn push_history(
    history: &mut VecDeque<Box<Accepted>>,
    value: Box<Accepted>,
) {
    if history.len() == HISTORY_LIMIT {
        history.pop_front();
    }
    history.push_back(value);
}

fn bounded<const N: usize>(text: &str) -> Result<heapless::String<N>, EditorError> {
    text.try_into().map_err(|_| EditorError::TextCapacity)
}

fn view_index(
    config: &Config,
    id: u8,
) -> Result<usize, EditorError> {
    config
        .views
        .iter()
        .position(|view| view.id == id)
        .ok_or(EditorError::MissingView(id))
}

fn widget_index(
    config: &Config,
    id: u16,
) -> Result<usize, EditorError> {
    config
        .widgets
        .iter()
        .position(|widget| widget.id == id)
        .ok_or(EditorError::MissingWidget(id))
}

fn check_selection(
    config: &Config,
    selection: Selection,
) -> Result<(), EditorError> {
    view_index(config, selection.view)?;
    if let Some(id) = selection.widget
        && config.widgets[widget_index(config, id)?].view != selection.view
    {
        return Err(EditorError::SelectionMismatch);
    }
    Ok(())
}

fn fresh_view(config: &Config) -> Result<u8, EditorError> {
    if config.views.len() == MAX_VIEWS {
        return Err(EditorError::ViewCapacity);
    }
    (1..=MAX_VIEWS as u8 + 1)
        .find(|id| config.views.iter().all(|view| view.id != *id))
        .ok_or(EditorError::ViewCapacity)
}

fn fresh_widget(config: &Config) -> Result<u16, EditorError> {
    if config.widgets.len() == MAX_WIDGETS {
        return Err(EditorError::WidgetCapacity);
    }
    (1..=MAX_WIDGETS as u16 + 1)
        .find(|id| config.widgets.iter().all(|widget| widget.id != *id))
        .ok_or(EditorError::WidgetCapacity)
}
