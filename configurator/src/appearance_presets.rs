//! Positive appearance allowlist and bounded browser presets. No firmware schema.
use std::sync::atomic::{AtomicUsize, Ordering};

use serde::{Deserialize, Serialize};

use crate::config::*;
use crate::editor::{EditorError, EditorStamp, InspectionTarget, StorageFailure};
use crate::field_registry::{FieldKey, Registry};

/// Maximum named presets in one browser collection.
pub const MAX_PRESETS: usize = 16;
/// Maximum UTF-8 bytes accepted or emitted for the complete versioned preset file.
pub const MAX_PRESET_BYTES: usize = 32 * 1024;
/// Versioned localStorage key; independent of project persistence.
pub const PRESET_STORAGE_KEY: &str = "ea888-view-studio-appearance-v1";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// Portable appearance allowlist; excludes placement, units, thresholds, sources and acquisition settings.
pub struct AppearanceSnapshot {
    /// RGB565 role palette, subject to shared contrast validation.
    pub colors: WidgetColors,
    /// Gauge presentation style; may change destination capability availability.
    pub gauge_style: GaugeKind,
    /// Decimal precision, admitted only in `0..=3`.
    pub decimals: u8,
    /// Desired graph visibility, copied only where destination supports it.
    pub graph: bool,
    /// Desired average visibility, copied only where destination supports it.
    pub average: bool,
    /// Desired minimum/maximum visibility, copied only where destination supports it.
    pub extrema: bool,
    /// Desired trend visibility, copied only where destination supports it.
    pub trend: bool,
}
impl AppearanceSnapshot {
    /// Returns the appearance allowlist from `widget` without validating or capturing destination authority.
    pub fn from_widget(widget: &Widget) -> Self {
        Self {
            colors: widget.colors,
            gauge_style: widget.gauge.kind,
            decimals: widget.display.decimals,
            graph: widget.display.graph,
            average: widget.display.average,
            extrema: widget.display.extrema,
            trend: widget.display.trend,
        }
    }

    /// Returns success for shared-valid contrast and precision through three decimals; otherwise the precise field
    /// error. Destination compatibility still requires editor validation after applying the snapshot.
    pub fn validate(self) -> Result<(), EditorError> {
        if let Some(role) = self.colors.invalid_role() {
            return Err(EditorError::Validation(ValidationError {
                code: ErrorCode::Contrast,
                field: Field::Color(role),
                view: None,
                widget: None,
                channel: None,
            }));
        }
        if self.decimals > 3 {
            return Err(EditorError::Validation(ValidationError {
                code: ErrorCode::Bounds,
                field: Field::Decimals,
                view: None,
                widget: None,
                channel: None,
            }));
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Destination set captured by a checked editor appearance request.
pub enum AppearanceScope {
    /// Only the currently selected widget.
    Selected,
    /// Every widget belonging to the selected view.
    ThisView,
    /// Every widget across all views.
    AllWidgets,
}
impl AppearanceScope {
    /// Stable UI order: selected widget, selected view, entire dashboard.
    pub const ALL: [Self; 3] = [Self::Selected, Self::ThisView, Self::AllWidgets];

    /// Returns the scope's user-facing destination label.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Selected => "Selected widget",
            Self::ThisView => "This view",
            Self::AllWidgets => "All widgets",
        }
    }

    /// Returns the stable DOM ID of this scope's preview-opening control for focus restoration.
    pub const fn preview_control_id(self) -> &'static str {
        match self {
            Self::Selected => "studio-appearance-open-selected",
            Self::ThisView => "studio-appearance-open-view",
            Self::AllWidgets => "studio-appearance-open-all",
        }
    }
}
/// Minted by Editor from clean accepted destinations; callers cannot retarget it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AppearanceRequest {
    /// Accepted editor instance/action identity at capture.
    pub(crate) stamp: EditorStamp,
    /// Indexed destination authority bound to the candidate epoch.
    pub(crate) target: InspectionTarget,
    /// Destination expansion selected at capture.
    pub(crate) scope: AppearanceScope,
    /// Validated portable values; does not confer authority to a different target.
    pub(crate) snapshot: AppearanceSnapshot,
}
impl AppearanceRequest {
    /// Returns captured portable values; editor identity is retained in the request.
    pub fn snapshot(self) -> AppearanceSnapshot { self.snapshot }

    /// Returns the captured destination scope.
    pub fn scope(self) -> AppearanceScope { self.scope }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
/// Read-only application summary; never a configuration proof or publication identity.
pub struct AppearancePreview {
    /// Number of visited destination widgets, including unchanged widgets.
    pub target_count: usize,
    /// Total unsupported target/field pairs whose old preference was retained.
    pub omitted_fields: usize,
    /// Graph, average, extrema, trend: exact unsupported target-field counts.
    pub omissions: [usize; 4],
    /// Whether at least one destination appearance differs after allowed copies.
    pub changed: bool,
    /// First full candidate validation error, or `None`; absence does not mint acceptance authority.
    pub error: Option<EditorError>,
}
/// Shared by preview and apply. Capability truth is the registry's actual
/// predicate on the resulting style/span; unsupported preferences are retained.
/// Applies `snapshot` to candidate `widget`, accumulating counts/change flags in `preview`.
/// Caller validates the snapshot and complete configuration; no failure is returned by this field copier.
pub(crate) fn apply_snapshot(
    widget: &mut Widget,
    snapshot: AppearanceSnapshot,
    preview: &mut AppearancePreview,
) {
    let before = AppearanceSnapshot::from_widget(widget);
    widget.colors = snapshot.colors;
    widget.gauge.kind = snapshot.gauge_style;
    widget.display.decimals = snapshot.decimals;
    let fields = [FieldKey::Graph, FieldKey::Average, FieldKey::Extrema, FieldKey::Trend];
    let values = [snapshot.graph, snapshot.average, snapshot.extrema, snapshot.trend];
    for (i, (key, value)) in fields.into_iter().zip(values).enumerate() {
        if Registry::widget_capability(key, widget).reason().is_none() {
            match i {
                0 => widget.display.graph = value,
                1 => widget.display.average = value,
                2 => widget.display.extrema = value,
                _ => widget.display.trend = value,
            }
        } else {
            preview.omissions[i] += 1;
            preview.omitted_fields += 1;
        }
    }
    preview.target_count += 1;
    preview.changed |= before != AppearanceSnapshot::from_widget(widget);
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Built-in RGB565 palette overrides for the selected appearance source.
pub enum ColorPalette {
    /// Shared default widget palette.
    Default,
    /// White content on black with grayscale tracks/borders.
    Monochrome,
    /// Red graph/gauge/peak accents and shared default warning colors.
    RedAccent,
}
impl ColorPalette {
    /// Stable palette-selector ordering.
    pub const ALL: [Self; 3] = [Self::Default, Self::Monochrome, Self::RedAccent];

    /// Returns the palette's user-facing name.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Default => "Default",
            Self::Monochrome => "High-contrast monochrome",
            Self::RedAccent => "Red accent",
        }
    }

    /// Returns a complete role palette; this does not alter existing widget state.
    pub fn colors(self) -> WidgetColors {
        if self == Self::Default {
            return WidgetColors::default();
        }
        let white = Color(0xFFFF);
        let red = Color::rgb(255, 48, 72);
        let mut colors = WidgetColors {
            background: Color(0),
            border: Color(0x4208),
            label: white,
            value: white,
            secondary: white,
            graph: white,
            gauge: white,
            track: Color(0x4208),
            ticks: white,
            average: white,
            minimum: white,
            maximum: white,
            trend: white,
            peak: white,
            warning: WarningColors {
                foreground: Color(0),
                background: white,
            },
        };
        if self == Self::RedAccent {
            colors.graph = red;
            colors.gauge = red;
            colors.peak = red;
            colors.track = Color::rgb(44, 48, 56);
            colors.ticks = Color::rgb(156, 168, 184);
            colors.warning = WarningColors::default();
        }
        colors
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Portable value source used when producing a new appearance offer.
pub enum AppearanceSource {
    /// Read current selected widget when the offer is created.
    Selected,
    /// Use the session clipboard, if a snapshot has been copied.
    Copied,
    /// Use explicitly selected stored values.
    Preset(
        /// Portable preset values, independent of the collection's later mutations.
        AppearanceSnapshot,
    ),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Immutable UI offer combining portable values with optional checked destination requests.
pub struct AppearanceOffer {
    /// Appearance-session choice generation, independent of editor revision.
    revision: u64,
    /// Effective source after palette override, or `None` when unavailable.
    snapshot: Option<AppearanceSnapshot>,
    /// Selected-widget copy authority, without the optional palette override.
    copy: Option<AppearanceRequest>,
    /// Selected/view/all requests; blocked destinations retain `None` independently.
    requests: [Option<AppearanceRequest>; 3],
}
impl AppearanceOffer {
    /// Returns effective portable values, or `None` for an unavailable source.
    pub fn snapshot(self) -> Option<AppearanceSnapshot> { self.snapshot }

    /// Returns captured selected-widget copy authority, or `None` when editor admission failed.
    pub fn copy_request(self) -> Option<AppearanceRequest> { self.copy }

    /// Returns captured authority for `scope`, or `None` when that destination could not be admitted.
    pub fn request(
        self,
        scope: AppearanceScope,
    ) -> Option<AppearanceRequest> {
        self.requests[match scope {
            AppearanceScope::Selected => 0,
            AppearanceScope::ThisView => 1,
            AppearanceScope::AllWidgets => 2,
        }]
    }
}
/// Persistent Studio state. Choices invalidate old UI events and confirmation;
/// clipboard values remain portable while destination authority is captured in
/// the offer and validated at activation without minting replacement authority.
pub struct AppearanceSession {
    /// Source selected for subsequently produced offers.
    source: AppearanceSource,
    /// Optional complete palette override, leaving other snapshot values unchanged.
    palette: Option<ColorPalette>,
    /// Last explicitly copied portable values; not an editor capability.
    copied: Option<AppearanceSnapshot>,
    /// Currently staged preview request, cleared by choice/copy/cancel/close retirement.
    request: Option<AppearanceRequest>,
    /// Checked generation invalidating old UI offers without wrapping.
    revision: u64,
}
impl Default for AppearanceSession {
    /// Returns an empty session sourcing the selected widget with no palette override or preview.
    fn default() -> Self {
        Self {
            source: AppearanceSource::Selected,
            palette: None,
            copied: None,
            request: None,
            revision: 0,
        }
    }
}
impl AppearanceSession {
    /// Returns the source currently selected for new offers.
    pub fn source(&self) -> AppearanceSource { self.source }

    /// Returns the selected palette override, or `None` to retain source colors.
    pub fn palette(&self) -> Option<ColorPalette> { self.palette }

    /// Returns the staged preview request, or `None`; callers must still recheck editor authority.
    pub fn request(&self) -> Option<AppearanceRequest> { self.request }

    /// Returns whether `offer` matches this session's choice generation; does not validate editor state.
    pub fn is_current(
        &self,
        offer: AppearanceOffer,
    ) -> bool {
        self.revision == offer.revision
    }

    /// Returns source values and captured destination requests using `editor` at `stamp`.
    /// Individual stale/blocked/invalid captures become `None`; no replacement authority is minted on activation.
    pub fn offer(
        &self,
        editor: &crate::editor::Editor,
        stamp: EditorStamp,
    ) -> AppearanceOffer {
        let selected = editor.selected_appearance(stamp).ok();
        let mut snapshot = match self.source {
            AppearanceSource::Selected => selected,
            AppearanceSource::Copied => self.copied,
            AppearanceSource::Preset(snapshot) => Some(snapshot),
        };
        if let Some(snapshot) = &mut snapshot
            && let Some(palette) = self.palette
        {
            snapshot.colors = palette.colors();
        }
        AppearanceOffer {
            revision: self.revision,
            snapshot,
            copy: selected.and_then(|snapshot| {
                editor
                    .appearance_request(AppearanceScope::Selected, snapshot, stamp)
                    .ok()
            }),
            requests: AppearanceScope::ALL
                .map(|scope| snapshot.and_then(|snapshot| editor.appearance_request(scope, snapshot, stamp).ok())),
        }
    }

    /// Advances choice identity and clears preview; returns exhaustion without mutation instead of wrapping.
    fn retire(&mut self) -> Result<(), PresetError> {
        self.revision = self.revision.checked_add(1).ok_or(PresetError::Exhausted)?;
        self.request = None;
        Ok(())
    }

    /// Selects `source` and optional `palette`, retiring old offers/preview; returns exhaustion without changing
    /// choices.
    pub fn choose(
        &mut self,
        source: AppearanceSource,
        palette: Option<ColorPalette>,
    ) -> Result<(), PresetError> {
        self.retire()?;
        self.source = source;
        self.palette = palette;
        Ok(())
    }

    /// Validates and stores portable `snapshot`, retiring old offers/preview without changing source choice.
    /// Returns validation/exhaustion errors without changing the clipboard.
    pub fn copy(
        &mut self,
        snapshot: AppearanceSnapshot,
    ) -> Result<(), PresetError> {
        snapshot.validate().map_err(PresetError::Snapshot)?;
        self.retire()?;
        self.copied = Some(snapshot);
        Ok(())
    }

    /// Stages captured `request` only if current `offer` contains that exact scope request.
    /// Returns stale error without replacing preview; editor admission is rechecked separately by the caller.
    pub fn stage(
        &mut self,
        request: AppearanceRequest,
        offer: AppearanceOffer,
    ) -> Result<(), PresetError> {
        if !self.is_current(offer) || offer.request(request.scope()) != Some(request) {
            return Err(PresetError::Stale);
        }
        self.request = Some(request);
        Ok(())
    }

    /// Retires all offers and staged preview; returns exhaustion without changing session state.
    pub fn cancel(&mut self) -> Result<(), PresetError> { self.retire() }

    /// Closes current `request` under current `offer` and returns its scope for focus restoration.
    /// Stale/exhaustion errors preserve session state.
    pub fn close_preview(
        &mut self,
        request: AppearanceRequest,
        offer: AppearanceOffer,
    ) -> Result<AppearanceScope, PresetError> {
        if !self.is_current(offer) || self.request != Some(request) {
            return Err(PresetError::Stale);
        }
        self.retire()?;
        Ok(request.scope())
    }

    /// Returns whether `offer` is still current and no preview has replaced the closed one.
    pub fn can_return_focus(
        &self,
        offer: AppearanceOffer,
    ) -> bool {
        self.is_current(offer) && self.request.is_none()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// Named portable appearance values; schema rejects unknown fields.
pub struct PresetRecord {
    /// Exact case-sensitive name: 1–32 printable ASCII bytes, not entirely spaces.
    name: heapless::String<32>,
    /// Contrast/precision-validated values when admitted through the store.
    pub snapshot: AppearanceSnapshot,
}
impl PresetRecord {
    /// Returns the exact stored name without trimming or normalization.
    pub fn name(&self) -> &str { &self.name }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// Versioned, bounded JSON envelope separate from dashboard/project formats.
struct PresetFile {
    /// Only version 1 is admitted.
    version: u16,
    /// At most 16 records, requiring unique exact names on admission.
    records: heapless::Vec<PresetRecord, MAX_PRESETS>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Preset admission, persistence or stale-authority failure.
pub enum PresetError {
    /// Serialized/input collection exceeds 32 KiB.
    TooLarge,
    /// JSON/schema or serialization failed.
    InvalidData,
    /// Envelope version is unsupported.
    Version,
    /// Name violates bounded printable-ASCII/nonblank rules.
    Name,
    /// Exact name already exists; replacement must be explicit.
    DuplicateName,
    /// All 16 named slots are occupied.
    Capacity,
    /// Portable colors or precision failed validation.
    Snapshot(
        /// Underlying shared field diagnostic.
        EditorError,
    ),
    /// Browser storage operation failed.
    Storage(
        /// Classified unavailable/quota failure.
        StorageFailure,
    ),
    /// Persisted observation differs or is unavailable; explicit replacement/reload is required.
    Conflict,
    /// Captured owner/version/request or offer no longer matches.
    Stale,
    /// Owner, content revision, request or session generation cannot advance without wrapping.
    Exhausted,
    /// Requested record index is absent.
    Index,
}
impl PresetError {
    /// Returns actionable bounded UI text without exposing stored content.
    pub fn message(self) -> &'static str {
        match self {
            Self::TooLarge => "Preset records are limited to 32 KiB",
            Self::InvalidData => "Preset data is invalid; previous presets retained",
            Self::Version => "Preset version is unavailable",
            Self::Name => "Names need 1–32 printable ASCII bytes and a non-space character",
            Self::DuplicateName => "That exact preset name already exists; use explicit Replace",
            Self::Capacity => "At most 16 named presets can be stored",
            Self::Snapshot(_) => "Preset colors or precision are invalid",
            Self::Storage(StorageFailure::Quota) => {
                "Preset storage is full; memory presets remain usable. Export a backup"
            }
            Self::Storage(_) => "Preset storage is unavailable; memory presets remain usable. Export a backup",
            Self::Conflict => "Stored presets changed; export, reload or explicitly replace them",
            Self::Stale => "Presets changed; refresh this action",
            Self::Exhausted => "Preset identity exhausted; export and reopen Studio",
            Self::Index => "This preset is unavailable",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Persistence status of the preset collection, independent of project save status.
pub enum PresetStatus {
    /// No stored collection was observed.
    Empty,
    /// Current in-memory collection matches an acknowledged storage observation.
    Saved,
    /// In-memory collection changed since its storage observation.
    Dirty,
    /// A write capability is outstanding.
    Saving,
    /// External storage changed; in-memory records remain usable.
    Conflict,
    /// Storage read/write failed; in-memory records remain usable.
    Failed(
        /// Classified browser failure.
        StorageFailure,
    ),
    /// Stored data or identity cannot be admitted.
    Unavailable(
        /// Admission/exhaustion diagnostic.
        PresetError,
    ),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Owner and checked collection revision captured by record actions.
pub struct PresetVersion {
    /// Globally issued owner; zero is the permanently exhausted sentinel.
    owner: usize,
    /// Advances on admitted in-memory collection publication, never wraps.
    revision: u64,
}
#[derive(Debug)]
/// Frozen compare-before-write request; browser adapter serializes access with Web Locks.
pub struct PresetSaveRequest {
    /// Exact collection owner/content revision at preparation.
    version: PresetVersion,
    /// Checked issuance number identifying the sole active write.
    request: u64,
    /// Versioned bounded JSON to persist after authority and observation recheck.
    bytes: String,
    /// Exact prior storage contents, or `None` for expected absence.
    observed: Option<String>,
}
impl PresetSaveRequest {
    /// Returns exact prepared JSON; this borrow does not authorize an unchecked write.
    pub fn json(&self) -> &str { &self.bytes }
}
/// Monotonic process-local owner issuer; exhaustion produces unavailable stores rather than reusing IDs.
static NEXT_PRESET_OWNER: AtomicUsize = AtomicUsize::new(1);
/// Separate browser persistence owner; no Editor identity or save status here.
pub struct PresetStore {
    /// Current admitted in-memory collection; preserved on failed import/reload/write.
    records: heapless::Vec<PresetRecord, MAX_PRESETS>,
    /// Owner and current content revision.
    version: PresetVersion,
    /// Last issued asynchronous write ID.
    request: u64,
    /// Only request currently allowed to finish a write, or `None`.
    active: Option<u64>,
    /// Last trusted storage text, or absence; meaningful only when observation is available.
    observed: Option<String>,
    /// Prevents ordinary save after unavailable/invalid/oversized observation.
    unavailable_observation: bool,
    /// External change latch, cleared only by admitted reload or successful explicit write.
    conflict: bool,
    /// Display status, distinct from whether an old request is still active.
    status: PresetStatus,
}
/// Returns exact bounded `text` as a name; rejects nonprintable/non-ASCII, empty or all-space names.
fn name(text: &str) -> Result<heapless::String<32>, PresetError> {
    if text.is_empty()
        || text.len() > 32
        || !text.bytes().all(|b| (32..=126).contains(&b))
        || text.bytes().all(|b| b == b' ')
    {
        return Err(PresetError::Name);
    }
    text.try_into().map_err(|_| PresetError::Name)
}
/// Returns the complete collection from bounded `text` only after version/name/uniqueness/snapshot validation.
/// Any malformed, oversized or invalid record rejects the entire import.
fn decode(text: &str) -> Result<heapless::Vec<PresetRecord, MAX_PRESETS>, PresetError> {
    if text.len() > MAX_PRESET_BYTES {
        return Err(PresetError::TooLarge);
    }
    let file: PresetFile = serde_json::from_str(text).map_err(|_| PresetError::InvalidData)?;
    if file.version != 1 {
        return Err(PresetError::Version);
    }
    for (i, record) in file.records.iter().enumerate() {
        name(&record.name)?;
        record.snapshot.validate().map_err(PresetError::Snapshot)?;
        if file.records[..i].iter().any(|other| other.name == record.name) {
            return Err(PresetError::DuplicateName);
        }
    }
    Ok(file.records)
}
/// Returns version-1 bounded JSON for already-admitted `records`, or serialization/size error.
fn encode(records: &heapless::Vec<PresetRecord, MAX_PRESETS>) -> Result<String, PresetError> {
    let text = serde_json::to_string(&PresetFile {
        version: 1,
        records: records.clone(),
    })
    .map_err(|_| PresetError::InvalidData)?;
    if text.len() > MAX_PRESET_BYTES {
        return Err(PresetError::TooLarge);
    }
    Ok(text)
}
impl PresetStore {
    /// Returns a fresh owner initialized from storage `observation` (`None` means absent).
    /// Invalid/unavailable observations produce an empty usable collection with ordinary saves blocked;
    /// owner exhaustion produces owner zero and unavailable status instead of panicking.
    pub fn restore(observation: Result<Option<String>, StorageFailure>) -> Self {
        let owner = NEXT_PRESET_OWNER
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |v| v.checked_add(1))
            .unwrap_or(0);
        let mut store = Self {
            records: heapless::Vec::new(),
            version: PresetVersion { owner, revision: 0 },
            request: 0,
            active: None,
            observed: None,
            unavailable_observation: false,
            conflict: false,
            status: PresetStatus::Empty,
        };
        match observation {
            Ok(Some(text)) => {
                match decode(&text) {
                    Ok(records) => {
                        store.records = records;
                        store.status = PresetStatus::Saved;
                    }
                    Err(error) => {
                        store.status = PresetStatus::Unavailable(error);
                        store.unavailable_observation = true;
                    }
                }
                if text.len() <= MAX_PRESET_BYTES {
                    store.observed = Some(text);
                }
            }
            Ok(None) => {}
            Err(failure) => {
                store.status = PresetStatus::Failed(failure);
                store.unavailable_observation = true;
            }
        }
        if owner == 0 {
            store.status = PresetStatus::Unavailable(PresetError::Exhausted);
        }
        store
    }

    /// Returns admitted in-memory records, including unsaved changes after storage failure/conflict.
    pub fn records(&self) -> &[PresetRecord] { &self.records }

    /// Returns owner/content identity to capture with indexed actions.
    pub fn version(&self) -> PresetVersion { self.version }

    /// Returns collection persistence status; no browser storage read is performed.
    pub fn status(&self) -> PresetStatus { self.status }

    /// Returns bounded versioned JSON for in-memory records, or encoding failure, even after persistence conflict.
    pub fn export(&self) -> Result<String, PresetError> { encode(&self.records) }

    /// Returns record `index` only for matching `version`; exhausted owner, stale identity and missing index are
    /// errors.
    pub fn record(
        &self,
        version: PresetVersion,
        index: usize,
    ) -> Result<&PresetRecord, PresetError> {
        if self.version.owner == 0 {
            return Err(PresetError::Exhausted);
        }
        if version != self.version {
            return Err(PresetError::Stale);
        }
        self.records.get(index).ok_or(PresetError::Index)
    }

    /// Replaces the collection with already-admitted `records` after bounded encoding and revision preflight.
    /// Returns encoding/exhaustion errors without mutation; success retires active writes and preserves conflict
    /// status.
    fn publish(
        &mut self,
        records: heapless::Vec<PresetRecord, MAX_PRESETS>,
    ) -> Result<(), PresetError> {
        encode(&records)?;
        if self.version.owner == 0 {
            return Err(PresetError::Exhausted);
        }
        let revision = self.version.revision.checked_add(1).ok_or(PresetError::Exhausted)?;
        self.records = records;
        self.version.revision = revision;
        self.active = None;
        self.status = if self.conflict {
            PresetStatus::Conflict
        } else {
            PresetStatus::Dirty
        };
        Ok(())
    }

    /// Adds valid portable `snapshot` under exact `text` name and marks memory dirty/conflicted.
    /// Returns name, duplicate, snapshot, capacity or encoding/exhaustion error without changing records.
    pub fn add(
        &mut self,
        text: &str,
        snapshot: AppearanceSnapshot,
    ) -> Result<(), PresetError> {
        let name = name(text)?;
        snapshot.validate().map_err(PresetError::Snapshot)?;
        if self.records.iter().any(|r| r.name == name) {
            return Err(PresetError::DuplicateName);
        }
        let mut records = self.records.clone();
        records
            .push(PresetRecord { name, snapshot })
            .map_err(|_| PresetError::Capacity)?;
        self.publish(records)
    }

    /// Replaces only record `index`'s values with valid `snapshot` under current `version`, retaining its name.
    /// Returns stale/index/validation/encoding/exhaustion errors without changing records.
    pub fn replace(
        &mut self,
        version: PresetVersion,
        index: usize,
        snapshot: AppearanceSnapshot,
    ) -> Result<(), PresetError> {
        self.record(version, index)?;
        snapshot.validate().map_err(PresetError::Snapshot)?;
        let mut records = self.records.clone();
        records[index].snapshot = snapshot;
        self.publish(records)
    }

    /// Removes record `index` under current `version`; returns stale/index/encoding/exhaustion errors atomically.
    pub fn remove(
        &mut self,
        version: PresetVersion,
        index: usize,
    ) -> Result<(), PresetError> {
        self.record(version, index)?;
        let mut records = self.records.clone();
        records.remove(index);
        self.publish(records)
    }

    /// Replaces all in-memory records with bounded versioned `text`; returns admission/exhaustion errors atomically.
    /// Import does not overwrite browser storage or clear an external conflict.
    pub fn import(
        &mut self,
        text: &str,
    ) -> Result<(), PresetError> {
        self.publish(decode(text)?)
    }

    /// Compares actual storage `observation` with the trusted baseline, retaining in-memory records.
    /// Changed/unavailable observations retire writes and set conflict/failure; identical trusted observations do
    /// nothing.
    pub fn observe(
        &mut self,
        observation: Result<Option<String>, StorageFailure>,
    ) {
        match observation {
            Ok(current) if !self.unavailable_observation && current == self.observed => {}
            Ok(_) => {
                self.active = None;
                self.conflict = true;
                self.status = PresetStatus::Conflict;
            }
            Err(failure) => {
                self.active = None;
                self.observed = None;
                self.unavailable_observation = true;
                self.status = PresetStatus::Failed(failure);
            }
        }
    }

    /// Explicitly replaces memory from actual storage `observation` (`None` clears the collection).
    /// Returns admission/storage/exhaustion errors while retaining records; storage/decode failures retire write
    /// authority. Success advances version, clears conflict and marks the observed collection saved/empty.
    pub fn reload(
        &mut self,
        observation: Result<Option<String>, StorageFailure>,
    ) -> Result<(), PresetError> {
        let current = match observation {
            Ok(current) => current,
            Err(failure) => {
                self.observe(Err(failure));
                return Err(PresetError::Storage(failure));
            }
        };
        let records = match &current {
            Some(text) => {
                match decode(text) {
                    Ok(records) => records,
                    Err(error) => {
                        self.failed_observation(error);
                        return Err(error);
                    }
                }
            }
            None => heapless::Vec::new(),
        };
        self.publish(records)?;
        self.observed = current;
        self.unavailable_observation = false;
        self.conflict = false;
        self.status = if self.observed.is_some() {
            PresetStatus::Saved
        } else {
            PresetStatus::Empty
        };
        Ok(())
    }

    /// Returns frozen write bytes and trusted compare baseline; conflict/unavailable observation blocks ordinary save.
    /// Encoding/exhaustion errors preserve issuance; preparation itself performs no browser I/O.
    pub fn begin_save(&mut self) -> Result<PresetSaveRequest, PresetError> {
        if self.conflict || self.unavailable_observation {
            return Err(PresetError::Conflict);
        }
        self.prepare(self.observed.clone())
    }

    /// Explicit replacement captures a fresh bounded observation, still compared
    /// inside the lock; this never grants an unconditional overwrite.
    /// `current` is the actual storage text or absence. Returns a prepared request, or size/encoding/exhaustion error;
    /// oversized observation retires authority and marks storage unavailable.
    pub fn begin_replace(
        &mut self,
        current: Option<&str>,
    ) -> Result<PresetSaveRequest, PresetError> {
        if current.is_some_and(|text| text.len() > MAX_PRESET_BYTES) {
            self.failed_observation(PresetError::TooLarge);
            return Err(PresetError::TooLarge);
        }
        self.prepare(current.map(str::to_owned))
    }

    /// Retires write authority and trusted baseline, recording admission `error` while retaining records.
    fn failed_observation(
        &mut self,
        error: PresetError,
    ) {
        self.active = None;
        self.observed = None;
        self.unavailable_observation = true;
        self.status = PresetStatus::Unavailable(error);
    }

    /// The browser passes the actual read result to this producer; a failure
    /// retires persistence authority instead of being only a transient notice.
    /// Returns a compare-before-write request from fresh `observation`, or storage/size/encoding/exhaustion error.
    pub fn begin_replace_observation(
        &mut self,
        observation: Result<Option<String>, StorageFailure>,
    ) -> Result<PresetSaveRequest, PresetError> {
        match observation {
            Ok(current) => self.begin_replace(current.as_deref()),
            Err(failure) => {
                self.observe(Err(failure));
                Err(PresetError::Storage(failure))
            }
        }
    }

    /// Returns a new frozen write with expected `observed` storage baseline; successful issuance replaces prior writes.
    /// Encoding/owner/request exhaustion leaves state unchanged. Caller must have admitted the baseline's bound.
    fn prepare(
        &mut self,
        observed: Option<String>,
    ) -> Result<PresetSaveRequest, PresetError> {
        let bytes = self.export()?;
        if self.version.owner == 0 {
            return Err(PresetError::Exhausted);
        }
        let request = self.request.checked_add(1).ok_or(PresetError::Exhausted)?;
        self.request = request;
        self.active = Some(request);
        self.status = PresetStatus::Saving;
        Ok(PresetSaveRequest {
            version: self.version,
            request,
            bytes,
            observed,
        })
    }

    /// Returns whether `request` matches both collection version and the sole active issuance number.
    pub fn save_is_current(
        &self,
        request: &PresetSaveRequest,
    ) -> bool {
        self.version == request.version && self.active == Some(request.request)
    }

    /// Returns success only for current `request` and exact `current` storage text/absence matching its baseline.
    /// Stale/conflict errors prohibit writing; caller must hold the browser lock across read, check and write.
    pub fn check_write(
        &self,
        request: &PresetSaveRequest,
        current: Option<&str>,
    ) -> Result<(), PresetError> {
        if !self.save_is_current(request) {
            return Err(PresetError::Stale);
        }
        if current != request.observed.as_deref() {
            return Err(PresetError::Conflict);
        }
        Ok(())
    }

    /// Records actual write `result` for current `request`, clearing its authority and retaining memory records.
    /// Returns stale error for obsolete completion; returns success even when a storage failure is recorded in status.
    /// Successful writes replace the trusted observation and clear conflict/unavailability.
    pub fn finish_save(
        &mut self,
        request: &PresetSaveRequest,
        result: Result<(), StorageFailure>,
    ) -> Result<(), PresetError> {
        if !self.save_is_current(request) {
            return Err(PresetError::Stale);
        }
        self.active = None;
        match result {
            Ok(()) => {
                self.observed = Some(request.bytes.clone());
                self.unavailable_observation = false;
                self.conflict = false;
                self.status = PresetStatus::Saved;
            }
            Err(failure) => self.status = PresetStatus::Failed(failure),
        }
        Ok(())
    }

    /// A rejected Web Locks promise owns the outcome only until its write
    /// callback has resolved the same request. A later promise rejection must
    /// not report the callback's outcome a second time.
    /// Returns whether current `request` was completed as unavailable; false means another outcome already retired it.
    pub fn reject_lock(
        &mut self,
        request: &PresetSaveRequest,
    ) -> bool {
        self.finish_save(request, Err(StorageFailure::Unavailable)).is_ok()
    }

    /// Retires write authority without rewriting status or records; browser adapters own timer/promise cancellation.
    pub fn cancel(&mut self) { self.active = None; }
}

#[cfg(test)]
mod exhaustion_tests {
    //! Checked-owner and counter boundaries must reject operations without partial publication.
    use super::*;
    /// Returns a portable snapshot from the reviewed default widget.
    fn snapshot() -> AppearanceSnapshot { AppearanceSnapshot::from_widget(&default_config().widgets[0]) }
    #[test]
    /// Sentinel owner zero cannot resolve records even when a caller presents the same sentinel version.
    fn exhausted_unissued_owner_cannot_resolve_a_record_from_another_unissued_owner() {
        let mut store = PresetStore::restore(Ok(None));
        store.add("Existing", snapshot()).unwrap();
        store.version.owner = 0;
        assert_eq!(store.record(store.version(), 0), Err(PresetError::Exhausted));
    }
    #[test]
    /// Collection/request/session exhaustion cannot wrap identities or overwrite records, clipboard or source choices.
    fn preset_revision_request_and_source_exhaustion_never_wrap_or_publish() {
        let mut store = PresetStore::restore(Ok(None));
        store.add("Existing", snapshot()).unwrap();
        store.version.revision = u64::MAX;
        let before = store.export().unwrap();
        assert_eq!(store.add("New", snapshot()), Err(PresetError::Exhausted));
        assert_eq!(store.export().unwrap(), before);
        store.request = u64::MAX;
        assert!(matches!(store.begin_save(), Err(PresetError::Exhausted)));
        assert_eq!(store.active, None);
        assert_eq!(store.status(), PresetStatus::Dirty);
        let mut session = AppearanceSession {
            revision: u64::MAX,
            ..AppearanceSession::default()
        };
        assert_eq!(
            session.choose(AppearanceSource::Copied, None),
            Err(PresetError::Exhausted)
        );
        assert_eq!(session.source(), AppearanceSource::Selected);
        assert_eq!(session.copy(snapshot()), Err(PresetError::Exhausted));
        assert_eq!(session.copied, None);
    }
}
