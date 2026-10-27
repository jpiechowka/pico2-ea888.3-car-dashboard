//! Transactional accepted project, repairable drafts and owner/revision-bound callbacks.
//! Accepted bytes/history change only after shared validation; rejected candidates
//! remain inspectable. Action, accepted-project and accepted-configuration revisions
//! are distinct. Browser adapters own scheduling/storage and return original tokens.
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

/// Applicability checks and raw-text parsing without accepted-state publication.
mod raw;
pub(crate) use raw::apply_raw;
use raw::{check_raw_available, check_raw_target};

/// Maximum snapshots retained independently in Undo and Redo histories.
pub const HISTORY_LIMIT: usize = 32;
/// Maximum UTF-8 bytes accepted for an ordinary raw field edit.
pub const MAX_RAW_TEXT_BYTES: usize = 1024;
/// Maximum simultaneously retained raw field entries.
pub const MAX_RAW_FIELDS: usize = 64;
/// Browser selection/configuration envelope version, independent of project/config versions.
pub const SAVED_STATE_VERSION: u16 = 1;
/// Maximum serialized browser envelope bytes, shared with configuration JSON admission.
pub const MAX_SAVED_BYTES: usize = MAX_JSON_BYTES;

// A replacement Editor must never accidentally accept an earlier instance's
// callbacks, even if both happen to have the same local revision/request.
/// Nonwrapping process-local editor-owner issuer; exhaustion refuses new editors.
static NEXT_EDITOR: AtomicUsize = AtomicUsize::new(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Editor-local checked generation, not wall time and never restored from history.
pub struct Revision(
    /// Monotonically issued generation; zero is initial state.
    u64,
);

impl Revision {
    /// Returns the exact generation for diagnostics/protocol transport.
    pub const fn get(self) -> u64 { self.0 }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Owner and revision cannot be supplied by a caller.
/// ```compile_fail
/// let mut stamp = dashboard_configurator::editor::Editor::default().stamp();
/// stamp.owner = 1;
/// ```
pub struct EditorStamp {
    /// Private editor instance identity; replacement editors never reuse it.
    owner: usize,
    /// Action revision, including rejected edits and selection changes.
    revision: Revision,
}

/// Acquisition synchronization key. Its revision changes only when accepted
/// canonical configuration changes; it is never restored from undo history.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AcceptedConfigKey {
    /// Owning editor identity, preventing cache/acquisition reuse across replacement.
    owner: usize,
    /// Last canonical-configuration transition, independent of selection/startup changes.
    revision: Revision,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Highest-priority reason snapshot/history UI actions are presently unavailable.
pub enum ActionBlock {
    /// Raw/rejected edits or required artwork geometry repair remain unresolved.
    InvalidDraft,
    /// Changed inline JSON awaits Apply/Discard.
    JsonPending,
    /// Startup timing text awaits Apply/Discard.
    StartupPending,
    /// A pointer gesture still owns layout interaction.
    PointerActive,
    /// An asynchronous import owns a pending request.
    ImportPending,
    /// A checked revision/request counter cannot issue another identity.
    IdentityExhausted,
}

impl From<ActionBlock> for EditorError {
    /// Returns the corresponding reducer error for readiness `block`.
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
/// Snapshot of action provenance and its optional current admission blocker.
pub struct ActionReadiness {
    /// Editor owner/action revision at inspection time.
    pub stamp: EditorStamp,
    /// `None` means presently ready; adapters still recheck immediately before action.
    pub block: Option<ActionBlock>,
}

/// A navigation capability for the exact current candidate. IDs are hints,
/// never the owner of an indexed semantic inspection target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InspectionTarget {
    /// Editor action identity at minting time.
    stamp: EditorStamp,
    /// JSON `(draft_epoch, text_epoch, has_candidate)` identity, if JSON is open.
    candidate_epoch: Option<(u64, u64, bool)>,
    /// Exact indexed semantic/raw/object target, never resolved by ambiguous IDs alone.
    key: InspectionKey,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Private target coordinates carried by an editor-issued inspection capability.
enum InspectionKey {
    /// Shared validation visitor's indexed location.
    Semantic(
        /// Exact schema location, including optional owner indices.
        ValidationLocation,
    ),
    /// Existing raw-entry position plus identity, preventing retargeting after reorder.
    Raw {
        /// Zero-based retained raw-input index.
        index: usize,
        /// Field expected at that index.
        field: RawField,
    },
    /// Explicit read-only object coordinates.
    Object(
        /// Indexed object requested by the inspector.
        InspectionOwner,
    ),
}

/// Requested browser inspection coordinates; only Editor can mint authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InspectionOwner {
    /// Configuration-level controls without a view/widget owner.
    Root,
    /// View at an indexed candidate position, not its potentially duplicated ID.
    View {
        /// Zero-based view-vector position.
        index: usize,
    },
    /// Widget at an indexed candidate position.
    Widget {
        /// Zero-based widget-vector position.
        index: usize,
    },
    /// Fixed channel catalogue entry.
    Channel(
        /// Channel whose source/history settings are inspected.
        Channel,
    ),
}

#[derive(Clone, Debug, PartialEq, Eq)]
/// Checked inspection coordinates for navigation; mutation requires a separate reducer gate.
pub struct ResolvedInspection {
    /// Indexed view owner, absent for targets without one.
    pub view_index: Option<usize>,
    /// Indexed widget owner, absent for targets without one.
    pub widget_index: Option<usize>,
    /// Fixed source-channel index, absent for targets without one.
    pub channel_index: Option<usize>,
    /// Shared semantic field when targeting a validation issue.
    pub field: Option<Field>,
    /// Retained raw field when targeting existing text repair.
    pub raw_field: Option<RawField>,
    /// Exact retained raw-entry index, absent for semantic/object navigation.
    pub raw_index: Option<usize>,
    /// False for duplicate/missing widget or view ID owners. Even when true,
    /// mutation must use normal reducer admission and field capability checks.
    pub id_lookup_unambiguous: bool,
    /// Indexed JSON pointer for semantic/object targets; raw-entry targets have no path.
    pub json_path: Option<String>,
}

impl ResolvedInspection {
    /// Returns human-readable JSON/index coordinates, or `Configuration` for a root target.
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
/// Selected named view and optional widget within that view; IDs are not vector indices.
pub struct Selection {
    /// Existing unique view ID.
    pub view: u8,
    /// Existing unique widget ID belonging to the view, or no widget selection.
    pub widget: Option<u16>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Field callback capability bound to editor instance and action revision.
pub struct FieldToken {
    /// Issuing editor identity.
    owner: usize,
    /// Action revision when the control callback was bound.
    revision: Revision,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Existing widget movement or a new library-channel placement.
pub enum PointerTarget {
    /// Move/resize an existing widget.
    Widget(
        /// Unique widget ID, not vector index.
        u16,
    ),
    /// Add a widget using shared channel defaults.
    Library(
        /// Channel whose default widget will be inserted.
        WidgetChannel,
    ),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// One active gesture's immutable editor/request/pointer/target identity.
pub struct PointerToken {
    /// Issuing editor identity.
    owner: usize,
    /// Checked asynchronous-request sequence.
    request: u64,
    /// Action revision after any pointer-start selection.
    revision: Revision,
    /// Browser pointer ID captured at gesture start, including its signed representation.
    pointer_id: i32,
    /// Original widget/library target; later events cannot replace it.
    target: PointerTarget,
    /// View selected at gesture start, used for library insertion.
    view: u8,
}

impl PointerToken {
    /// Returns the exact captured browser pointer ID for capture/release matching.
    pub fn pointer_id(self) -> i32 { self.pointer_id }

    /// Returns the original gesture target without granting a fresh callback identity.
    pub fn target(self) -> PointerTarget { self.target }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Capability for one asynchronous import; later edits or replacement retire it.
pub struct ImportToken {
    /// Issuing editor identity.
    owner: usize,
    /// Checked import request sequence.
    request: u64,
    /// Action revision at import start.
    revision: Revision,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Explicit import codec selection; complete projects retain startup/artwork.
pub enum ImportFormat {
    /// Complete versioned project archive.
    Project,
    /// Dashboard JSON, with supported legacy migration through the shared codec.
    Json,
    /// Dashboard-only compact binary.
    Binary,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Prepared save identity; an old successful write can affect stored revision evidence.
pub struct SaveToken {
    /// Issuing editor identity.
    owner: usize,
    /// Checked save request sequence.
    request: u64,
    /// Action revision whose bytes were captured.
    revision: Revision,
}

/// Immutable browser-save payload whose actual storage completion must be acknowledged.
pub struct SaveRequest {
    /// Original save capability checked immediately before the adapter writes.
    token: SaveToken,
    /// Bounded versioned selection/configuration envelope.
    json: String,
    /// Complete accepted project sharing immutable artwork allocations.
    project: Project,
}

impl SaveRequest {
    /// Returns the original save capability; adapters must not refresh it after waiting.
    pub fn token(&self) -> SaveToken { self.token }

    /// Returns the exact serialized envelope bytes the adapter must write.
    pub fn json(&self) -> &str { &self.json }

    /// Returns the captured complete project paired with the selection envelope.
    pub fn project(&self) -> &Project { &self.project }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Sanitized browser persistence failure; raw exceptions stay outside the reducer.
pub enum StorageFailure {
    /// Permission/lock/conflict policy denied the operation.
    Denied,
    /// Browser storage quota prevented completion.
    Quota,
    /// Storage unavailable or completion could not be established.
    Unavailable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Derived save indicator; only acknowledged actual writes can produce Saved.
pub enum SaveStatus {
    /// Current action revision differs from the last known stored revision.
    Dirty,
    /// Unresolved drafts or artwork mismatch block a current snapshot.
    InvalidDraft,
    /// A prepared save request is awaiting its real completion.
    Saving,
    /// Last acknowledged stored revision equals current action revision.
    Saved,
    /// The latest current save failed.
    Failed(
        /// Sanitized storage failure category.
        StorageFailure,
    ),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Raw editable widget property; quantities remain in the channel's canonical units.
pub enum WidgetField {
    /// Widget's bounded user-visible label.
    Label,
    /// Number of decimal places in rendered numeric values.
    Decimals,
    /// Fixed history graph lower bound, available only in fixed-range mode.
    GraphMinimum,
    /// Fixed history graph upper bound, available only in fixed-range mode.
    GraphMaximum,
    /// Configured gauge lower bound in canonical channel units.
    GaugeMinimum,
    /// Configured gauge upper bound in canonical channel units.
    GaugeMaximum,
    /// One widget palette role, parsed through the shared colour parser.
    Color(
        /// Palette member being edited.
        ColorRole,
    ),
    /// Bounded warning popup text.
    WarningMessage,
    /// Primary warning trigger threshold in canonical channel units.
    WarningTrigger,
    /// Primary hysteresis clear threshold in canonical channel units.
    WarningClear,
    /// Minimum qualified load percentage for warning evaluation.
    WarningLoad,
    /// Warning blink on duration in milliseconds.
    WarningOn,
    /// Warning blink off duration in milliseconds.
    WarningOff,
    /// Display smoothing duration in milliseconds; does not delay raw warnings.
    SmoothingMilliseconds,
    /// Peak indicator hold duration in milliseconds.
    PeakHoldMilliseconds,
    /// Manual/reference value in canonical channel units.
    Reference,
    /// Existing gauge-band lower endpoint in canonical units.
    BandMinimum(
        /// Zero-based band index.
        u8,
    ),
    /// Existing gauge-band upper endpoint in canonical units.
    BandMaximum(
        /// Zero-based band index.
        u8,
    ),
    /// Existing band's bounded display label.
    BandLabel(
        /// Zero-based band index.
        u8,
    ),
    /// Existing band's colour.
    BandColor(
        /// Zero-based band index.
        u8,
    ),
    /// Upper trigger endpoint for window warning modes, in canonical units.
    WarningTriggerHigh,
    /// Upper hysteresis-clear endpoint for window modes, in canonical units.
    WarningClearHigh,
    /// Warning acknowledgement snooze duration in seconds.
    WarningSnoozeSeconds,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Raw BLE peer/timing controls; edits configure the prototype without generating traffic.
pub enum BleField {
    /// Colon-separated six-byte address in displayed/stored order.
    Address,
    /// Connection-attempt timeout in milliseconds.
    ConnectTimeout,
    /// Adapter response timeout in milliseconds.
    ResponseTimeout,
    /// Gap between permitted adapter requests in milliseconds.
    PollGap,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Raw normal/warning RGB-pattern controls, validated by the shared configuration rules.
pub enum LedField {
    /// Pattern RGB565 colour parsed from user text.
    Color,
    /// Pattern brightness percentage.
    Brightness,
    /// Pattern period in milliseconds.
    Period,
    /// Blink on-duty percentage.
    OnPercent,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Fuel-assumption inputs used for derived AFR; none measures actual fuel composition.
pub enum FuelField {
    /// Explicit stoichiometric air/fuel mass ratio.
    Ratio,
    /// Declared ethanol blend percentage under the selected blend basis.
    Percent,
    /// Assumed gasoline stoichiometric air/fuel mass ratio.
    GasolineRatio,
    /// Assumed ethanol stoichiometric air/fuel mass ratio.
    EthanolRatio,
    /// Assumed gasoline density in kg/L at the declared reference temperature.
    GasolineDensity,
    /// Assumed ethanol density in kg/L at the declared reference temperature.
    EthanolDensity,
    /// Common density reference temperature in degrees Celsius.
    ReferenceCelsius,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Raw diagnostic source routing/timing controls; acceptance does not qualify ECU support.
pub enum SourceField {
    /// Physical 11-bit CAN request identifier, decimal or `0x` hexadecimal text.
    Request,
    /// Physical 11-bit CAN response identifier, decimal or `0x` hexadecimal text.
    Response,
    /// Requested polling interval in milliseconds when polling is enabled.
    PollMilliseconds,
    /// Sample freshness limit in milliseconds.
    FreshnessMilliseconds,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Stable identity for one retained raw text input, with variant-dependent applicability.
pub enum RawField {
    /// BLE prototype peer/timing input.
    Ble(
        /// Concrete BLE control.
        BleField,
    ),
    /// Shared channel history duration in seconds.
    HistorySeconds(
        /// Channel whose widgets share this history window.
        Channel,
    ),
    /// One property of the normal or warning RGB pattern.
    Led {
        /// True selects warning pattern, false selects normal pattern.
        warning: bool,
        /// Concrete RGB-pattern control.
        field: LedField,
    },
    /// Ordinary configurable button-hold threshold in milliseconds, excluding fixed Y recovery.
    ControlHoldMilliseconds,
    /// Name of the selected view.
    ViewName(
        /// Unique view ID.
        u8,
    ),
    /// Property of the selected or explicitly inspected widget.
    Widget(
        /// Unique widget ID.
        u16,
        /// Concrete widget property.
        WidgetField,
    ),
    /// Input supported by the current declared fuel-assumption variant.
    Fuel(
        /// Concrete assumption input.
        FuelField,
    ),
    /// One diagnostic channel source control.
    Source(
        /// Fixed channel catalogue entry.
        Channel,
        /// Concrete routing/timing property.
        SourceField,
    ),
    /// Duration of each warning page in milliseconds.
    WarningPageMilliseconds,
    /// Shared warning-overlay colour.
    WarningColor {
        /// True selects text, false selects background.
        foreground: bool,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
/// Original user text retained for repair until its complete candidate validates.
pub struct RawInput {
    /// Applicable input identity in the current accepted/rejected candidate.
    pub field: RawField,
    /// At most `MAX_RAW_TEXT_BYTES` UTF-8 bytes, including incomplete/invalid syntax.
    pub text: String,
}

#[derive(Clone, Debug, PartialEq)]
/// Typed reducer command; [`Editor::apply`] validates the complete resulting candidate.
/// IDs identify objects, indices identify ordering/bands, and measurements use canonical units.
pub enum Edit {
    /// Change device orientation and deterministically repack affected views.
    SetOrientation(
        /// New device default including mounting flip.
        Orientation,
    ),
    /// Set/clear a named view's orientation override and repack changed geometry.
    SetViewOrientation {
        /// Unique view ID.
        id: u8,
        /// Override or `None` to inherit device orientation.
        orientation: Option<Orientation>,
    },
    /// Apply an already captured scoped appearance request atomically.
    ApplyAppearance(
        /// Snapshot and exact editor/target authority to recheck.
        AppearanceRequest,
    ),
    /// Change a channel's shared history window.
    SetHistory {
        /// Channel whose widgets share this duration.
        channel: Channel,
        /// History window length in seconds, checked by shared validation.
        seconds: u16,
    },
    /// Select reduced-motion rendering behavior.
    SetReducedMotion(
        /// Whether reduced motion is enabled.
        bool,
    ),
    /// Replace the complete RGB policy candidate.
    SetLed(
        /// Normal/warning patterns and off-screen policy.
        LedSettings,
    ),
    /// Replace BLE prototype settings without sending traffic.
    SetBle(
        /// Disabled/enabled peer/profile/timing configuration to validate.
        BleSettings,
    ),
    /// Remove a band and its raw fields, reindexing later retained band inputs.
    RemoveBand {
        /// Unique widget ID.
        id: u16,
        /// Zero-based existing band index.
        index: u8,
    },
    /// Replace control mappings; retained hold-duration text still takes precedence.
    SetControls(
        /// Complete mapping candidate checked for protected/reachable routes.
        ControlMap,
    ),
    /// Add an empty named view with a fresh ID and select it.
    AddView {
        /// New bounded display name.
        name: String,
    },
    /// Rename an existing view.
    RenameView {
        /// Unique view ID.
        id: u8,
        /// Replacement bounded display name.
        name: String,
    },
    /// Delete a nonfinal view and its widgets, repairing boot/selected view as needed.
    DeleteView(
        /// Unique view ID to remove.
        u8,
    ),
    /// Clone view settings/widgets with fresh IDs and select the new adjacent view.
    DuplicateView {
        /// Unique source view ID.
        id: u8,
        /// New bounded display name.
        name: String,
    },
    /// Move a named view within display/rotation order.
    ReorderView {
        /// Unique view ID to move.
        id: u8,
        /// Destination zero-based index in the existing view vector.
        index: usize,
    },
    /// Choose the view selected by a new device boot.
    SetBootView(
        /// Existing unique view ID.
        u8,
    ),
    /// Add a default channel widget with fresh ID and select it.
    AddWidget {
        /// Destination view ID.
        view: u8,
        /// Channel supplying shared widget defaults.
        channel: WidgetChannel,
        /// Requested grid origin/span, validated for fit/collision.
        placement: Placement,
    },
    /// Delete an existing widget, clearing selection when it was selected.
    DeleteWidget(
        /// Unique widget ID.
        u16,
    ),
    /// Clone a widget with fresh ID into a specified view/placement and select it.
    DuplicateWidget {
        /// Unique source widget ID.
        id: u16,
        /// Destination view ID.
        view: u8,
        /// New grid origin/span, validated for fit/collision.
        placement: Placement,
    },
    /// Move an existing widget, preserving its span and selecting the destination.
    MoveWidget {
        /// Unique widget ID.
        id: u16,
        /// Destination view ID.
        view: u8,
        /// Zero-based destination grid column.
        column: u8,
        /// Zero-based destination grid row.
        row: u8,
    },
    /// Change an existing widget's span at its retained grid origin.
    ResizeWidget {
        /// Unique widget ID.
        id: u16,
        /// Requested supported grid span.
        span: Span,
    },
    /// Replace grid origin/span in the widget's existing view and select it.
    PlaceWidget {
        /// Unique widget ID.
        id: u16,
        /// Full requested grid placement.
        placement: Placement,
    },
    /// Move a widget in configuration order without changing its position.
    ReorderWidget {
        /// Unique widget ID.
        id: u16,
        /// Destination zero-based index in the complete widget vector.
        index: usize,
    },
    /// Replace the widget matching the candidate's ID, preserving raw field repairs.
    UpdateWidget(
        /// Full unvalidated widget candidate.
        Widget,
    ),
    /// Change warning mode with coherent default/swap handling for its endpoints.
    SetWarningMode {
        /// Unique widget ID.
        id: u16,
        /// Requested threshold/window mode.
        mode: LimitMode,
    },
    /// Replace a diagnostic source definition without qualifying or issuing ECU requests.
    SetSource {
        /// Fixed channel catalogue entry.
        channel: Channel,
        /// Complete routing/scaling/timing candidate.
        source: ChannelSource,
    },
    /// Replace explicit fuel assumptions used for derived AFR.
    SetFuel(
        /// New model variant and its declared parameters.
        Fuel,
    ),
    /// Replace shared warning popup behavior.
    SetWarningPresentation(
        /// New warning presentation settings.
        WarningPresentation,
    ),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Reducer rejection; methods document whether a failed operation retains a repair draft.
pub enum EditorError {
    /// Complete project/startup/artwork admission failed.
    Project(
        /// Exact project validation/encoding failure.
        ProjectError,
    ),
    /// Shared configuration validation failed.
    Validation(
        /// Field/location-specific shared validation error.
        ValidationError,
    ),
    /// Configuration or saved-envelope encoding/parsing failed.
    Codec(
        /// Shared format/size/schema error.
        ConfigError,
    ),
    /// Required view ID is absent.
    MissingView(
        /// Requested view ID.
        u8,
    ),
    /// Required widget ID is absent.
    MissingWidget(
        /// Requested widget ID.
        u16,
    ),
    /// Attempted to remove the last remaining view.
    LastView,
    /// Bounded view collection cannot admit another view.
    ViewCapacity,
    /// Bounded widget collection cannot admit the requested insertion/copy.
    WidgetCapacity,
    /// Text exceeds the destination bounded string capacity.
    TextCapacity,
    /// Vector/band/inspection index is outside its current collection.
    InvalidIndex,
    /// Complete orientation repacking found no valid placement.
    LayoutCannotFit(
        /// View ID that cannot fit its widgets.
        u8,
    ),
    /// Widget/field ownership does not match the current selected view/widget.
    SelectionMismatch,
    /// Undo history is empty.
    NoUndo,
    /// Redo history is empty.
    NoRedo,
    /// Checked editor/request/draft identity would overflow.
    CounterExhausted,
    /// Retained invalid edits block this operation.
    InvalidDraft,
    /// Pending inline JSON requires Apply/Discard.
    JsonPending,
    /// Pending startup timing requires Apply/Discard, or no draft exists to Apply.
    StartupPending,
    /// Inline JSON exceeds the shared byte bound.
    JsonTooLong,
    /// A pointer gesture blocks snapshot/project editing.
    PointerActive,
    /// An import request blocks snapshot/project editing.
    ImportPending,
    /// Duplicate object IDs prevent unique mutation targeting.
    AmbiguousTarget,
    /// Callback owner/revision/request/target no longer matches current state.
    StaleToken,
    /// Raw input exceeds its retention bound.
    RawTooLong,
    /// Too many independent raw inputs are retained simultaneously.
    RawCapacity,
    /// Field does not apply to the current owner/variant or no retained entry exists.
    FieldUnavailable,
    /// Requested option change would hide/remove an unresolved raw control.
    RepairFieldFirst(
        /// Existing field that must be repaired first.
        RawField,
    ),
    /// Raw text cannot parse as the destination type.
    Parse(
        /// Field whose retained text failed parsing.
        RawField,
    ),
    /// Stored editor envelope version is unsupported.
    SavedVersion,
    /// Actual browser write acknowledgement reported failure.
    Storage(
        /// Sanitized persistence failure.
        StorageFailure,
    ),
}

#[derive(Clone)]
/// Immutable-by-publication project snapshot shared by accepted state and history.
struct Accepted {
    /// Fully validated canonical dashboard configuration.
    preview: ValidatedConfig,
    /// Valid view/widget selection belonging to this snapshot.
    selection: Selection,
    /// Accepted startup profile; artwork mismatch may remain repairable.
    startup: StartupProfile,
    /// Shared immutable converted/source artwork, absent for procedural startup.
    artwork: Option<Arc<Artwork>>,
}

/// Typed failed candidate retained for visual repair without entering accepted history.
struct Rejected {
    /// Unvalidated configuration after the rejected operation and raw-field replay.
    config: Config,
    /// Inspectable candidate selection, repaired when an operation removed its target.
    selection: Selection,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// Bounded browser envelope; raw JSON `C` permits strict nested schema decoding on restore.
struct SavedEnvelope<C> {
    /// Independent editor envelope version.
    editor_version: u16,
    /// Selected named view ID.
    selected_view: u8,
    /// Selected widget ID in that view, or absent.
    selected_widget: Option<u16>,
    /// Dashboard configuration or its raw JSON during restore validation.
    config: C,
    /// Accepted startup profile; missing legacy field defaults through serde.
    #[serde(default)]
    startup: StartupProfile,
    /// Converted artwork identity paired with the atomically stored project, if present.
    #[serde(default)]
    artwork: Option<[u8; 32]>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Raw startup timing control; both numbers are applied together.
pub enum StartupNumber {
    /// Status scene duration in milliseconds.
    StatusMilliseconds,
    /// Splash scene duration in milliseconds.
    SplashMilliseconds,
}
#[derive(Clone)]
/// Pending startup times, preserving rejected input apart from the accepted profile.
struct StartupTimingDraft {
    /// Raw decimal status milliseconds, at most 32 retained Unicode scalars.
    status: String,
    /// Raw decimal splash milliseconds, at most 32 retained Unicode scalars.
    splash: String,
    /// Per-field original input exceeded 32 UTF-8 bytes; prefix cannot be admitted.
    oversized: [bool; 2],
}
impl StartupTimingDraft {
    /// Returns `profile` with both times parsed and shared validity checked; oversized,
    /// parse/range failures return an error without changing retained text or caller state.
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

/// Accepted project reducer with bounded histories, repair drafts and private callback identities.
pub struct Editor {
    /// Nonreused editor instance identity.
    owner: usize,
    /// Current accepted project and selection; raw/rejected state never replaces it directly.
    accepted: Box<Accepted>,
    /// Current action generation, also advanced by rejected text/layout interactions.
    revision: Revision,
    /// Last accepted project/selection publication generation.
    accepted_revision: Revision,
    /// Last canonical dashboard configuration transition generation.
    accepted_config_revision: Revision,
    /// Pending status/splash raw times, absent when applied/discarded.
    startup_timing: Option<StartupTimingDraft>,
    /// Current editable or inspection-only JSON transaction.
    json: Option<JsonDraft>,
    /// Last issued JSON draft generation, never restored by Undo.
    json_epoch: u64,
    /// At most 32 previous accepted snapshots, oldest first.
    undo: VecDeque<Box<Accepted>>,
    /// At most 32 undone accepted snapshots, next redo at the back.
    redo: VecDeque<Box<Accepted>>,
    /// At most 64 retained raw field inputs; valid transactions clear them together.
    raw: Vec<RawInput>,
    /// Latest typed rejected candidate, if any.
    rejected: Option<Box<Rejected>>,
    /// Parse/validation reason paired with retained invalid draft state.
    draft_error: Option<EditorError>,
    /// Last issued pointer/import/save request generation.
    request: u64,
    /// Current pointer gesture capability, absent after cancellation/retirement.
    pointer: Option<PointerToken>,
    /// Current import capability, absent after completion/cancellation/retirement.
    import: Option<ImportToken>,
    /// Current prepared save capability; its real write still needs acknowledgement.
    save: Option<SaveToken>,
    /// Revision last known to have actually reached browser storage, even if older.
    saved_revision: Option<Revision>,
    /// Latest current-write failure; cleared by a new interaction or successful write.
    save_failure: Option<StorageFailure>,
}

impl Default for Editor {
    /// Returns a fresh default editor; panics if reviewed defaults fail validation or
    /// the process-wide checked owner issuer is exhausted.
    fn default() -> Self { Self::new(default_config()).expect("reviewed default") }
}

impl Editor {
    /// Validates `config` and returns an unsaved editor selecting its boot view.
    /// Returns validation or global owner-counter exhaustion errors before constructing state.
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

    /// Returns accepted startup settings, excluding unapplied timing text.
    pub fn startup_profile(&self) -> StartupProfile { self.accepted.startup }

    /// Returns whether either startup timing control belongs to an unapplied draft.
    pub fn has_startup_draft(&self) -> bool { self.startup_timing.is_some() }

    /// Returns retained text for `field`, or the accepted millisecond value when no draft exists.
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

    /// Returns true only when a draft exists and both numbers pass shared startup validation.
    pub fn startup_draft_valid(&self) -> bool {
        self.startup_timing
            .as_ref()
            .is_some_and(|d| d.profile(self.accepted.startup).is_ok())
    }

    /// Returns the unapplied/invalid timing banner, or `None` when no timing draft exists.
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
    /// `stamp` must be current; `field` selects the millisecond control. `text` retains at
    /// most 32 characters, with original UTF-8 length over 32 bytes marked invalid.
    /// Returns the advanced revision even for invalid text; stale/draft/counter errors leave state intact.
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

    /// Validates both numbers for current `stamp`; returns the advanced revision.
    /// A changed profile creates one history entry; equal settings still retire callbacks.
    /// Missing/invalid drafts, stale stamps and admission errors retain the draft.
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

    /// Discards timing text for current `stamp`; returns the current or advanced revision.
    /// Stale/counter errors preserve state. No history entry is created.
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

    /// Returns shared accepted artwork, including artwork retained during geometry repair, or `None`.
    pub fn startup_artwork(&self) -> Option<&Arc<Artwork>> { self.accepted.artwork.as_ref() }

    /// Retain source and converted bytes while an orientation edit awaits reconversion.
    /// Returns whether retained artwork differs from effective splash geometry.
    pub fn artwork_geometry_mismatch(&self) -> bool {
        // Artwork admission already checked its immutable bytes against this geometry.
        self.accepted.artwork.as_ref().is_some_and(|art| {
            art.geometry()
                != self
                    .accepted
                    .startup
                    .effective_splash_orientation(self.preview().config().orientation)
                    .geometry()
        })
    }

    /// Returns success for clean, idle editing; rejects drafts, active pointers and imports.
    /// Retained artwork geometry mismatch is deliberately repairable here.
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
    /// Returns a validated project or an action-admission/project error without changing state.
    pub fn accepted_project(&self) -> Result<Project, EditorError> {
        self.ensure_snapshot_ready()?;
        Project::new(self.preview(), self.accepted.startup, self.accepted.artwork.clone()).map_err(EditorError::Project)
    }

    /// Returns encoded accepted-project bytes with omissions controlled by `mode`.
    /// Pending/invalid state, identity exhaustion and project encoding errors block export.
    pub fn export_project(
        &self,
        mode: ExportMode,
    ) -> Result<Vec<u8>, EditorError> {
        self.accepted_project()?.encode(mode).map_err(EditorError::Project)
    }

    /// Applies `profile` as one undoable project edit; returns its revision or the unchanged revision.
    /// Non-custom themes remove artwork; custom geometry mismatches retain it for repair.
    /// Orientation changes may retire active operations; other changes require idle editing.
    /// Validation/admission/counter errors preserve accepted state.
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

    /// Admits immutable `artwork` for current `stamp`, selects Custom, and returns the project revision.
    /// Stale, busy, geometry/project and counter errors preserve the accepted project.
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
    /// `token` must still own the import; `artwork` must match current splash geometry.
    /// Returns the accepted revision. After token admission, failure consumes the import but retains the project.
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

    /// Publishes already-admitted `preview`, `selection`, `startup` and shared `artwork`.
    /// Returns the current revision for canonical equality, otherwise advances once and records history.
    /// Codec/counter errors precede mutation; callers must validate selection and project compatibility.
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

    /// Consumes current `token`, decodes project `bytes`, and returns the publication revision.
    /// Stale/dirty admission retains the token; later decode/admission failures consume it but retain accepted state.
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
    /// Consumes current `token` and returns the revision after accepting `project`.
    /// Stale/dirty admission leaves state intact; later admission errors retain the previous project.
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

    /// Decodes `project`'s dashboard and returns its accepted revision, selecting its boot view.
    /// Decode/project/counter errors preserve accepted state; artwork allocations remain shared.
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

    /// Returns the last accepted dashboard proof, excluding all diagnostic candidates.
    pub fn preview(&self) -> &ValidatedConfig { &self.accepted.preview }

    /// Returns rejected-candidate selection when present, otherwise accepted selection.
    pub fn selection(&self) -> Selection {
        self.rejected
            .as_ref()
            .map_or(self.accepted.selection, |draft| draft.selection)
    }

    /// Editable geometry, including rejected structural changes. This is not a
    /// validation proof and must never be used for saving or firmware export.
    /// Returns the retained rejected configuration or accepted configuration; this borrow is not a validation proof.
    pub fn editing_config(&self) -> &Config {
        self.rejected
            .as_ref()
            .map_or(self.preview().config(), |draft| &draft.config)
    }

    /// Returns whether raw drafts are absent or fully parse to a placement-only validation failure.
    /// Callers separately gate pending JSON/startup text and gesture ownership.
    pub fn can_edit_layout(&self) -> bool {
        if self.raw.is_empty() {
            return true;
        }
        let mut candidate = self.editing_config().clone();
        self.raw.iter().all(|input| apply_raw(&mut candidate, input).is_ok())
            && validate(candidate).is_err_and(|e| matches!(e.field, Field::Placement))
    }

    /// Returns the action revision, which also advances for retained rejected edits.
    pub fn revision(&self) -> Revision { self.revision }

    /// Returns editor-instance and action-revision identity for delayed callbacks.
    pub fn stamp(&self) -> EditorStamp {
        EditorStamp {
            owner: self.owner,
            revision: self.revision,
        }
    }

    /// Returns the revision of last accepted project/selection publication. Raw/rejected
    /// activity advances ordinary revision without publishing accepted state.
    pub fn accepted_revision(&self) -> Revision { self.accepted_revision }

    /// Returns owner and dashboard-content lineage; selection/startup/artwork changes do not advance it.
    pub fn accepted_config_key(&self) -> AcceptedConfigKey {
        AcceptedConfigKey {
            owner: self.owner,
            revision: self.accepted_config_revision,
        }
    }

    /// Returns whether owned JSON text remains unapplied, regardless of parse success.
    pub fn has_pending_json(&self) -> bool { self.json.as_ref().is_some_and(|draft| draft.pending) }

    /// Returns current identity and the first snapshot/action blocker, including artwork mismatch and exhausted
    /// counters.
    pub fn action_readiness(&self) -> ActionReadiness { self.readiness(true) }

    /// History can repair retained artwork with a different splash geometry.
    /// UI commands still wait for pending drafts, gestures and imports.
    /// Returns current identity and the first history-command blocker.
    pub fn history_readiness(&self) -> ActionReadiness { self.readiness(false) }

    /// Returns identity plus ordered state/counter admission; `require_artwork_match` enables the geometry gate.
    fn readiness(
        &self,
        require_artwork_match: bool,
    ) -> ActionReadiness {
        let block = self.state_block(require_artwork_match).or_else(|| {
            (self.revision.0 == u64::MAX || self.request == u64::MAX).then_some(ActionBlock::IdentityExhausted)
        });
        ActionReadiness {
            stamp: self.stamp(),
            block,
        }
    }

    /// Data/lifecycle admission shared by new snapshots and already-issued
    /// writes. Exhausted issuance cannot invalidate an existing save token.
    /// Returns the first lifecycle/data blocker, or `None` for an admissible existing snapshot.
    fn snapshot_state_block(&self) -> Option<ActionBlock> { self.state_block(true) }

    /// Returns the first JSON, startup, invalid, pointer or import blocker, or `None`.
    /// `require_artwork_match` additionally treats retained geometry mismatch as invalid.
    fn state_block(
        &self,
        require_artwork_match: bool,
    ) -> Option<ActionBlock> {
        if self.has_pending_json() {
            Some(ActionBlock::JsonPending)
        } else if self.has_startup_draft() {
            Some(ActionBlock::StartupPending)
        } else if self.has_invalid_draft() || (require_artwork_match && self.artwork_geometry_mismatch()) {
            Some(ActionBlock::InvalidDraft)
        } else if self.pointer.is_some() {
            Some(ActionBlock::PointerActive)
        } else if self.import.is_some() {
            Some(ActionBlock::ImportPending)
        } else {
            None
        }
    }

    /// Returns the owned JSON workspace, or `None` while closed.
    pub fn json_draft(&self) -> Option<&JsonDraft> { self.json.as_ref() }

    /// Returns an existing handle or opens bounded JSON without advancing action revision.
    /// Invalid visual state opens read-only diagnostic JSON. Startup, codec/size and epoch errors leave state intact.
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

    /// Replaces owned JSON `text` under current `handle`; returns a debounce parse capability.
    /// Bounds/read-only/identity errors preserve state. Success retires I/O and pointers without publishing a revision.
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
    /// `token` must match the current pending editable text. Returns success even for invalid JSON;
    /// syntax/validation diagnostics live in the draft. Stale tokens return an error without parsing.
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

    /// Re-parses and validates owned pending JSON; returns the publication revision or current revision if unchanged
    /// text. A pending application records one history entry even for equal content. Invalid/stale/counter/codec
    /// errors preserve accepted state and draft; missing selection falls back to the imported boot view.
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

    /// Closes owned JSON without changing revision/history. Returns success, including when already closed.
    pub fn discard_json(&mut self) -> Result<(), EditorError> {
        self.json = None;
        Ok(())
    }

    /// Returns a syntax/invalid/unapplied candidate banner, or `None` for closed/unparsed/nonpending clean JSON.
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

    /// Returns success only if `handle` matches owned draft, action stamp and accepted revision; otherwise stale.
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

    /// Returns a current candidate capability for semantic `location`; rejects unresolved or ambiguous targets.
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

    /// Returns a current candidate capability for indexed `owner`, or an unresolved-target error.
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
    /// Calls `inspect` with parsed JSON, diagnostic visual state or accepted configuration in that order;
    /// returns its result. The borrowed configuration may be invalid and is never published here.
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

    /// Returns an inspection capability for retained raw `index`; rejects missing or no-longer-resolvable fields.
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
    /// Returns resolved indices/path for current `target`; errors identify stale identity, missing/ambiguous
    /// semantic owners, invalid indices or unavailable fields. Indexed duplicate objects remain navigable.
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

    /// Returns JSON workspace/text epochs and candidate availability, or `None` outside JSON inspection.
    fn inspection_epoch(&self) -> Option<(u64, u64, bool)> {
        self.json.as_ref().map(|draft| {
            (
                draft.handle.epoch,
                draft.text_epoch,
                matches!(draft.state, JsonState::Candidate { .. }),
            )
        })
    }

    /// Returns `(undo, redo)` entry counts, each bounded by [`HISTORY_LIMIT`].
    pub fn history_len(&self) -> (usize, usize) { (self.undo.len(), self.redo.len()) }

    /// Returns current editor/action/selection authority for ordinary raw controls.
    pub fn field_token(&self) -> FieldToken {
        FieldToken {
            owner: self.owner,
            revision: self.revision,
        }
    }

    /// Returns retained raw controls in replay order; successful complete validation clears this slice.
    pub fn raw_fields(&self) -> &[RawInput] { &self.raw }

    /// Inspector reveal seam for inactive preferences. Raw drafts and precise
    /// shared errors both keep their actual controls visible, including future
    /// candidate adapters that have not constructed per-field raw text.
    /// Returns whether widget `id` owns any retained `fields` or the precise `validation_field` error.
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

    /// Returns the retained visual/raw failure, or `None` when no such draft needs repair.
    pub fn draft_error(&self) -> Option<EditorError> { self.draft_error }

    /// Returns whether raw or rejected visual state exists; JSON/startup drafts have separate gates.
    pub fn has_invalid_draft(&self) -> bool { !self.raw.is_empty() || self.rejected.is_some() }

    /// Inspection-only candidate. It never replaces the accepted document or
    /// enters serialization, acquisition, history, or firmware configuration.
    /// Returns a replayed candidate retaining all raw strings, or `None` without a visual/raw draft.
    /// Panics only if the internal invariant that invalid drafts retain an error is violated.
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

    /// Returns current gesture ownership, or `None` after cancellation, completion or a retiring edit.
    pub fn active_pointer(&self) -> Option<PointerToken> { self.pointer }

    /// Returns whether `token` owns the current revision's import with no invalid visual or pending JSON draft.
    pub fn import_is_current(
        &self,
        token: ImportToken,
    ) -> bool {
        self.import == Some(token)
            && token.revision == self.revision
            && !self.has_invalid_draft()
            && !self.has_pending_json()
    }

    /// Returns the last acknowledged persisted revision, including late writes, or `None` before one is known.
    pub fn last_saved_revision(&self) -> Option<Revision> { self.saved_revision }

    /// Check immediately before a prepared write. Serialize browser writes;
    /// never interpret a cancelled timer as a successful storage completion.
    /// Returns whether `token` still owns this revision's write and lifecycle/data gates permit it.
    pub fn save_is_current(
        &self,
        token: SaveToken,
    ) -> bool {
        self.save == Some(token) && token.revision == self.revision && self.snapshot_state_block().is_none()
    }

    /// Returns invalid, saving, failed, saved or dirty status in that priority order; no storage I/O occurs.
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
    /// Returns frozen envelope/project and a newly issued write token; admission/encoding/counter errors
    /// leave issuance and existing save state unchanged.
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
    /// Returns bounded JSON or a snapshot-admission/serialization error.
    pub fn saved_envelope_json(&self) -> Result<String, EditorError> {
        self.ensure_snapshot_ready()?;
        self.serialize_saved_envelope()
    }

    /// Returns bounded selection/config/startup/artwork-identity JSON; serialization/size errors retain state.
    /// Caller must enforce snapshot admission before using these bytes for persistence.
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
    /// `token` identifies the actual write and `result` its storage outcome. Returns success for a current
    /// successful write, or stale/draft/storage failure. A successful stale same-owner write still updates
    /// the recorded persisted revision; it never publishes editor content.
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
    /// Returns an editor restored from bounded `bytes`, marked saved at revision zero.
    /// Rejects envelope/schema/selection errors, custom/artwork-dependent state and owner exhaustion.
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
    /// Returns a saved editor from project `bytes` and matching `envelope`; decode, identity,
    /// schema/selection and owner-counter failures reject the complete restore.
    pub fn from_project_saved(
        bytes: &[u8],
        envelope: &str,
    ) -> Result<Self, EditorError> {
        let project = Project::decode(bytes).map_err(EditorError::Project)?;
        Self::from_prepared_project_saved(project, envelope)
    }

    /// Returns a saved editor from prepared `project` and bounded `envelope` only when dashboard bytes,
    /// startup and artwork identity agree exactly. Schema/project/selection/counter errors reject restoration.
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

    /// Returns a new gesture capability binding DOM `pointer_id` to `target` and current view/revision.
    /// Existing widgets are selected first; library additions require clean state. Draft/target/counter errors
    /// reject issuance, while successful issuance replaces previous pointer ownership.
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

    /// Consumes current `token` only for matching DOM `pointer_id`, then applies grid `placement`.
    /// Returns the edit revision or stale/edit error. A failed placement may remain as a rejected candidate;
    /// the consumed gesture is never automatically resumed.
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

    /// Clears gesture ownership if `token` is current; returns success or stale-token error without changing history.
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

    /// Returns a new import capability for clean state, replacing prior import and cancelling pointer ownership.
    /// Draft/counter errors preserve state; this method performs no file read or worker operation.
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
    /// `token` owns the read; `format` selects decoding of bounded `bytes`. Returns the publication revision
    /// or stale/draft/codec/project/counter error. Dashboard-only imports retain startup and artwork.
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

    /// Clears current `token` ownership; returns success or stale-token error. Adapters must cancel their I/O
    /// separately.
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
    /// `token` binds current selection, `field` selects an applicable control, and `text` is UTF-8 bounded.
    /// Returns the new action revision; stale/draft/target/capacity/counter errors reject the keystroke.
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
    /// `target` must resolve a retained raw entry; `text` replaces its bounded contents.
    /// Returns the action revision even if still invalid, or an admission/identity/target/capacity error.
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
    /// `target` must name an unambiguous accepted object owning `field`; `text` is bounded replacement text.
    /// Returns the action revision even if validation then retains a draft; admission/target/counter errors precede
    /// mutation.
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
                | RawField::Ble(_)
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

    /// Replaces/adds bounded `text` for already-authorized `field`, then replays all retained raw controls.
    /// Returns the advanced revision whether accepted or retained invalid; capacity/counter errors reject the
    /// keystroke. Only fully validated content enters history. Raw replay stops at its first error in this
    /// transaction.
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

    /// Clears visual/raw rejection without history; returns the advanced revision, or unchanged revision if clean.
    /// Pending JSON/startup and counter errors preserve the draft.
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

    /// Returns accepted dashboard-only JSON; admission/codec errors block export. Startup/artwork are omitted.
    pub fn export_json(&self) -> Result<String, EditorError> {
        self.ensure_snapshot_ready()?;
        to_json(self.preview()).map_err(EditorError::Codec)
    }

    /// Returns exact-length accepted dashboard binary, bounded by [`MAX_FILE_BYTES`]; startup/artwork are omitted.
    /// Snapshot-admission or codec errors return no export.
    pub fn export_binary(&self) -> Result<Vec<u8>, EditorError> {
        self.ensure_snapshot_ready()?;
        let mut bytes = vec![0; MAX_FILE_BYTES];
        let len = encode(self.preview(), &mut bytes).map_err(EditorError::Codec)?.len();
        bytes.truncate(len);
        Ok(bytes)
    }

    /// Returns the selected widget's appearance for current `stamp`; stale, snapshot or selection errors reject
    /// capture.
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

    /// Returns immutable appearance intent binding validated `snapshot` and `scope` to current `stamp` and selection.
    /// Stale, snapshot-admission, appearance-validation and target errors reject issuance without mutation.
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

    /// Returns resolved ownership for `request` after rechecking stamp, admission, snapshot and target uniqueness.
    /// Any stale, blocked, invalid or ambiguous request returns an error without mutation.
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

    /// Returns a disposable configuration and affected/skipped counts for checked `request`.
    /// Admission/target errors abort; the candidate is not a validation proof and is never published here.
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

    /// Returns affected/skipped counts and first shared validation error for `request` without publishing state.
    /// Invalid candidate content is reported inside the preview; request-admission failures return `Err`.
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

    /// Applies `edit` to a disposable candidate and returns its accepted revision (or unchanged revision for equality).
    /// Ordinary validation failure retains an inspectable rejected candidate and advances action identity while
    /// preserving accepted state/history. Preflight, capacity and counter failures do not publish that candidate.
    /// Appearance application is atomic on failure. Option changes cannot silently discard unrelated raw repairs.
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
                | Edit::SetBle(_)
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
            Edit::SetBle(value) => config.ble = value,
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
                    .push(default_widget_for_channel(channel, id, view, placement))
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
                let defaults = default_widget_for_channel(
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
    /// Returns the result of `edit` only if `token` still binds current selection/revision; otherwise stale without
    /// mutation.
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

    /// Changes to valid candidate `selection` without a history entry; returns current or advanced revision.
    /// JSON/startup, non-placement raw repair, missing/mismatched targets and exhausted counters reject navigation.
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

    /// Restores one accepted project from undo and returns its new revision, retiring pending operations.
    /// Draft, empty-history, codec and counter errors preserve both histories; UI additionally uses history readiness.
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

    /// Restores one accepted project from redo and returns its new revision, retiring pending operations.
    /// Draft, empty-history, codec and counter errors preserve both histories; UI additionally uses history readiness.
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

    /// Returns the next action revision without reserving it, or exhaustion instead of wrapping.
    fn next_revision(&self) -> Result<Revision, EditorError> {
        self.revision
            .0
            .checked_add(1)
            .map(Revision)
            .ok_or(EditorError::CounterExhausted)
    }

    /// Returns the next asynchronous request identity without reserving it, or exhaustion instead of wrapping.
    fn next_request(&self) -> Result<u64, EditorError> {
        self.request.checked_add(1).ok_or(EditorError::CounterExhausted)
    }

    /// Publishes `preview` with valid `selection`, retaining startup/artwork; returns current revision for equality
    /// or a new revision with one history entry. Selection/codec/counter errors precede mutation.
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

    /// Installs reserved `revision` and retires JSON/startup drafts, pointers, imports and save requests/errors.
    /// Caller owns accepted-content/history/raw-draft updates; persisted-revision evidence is retained.
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

    /// Returns success without pending JSON/startup or invalid visual/raw drafts; does not gate gestures/imports.
    fn ensure_clean(&self) -> Result<(), EditorError> {
        self.ensure_no_json()?;
        if self.has_invalid_draft() {
            Err(EditorError::InvalidDraft)
        } else {
            Ok(())
        }
    }

    /// Returns success without pending JSON or startup timing, otherwise the first corresponding draft error.
    fn ensure_no_json(&self) -> Result<(), EditorError> {
        if self.has_pending_json() {
            Err(EditorError::JsonPending)
        } else if self.has_startup_draft() {
            Err(EditorError::StartupPending)
        } else {
            Ok(())
        }
    }

    /// Returns success only when action readiness permits a new snapshot, mapping its first blocker to an error.
    fn ensure_snapshot_ready(&self) -> Result<(), EditorError> {
        self.action_readiness().block.map_or(Ok(()), |block| Err(block.into()))
    }
}

/// Returns whether `before` and `after` encode identically, preserving distinctions such as signed zero.
/// Shared binary codec failure returns an error; no editor state is changed.
fn same_canonical_config(
    before: &ValidatedConfig,
    after: &ValidatedConfig,
) -> Result<bool, EditorError> {
    let mut before_bytes = vec![0; MAX_FILE_BYTES];
    let mut after_bytes = vec![0; MAX_FILE_BYTES];
    Ok(encode(before, &mut before_bytes).map_err(EditorError::Codec)?
        == encode(after, &mut after_bytes).map_err(EditorError::Codec)?)
}

/// Returns the sole index of view `id` in `config`; missing and duplicate IDs produce distinct errors.
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

/// Returns the sole index of widget `id` in `config`; missing and duplicate IDs produce distinct errors.
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

/// Returns a bounded codec diagnostic retaining `error`'s line/column without embedding source text.
fn json_error(error: serde_json::Error) -> EditorError {
    EditorError::Codec(ConfigError::Json {
        line: error.line(),
        column: error.column(),
    })
}

/// Appends accepted `value` to `history`, evicting its oldest entry at [`HISTORY_LIMIT`].
/// Caller maintains the invariant that history never already exceeds the limit; artwork remains shared.
fn push_history(
    history: &mut VecDeque<Box<Accepted>>,
    value: Box<Accepted>,
) {
    if history.len() == HISTORY_LIMIT {
        history.pop_front();
    }
    history.push_back(value);
}

/// Returns owned UTF-8 `text` within `N` bytes, or text-capacity error without truncation.
fn bounded<const N: usize>(text: &str) -> Result<heapless::String<N>, EditorError> {
    text.try_into().map_err(|_| EditorError::TextCapacity)
}

/// Returns the unique view `id` index in `config`, rejecting missing or duplicate IDs.
fn view_index(
    config: &Config,
    id: u8,
) -> Result<usize, EditorError> {
    unique_view_index(config, id)
}

/// Returns the unique widget `id` index in `config`, rejecting missing or duplicate IDs.
fn widget_index(
    config: &Config,
    id: u16,
) -> Result<usize, EditorError> {
    unique_widget_index(config, id)
}

/// Returns success if `selection` resolves uniquely in `config` and any widget belongs to its view.
/// Missing/duplicate IDs and cross-view widget ownership are errors.
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

/// Returns the first unused positive view ID in bounded `config`, or capacity error when no slot/ID remains.
fn fresh_view(config: &Config) -> Result<u8, EditorError> {
    if config.views.len() == MAX_VIEWS {
        return Err(EditorError::ViewCapacity);
    }
    (1..=MAX_VIEWS as u8 + 1)
        .find(|id| config.views.iter().all(|view| view.id != *id))
        .ok_or(EditorError::ViewCapacity)
}

/// Returns the first unused positive widget ID in bounded `config`, or capacity error when no slot/ID remains.
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
    //! Atomicity and stale-capability regressions at exhausted counters and retained repairs.
    use super::*;

    /// A cached resource result cannot bypass newly exhausted request issuance at the same action stamp.
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

    /// Appearance admission failure preserves an already-issued save and every editor state component.
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

    /// Exhausted located editing cannot publish a raw field or mutate accepted/history state.
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

    /// The final issued save can complete after further issuance becomes impossible.
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

    /// Retained-raw authority rechecks identity, index, unique ownership and applicability before mutation.
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

    /// JSON counter exhaustion preserves text, identities, history and existing callback ownership.
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

    /// History command readiness rejects exhausted revision and request counters independently.
    #[test]
    fn history_commands_keep_revision_and_request_exhaustion_blocked() {
        use crate::workspace::Command;
        for request_exhausted in [false, true] {
            let mut editor = Editor::default();
            editor.apply(Edit::SetReducedMotion(true)).unwrap();
            editor.apply(Edit::SetReducedMotion(false)).unwrap();
            editor.undo().unwrap();
            if request_exhausted {
                editor.request = u64::MAX;
            } else {
                editor.revision = Revision(u64::MAX);
            }
            assert_eq!(editor.history_readiness().block, Some(ActionBlock::IdentityExhausted));
            for command in [Command::Undo, Command::Redo] {
                assert_eq!(
                    command.availability(&editor, editor.stamp()),
                    Some("Editor identity exhausted; no new action can be accepted")
                );
            }
        }
    }

    /// Apply/undo/redo preflight counter capacity before changing accepted configuration or either history.
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

    /// Band deletion preserves the full failed transaction and remaps retained repair fields only on admitted edits.
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
