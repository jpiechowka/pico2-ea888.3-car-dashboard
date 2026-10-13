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

pub trait ConsoleSink {
    fn write(
        &mut self,
        severity: Severity,
        text: &str,
    );
}

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
    pub category: Category,
    pub severity: Severity,
    pub code: &'static str,
    pub subject: Option<&'static str>,
    pub message: String,
}
impl Notice {
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
pub enum StorageArea {
    Draft,
    Preferences,
    Presets,
}
impl StorageArea {
    pub fn label(self) -> &'static str {
        match self {
            Self::Draft => "Draft",
            Self::Preferences => "Preferences",
            Self::Presets => "Presets",
        }
    }

    fn index(self) -> usize {
        match self {
            Self::Draft => 0,
            Self::Preferences => 1,
            Self::Presets => 2,
        }
    }
}
#[derive(Clone, Copy)]
pub enum StorageIssue {
    Failure(StorageFailure),
    Corrupt,
    Conflict,
    Exhausted,
}

/// The checked outcome of this save attempt, independent of an earlier UI conflict.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DraftSaveAttempt {
    Conflict,
    Completed(Result<(), StorageFailure>),
}
impl DraftSaveAttempt {
    pub fn is_conflict(self) -> bool { matches!(self, Self::Conflict) }

    pub fn acknowledgment(self) -> Result<(), StorageFailure> {
        match self {
            Self::Conflict => Err(StorageFailure::Denied),
            Self::Completed(result) => result,
        }
    }

    /// Conflict is reported by the existing conflict hook exactly once.
    pub fn storage_result(self) -> Option<Result<(), StorageIssue>> {
        match self {
            Self::Conflict => None,
            Self::Completed(result) => Some(result.map_err(StorageIssue::Failure)),
        }
    }
}

/// Called inside the draft lock, after token currentness has been checked.
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
pub enum RenderLane {
    Appearance,
    Simulation,
    Timing,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RenderIssue {
    Generic,
    CanvasAllocation,
    CanvasContext,
    CanvasDraw,
    Preview(crate::preview::PreviewError),
}
impl std::fmt::Display for RenderIssue {
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
    fn index(self) -> usize {
        match self {
            Self::Appearance => 0,
            Self::Simulation => 1,
            Self::Timing => 2,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Appearance => "Appearance",
            Self::Simulation => "Simulation",
            Self::Timing => "Performance timing",
        }
    }
}
#[derive(Default)]
pub struct AdapterState {
    storage_failed: [[bool; 2]; 3],
    render_failed: [Option<RenderIssue>; 3],
}
impl AdapterState {
    pub fn storage(
        &mut self,
        area: StorageArea,
        result: Result<(), StorageIssue>,
    ) -> Option<Notice> {
        self.storage_at(area, result, true)
    }

    pub fn storage_read(
        &mut self,
        area: StorageArea,
        result: Result<(), StorageIssue>,
    ) -> Option<Notice> {
        self.storage_at(area, result, false)
    }

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

    pub fn render(
        &mut self,
        lane: RenderLane,
        success: bool,
    ) -> Option<Notice> {
        self.render_result(lane, if success { Ok(()) } else { Err(RenderIssue::Generic) })
    }

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

pub enum ImportFailure {
    TooLarge,
    Read,
    Rejected(EditorError),
}
pub fn import_outcome(result: Result<(), ImportFailure>) -> Notice {
    let message = match result {
        Ok(()) => "Configuration imported and accepted",
        Err(ImportFailure::TooLarge) => "Import exceeds 128 KiB; document unchanged",
        Err(ImportFailure::Read) => "Import file could not be read; document unchanged",
        Err(ImportFailure::Rejected(EditorError::JsonPending)) => "Import blocked by pending JSON",
        Err(ImportFailure::Rejected(EditorError::StaleToken)) => "Stale import ignored; document unchanged",
        Err(ImportFailure::Rejected(_)) => "Import rejected by codec, validation or current action readiness",
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
        message: message.into(),
    }
}
#[derive(Clone, Copy)]
pub enum TransferFailure {
    Clipboard,
    Download,
    Serialization,
}
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
pub fn frame_completion_error() -> Notice {
    Notice {
        category: Category::Rendering,
        severity: Severity::Error,
        code: "frame.completion_rejected",
        subject: Some("Simulation"),
        message: "Frame completion rejected; previous completed state remains authoritative".into(),
    }
}

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

pub struct SimulationObservation {
    page: ControlPage,
    view: u8,
    brightness: Brightness,
    fps: dashboard_core::render::FpsMode,
    psi: bool,
    reset: Option<u64>,
    warning: Option<WarningFeedback>,
    quality: [Quality; 8],
    inputs: [Option<crate::simulator::InputOutcome>; 8],
    inspection: Option<&'static str>,
    control_outcome: Option<&'static str>,
}
impl SimulationObservation {
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
pub fn checked_wall_clock(value: f64) -> Option<i64> {
    if value.is_finite() && value.fract() == 0. && value >= i64::MIN as f64 && value < -(i64::MIN as f64) {
        Some(value as i64)
    } else {
        None
    }
}

/// Only the field's own accepted keystrokes may produce its later blur record.
pub struct FieldCommit {
    initial: AcceptedConfigKey,
    latest: Option<AcceptedConfigKey>,
}
impl FieldCommit {
    pub fn begin(editor: &Editor) -> Self {
        Self {
            initial: editor.accepted_config_key(),
            latest: None,
        }
    }

    pub fn observe(
        &mut self,
        before: AcceptedConfigKey,
        editor: &Editor,
    ) {
        if before != editor.accepted_config_key() && !editor.has_invalid_draft() {
            self.latest = Some(editor.accepted_config_key());
        }
    }

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
pub struct PauseState {
    paused: bool,
    anchor: Option<EventSeq>,
    inserted: DiagnosticCount,
}
pub struct PauseReport {
    pub paused: bool,
    pub anchor: Option<EventSeq>,
    pub gap: bool,
    pub new_events: DiagnosticCount,
}
impl PauseState {
    pub fn is_paused(&self) -> bool { self.paused }

    pub fn pause(
        &mut self,
        snapshot: &EventSnapshot,
    ) {
        self.paused = true;
        self.anchor = snapshot.newest;
        self.inserted = snapshot.counters.inserted;
    }

    pub fn resume(&mut self) { self.paused = false; }

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
