//! Positive appearance allowlist and bounded browser presets. No firmware schema.
use std::sync::atomic::{AtomicUsize, Ordering};

use serde::{Deserialize, Serialize};

use crate::config::*;
use crate::editor::{EditorError, EditorStamp, InspectionTarget, StorageFailure};
use crate::field_registry::{FieldKey, Registry};

pub const MAX_PRESETS: usize = 16;
pub const MAX_PRESET_BYTES: usize = 32 * 1024;
pub const PRESET_STORAGE_KEY: &str = "ea888-view-studio-appearance-v1";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppearanceSnapshot {
    pub colors: WidgetColors,
    pub gauge_style: GaugeKind,
    pub decimals: u8,
    pub graph: bool,
    pub average: bool,
    pub extrema: bool,
    pub trend: bool,
}
impl AppearanceSnapshot {
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
pub enum AppearanceScope {
    Selected,
    ThisView,
    AllWidgets,
}
impl AppearanceScope {
    pub const ALL: [Self; 3] = [Self::Selected, Self::ThisView, Self::AllWidgets];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Selected => "Selected widget",
            Self::ThisView => "This view",
            Self::AllWidgets => "All widgets",
        }
    }

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
    pub(crate) stamp: EditorStamp,
    pub(crate) target: InspectionTarget,
    pub(crate) scope: AppearanceScope,
    pub(crate) snapshot: AppearanceSnapshot,
}
impl AppearanceRequest {
    pub fn snapshot(self) -> AppearanceSnapshot { self.snapshot }

    pub fn scope(self) -> AppearanceScope { self.scope }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AppearancePreview {
    pub target_count: usize,
    pub omitted_fields: usize,
    /// Graph, average, extrema, trend: exact unsupported target-field counts.
    pub omissions: [usize; 4],
    pub changed: bool,
    pub error: Option<EditorError>,
}
/// Shared by preview and apply. Capability truth is the registry's actual
/// predicate on the resulting style/span; unsupported preferences are retained.
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
pub enum ColorPalette {
    Default,
    Monochrome,
    RedAccent,
}
impl ColorPalette {
    pub const ALL: [Self; 3] = [Self::Default, Self::Monochrome, Self::RedAccent];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Default => "Default",
            Self::Monochrome => "High-contrast monochrome",
            Self::RedAccent => "Red accent",
        }
    }

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
pub enum AppearanceSource {
    Selected,
    Copied,
    Preset(AppearanceSnapshot),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AppearanceOffer {
    revision: u64,
    snapshot: Option<AppearanceSnapshot>,
    copy: Option<AppearanceRequest>,
    requests: [Option<AppearanceRequest>; 3],
}
impl AppearanceOffer {
    pub fn snapshot(self) -> Option<AppearanceSnapshot> { self.snapshot }

    pub fn copy_request(self) -> Option<AppearanceRequest> { self.copy }

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
    source: AppearanceSource,
    palette: Option<ColorPalette>,
    copied: Option<AppearanceSnapshot>,
    request: Option<AppearanceRequest>,
    revision: u64,
}
impl Default for AppearanceSession {
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
    pub fn source(&self) -> AppearanceSource { self.source }

    pub fn palette(&self) -> Option<ColorPalette> { self.palette }

    pub fn request(&self) -> Option<AppearanceRequest> { self.request }

    pub fn is_current(
        &self,
        offer: AppearanceOffer,
    ) -> bool {
        self.revision == offer.revision
    }

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

    fn retire(&mut self) -> Result<(), PresetError> {
        self.revision = self.revision.checked_add(1).ok_or(PresetError::Exhausted)?;
        self.request = None;
        Ok(())
    }

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

    pub fn copy(
        &mut self,
        snapshot: AppearanceSnapshot,
    ) -> Result<(), PresetError> {
        snapshot.validate().map_err(PresetError::Snapshot)?;
        self.retire()?;
        self.copied = Some(snapshot);
        Ok(())
    }

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

    pub fn cancel(&mut self) -> Result<(), PresetError> { self.retire() }

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

    pub fn can_return_focus(
        &self,
        offer: AppearanceOffer,
    ) -> bool {
        self.is_current(offer) && self.request.is_none()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PresetRecord {
    name: heapless::String<32>,
    pub snapshot: AppearanceSnapshot,
}
impl PresetRecord {
    pub fn name(&self) -> &str { &self.name }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PresetFile {
    version: u16,
    records: heapless::Vec<PresetRecord, MAX_PRESETS>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PresetError {
    TooLarge,
    InvalidData,
    Version,
    Name,
    DuplicateName,
    Capacity,
    Snapshot(EditorError),
    Storage(StorageFailure),
    Conflict,
    Stale,
    Exhausted,
    Index,
}
impl PresetError {
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
pub enum PresetStatus {
    Empty,
    Saved,
    Dirty,
    Saving,
    Conflict,
    Failed(StorageFailure),
    Unavailable(PresetError),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PresetVersion {
    owner: usize,
    revision: u64,
}
#[derive(Debug)]
pub struct PresetSaveRequest {
    version: PresetVersion,
    request: u64,
    bytes: String,
    observed: Option<String>,
}
impl PresetSaveRequest {
    pub fn json(&self) -> &str { &self.bytes }
}
static NEXT_PRESET_OWNER: AtomicUsize = AtomicUsize::new(1);
/// Separate browser persistence owner; no Editor identity or save status here.
pub struct PresetStore {
    records: heapless::Vec<PresetRecord, MAX_PRESETS>,
    version: PresetVersion,
    request: u64,
    active: Option<u64>,
    observed: Option<String>,
    unavailable_observation: bool,
    conflict: bool,
    status: PresetStatus,
}
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

    pub fn records(&self) -> &[PresetRecord] { &self.records }

    pub fn version(&self) -> PresetVersion { self.version }

    pub fn status(&self) -> PresetStatus { self.status }

    pub fn export(&self) -> Result<String, PresetError> { encode(&self.records) }

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

    pub fn import(
        &mut self,
        text: &str,
    ) -> Result<(), PresetError> {
        self.publish(decode(text)?)
    }

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

    pub fn begin_save(&mut self) -> Result<PresetSaveRequest, PresetError> {
        if self.conflict || self.unavailable_observation {
            return Err(PresetError::Conflict);
        }
        self.prepare(self.observed.clone())
    }

    /// Explicit replacement captures a fresh bounded observation, still compared
    /// inside the lock; this never grants an unconditional overwrite.
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

    pub fn save_is_current(
        &self,
        request: &PresetSaveRequest,
    ) -> bool {
        self.version == request.version && self.active == Some(request.request)
    }

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
    pub fn reject_lock(
        &mut self,
        request: &PresetSaveRequest,
    ) -> bool {
        self.finish_save(request, Err(StorageFailure::Unavailable)).is_ok()
    }

    pub fn cancel(&mut self) { self.active = None; }
}

#[cfg(test)]
mod exhaustion_tests {
    use super::*;
    fn snapshot() -> AppearanceSnapshot { AppearanceSnapshot::from_widget(&default_config().widgets[0]) }
    #[test]
    fn exhausted_unissued_owner_cannot_resolve_a_record_from_another_unissued_owner() {
        let mut store = PresetStore::restore(Ok(None));
        store.add("Existing", snapshot()).unwrap();
        store.version.owner = 0;
        assert_eq!(store.record(store.version(), 0), Err(PresetError::Exhausted));
    }
    #[test]
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
