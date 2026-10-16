use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use dashboard_config::startup::StartupProfile;
use dashboard_config::*;
use dashboard_config_tools::project::{Artwork, ExportMode, Project, ProjectError};
use serde::{Deserialize, Serialize};

use crate::appearance_presets::{
    AppearancePreview,
    AppearanceRequest,
    AppearanceScope,
    AppearanceSnapshot,
    apply_snapshot,
};
use crate::json_draft::{JsonDraft, JsonState};
pub use crate::json_draft::{JsonDraftHandle, JsonParseToken};

mod raw;
pub(crate) use raw::apply_raw;
use raw::{check_raw_available, check_raw_target};

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

impl Revision {
    pub const fn get(self) -> u64 { self.0 }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Owner and revision cannot be supplied by a caller.
/// ```compile_fail
/// let mut stamp = dashboard_configurator::editor::Editor::default().stamp();
/// stamp.owner = 1;
/// ```
pub struct EditorStamp {
    owner: usize,
    revision: Revision,
}

/// Acquisition synchronization key. Its revision changes only when accepted
/// canonical configuration changes; it is never restored from undo history.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AcceptedConfigKey {
    owner: usize,
    revision: Revision,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionBlock {
    InvalidDraft,
    JsonPending,
    StartupPending,
    PointerActive,
    ImportPending,
    IdentityExhausted,
}

impl From<ActionBlock> for EditorError {
    fn from(block: ActionBlock) -> Self {
        match block {
            ActionBlock::InvalidDraft => Self::InvalidDraft,
            ActionBlock::JsonPending => Self::JsonPending,
            ActionBlock::StartupPending => Self::StartupPending,
            ActionBlock::PointerActive => Self::PointerActive,
            ActionBlock::ImportPending => Self::ImportPending,
            ActionBlock::IdentityExhausted => Self::CounterExhausted,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActionReadiness {
    pub stamp: EditorStamp,
    pub block: Option<ActionBlock>,
}

/// A navigation capability for the exact current candidate. IDs are hints,
/// never the owner of an indexed semantic inspection target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InspectionTarget {
    stamp: EditorStamp,
    candidate_epoch: Option<(u64, u64, bool)>,
    key: InspectionKey,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InspectionKey {
    Semantic(ValidationLocation),
    Raw { index: usize, field: RawField },
    Object(InspectionOwner),
}

/// Requested browser inspection coordinates; only Editor can mint authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InspectionOwner {
    Root,
    View { index: usize },
    Widget { index: usize },
    Channel(Channel),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedInspection {
    pub view_index: Option<usize>,
    pub widget_index: Option<usize>,
    pub channel_index: Option<usize>,
    pub field: Option<Field>,
    pub raw_field: Option<RawField>,
    pub raw_index: Option<usize>,
    /// False for duplicate/missing widget or view ID owners. Even when true,
    /// mutation must use normal reducer admission and field capability checks.
    pub id_lookup_unambiguous: bool,
    pub json_path: Option<String>,
}

impl ResolvedInspection {
    pub fn display_location(&self) -> String {
        let mut parts = Vec::new();
        if let Some(path) = self.json_path.as_deref().filter(|path| !path.is_empty()) {
            parts.push(format!("JSON path {path}"));
        }
        if let Some(index) = self.view_index {
            parts.push(format!("view index {index}"));
        }
        if let Some(index) = self.widget_index {
            parts.push(format!("widget index {index}"));
        }
        if let Some(index) = self.channel_index {
            parts.push(format!("channel index {index}"));
        }
        if let Some(index) = self.raw_index {
            parts.push(format!("raw input {index}"));
        }
        if parts.is_empty() {
            "Configuration".into()
        } else {
            parts.join(" · ")
        }
    }
}

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
    Project,
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
    project: Project,
}

impl SaveRequest {
    pub fn token(&self) -> SaveToken { self.token }

    pub fn json(&self) -> &str { &self.json }

    pub fn project(&self) -> &Project { &self.project }
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
    SetOrientation(Orientation),
    SetViewOrientation {
        id: u8,
        orientation: Option<Orientation>,
    },
    ApplyAppearance(AppearanceRequest),
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
    Project(ProjectError),
    Validation(ValidationError),
    Codec(ConfigError),
    MissingView(u8),
    MissingWidget(u16),
    LastView,
    ViewCapacity,
    WidgetCapacity,
    TextCapacity,
    InvalidIndex,
    LayoutCannotFit(u8),
    SelectionMismatch,
    NoUndo,
    NoRedo,
    CounterExhausted,
    InvalidDraft,
    JsonPending,
    StartupPending,
    JsonTooLong,
    PointerActive,
    ImportPending,
    AmbiguousTarget,
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
    startup: StartupProfile,
    artwork: Option<Arc<Artwork>>,
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
    #[serde(default)]
    startup: StartupProfile,
    #[serde(default)]
    artwork: Option<[u8; 32]>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StartupNumber {
    StatusMilliseconds,
    SplashMilliseconds,
}
#[derive(Clone)]
struct StartupTimingDraft {
    status: String,
    splash: String,
    oversized: [bool; 2],
}
impl StartupTimingDraft {
    fn profile(
        &self,
        mut profile: StartupProfile,
    ) -> Result<StartupProfile, EditorError> {
        if self.oversized.iter().any(|b| *b) {
            return Err(EditorError::RawTooLong);
        }
        profile.status_ms = self
            .status
            .parse()
            .map_err(|_| EditorError::Project(ProjectError::Profile))?;
        profile.duration_ms = self
            .splash
            .parse()
            .map_err(|_| EditorError::Project(ProjectError::Profile))?;
        if !profile.valid() {
            return Err(EditorError::Project(ProjectError::Profile));
        }
        Ok(profile)
    }
}

pub struct Editor {
    owner: usize,
    accepted: Box<Accepted>,
    revision: Revision,
    accepted_revision: Revision,
    accepted_config_revision: Revision,
    startup_timing: Option<StartupTimingDraft>,
    json: Option<JsonDraft>,
    json_epoch: u64,
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
            accepted: Box::new(Accepted {
                preview,
                selection,
                startup: StartupProfile::default(),
                artwork: None,
            }),
            revision: Revision(0),
            accepted_revision: Revision(0),
            accepted_config_revision: Revision(0),
            startup_timing: None,
            json: None,
            json_epoch: 0,
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

    pub fn startup_profile(&self) -> StartupProfile { self.accepted.startup }

    pub fn has_startup_draft(&self) -> bool { self.startup_timing.is_some() }

    pub fn startup_timing_text(
        &self,
        field: StartupNumber,
    ) -> String {
        match (&self.startup_timing, field) {
            (Some(d), StartupNumber::StatusMilliseconds) => d.status.clone(),
            (Some(d), StartupNumber::SplashMilliseconds) => d.splash.clone(),
            (None, StartupNumber::StatusMilliseconds) => self.accepted.startup.status_ms.to_string(),
            (None, StartupNumber::SplashMilliseconds) => self.accepted.startup.duration_ms.to_string(),
        }
    }

    pub fn startup_draft_valid(&self) -> bool {
        self.startup_timing
            .as_ref()
            .is_some_and(|d| d.profile(self.accepted.startup).is_ok())
    }

    pub fn startup_draft_label(&self) -> Option<&'static str> {
        self.startup_timing.as_ref().map(|_| {
            if self.startup_draft_valid() {
                "UNAPPLIED STARTUP TIMING · showing accepted preview"
            } else {
                "INVALID STARTUP PREVIEW · showing last accepted settings"
            }
        })
    }

    /// Own raw text without publishing it; revision changes retire pending imports and writes.
    pub fn set_startup_number(
        &mut self,
        stamp: EditorStamp,
        field: StartupNumber,
        text: &str,
    ) -> Result<Revision, EditorError> {
        if stamp != self.stamp() {
            return Err(EditorError::StaleToken);
        }
        if self.has_pending_json() {
            return Err(EditorError::JsonPending);
        }
        if self.has_invalid_draft() {
            return Err(EditorError::InvalidDraft);
        }
        let revision = self.next_revision()?;
        let mut draft = self.startup_timing.clone().unwrap_or_else(|| {
            StartupTimingDraft {
                status: self.accepted.startup.status_ms.to_string(),
                splash: self.accepted.startup.duration_ms.to_string(),
                oversized: [false; 2],
            }
        });
        let index = if field == StartupNumber::StatusMilliseconds {
            0
        } else {
            1
        };
        draft.oversized[index] = text.len() > 32;
        let retained = text.chars().take(32).collect();
        if index == 0 {
            draft.status = retained;
        } else {
            draft.splash = retained;
        }
        self.changed(revision);
        self.startup_timing = Some(draft);
        Ok(revision)
    }

    /// Validate both numbers with the shared profile validator and publish one history entry.
    pub fn apply_startup_timing(
        &mut self,
        stamp: EditorStamp,
    ) -> Result<Revision, EditorError> {
        if stamp != self.stamp() {
            return Err(EditorError::StaleToken);
        }
        let draft = self.startup_timing.clone().ok_or(EditorError::StartupPending)?;
        let profile = draft.profile(self.accepted.startup)?;
        let next = self.next_revision()?;
        self.startup_timing = None;
        match self.set_startup_profile(profile) {
            Ok(revision) => {
                if revision != next {
                    self.changed(next);
                }
                Ok(self.revision)
            }
            Err(e) => {
                self.startup_timing = Some(draft);
                Err(e)
            }
        }
    }

    pub fn discard_startup_timing(
        &mut self,
        stamp: EditorStamp,
    ) -> Result<Revision, EditorError> {
        if stamp != self.stamp() {
            return Err(EditorError::StaleToken);
        }
        if self.startup_timing.is_some() {
            self.changed(self.next_revision()?);
            self.startup_timing = None;
        }
        Ok(self.revision)
    }

    pub fn startup_artwork(&self) -> Option<&Arc<Artwork>> { self.accepted.artwork.as_ref() }

    /// Retain source and converted bytes while an orientation edit awaits reconversion.
    pub fn artwork_geometry_mismatch(&self) -> bool {
        self.accepted.artwork.as_ref().is_some_and(|art| {
            dashboard_config::startup_asset::Asset::parse(art.converted()).map_or(true, |asset| {
                asset.geometry()
                    != self
                        .accepted
                        .startup
                        .effective_splash_orientation(self.preview().config().orientation)
                        .geometry()
            })
        })
    }

    fn ensure_project_edit_ready(&self) -> Result<(), EditorError> {
        // Artwork mismatch blocks snapshots, while conversion and orientation repair remain available.
        self.ensure_clean()?;
        if self.pointer.is_some() {
            return Err(EditorError::PointerActive);
        }
        if self.import.is_some() {
            return Err(EditorError::ImportPending);
        }
        Ok(())
    }

    /// A cheap immutable snapshot; artwork byte allocations are shared with history.
    pub fn accepted_project(&self) -> Result<Project, EditorError> {
        self.ensure_snapshot_ready()?;
        Project::new(self.preview(), self.accepted.startup, self.accepted.artwork.clone()).map_err(EditorError::Project)
    }

    pub fn export_project(
        &self,
        mode: ExportMode,
    ) -> Result<Vec<u8>, EditorError> {
        self.accepted_project()?.encode(mode).map_err(EditorError::Project)
    }

    pub fn set_startup_profile(
        &mut self,
        profile: StartupProfile,
    ) -> Result<Revision, EditorError> {
        if profile.status_orientation != self.accepted.startup.status_orientation
            || profile.splash_orientation != self.accepted.startup.splash_orientation
        {
            // Logical shape changes retire pending import/worker capabilities at commit.
            self.ensure_clean()?;
        } else {
            self.ensure_project_edit_ready()?;
        }
        let artwork = if profile.theme == dashboard_config::startup::StartupTheme::Custom {
            self.accepted.artwork.clone()
        } else {
            None
        };
        match Project::new(self.preview(), profile, artwork.clone()) {
            Ok(_) | Err(ProjectError::Orientation) => {}
            Err(error) => return Err(EditorError::Project(error)),
        }
        self.accept_project(self.preview().clone(), self.accepted.selection, profile, artwork)
    }

    pub fn set_artwork_if_current(
        &mut self,
        stamp: EditorStamp,
        artwork: Arc<Artwork>,
    ) -> Result<Revision, EditorError> {
        if stamp != self.stamp() {
            return Err(EditorError::StaleToken);
        }
        self.ensure_project_edit_ready()?;
        let mut profile = self.accepted.startup;
        profile.theme = dashboard_config::startup::StartupTheme::Custom;
        Project::new(self.preview(), profile, Some(artwork.clone())).map_err(EditorError::Project)?;
        self.accept_project(self.preview().clone(), self.accepted.selection, profile, Some(artwork))
    }

    /// Finish an admitted artwork import before allowing snapshot actions again.
    pub fn complete_artwork_import(
        &mut self,
        token: ImportToken,
        artwork: Arc<Artwork>,
    ) -> Result<Revision, EditorError> {
        if !self.import_is_current(token) {
            return Err(EditorError::StaleToken);
        }
        self.ensure_clean()?;
        self.import = None;
        self.set_artwork_if_current(self.stamp(), artwork)
    }

    fn accept_project(
        &mut self,
        preview: ValidatedConfig,
        selection: Selection,
        startup: StartupProfile,
        artwork: Option<Arc<Artwork>>,
    ) -> Result<Revision, EditorError> {
        let same_config = same_canonical_config(self.preview(), &preview)?;
        let same_artwork = match (&self.accepted.artwork, &artwork) {
            (None, None) => true,
            (Some(a), Some(b)) => Arc::ptr_eq(a, b) || a.recipe() == b.recipe(),
            _ => false,
        };
        if same_config && selection == self.accepted.selection && startup == self.accepted.startup && same_artwork {
            return Ok(self.revision);
        }
        let revision = self.next_revision()?;
        let current = std::mem::replace(
            &mut self.accepted,
            Box::new(Accepted {
                preview,
                selection,
                startup,
                artwork,
            }),
        );
        push_history(&mut self.undo, current);
        self.redo.clear();
        self.changed(revision);
        self.accepted_revision = revision;
        if !same_config {
            self.accepted_config_revision = revision;
        }
        Ok(revision)
    }

    pub fn complete_project_import(
        &mut self,
        token: ImportToken,
        bytes: &[u8],
    ) -> Result<Revision, EditorError> {
        if !self.import_is_current(token) {
            return Err(EditorError::StaleToken);
        }
        self.ensure_clean()?;
        self.import = None;
        let project = Project::decode(bytes).map_err(EditorError::Project)?;
        self.accept_imported_project(project)
    }

    /// Browser backing preparation finishes before this final token-checked commit.
    pub fn complete_prepared_project_import(
        &mut self,
        token: ImportToken,
        project: Project,
    ) -> Result<Revision, EditorError> {
        if !self.import_is_current(token) {
            return Err(EditorError::StaleToken);
        }
        self.ensure_clean()?;
        self.import = None;
        self.accept_imported_project(project)
    }

    fn accept_imported_project(
        &mut self,
        project: Project,
    ) -> Result<Revision, EditorError> {
        let preview = decode(project.dashboard_bytes()).map_err(EditorError::Codec)?;
        let selection = Selection {
            view: preview.config().boot_view,
            widget: None,
        };
        self.accept_project(preview, selection, project.profile(), project.artwork().cloned())
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

    pub fn stamp(&self) -> EditorStamp {
        EditorStamp {
            owner: self.owner,
            revision: self.revision,
        }
    }

    /// Revision of last accepted config/selection publication. Raw/rejected
    /// activity advances ordinary revision without publishing accepted state.
    pub fn accepted_revision(&self) -> Revision { self.accepted_revision }

    pub fn accepted_config_key(&self) -> AcceptedConfigKey {
        AcceptedConfigKey {
            owner: self.owner,
            revision: self.accepted_config_revision,
        }
    }

    pub fn has_pending_json(&self) -> bool { self.json.as_ref().is_some_and(|draft| draft.pending) }

    pub fn action_readiness(&self) -> ActionReadiness {
        let block = self.snapshot_state_block().or_else(|| {
            (self.revision.0 == u64::MAX || self.request == u64::MAX).then_some(ActionBlock::IdentityExhausted)
        });
        ActionReadiness {
            stamp: self.stamp(),
            block,
        }
    }

    /// Data/lifecycle admission shared by new snapshots and already-issued
    /// writes. Exhausted issuance cannot invalidate an existing save token.
    fn snapshot_state_block(&self) -> Option<ActionBlock> {
        if self.has_pending_json() {
            Some(ActionBlock::JsonPending)
        } else if self.has_startup_draft() {
            Some(ActionBlock::StartupPending)
        } else if self.has_invalid_draft() || self.artwork_geometry_mismatch() {
            Some(ActionBlock::InvalidDraft)
        } else if self.pointer.is_some() {
            Some(ActionBlock::PointerActive)
        } else if self.import.is_some() {
            Some(ActionBlock::ImportPending)
        } else {
            None
        }
    }

    pub fn json_draft(&self) -> Option<&JsonDraft> { self.json.as_ref() }

    pub fn open_json(&mut self) -> Result<JsonDraftHandle, EditorError> {
        if self.has_startup_draft() {
            return Err(EditorError::StartupPending);
        }
        if let Some(draft) = &self.json {
            return Ok(draft.handle);
        }
        let epoch = self.json_epoch.checked_add(1).ok_or(EditorError::CounterExhausted)?;
        let handle = JsonDraftHandle {
            stamp: self.stamp(),
            accepted_revision: self.accepted_revision,
            epoch,
        };
        let diagnostic = self.diagnostic();
        let (text, state, read_only) = if let Some(diagnostic) = diagnostic {
            let text = serde_json::to_string(&diagnostic.config).map_err(json_error)?;
            let report = diagnostic.validation_report();
            (
                text,
                JsonState::Candidate {
                    config: Box::new(diagnostic.config),
                    report,
                },
                true,
            )
        } else {
            (
                to_json(self.preview()).map_err(EditorError::Codec)?,
                JsonState::Unparsed,
                false,
            )
        };
        if text.len() > MAX_JSON_BYTES {
            return Err(EditorError::JsonTooLong);
        }
        self.json = Some(JsonDraft {
            handle,
            text,
            text_epoch: 0,
            pending: false,
            read_only,
            state,
        });
        self.json_epoch = epoch;
        Ok(handle)
    }

    pub fn set_json_text(
        &mut self,
        handle: JsonDraftHandle,
        text: String,
    ) -> Result<JsonParseToken, EditorError> {
        self.check_json_handle(handle)?;
        self.next_revision()?;
        let token = self.json.as_mut().ok_or(EditorError::StaleToken)?.replace_text(text)?;
        // JSON typing does not publish an editor revision, but older I/O must
        // never revive after Discard at that same revision.
        self.import = None;
        self.save = None;
        self.pointer = None;
        Ok(token)
    }

    /// Debounce completion parses the actual owned text, never caller-supplied
    /// Config/result data. Check every identity component before doing work.
    pub fn finish_json_parse(
        &mut self,
        token: JsonParseToken,
    ) -> Result<(), EditorError> {
        self.check_json_handle(token.handle)?;
        let draft = self.json.as_mut().ok_or(EditorError::StaleToken)?;
        if draft.text_epoch != token.text_epoch || !draft.pending || draft.read_only {
            return Err(EditorError::StaleToken);
        }
        draft.parse();
        Ok(())
    }

    pub fn apply_json(&mut self) -> Result<Revision, EditorError> {
        let draft = self.json.as_ref().ok_or(EditorError::StaleToken)?;
        self.check_json_handle(draft.handle)?;
        if draft.read_only || self.has_invalid_draft() {
            return Err(EditorError::InvalidDraft);
        }
        if !draft.pending {
            return Ok(self.revision);
        }
        let config = parse_current_json_candidate(draft.text.as_bytes()).map_err(EditorError::Codec)?;
        let preview = validate(config).map_err(EditorError::Validation)?;
        let selection = if check_selection(preview.config(), self.accepted.selection).is_ok() {
            self.accepted.selection
        } else {
            Selection {
                view: preview.config().boot_view,
                widget: None,
            }
        };
        let changed_config = !same_canonical_config(self.preview(), &preview)?;
        let revision = self.next_revision()?;
        let next = Box::new(Accepted {
            preview,
            selection,
            startup: self.accepted.startup,
            artwork: self.accepted.artwork.clone(),
        });
        let current = std::mem::replace(&mut self.accepted, next);
        push_history(&mut self.undo, current);
        self.redo.clear();
        if changed_config {
            self.accepted_config_revision = revision;
        }
        self.changed(revision);
        self.accepted_revision = revision;
        Ok(revision)
    }

    pub fn discard_json(&mut self) -> Result<(), EditorError> {
        self.json = None;
        Ok(())
    }

    pub fn json_preview_label(&self) -> Option<&'static str> {
        let draft = self.json.as_ref()?;
        match &draft.state {
            JsonState::SyntaxError { .. } => Some("INVALID JSON"),
            JsonState::Candidate { report, .. }
                if draft.read_only
                    || !report.issues.is_empty()
                    || !report.raw_errors.is_empty()
                    || report.omitted != 0 =>
            {
                Some("INVALID PREVIEW")
            }
            JsonState::Candidate { .. } if draft.pending => Some("UNAPPLIED JSON PREVIEW"),
            _ => None,
        }
    }

    fn check_json_handle(
        &self,
        handle: JsonDraftHandle,
    ) -> Result<(), EditorError> {
        if self.json.as_ref().is_none_or(|draft| draft.handle != handle)
            || handle.stamp != self.stamp()
            || handle.accepted_revision != self.accepted_revision
        {
            Err(EditorError::StaleToken)
        } else {
            Ok(())
        }
    }

    pub fn inspection_target(
        &self,
        location: ValidationLocation,
    ) -> Result<InspectionTarget, EditorError> {
        let target = InspectionTarget {
            stamp: self.stamp(),
            candidate_epoch: self.inspection_epoch(),
            key: InspectionKey::Semantic(location),
        };
        self.inspect(target)?;
        Ok(target)
    }

    pub fn object_inspection_target(
        &self,
        owner: InspectionOwner,
    ) -> Result<InspectionTarget, EditorError> {
        let target = InspectionTarget {
            stamp: self.stamp(),
            candidate_epoch: self.inspection_epoch(),
            key: InspectionKey::Object(owner),
        };
        self.inspect(target)?;
        Ok(target)
    }

    /// One shared, borrowed candidate selection for bounded browser discovery.
    pub(crate) fn with_inspection_config<R>(
        &self,
        inspect: impl FnOnce(&Config) -> R,
    ) -> R {
        if let Some(JsonState::Candidate { config, .. }) = self.json.as_ref().map(|draft| &draft.state) {
            return inspect(config);
        }
        let visual = self.diagnostic();
        inspect(visual.as_ref().map_or(self.preview().config(), |draft| &draft.config))
    }

    pub fn raw_inspection_target(
        &self,
        index: usize,
    ) -> Result<InspectionTarget, EditorError> {
        let field = self.raw.get(index).ok_or(EditorError::InvalidIndex)?.field;
        let target = InspectionTarget {
            stamp: self.stamp(),
            candidate_epoch: self.inspection_epoch(),
            key: InspectionKey::Raw { index, field },
        };
        self.inspect(target)?;
        Ok(target)
    }

    /// Pure navigation. No selection, revision, text, I/O or acceptance state
    /// changes; consumers open their workspace/group before focusing details.
    pub fn inspect(
        &self,
        target: InspectionTarget,
    ) -> Result<ResolvedInspection, EditorError> {
        if target.stamp != self.stamp() || target.candidate_epoch != self.inspection_epoch() {
            return Err(EditorError::StaleToken);
        }
        let visual = self.diagnostic();
        let config = match self.json.as_ref().map(|draft| &draft.state) {
            Some(JsonState::Candidate { config, .. }) => config.as_ref(),
            _ => visual.as_ref().map_or(self.preview().config(), |draft| &draft.config),
        };
        let (view_index, widget_index, channel_index, field, raw_field, raw_index, json_path) = match target.key {
            InspectionKey::Object(owner) => {
                let (view, widget, channel, path) = match owner {
                    InspectionOwner::Root => (None, None, None, String::new()),
                    InspectionOwner::View { index } => (Some(index), None, None, format!("/views/{index}")),
                    InspectionOwner::Widget { index } => (None, Some(index), None, format!("/widgets/{index}")),
                    InspectionOwner::Channel(channel) => {
                        (
                            None,
                            None,
                            Some(channel.index()),
                            format!("/sources/{}", channel.index()),
                        )
                    }
                };
                (view, widget, channel, None, None, None, Some(path))
            }
            InspectionKey::Semantic(location) => {
                (
                    location.view_index(),
                    location.widget_index(),
                    location.channel_index(),
                    Some(location.field()),
                    None,
                    None,
                    Some(location.to_string()),
                )
            }
            InspectionKey::Raw { index, field } => {
                if self.raw.get(index).is_none_or(|raw| raw.field != field) {
                    return Err(EditorError::StaleToken);
                }
                let view = match field {
                    RawField::ViewName(id) => Some(unique_view_index(config, id)?),
                    _ => None,
                };
                let widget = match field {
                    RawField::Widget(id, _) => Some(unique_widget_index(config, id)?),
                    _ => None,
                };
                if let Some(index) = widget {
                    unique_view_index(config, config.widgets[index].view)?;
                }
                check_raw_available(config, field)?;
                let channel = match field {
                    RawField::Source(channel, _) | RawField::HistorySeconds(channel) => Some(channel.index()),
                    _ => None,
                };
                (view, widget, channel, None, Some(field), Some(index), None)
            }
        };
        if view_index.is_some_and(|index| config.views.get(index).is_none())
            || widget_index.is_some_and(|index| config.widgets.get(index).is_none())
            || channel_index.is_some_and(|index| config.sources.get(index).is_none())
        {
            return Err(EditorError::InvalidIndex);
        }
        if let Some(index) = widget_index {
            let band = match field {
                Some(
                    Field::BandLabel(i)
                    | Field::BandMinimum(i)
                    | Field::BandMaximum(i)
                    | Field::BandOrder(i)
                    | Field::BandColor(i),
                ) => Some(i as usize),
                _ => None,
            };
            if band.is_some_and(|band| config.widgets[index].advanced.bands().get(band).is_none()) {
                return Err(EditorError::InvalidIndex);
            }
        }
        let id_lookup_unambiguous = widget_index.is_none_or(|index| {
            let widget = &config.widgets[index];
            unique_widget_index(config, widget.id).is_ok() && unique_view_index(config, widget.view).is_ok()
        }) && view_index
            .is_none_or(|index| unique_view_index(config, config.views[index].id).is_ok());
        Ok(ResolvedInspection {
            view_index,
            widget_index,
            channel_index,
            field,
            raw_field,
            raw_index,
            id_lookup_unambiguous,
            json_path,
        })
    }

    fn inspection_epoch(&self) -> Option<(u64, u64, bool)> {
        self.json.as_ref().map(|draft| {
            (
                draft.handle.epoch,
                draft.text_epoch,
                matches!(draft.state, JsonState::Candidate { .. }),
            )
        })
    }

    pub fn history_len(&self) -> (usize, usize) { (self.undo.len(), self.redo.len()) }

    pub fn field_token(&self) -> FieldToken {
        FieldToken {
            owner: self.owner,
            revision: self.revision,
        }
    }

    pub fn raw_fields(&self) -> &[RawInput] { &self.raw }

    /// Inspector reveal seam for inactive preferences. Raw drafts and precise
    /// shared errors both keep their actual controls visible, including future
    /// candidate adapters that have not constructed per-field raw text.
    pub fn widget_fields_need_repair(
        &self,
        id: u16,
        fields: &[WidgetField],
        validation_field: Field,
    ) -> bool {
        self.raw.iter().any(
            |input| matches!(input.field, RawField::Widget(widget, field) if widget == id && fields.contains(&field)),
        ) || matches!(self.draft_error, Some(EditorError::Validation(error)) if error.widget == Some(id) && error.field == validation_field)
    }

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
        self.import == Some(token)
            && token.revision == self.revision
            && !self.has_invalid_draft()
            && !self.has_pending_json()
    }

    pub fn last_saved_revision(&self) -> Option<Revision> { self.saved_revision }

    /// Check immediately before a prepared write. Serialize browser writes;
    /// never interpret a cancelled timer as a successful storage completion.
    pub fn save_is_current(
        &self,
        token: SaveToken,
    ) -> bool {
        self.save == Some(token) && token.revision == self.revision && self.snapshot_state_block().is_none()
    }

    pub fn save_status(&self) -> SaveStatus {
        if self.has_invalid_draft()
            || self.has_pending_json()
            || self.has_startup_draft()
            || self.artwork_geometry_mismatch()
        {
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
        self.ensure_snapshot_ready()?;
        let request = self.next_request()?;
        let json = self.serialize_saved_envelope()?;
        let project = self.accepted_project()?;
        self.request = request;
        let token = SaveToken {
            owner: self.owner,
            request,
            revision: self.revision,
        };
        self.save = Some(token);
        self.save_failure = None;
        Ok(SaveRequest { token, json, project })
    }

    /// Exact browser-storage bytes without issuing a request or changing save
    /// status. Resource inspection uses this only after the same admission gate.
    pub fn saved_envelope_json(&self) -> Result<String, EditorError> {
        self.ensure_snapshot_ready()?;
        self.serialize_saved_envelope()
    }

    fn serialize_saved_envelope(&self) -> Result<String, EditorError> {
        let envelope = SavedEnvelope {
            editor_version: SAVED_STATE_VERSION,
            selected_view: self.selection().view,
            selected_widget: self.selection().widget,
            config: self.preview().config(),
            startup: self.accepted.startup,
            artwork: self.accepted.artwork.as_ref().map(|a| a.identity()),
        };
        let json = serde_json::to_string(&envelope).map_err(json_error)?;
        if json.len() > MAX_SAVED_BYTES {
            return Err(EditorError::Codec(ConfigError::Length));
        }
        Ok(json)
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
        if !envelope.startup.valid()
            || envelope.artwork.is_some()
            || envelope.startup.theme == dashboard_config::startup::StartupTheme::Custom
        {
            return Err(EditorError::Project(ProjectError::Profile));
        }
        let mut editor = Self::new(restored.config().clone())?;
        editor.accepted.startup = envelope.startup;
        check_selection(editor.preview().config(), selection)?;
        editor.accepted.selection = selection;
        editor.saved_revision = Some(editor.revision);
        Ok(editor)
    }

    /// Restore the atomically stored complete project and its bounded selection envelope.
    pub fn from_project_saved(
        bytes: &[u8],
        envelope: &str,
    ) -> Result<Self, EditorError> {
        let project = Project::decode(bytes).map_err(EditorError::Project)?;
        Self::from_prepared_project_saved(project, envelope)
    }

    pub fn from_prepared_project_saved(
        project: Project,
        envelope: &str,
    ) -> Result<Self, EditorError> {
        if envelope.len() > MAX_SAVED_BYTES {
            return Err(EditorError::Codec(ConfigError::Length));
        }
        let saved: SavedEnvelope<Box<serde_json::value::RawValue>> =
            serde_json::from_str(envelope).map_err(json_error)?;
        if saved.editor_version != SAVED_STATE_VERSION {
            return Err(EditorError::SavedVersion);
        }

        let dashboard = from_json_bytes(saved.config.get().as_bytes()).map_err(EditorError::Codec)?;
        let expected =
            Project::new(&dashboard, saved.startup, project.artwork().cloned()).map_err(EditorError::Project)?;
        if expected.dashboard_bytes() != project.dashboard_bytes()
            || saved.startup != project.profile()
            || saved.artwork != project.artwork().map(|a| a.identity())
        {
            return Err(EditorError::Project(ProjectError::Integrity));
        }
        let mut editor = Self::new(dashboard.config().clone())?;
        let selection = Selection {
            view: saved.selected_view,
            widget: saved.selected_widget,
        };
        check_selection(editor.preview().config(), selection)?;
        editor.accepted.selection = selection;
        editor.accepted.startup = project.profile();
        editor.accepted.artwork = project.artwork().cloned();
        editor.saved_revision = Some(editor.revision);
        Ok(editor)
    }

    pub fn begin_pointer(
        &mut self,
        pointer_id: i32,
        target: PointerTarget,
    ) -> Result<PointerToken, EditorError> {
        self.ensure_no_json()?;
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
        if format == ImportFormat::Project {
            return self.complete_project_import(token, bytes);
        }
        if self.import != Some(token) || token.revision != self.revision {
            return Err(EditorError::StaleToken);
        }
        self.ensure_clean()?;
        self.import = None;
        let preview = match format {
            ImportFormat::Project => unreachable!("project handled before dashboard parsing"),
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
        self.accepted_revision = revision;
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
        self.ensure_no_json()?;
        if text.len() > MAX_RAW_TEXT_BYTES {
            return Err(EditorError::RawTooLong);
        }
        // A retained raw field remains repairable after a mapping operation
        // creates a rejected candidate. It must still exist and apply to the
        // current candidate; the accepted document cannot prove that target.
        check_raw_target(self.editing_config(), self.selection(), field)?;
        self.set_raw_transaction(field, text)
    }

    /// Repair only the actual retained raw entry authorized by a current
    /// inspection capability. This does not select its owner or permit new
    /// arbitrary fields; ordinary set_raw retains its selection guard.
    pub fn set_inspected_raw(
        &mut self,
        target: InspectionTarget,
        text: String,
    ) -> Result<Revision, EditorError> {
        self.ensure_no_json()?;
        let resolved = self.inspect(target)?;
        let field = resolved.raw_field.ok_or(EditorError::FieldUnavailable)?;
        check_raw_available(self.editing_config(), field)?;
        self.set_raw_transaction(field, text)
    }

    /// Explicit clean accepted-object editing without changing persisted selection.
    /// Invalid repair remains restricted to a genuine retained Raw capability.
    pub fn set_located_raw(
        &mut self,
        target: InspectionTarget,
        field: RawField,
        text: String,
    ) -> Result<Revision, EditorError> {
        self.ensure_snapshot_ready()?;
        let resolved = self.inspect(target)?;
        let InspectionKey::Object(owner) = target.key else {
            return Err(EditorError::FieldUnavailable);
        };
        let config = self.preview().config();
        let matches = match (owner, field) {
            (InspectionOwner::Widget { index }, RawField::Widget(id, _)) => {
                config.widgets.get(index).is_some_and(|widget| widget.id == id)
            }
            (InspectionOwner::View { index }, RawField::ViewName(id)) => {
                config.views.get(index).is_some_and(|view| view.id == id)
            }
            (InspectionOwner::Channel(a), RawField::Source(b, _) | RawField::HistorySeconds(b)) => a == b,
            (
                InspectionOwner::Root,
                RawField::Fuel(_)
                | RawField::Led { .. }
                | RawField::ControlHoldMilliseconds
                | RawField::WarningPageMilliseconds
                | RawField::WarningColor { .. },
            ) => true,
            _ => false,
        };
        if !matches || !resolved.id_lookup_unambiguous {
            return Err(EditorError::FieldUnavailable);
        }
        check_raw_available(config, field)?;
        self.set_raw_transaction(field, text)
    }

    fn set_raw_transaction(
        &mut self,
        field: RawField,
        text: String,
    ) -> Result<Revision, EditorError> {
        if text.len() > MAX_RAW_TEXT_BYTES {
            return Err(EditorError::RawTooLong);
        }
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
                self.accepted_revision = revision;
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
        self.ensure_no_json()?;
        if self.has_invalid_draft() {
            let revision = self.next_revision()?;
            self.raw.clear();
            self.rejected = None;
            self.draft_error = None;
            self.changed(revision);
            self.accepted_revision = revision;
        }
        Ok(self.revision)
    }

    pub fn export_json(&self) -> Result<String, EditorError> {
        self.ensure_snapshot_ready()?;
        to_json(self.preview()).map_err(EditorError::Codec)
    }

    pub fn export_binary(&self) -> Result<Vec<u8>, EditorError> {
        self.ensure_snapshot_ready()?;
        let mut bytes = vec![0; MAX_FILE_BYTES];
        let len = encode(self.preview(), &mut bytes).map_err(EditorError::Codec)?.len();
        bytes.truncate(len);
        Ok(bytes)
    }

    pub fn selected_appearance(
        &self,
        stamp: EditorStamp,
    ) -> Result<AppearanceSnapshot, EditorError> {
        if stamp != self.stamp() {
            return Err(EditorError::StaleToken);
        }
        self.ensure_snapshot_ready()?;
        let id = self.accepted.selection.widget.ok_or(EditorError::SelectionMismatch)?;
        let index = unique_widget_index(self.preview().config(), id)?;
        Ok(AppearanceSnapshot::from_widget(&self.preview().config().widgets[index]))
    }

    pub fn appearance_request(
        &self,
        scope: AppearanceScope,
        snapshot: AppearanceSnapshot,
        stamp: EditorStamp,
    ) -> Result<AppearanceRequest, EditorError> {
        if stamp != self.stamp() {
            return Err(EditorError::StaleToken);
        }
        self.ensure_snapshot_ready()?;
        snapshot.validate()?;
        let config = self.preview().config();
        let owner = match scope {
            AppearanceScope::Selected => {
                InspectionOwner::Widget {
                    index: unique_widget_index(
                        config,
                        self.accepted.selection.widget.ok_or(EditorError::SelectionMismatch)?,
                    )?,
                }
            }
            AppearanceScope::ThisView => {
                InspectionOwner::View {
                    index: unique_view_index(config, self.accepted.selection.view)?,
                }
            }
            AppearanceScope::AllWidgets => InspectionOwner::Root,
        };
        Ok(AppearanceRequest {
            stamp,
            target: self.object_inspection_target(owner)?,
            scope,
            snapshot,
        })
    }

    pub fn check_appearance_request(
        &self,
        request: AppearanceRequest,
    ) -> Result<ResolvedInspection, EditorError> {
        if request.stamp != self.stamp() {
            return Err(EditorError::StaleToken);
        }
        self.ensure_snapshot_ready()?;
        request.snapshot.validate()?;
        let resolved = self.inspect(request.target)?;
        if !resolved.id_lookup_unambiguous {
            return Err(EditorError::AmbiguousTarget);
        }
        Ok(resolved)
    }

    fn appearance_candidate(
        &self,
        request: AppearanceRequest,
    ) -> Result<(Config, AppearancePreview), EditorError> {
        let resolved = self.check_appearance_request(request)?;
        let mut config = self.preview().config().clone();
        let view = match request.scope {
            AppearanceScope::ThisView => Some(config.views[resolved.view_index.ok_or(EditorError::InvalidIndex)?].id),
            _ => None,
        };
        let mut preview = AppearancePreview::default();
        for (index, widget) in config.widgets.iter_mut().enumerate() {
            let included = match request.scope {
                AppearanceScope::Selected => Some(index) == resolved.widget_index,
                AppearanceScope::ThisView => Some(widget.view) == view,
                AppearanceScope::AllWidgets => true,
            };
            if included {
                apply_snapshot(widget, request.snapshot, &mut preview);
            }
        }
        Ok((config, preview))
    }

    pub fn appearance_preview(
        &self,
        request: AppearanceRequest,
    ) -> Result<AppearancePreview, EditorError> {
        let (config, mut preview) = self.appearance_candidate(request)?;
        // Read-only preview never issues a Config cache identity or a proof.
        // The same authoritative visitor is used by validate at actual Apply.
        let _ = visit_validation(&config, |issue| {
            preview.error = Some(EditorError::Validation(issue.error));
            core::ops::ControlFlow::Break(())
        });
        Ok(preview)
    }

    pub fn apply(
        &mut self,
        edit: Edit,
    ) -> Result<Revision, EditorError> {
        if let Edit::ApplyAppearance(request) = edit {
            let (config, _) = self.appearance_candidate(request)?;
            let preview = validate(config).map_err(EditorError::Validation)?;
            return self.accept_validated(preview, self.accepted.selection);
        }
        self.ensure_no_json()?;
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
        let mut remapped_raw = None;
        match edit {
            Edit::ApplyAppearance(_) => unreachable!("appearance is admitted atomically above"),
            Edit::SetHistory { channel, seconds } => config.history_seconds[channel.index()] = seconds,
            Edit::SetOrientation(orientation) => {
                let before = config.clone();
                config.orientation = orientation;
                crate::orientation::repack_changed_views(&before, &mut config)?;
            }
            Edit::SetViewOrientation { id, orientation } => {
                let index = view_index(&config, id)?;
                let before = config.clone();
                config.views[index].orientation = orientation;
                crate::orientation::repack_changed_views(&before, &mut config)?;
            }
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
                let raw = remapped_raw.insert(self.raw.clone());
                raw.retain_mut(|raw| {
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
                        orientation: None,
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
                let orientation = config.views[index].orientation;
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
                            orientation,
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
        let candidate_raw = remapped_raw.as_ref().unwrap_or(&self.raw);
        for input in candidate_raw {
            check_raw_available(&config, input.field).map_err(|_| EditorError::RepairFieldFirst(input.field))?;
        }
        let mut raw_error = None;
        for input in candidate_raw {
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
                    self.accepted_revision = revision;
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
                if let Some(raw) = remapped_raw {
                    self.raw = raw;
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
        self.ensure_no_json()?;
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
            if !self.has_invalid_draft() {
                self.accepted_revision = self.revision;
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
        let changed_config =
            !same_canonical_config(self.preview(), &self.undo.back().ok_or(EditorError::NoUndo)?.preview)?;
        let previous = self.undo.pop_back().ok_or(EditorError::NoUndo)?;
        let current = std::mem::replace(&mut self.accepted, previous);
        push_history(&mut self.redo, current);
        self.changed(revision);
        self.accepted_revision = revision;
        if changed_config {
            self.accepted_config_revision = revision;
        }
        Ok(revision)
    }

    pub fn redo(&mut self) -> Result<Revision, EditorError> {
        self.ensure_clean()?;
        if self.redo.is_empty() {
            return Err(EditorError::NoRedo);
        }
        let revision = self.next_revision()?;
        let changed_config =
            !same_canonical_config(self.preview(), &self.redo.back().ok_or(EditorError::NoRedo)?.preview)?;
        let next = self.redo.pop_back().ok_or(EditorError::NoRedo)?;
        let current = std::mem::replace(&mut self.accepted, next);
        push_history(&mut self.undo, current);
        self.changed(revision);
        self.accepted_revision = revision;
        if changed_config {
            self.accepted_config_revision = revision;
        }
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
        let same_config = same_canonical_config(self.preview(), &preview)?;
        if same_config && selection == self.accepted.selection {
            return Ok(self.revision);
        }
        let revision = self.next_revision()?;
        let next = Box::new(Accepted {
            preview,
            selection,
            startup: self.accepted.startup,
            artwork: self.accepted.artwork.clone(),
        });
        let current = std::mem::replace(&mut self.accepted, next);
        push_history(&mut self.undo, current);
        self.redo.clear();
        self.changed(revision);
        self.accepted_revision = revision;
        if !same_config {
            self.accepted_config_revision = revision;
        }
        Ok(revision)
    }

    fn changed(
        &mut self,
        revision: Revision,
    ) {
        self.revision = revision;
        self.json = None;
        self.startup_timing = None;
        self.pointer = None;
        self.import = None;
        self.save = None;
        self.save_failure = None;
    }

    fn ensure_clean(&self) -> Result<(), EditorError> {
        self.ensure_no_json()?;
        if self.has_invalid_draft() {
            Err(EditorError::InvalidDraft)
        } else {
            Ok(())
        }
    }

    fn ensure_no_json(&self) -> Result<(), EditorError> {
        if self.has_pending_json() {
            Err(EditorError::JsonPending)
        } else if self.has_startup_draft() {
            Err(EditorError::StartupPending)
        } else {
            Ok(())
        }
    }

    fn ensure_snapshot_ready(&self) -> Result<(), EditorError> {
        self.action_readiness().block.map_or(Ok(()), |block| Err(block.into()))
    }
}

fn same_canonical_config(
    before: &ValidatedConfig,
    after: &ValidatedConfig,
) -> Result<bool, EditorError> {
    let mut before_bytes = vec![0; MAX_FILE_BYTES];
    let mut after_bytes = vec![0; MAX_FILE_BYTES];
    Ok(encode(before, &mut before_bytes).map_err(EditorError::Codec)?
        == encode(after, &mut after_bytes).map_err(EditorError::Codec)?)
}

fn unique_view_index(
    config: &Config,
    id: u8,
) -> Result<usize, EditorError> {
    let mut indices = config
        .views
        .iter()
        .enumerate()
        .filter_map(|(index, view)| (view.id == id).then_some(index));
    let index = indices.next().ok_or(EditorError::MissingView(id))?;
    if indices.next().is_some() {
        return Err(EditorError::AmbiguousTarget);
    }
    Ok(index)
}

fn unique_widget_index(
    config: &Config,
    id: u16,
) -> Result<usize, EditorError> {
    let mut indices = config
        .widgets
        .iter()
        .enumerate()
        .filter_map(|(index, widget)| (widget.id == id).then_some(index));
    let index = indices.next().ok_or(EditorError::MissingWidget(id))?;
    if indices.next().is_some() {
        return Err(EditorError::AmbiguousTarget);
    }
    Ok(index)
}

fn json_error(error: serde_json::Error) -> EditorError {
    EditorError::Codec(ConfigError::Json {
        line: error.line(),
        column: error.column(),
    })
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
    unique_view_index(config, id)
}

fn widget_index(
    config: &Config,
    id: u16,
) -> Result<usize, EditorError> {
    unique_widget_index(config, id)
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

#[cfg(test)]
mod transaction_tests {
    use super::*;

    #[test]
    fn resource_cache_rechecks_identity_exhaustion_with_an_unchanged_stamp() {
        use crate::resources::{ResourceAvailability, ResourceCache};

        let mut editor = Editor::default();
        let mut cache = ResourceCache::default();
        let original = cache.inspect(&editor).unwrap().accepted.unwrap();
        editor.request = u64::MAX;
        assert_eq!(editor.stamp(), original.stamp);
        assert_eq!(
            cache.inspect(&editor).unwrap().availability,
            ResourceAvailability::LastAccepted(ActionBlock::IdentityExhausted)
        );
        assert_eq!(editor.saved_envelope_json(), Err(EditorError::CounterExhausted));
    }

    #[test]
    fn appearance_issuance_exhaustion_preserves_existing_save_and_all_state() {
        let mut editor = Editor::default();
        editor
            .select(Selection {
                view: 1,
                widget: Some(1),
            })
            .unwrap();
        let save = editor.begin_save().unwrap();
        let mut snapshot = editor.selected_appearance(editor.stamp()).unwrap();
        snapshot.decimals = 3;
        let request = editor
            .appearance_request(AppearanceScope::AllWidgets, snapshot, editor.stamp())
            .unwrap();
        editor.request = u64::MAX;
        let before = (
            editor.stamp(),
            editor.accepted_config_key(),
            editor.history_len(),
            editor.selection(),
        );
        let config = serde_json::to_string(editor.preview().config()).unwrap();
        assert_eq!(
            editor.apply(Edit::ApplyAppearance(request)),
            Err(EditorError::CounterExhausted)
        );
        assert_eq!(
            (
                editor.stamp(),
                editor.accepted_config_key(),
                editor.history_len(),
                editor.selection()
            ),
            before
        );
        assert_eq!(serde_json::to_string(editor.preview().config()).unwrap(), config);
        assert!(!editor.has_invalid_draft());
        assert!(editor.save_is_current(save.token()));
        editor.acknowledge_save(save.token(), Ok(())).unwrap();
        assert_eq!(editor.save_status(), SaveStatus::Saved);
    }

    #[test]
    fn located_object_exhaustion_never_publishes_raw_or_history() {
        for request_exhausted in [false, true] {
            let mut editor = Editor {
                revision: Revision(if request_exhausted { 0 } else { u64::MAX }),
                request: if request_exhausted { u64::MAX } else { 0 },
                ..Editor::default()
            };
            let target = editor.object_inspection_target(InspectionOwner::Root).unwrap();
            let before = (
                editor.stamp(),
                editor.accepted_config_key(),
                editor.history_len(),
                editor.selection(),
                editor.preview().clone(),
            );
            assert_eq!(
                editor.set_located_raw(target, RawField::ControlHoldMilliseconds, "900".into()),
                Err(EditorError::CounterExhausted)
            );
            assert_eq!(
                (
                    editor.stamp(),
                    editor.accepted_config_key(),
                    editor.history_len(),
                    editor.selection(),
                    editor.preview().clone()
                ),
                before
            );
            assert!(editor.raw_fields().is_empty());
        }
    }

    #[test]
    fn final_save_request_remains_current_and_completes_while_next_issuance_is_exhausted() {
        let mut editor = Editor {
            request: u64::MAX - 1,
            ..Editor::default()
        };
        let request = editor.begin_save().unwrap();
        let token = request.token();
        assert_eq!(token.request, u64::MAX);
        assert_eq!(editor.action_readiness().block, Some(ActionBlock::IdentityExhausted));
        assert_eq!(editor.save_status(), SaveStatus::Saving);
        assert!(
            editor.save_is_current(token),
            "the last successfully issued save must remain writable"
        );
        let before = (editor.stamp(), editor.accepted_config_key(), editor.history_len());
        assert!(matches!(editor.begin_save(), Err(EditorError::CounterExhausted)));
        assert!(
            editor.save_is_current(token),
            "failed next issuance must preserve the already issued request"
        );
        assert_eq!(
            (editor.stamp(), editor.accepted_config_key(), editor.history_len()),
            before
        );
        editor.acknowledge_save(token, Ok(())).unwrap();
        assert_eq!(editor.save_status(), SaveStatus::Saved);
        assert_eq!(editor.last_saved_revision(), Some(editor.revision()));
        assert!(!editor.save_is_current(token));
        assert!(matches!(editor.begin_save(), Err(EditorError::CounterExhausted)));
        assert_eq!(editor.request, u64::MAX);
        assert_eq!(editor.save_status(), SaveStatus::Saved);
    }

    #[test]
    fn inspected_raw_authority_rechecks_index_ambiguity_missing_owner_applicability_and_exhaustion() {
        for case in 0..5 {
            let mut config = default_config();
            config.widgets[0].display.graph_range = GraphRange::Fixed { min: -0.4, max: 2.5 };
            let mut editor = Editor::new(config).unwrap();
            editor
                .select(Selection {
                    view: 1,
                    widget: Some(1),
                })
                .unwrap();
            let field = RawField::Widget(1, WidgetField::GraphMinimum);
            editor.set_raw(editor.field_token(), field, "bad".into()).unwrap();
            editor.rejected = Some(Box::new(Rejected {
                config: editor.preview().config().clone(),
                selection: editor.selection(),
            }));
            let mut target = editor.raw_inspection_target(0).unwrap();
            let expected = match case {
                0 => {
                    target.key = InspectionKey::Raw { index: 1, field };
                    EditorError::StaleToken
                }
                1 => {
                    editor.rejected.as_mut().unwrap().config.widgets[1].id = 1;
                    EditorError::AmbiguousTarget
                }
                2 => {
                    editor.rejected.as_mut().unwrap().config.widgets.clear();
                    EditorError::MissingWidget(1)
                }
                3 => {
                    editor.rejected.as_mut().unwrap().config.widgets[0].display.graph_range = GraphRange::Auto;
                    EditorError::FieldUnavailable
                }
                _ => {
                    editor.revision = Revision(u64::MAX);
                    target = editor.raw_inspection_target(0).unwrap();
                    EditorError::CounterExhausted
                }
            };
            let before = (
                editor.stamp(),
                editor.accepted_config_key(),
                editor.history_len(),
                editor.raw.clone(),
                editor.selection(),
            );
            assert_eq!(editor.set_inspected_raw(target, "-0.4".into()), Err(expected));
            assert_eq!(
                (
                    editor.stamp(),
                    editor.accepted_config_key(),
                    editor.history_len(),
                    editor.raw.clone(),
                    editor.selection()
                ),
                before
            );
        }
    }

    #[test]
    fn json_exhaustion_leaves_owned_text_identity_history_and_callbacks_unchanged() {
        let mut editor = Editor::default();
        let handle = editor.open_json().unwrap();
        let original = editor.json_draft().unwrap().text().to_owned();
        editor.json.as_mut().unwrap().text_epoch = u64::MAX;
        let before = (editor.stamp(), editor.accepted_config_key(), editor.history_len());
        assert_eq!(
            editor.set_json_text(handle, "{}".into()),
            Err(EditorError::CounterExhausted)
        );
        assert_eq!(editor.json_draft().unwrap().text(), original);
        assert!(!editor.has_pending_json());
        assert_eq!(
            (editor.stamp(), editor.accepted_config_key(), editor.history_len()),
            before
        );
        editor.discard_json().unwrap();
        editor.json_epoch = u64::MAX;
        assert_eq!(editor.open_json(), Err(EditorError::CounterExhausted));
        assert!(editor.json_draft().is_none());
        editor.json_epoch = 0;
        editor.revision = Revision(u64::MAX);
        let handle = editor.open_json().unwrap();
        assert_eq!(editor.action_readiness().block, Some(ActionBlock::IdentityExhausted));
        assert_eq!(
            editor.set_json_text(handle, "{}".into()),
            Err(EditorError::CounterExhausted)
        );
        assert!(!editor.has_pending_json());
    }

    #[test]
    fn accepted_configuration_exhaustion_is_atomic_for_apply_undo_and_redo() {
        let mut editor = Editor::default();
        editor.apply(Edit::SetReducedMotion(true)).unwrap();
        editor.undo().unwrap();
        let key = editor.accepted_config_key();
        let accepted = editor.preview().clone();
        let history = editor.history_len();
        editor.revision = Revision(u64::MAX);
        assert_eq!(editor.redo(), Err(EditorError::CounterExhausted));
        assert_eq!(
            editor.apply(Edit::SetReducedMotion(true)),
            Err(EditorError::CounterExhausted)
        );
        assert_eq!(editor.accepted_config_key(), key);
        assert_eq!(editor.preview(), &accepted);
        assert_eq!(editor.history_len(), history);
        editor.revision = Revision(5);
        editor.redo().unwrap();
        let key = editor.accepted_config_key();
        let history = editor.history_len();
        editor.revision = Revision(u64::MAX);
        assert_eq!(editor.undo(), Err(EditorError::CounterExhausted));
        assert_eq!(editor.accepted_config_key(), key);
        assert_eq!(editor.history_len(), history);
    }

    #[test]
    fn rejected_band_removal_preserves_the_complete_transaction() {
        // Both a successful repair and a still-invalid partial repair must
        // reserve the revision before deleting or retargeting retained text.
        for partial in [false, true] {
            let mut config = default_config();
            config.widgets.truncate(1);
            config.widgets[0]
                .advanced
                .set_bands(&[
                    Band::new(0., 1., Color(0xFFFF), "ONE").unwrap(),
                    Band::new(1., 2., Color(0xFFFF), "TWO").unwrap(),
                ])
                .unwrap();
            let mut editor = Editor::new(config).unwrap();
            editor
                .select(Selection {
                    view: 1,
                    widget: Some(1),
                })
                .unwrap();
            editor.apply(Edit::SetReducedMotion(true)).unwrap();
            editor.apply(Edit::SetReducedMotion(false)).unwrap();
            editor.undo().unwrap();
            for (field, value) in [
                (WidgetField::BandMinimum(0), "bad"),
                (WidgetField::BandMaximum(1), if partial { "bad" } else { "2.5" }),
            ] {
                editor
                    .set_raw(editor.field_token(), RawField::Widget(1, field), value.into())
                    .unwrap();
            }
            // Populate all callback slots to detect accidental retirement.
            editor.revision = Revision(u64::MAX);
            editor.pointer = Some(PointerToken {
                owner: editor.owner,
                request: 11,
                revision: editor.revision,
                pointer_id: 7,
                target: PointerTarget::Widget(1),
                view: 1,
            });
            editor.import = Some(ImportToken {
                owner: editor.owner,
                request: 12,
                revision: editor.revision,
            });
            editor.save = Some(SaveToken {
                owner: editor.owner,
                request: 13,
                revision: editor.revision,
            });
            editor.request = 13;
            editor.saved_revision = Some(Revision(1));
            editor.save_failure = Some(StorageFailure::Quota);
            let accepted = editor.preview().clone();
            let candidate = editor.editing_config().clone();
            let raw = editor.raw.clone();
            let error = editor.draft_error;
            let diagnostic = editor.diagnostic().map(|d| (d.config, d.raw, d.error));
            let selection = editor.selection();
            let tokens = (editor.field_token(), editor.pointer, editor.import, editor.save);
            let history = |e: &Editor| {
                (
                    e.undo
                        .iter()
                        .map(|s| (s.preview.clone(), s.selection))
                        .collect::<Vec<_>>(),
                    e.redo
                        .iter()
                        .map(|s| (s.preview.clone(), s.selection))
                        .collect::<Vec<_>>(),
                )
            };
            let before_history = history(&editor);
            assert_eq!(
                editor.apply(Edit::RemoveBand { id: 1, index: 0 }),
                Err(EditorError::CounterExhausted)
            );
            assert_eq!(editor.preview(), &accepted);
            assert_eq!(editor.editing_config(), &candidate);
            assert_eq!(editor.raw, raw);
            assert_eq!(editor.draft_error, error);
            assert_eq!(editor.diagnostic().map(|d| (d.config, d.raw, d.error)), diagnostic);
            assert_eq!(editor.selection(), selection);
            assert_eq!(
                (editor.field_token(), editor.pointer, editor.import, editor.save),
                tokens
            );
            assert_eq!(history(&editor), before_history);
            assert_eq!(editor.revision, Revision(u64::MAX));
            assert_eq!(editor.request, 13);
            assert_eq!(editor.saved_revision, Some(Revision(1)));
            assert_eq!(editor.save_failure, Some(StorageFailure::Quota));
            // Once admission is possible, the same operation either repairs
            // completely or retains the surviving band's invalid text at 0.
            editor.revision = Revision(20);
            let result = editor.apply(Edit::RemoveBand { id: 1, index: 0 });
            assert_eq!(editor.editing_config().widgets[0].advanced.bands().len(), 1);
            assert!(editor.pointer.is_none() && editor.import.is_none() && editor.save.is_none());
            assert_eq!(editor.revision, Revision(21));
            if partial {
                assert_eq!(
                    result,
                    Err(EditorError::Parse(RawField::Widget(1, WidgetField::BandMaximum(0))))
                );
                assert_eq!(
                    editor.raw,
                    vec![RawInput {
                        field: RawField::Widget(1, WidgetField::BandMaximum(0)),
                        text: "bad".into()
                    }]
                );
                assert_eq!(editor.preview(), &accepted);
                assert_eq!(history(&editor), before_history);
                editor
                    .set_raw(
                        editor.field_token(),
                        RawField::Widget(1, WidgetField::BandMaximum(0)),
                        "2.5".into(),
                    )
                    .unwrap();
            } else {
                assert!(result.is_ok());
            }
            assert!(!editor.has_invalid_draft());
            assert_eq!(editor.preview().config().widgets[0].advanced.bands()[0].max, 2.5);
            editor.undo().unwrap();
            assert_eq!(editor.preview(), &accepted);
        }
    }
}
