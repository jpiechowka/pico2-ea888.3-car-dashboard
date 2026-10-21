//! Typed producer outcomes; the event store owns all retention and console policy.
use dashboard_core::backlight::Brightness;
use dashboard_core::popup::Popup;
use dashboard_core::samples::{Quality, SensorId};
use dashboard_core::warning_state::WarningFeedback;

use crate::appearance_presets::{PresetError, PresetStore};
use crate::config::ControlPage;
use crate::editor::{AcceptedConfigKey, Editor, EditorError, Revision, StorageFailure};
use crate::events::*;
use crate::simulator::ControlSimulator;

/// Console output boundary invoked only after the event store's policy admits emission.
pub trait ConsoleSink {
    /// Writes already-formatted bounded `text` at `severity`; adapters must treat text as data, not a format string.
    fn write(
        &mut self,
        severity: Severity,
        text: &str,
    );
}

/// Returns `text` with Unicode control characters escaped so local diagnostics cannot inject console control sequences.
fn console_text(text: &str) -> String {
    text.chars()
        .flat_map(|c| {
            if c.is_control() {
                c.escape_default().collect::<Vec<_>>()
            } else {
                vec![c]
            }
        })
        .collect()
}

/// Records `input` in `store` and returns its retention/console outcome or recording-admission error.
/// `console` receives escaped text only when `console_enabled` and the store explicitly requests emission.
/// Sink panics propagate after recording; console delivery is not transactional with local retention.
pub fn emit(
    store: &mut EventStore,
    console: &mut impl ConsoleSink,
    console_enabled: bool,
    input: EventInput<'_>,
) -> Result<RecordOutcome, EventError> {
    let outcome = store.record(input)?;
    if console_enabled
        && outcome.console == ConsoleDisposition::Emit
        && let Some(event) = store.get(outcome.seq)
    {
        let text = format!(
            "#{} {:?} {:?} {}{}: {}",
            event.seq.get(),
            event.severity,
            event.category,
            event.code.get(),
            event
                .subject
                .as_deref()
                .map(|s| format!(" [{}]", console_text(s)))
                .unwrap_or_default(),
            console_text(&event.message)
        );
        console.write(event.severity, &text);
    }
    Ok(outcome)
}

/// No raw configuration, browser exception or helper input enters this DTO.
#[derive(Debug)]
pub struct Notice {
    /// Subsystem associated with the typed outcome.
    pub category: Category,
    /// Local policy severity.
    pub severity: Severity,
    /// Static application code, validated when recorded.
    pub code: &'static str,
    /// Optional static correlation label, never user payload.
    pub subject: Option<&'static str>,
    /// Application-formatted diagnostic text; event retention applies byte bounds.
    pub message: String,
}
impl Notice {
    /// Returns borrowed event input using monotonic `elapsed_ms` and optional Unix `wall_clock_ms`.
    pub fn input(
        &self,
        elapsed_ms: u64,
        wall_clock_ms: Option<i64>,
    ) -> EventInput<'_> {
        EventInput {
            category: self.category,
            severity: self.severity,
            code: self.code,
            subject: self.subject,
            message: &self.message,
            elapsed_ms,
            wall_clock_ms,
        }
    }

    /// Returns an informational notice from `category`, static `code`/`subject` and sanitized `message`.
    /// Code syntax and retained text bounds are enforced later by the event store.
    pub fn info(
        category: Category,
        code: &'static str,
        subject: Option<&'static str>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            category,
            severity: Severity::Info,
            code,
            subject,
            message: message.into(),
        }
    }
}

#[derive(Clone, Copy)]
/// Independent browser persistence areas whose read/write recovery histories must not cross.
pub enum StorageArea {
    /// Accepted project/draft persistence.
    Draft,
    /// Workspace and UI preferences.
    Preferences,
    /// Named portable appearance values.
    Presets,
}
impl StorageArea {
    /// Returns the bounded static storage-area display label.
    pub fn label(self) -> &'static str {
        match self {
            Self::Draft => "Draft",
            Self::Preferences => "Preferences",
            Self::Presets => "Presets",
        }
    }

    /// Returns the stable zero-based adapter-state slot in `0..3`.
    fn index(self) -> usize {
        match self {
            Self::Draft => 0,
            Self::Preferences => 1,
            Self::Presets => 2,
        }
    }
}
#[derive(Clone, Copy)]
/// Sanitized persistence outcome without stored text or raw browser exceptions.
pub enum StorageIssue {
    /// Browser storage operation failed.
    Failure(
        /// Classified quota, denial or unavailable outcome.
        StorageFailure,
    ),
    /// Saved bytes/schema cannot be admitted.
    Corrupt,
    /// Another context changed the observed saved value.
    Conflict,
    /// Persistent owner/revision/request identity cannot advance safely.
    Exhausted,
}

/// The checked outcome of this save attempt, independent of an earlier UI conflict.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DraftSaveAttempt {
    /// Compare-before-write detected changed storage; no write was attempted.
    Conflict,
    /// Actual read/write outcome after conflict admission.
    Completed(
        /// Success only after write callback succeeds; errors include failed prior read.
        Result<(), StorageFailure>,
    ),
}
impl DraftSaveAttempt {
    /// Returns whether this specific attempt detected a cross-tab conflict.
    pub fn is_conflict(self) -> bool { matches!(self, Self::Conflict) }

    /// Returns editor acknowledgment outcome, mapping conflict to denial without claiming a completed write.
    pub fn acknowledgment(self) -> Result<(), StorageFailure> {
        match self {
            Self::Conflict => Err(StorageFailure::Denied),
            Self::Completed(result) => result,
        }
    }

    /// Conflict is reported by the existing conflict hook exactly once.
    /// Returns classified read/write outcome, or `None` when the conflict hook owns reporting.
    pub fn storage_result(self) -> Option<Result<(), StorageIssue>> {
        match self {
            Self::Conflict => None,
            Self::Completed(result) => Some(result.map_err(StorageIssue::Failure)),
        }
    }
}

/// Called inside the draft lock, after token currentness has been checked.
/// Compares actual `current` storage with trusted `saved` unless explicit `replace` is set.
/// Returns read failure/conflict without calling `write`, otherwise its actual outcome; no retry occurs.
pub fn attempt_draft_save(
    replace: bool,
    current: Result<Option<String>, StorageFailure>,
    saved: &Option<String>,
    write: impl FnOnce() -> Result<(), StorageFailure>,
) -> DraftSaveAttempt {
    match current {
        Err(failure) => DraftSaveAttempt::Completed(Err(failure)),
        Ok(current) if !replace && current != *saved => DraftSaveAttempt::Conflict,
        Ok(_) => DraftSaveAttempt::Completed(write()),
    }
}

/// Reload consumes the actual observation and reports this read's result only.
/// A prior failed write remains independent in AdapterState.
/// Returns both `store` reload admission and the classified read outcome for actual storage `observation`.
/// Failed reload preserves records according to PresetStore's contract; no browser I/O is performed here.
pub fn reload_preset_attempt(
    store: &mut PresetStore,
    observation: Result<Option<String>, StorageFailure>,
) -> (Result<(), PresetError>, Result<(), StorageIssue>) {
    let result = store.reload(observation);
    let read = result.map_err(|error| {
        match error {
            PresetError::Storage(failure) => StorageIssue::Failure(failure),
            PresetError::Exhausted => StorageIssue::Exhausted,
            _ => StorageIssue::Corrupt,
        }
    });
    (result, read)
}
#[derive(Clone, Copy)]
/// Independent render/recovery channels with separate diagnostic state.
pub enum RenderLane {
    /// Static accepted/diagnostic appearance canvas.
    Appearance,
    /// Stateful synthetic controls and completed-frame preview.
    Simulation,
    /// Explicit performance-timing render workload.
    Timing,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Typed rendering failure retained without raw browser exceptions.
pub enum RenderIssue {
    /// Unclassified renderer or scheduler failure.
    Generic,
    /// Browser ImageData allocation failed.
    CanvasAllocation,
    /// Canvas 2D rendering context is unavailable.
    CanvasContext,
    /// Browser rejected pixel publication.
    CanvasDraw,
    /// Shared preview admission/render/completion failed.
    Preview(
        /// Exact bounded preview failure.
        crate::preview::PreviewError,
    ),
}
impl std::fmt::Display for RenderIssue {
    /// Writes bounded diagnostic text to `f`; returns any formatter failure.
    fn fmt(
        &self,
        f: &mut std::fmt::Formatter<'_>,
    ) -> std::fmt::Result {
        match self {
            Self::Generic => f.write_str("Rendering or scheduling failed"),
            Self::CanvasAllocation => f.write_str("Canvas image allocation failed"),
            Self::CanvasContext => f.write_str("Canvas context unavailable"),
            Self::CanvasDraw => f.write_str("Canvas draw failed"),
            Self::Preview(error) => write!(f, "Preview failed: {error:?}"),
        }
    }
}
impl RenderLane {
    /// Returns the stable zero-based render-state slot in `0..3`.
    fn index(self) -> usize {
        match self {
            Self::Appearance => 0,
            Self::Simulation => 1,
            Self::Timing => 2,
        }
    }

    /// Returns the static lane label for diagnostic subjects.
    fn label(self) -> &'static str {
        match self {
            Self::Appearance => "Appearance",
            Self::Simulation => "Simulation",
            Self::Timing => "Performance timing",
        }
    }
}
#[derive(Default)]
/// Transition memory preventing repeated render notices and keeping storage read/write recovery independent.
pub struct AdapterState {
    /// Failure latches by storage area and operation (`0` read, `1` write).
    storage_failed: [[bool; 2]; 3],
    /// Last failure per render lane; `None` means success/unfailed.
    render_failed: [Option<RenderIssue>; 3],
}
impl AdapterState {
    /// Records write `result` for `area`; returns every failure or its first subsequent recovery notice, otherwise
    /// `None`.
    pub fn storage(
        &mut self,
        area: StorageArea,
        result: Result<(), StorageIssue>,
    ) -> Option<Notice> {
        self.storage_at(area, result, true)
    }

    /// Records read `result` for `area`; returns every failure or its first subsequent recovery notice, otherwise
    /// `None`.
    pub fn storage_read(
        &mut self,
        area: StorageArea,
        result: Result<(), StorageIssue>,
    ) -> Option<Notice> {
        self.storage_at(area, result, false)
    }

    /// Updates `area`'s read/write latch selected by `write` from actual `result`.
    /// Returns typed failure/recovery notice, or `None` for repeated success; reads never clear write failures.
    fn storage_at(
        &mut self,
        area: StorageArea,
        result: Result<(), StorageIssue>,
        write: bool,
    ) -> Option<Notice> {
        let failed = &mut self.storage_failed[area.index()][usize::from(write)];
        let operation = if write { "Write" } else { "Read" };
        match result {
            Err(issue) => {
                *failed = true;
                let message = match issue {
                    StorageIssue::Failure(StorageFailure::Quota) => "Browser storage is full; export a backup",
                    StorageIssue::Failure(StorageFailure::Denied) => {
                        "Browser storage permission denied; export remains available"
                    }
                    StorageIssue::Failure(StorageFailure::Unavailable) => {
                        "Browser storage unavailable; export remains available"
                    }
                    StorageIssue::Corrupt => {
                        "Stored record is invalid or incompatible; retained separately for recovery"
                    }
                    StorageIssue::Conflict => {
                        "Stored record changed in another tab; explicit replacement or reload required"
                    }
                    StorageIssue::Exhausted => "Storage record identity exhausted; export and reopen Studio",
                };
                Some(Notice {
                    category: Category::Storage,
                    severity: Severity::Warn,
                    code: match issue {
                        StorageIssue::Corrupt => "storage.corrupt",
                        StorageIssue::Conflict => "storage.conflict",
                        StorageIssue::Exhausted => "storage.identity_exhausted",
                        _ => "storage.failed",
                    },
                    subject: Some(area.label()),
                    message: format!("{operation}: {message}"),
                })
            }
            Ok(()) if *failed => {
                *failed = false;
                Some(Notice::info(
                    Category::Storage,
                    "storage.recovered",
                    Some(area.label()),
                    format!("{operation}: browser storage recovered"),
                ))
            }
            Ok(()) => None,
        }
    }

    /// Records generic `success`/failure for `lane`; returns a notice only when its retained outcome changes.
    pub fn render(
        &mut self,
        lane: RenderLane,
        success: bool,
    ) -> Option<Notice> {
        self.render_result(lane, if success { Ok(()) } else { Err(RenderIssue::Generic) })
    }

    /// Records typed `result` for `lane`; returns first/changed failure or recovery, and `None` for identical outcomes.
    pub fn render_result(
        &mut self,
        lane: RenderLane,
        result: Result<(), RenderIssue>,
    ) -> Option<Notice> {
        let failed = &mut self.render_failed[lane.index()];
        let next = result.err();
        if *failed == next {
            return None;
        }
        *failed = next;
        let success = next.is_none();
        Some(Notice {
            category: Category::Rendering,
            severity: if success { Severity::Info } else { Severity::Error },
            code: if success { "render.recovered" } else { "render.failed" },
            subject: Some(lane.label()),
            message: next
                .map(|issue| format!("{issue}; last completed state remains authoritative"))
                .unwrap_or_else(|| "Rendering recovered".into()),
        })
    }
}

/// Returns a validation/recovery notice only when `was_invalid` differs from current `invalid`.
pub fn validation_transition(
    was_invalid: bool,
    invalid: bool,
) -> Option<Notice> {
    (was_invalid != invalid).then(|| {
        Notice {
            category: Category::Validation,
            severity: if invalid { Severity::Warn } else { Severity::Info },
            code: if invalid {
                "validation.invalid"
            } else {
                "validation.valid"
            },
            subject: None,
            message: if invalid {
                "Candidate became invalid; repair or discard before snapshot actions"
            } else {
                "Candidate valid; accepted state is available"
            }
            .into(),
        }
    })
}

/// Import stage failure without file contents or raw exceptions.
pub enum ImportFailure {
    /// Input exceeded its route-specific byte cap before reading/admission.
    TooLarge {
        /// Maximum permitted file bytes for this import format.
        limit_bytes: usize,
    },
    /// Browser read did not produce usable bytes.
    Read,
    /// Reducer refused decoded/imported content or its authority.
    Rejected(
        /// Exact typed admission failure.
        EditorError,
    ),
}
/// Returns one sanitized import outcome notice from actual `result`, without embedding file text or paths.
pub fn import_outcome(result: Result<(), ImportFailure>) -> Notice {
    let message = match result {
        Ok(()) => "Configuration imported and accepted".into(),
        Err(ImportFailure::TooLarge { limit_bytes }) => {
            format!("Import exceeds its {limit_bytes} byte size limit; document unchanged")
        }
        Err(ImportFailure::Read) => "Import file could not be read; document unchanged".into(),
        Err(ImportFailure::Rejected(EditorError::JsonPending)) => "Import blocked by pending JSON".into(),
        Err(ImportFailure::Rejected(EditorError::StaleToken)) => "Stale import ignored; document unchanged".into(),
        Err(ImportFailure::Rejected(_)) => "Import rejected by codec, validation or current action readiness".into(),
    };
    Notice {
        category: Category::ImportExport,
        severity: if result.is_ok() { Severity::Info } else { Severity::Warn },
        code: if result.is_ok() {
            "import.accepted"
        } else {
            "import.rejected"
        },
        subject: None,
        message,
    }
}
#[derive(Clone, Copy)]
/// Local export destination or serialization failure.
pub enum TransferFailure {
    /// Clipboard API was unavailable/denied/rejected.
    Clipboard,
    /// Browser download could not be started.
    Download,
    /// Export encoding failed or exceeded its checked size limit.
    Serialization,
}
/// Returns a typed actionable notice for `failure`, preserving the retained source data.
pub fn transfer_failure(failure: TransferFailure) -> Notice {
    Notice {
        category: Category::ImportExport,
        severity: Severity::Warn,
        code: match failure {
            TransferFailure::Clipboard => "clipboard.failed",
            TransferFailure::Download => "download.failed",
            TransferFailure::Serialization => "export.failed",
        },
        subject: None,
        message: match failure {
            TransferFailure::Clipboard => "Clipboard unavailable or denied; select the displayed export text",
            TransferFailure::Download => "Browser download could not start; retained records remain available",
            TransferFailure::Serialization => "Export could not be serialized within its checked size limit",
        }
        .into(),
    }
}
/// Returns the bounded notice for rejected synthetic frame completion; prior completed state stays authoritative.
pub fn frame_completion_error() -> Notice {
    Notice {
        category: Category::Rendering,
        severity: Severity::Error,
        code: "frame.completion_rejected",
        subject: Some("Simulation"),
        message: "Frame completion rejected; previous completed state remains authoritative".into(),
    }
}

/// Acknowledges completed frame `token` in `sim`; returns success or a typed notice when ownership rejects completion.
/// This is synthetic presentation acknowledgment, not a hardware transfer/scanout observation.
pub fn complete_simulation_frame(
    sim: &mut ControlSimulator,
    token: dashboard_core::display::pipeline::FrameToken,
) -> Result<(), Notice> {
    if sim.complete_frame(token) {
        Ok(())
    } else {
        Err(frame_completion_error())
    }
}

/// Invoke the actual canvas consumer only for this source's exact retained candidate.
/// Returns `consume`'s result for `preview` pixels bound to `sim` and `token`; invalid/foreign/missing candidates
/// return an error without invoking `consume`. No frame completion or pixel ownership transfer occurs here.
pub fn consume_simulation_pixels(
    sim: &ControlSimulator,
    preview: &crate::preview::Preview,
    token: dashboard_core::display::pipeline::FrameToken,
    consume: impl FnOnce(&[u8]) -> Result<(), RenderIssue>,
) -> Result<(), RenderIssue> {
    if sim.invalid() {
        return Err(RenderIssue::Preview(crate::preview::PreviewError::InspectionOnly));
    }
    let pixels = preview
        .pending_pixels(sim, token)
        .ok_or(RenderIssue::Preview(crate::preview::PreviewError::PendingFrame))?;
    consume(pixels)
}

/// Restoration must succeed before completion; failure retains old metadata and bytes.
/// Calls `consume` on pixels bound to `sim`, `preview` and `token`, then acknowledges synthetic completion and
/// finishes preview ownership. Returns consumer/admission/completion errors; consumer side effects are not rolled back.
pub fn complete_simulation_pixels(
    sim: &mut ControlSimulator,
    preview: &mut crate::preview::Preview,
    token: dashboard_core::display::pipeline::FrameToken,
    consume: impl FnOnce(&[u8]) -> Result<(), RenderIssue>,
) -> Result<(), RenderIssue> {
    consume_simulation_pixels(sim, preview, token, consume)?;
    complete_simulation_frame(sim, token)
        .map_err(|_| RenderIssue::Preview(crate::preview::PreviewError::PendingFrame))?;
    preview.finish_pending(token);
    Ok(())
}

/// Small typed simulator state snapshot for transition diagnostics, excluding sample payloads and per-frame noise.
pub struct SimulationObservation {
    /// Current logical control page.
    page: ControlPage,
    /// Selected named dashboard view ID.
    view: u8,
    /// Current session backlight level.
    brightness: Brightness,
    /// Requested diagnostic FPS display mode, not measured physical FPS.
    fps: dashboard_core::render::FpsMode,
    /// Physical-equivalent boost unit toggle (`true` means PSI).
    psi: bool,
    /// Pending statistics reset token, or `None` when no reset awaits acknowledgment.
    reset: Option<u64>,
    /// Visible warning-popup acknowledgment feedback, or `None` for other/no popup.
    warning: Option<WarningFeedback>,
    /// Qualified sample quality in SensorId order at the captured simulator time.
    quality: [Quality; 8],
    /// Last per-sensor input admission outcomes in SensorId order.
    inputs: [Option<crate::simulator::InputOutcome>; 8],
    /// Latest bounded invalid-inspection outcome, if present.
    inspection: Option<&'static str>,
    /// Latest bounded control-rejection outcome, if present.
    control_outcome: Option<&'static str>,
}
impl SimulationObservation {
    /// Returns typed current state from `sim`, resolving sample quality at its monotonic simulation time.
    pub fn capture(sim: &ControlSimulator) -> Self {
        let session = sim.session();
        let snapshot = sim.snapshot();
        Self {
            page: session.page(),
            view: session.view_id(),
            brightness: session.backlight().brightness(),
            fps: session.fps(),
            psi: session.boost_psi(),
            reset: sim.pending_reset_token(),
            warning: match session.popup() {
                Some(Popup::Warnings(_, feedback)) => Some(feedback),
                _ => None,
            },
            quality: SensorId::ALL.map(|id| snapshot.quality(id, sim.now_ms())),
            inputs: sim.input_outcomes(),
            inspection: sim.inspection_outcome(),
            control_outcome: sim.control_outcome(),
        }
    }
}
/// Returns notices for relevant differences from `before` to `after`, including quality/recovery and control outcomes.
/// Unchanged state and disappearing optional outcomes are quiet; no raw sample values or per-frame events are emitted.
pub fn simulation_changes(
    before: &SimulationObservation,
    after: &SimulationObservation,
) -> Vec<Notice> {
    let mut changes = Vec::new();
    if before.page != after.page
        || before.view != after.view
        || before.brightness != after.brightness
        || before.fps != after.fps
        || before.psi != after.psi
    {
        changes.push(Notice::info(
            Category::Simulation,
            "simulation.control",
            None,
            format!(
                "{} · view {} · {} · {} · {}",
                after.page.label(),
                after.view,
                after.brightness.label(),
                after.fps.label(),
                if after.psi { "PSI" } else { "bar" }
            ),
        ));
    }
    if before.reset != after.reset {
        changes.push(Notice::info(
            Category::Simulation,
            if after.reset.is_some() {
                "simulation.reset_requested"
            } else {
                "simulation.reset_ack"
            },
            None,
            if after.reset.is_some() {
                "Statistics reset requested"
            } else {
                "Statistics reset acknowledged"
            },
        ));
    }
    if before.warning != after.warning
        && let Some(feedback) = after.warning
    {
        changes.push(Notice {
            category: Category::Simulation,
            severity: if matches!(
                feedback,
                WarningFeedback::Rejected | WarningFeedback::Unavailable | WarningFeedback::Busy
            ) {
                Severity::Warn
            } else {
                Severity::Info
            },
            code: "simulation.warning_ack",
            subject: None,
            message: feedback.label().into(),
        });
    }
    for (i, id) in SensorId::ALL.iter().enumerate() {
        if before.quality[i] != after.quality[i] {
            changes.push(Notice {
                category: Category::Simulation,
                severity: if after.quality[i] == Quality::Valid {
                    Severity::Info
                } else {
                    Severity::Warn
                },
                code: "simulation.quality",
                subject: Some(id.label()),
                message: format!("{} → {}", before.quality[i].label(), after.quality[i].label()),
            });
        }
        if before.inputs[i] != after.inputs[i]
            && let Some(outcome) = after.inputs[i]
        {
            changes.push(Notice::info(
                Category::Simulation,
                "simulation.input",
                Some(id.label()),
                outcome.label(),
            ));
        }
    }
    if before.inspection != after.inspection
        && let Some(outcome) = after.inspection
    {
        changes.push(Notice::info(
            Category::Simulation,
            "simulation.inspection",
            None,
            outcome,
        ));
    }
    if before.control_outcome != after.control_outcome
        && let Some(outcome) = after.control_outcome
    {
        changes.push(Notice::info(
            Category::Simulation,
            "simulation.control_rejected",
            None,
            outcome,
        ));
    }
    changes
}

/// Browser Date.now is an integer; the positive i64 bound is exclusive in f64.
/// Returns integer Unix milliseconds for finite integral `value` inside i64 range, otherwise `None` without saturation.
pub fn checked_wall_clock(value: f64) -> Option<i64> {
    if value.is_finite() && value.fract() == 0. && value >= i64::MIN as f64 && value < -(i64::MIN as f64) {
        Some(value as i64)
    } else {
        None
    }
}

/// Only the field's own accepted keystrokes may produce its later blur record.
pub struct FieldCommit {
    /// Accepted dashboard lineage when this field first gained commit tracking.
    initial: AcceptedConfigKey,
    /// Latest accepted lineage produced by this field's observed keystrokes.
    latest: Option<AcceptedConfigKey>,
}
impl FieldCommit {
    /// Returns fresh field tracking anchored to `editor`'s accepted dashboard content.
    pub fn begin(editor: &Editor) -> Self {
        Self {
            initial: editor.accepted_config_key(),
            latest: None,
        }
    }

    /// Records this field's accepted content change when prior `before` differs from clean `editor` lineage.
    /// Unchanged or rejected keystrokes do not replace the latest accepted lineage.
    pub fn observe(
        &mut self,
        before: AcceptedConfigKey,
        editor: &Editor,
    ) {
        if before != editor.accepted_config_key() && !editor.has_invalid_draft() {
            self.latest = Some(editor.accepted_config_key());
        }
    }

    /// Consumes tracked keystroke evidence and returns `editor`'s accepted revision only if that exact changed
    /// lineage remains current and clean; returns `None` after unrelated edits, rejection or no accepted change.
    pub fn finish(
        &mut self,
        editor: &Editor,
    ) -> Option<Revision> {
        self.latest
            .take()
            .filter(|key| *key != self.initial && *key == editor.accepted_config_key() && !editor.has_invalid_draft())
            .map(|_| editor.accepted_revision())
    }
}

#[derive(Default, Clone, Copy)]
/// Event-log display pause anchored to retained identity; recording continues independently.
pub struct PauseState {
    /// Whether display follows a fixed anchor instead of live newest records.
    paused: bool,
    /// Store-wide newest event at pause, or `None` for an empty store.
    anchor: Option<EventSeq>,
    /// Lifetime insertion count captured at pause for later new-record accounting.
    inserted: DiagnosticCount,
}
/// Read-only pause status with explicit eviction gaps and inexact insertion counts.
pub struct PauseReport {
    /// Whether display is currently paused.
    pub paused: bool,
    /// Original anchor, or current oldest retained identity after an eviction gap.
    pub anchor: Option<EventSeq>,
    /// Paused nonempty anchor was evicted or cleared.
    pub gap: bool,
    /// Insertions since pause, zero while live; saturation records uncertain subtraction.
    pub new_events: DiagnosticCount,
}
impl PauseState {
    /// Returns whether the display is paused; event recording is unaffected.
    pub fn is_paused(&self) -> bool { self.paused }

    /// Captures `snapshot`'s store-wide newest identity and insertion count, replacing any previous pause anchor.
    pub fn pause(
        &mut self,
        snapshot: &EventSnapshot,
    ) {
        self.paused = true;
        self.anchor = snapshot.newest;
        self.inserted = snapshot.counters.inserted;
    }

    /// Resumes live display without clearing the retained anchor or changing the event store.
    pub fn resume(&mut self) { self.paused = false; }

    /// Returns current pause/gap/insertion accounting against `store` without mutating either owner.
    /// After anchor eviction, uses the oldest retained identity or `None` if the store is empty.
    pub fn report(
        &self,
        store: &EventStore,
    ) -> PauseReport {
        let snapshot = store.snapshot(&EventFilter::default().limit(0));
        let gap = self.paused && self.anchor.is_some_and(|seq| !store.contains(seq));
        PauseReport {
            paused: self.paused,
            anchor: if gap { snapshot.oldest } else { self.anchor },
            gap,
            new_events: DiagnosticCount {
                value: if self.paused {
                    snapshot.counters.inserted.value.saturating_sub(self.inserted.value)
                } else {
                    0
                },
                saturated: self.paused && (snapshot.counters.inserted.saturated || self.inserted.saturated),
            },
        }
    }
}
