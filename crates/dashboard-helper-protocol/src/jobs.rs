//! Bounded operation progress, lifecycle and replay contracts. Validation checks
//! internal coherence; the native job owner enforces transitions and session authorization.

use serde::{Deserialize, Serialize};

use crate::*;

/// Define documented closed wire enums, preserving declaration order and snake-case Serde names.
macro_rules! enums { ($($(#[$a:meta])* $name:ident { $($(#[$b:meta])* $v:ident),+ $(,)? })+) => {$ (
    $(#[$a])*
    #[derive(Clone,Copy,Debug,PartialEq,Eq,PartialOrd,Ord,Serialize,Deserialize)]
    #[serde(rename_all="snake_case")]
    pub enum $name { $($(#[$b])* $v),+ }
)+}; }
enums! {
 /// Admitted operation family, defining its exact ordered stage layout.
 OperationKind {
  /// Compile and validate an immutable project snapshot.
  Build,
  /// Explicitly transfer, verify and request reboot of a selected device.
  Flash,
  /// Install or validate explicitly requested host prerequisites.
  Setup,
  /// Inspect prerequisites without installing them.
  PrerequisiteCheck,
  /// Enumerate eligible USB descriptors without authorizing a write.
  DeviceScan,
  /// Remove explicitly selected, unprotected owned attempts.
  Cleanup
 }
 /// Build/flash stages; each operation uses only its documented ordered subset.
 FirmwareStage {
  /// Establish required host tools and versions.
  Prerequisites,
  /// Freeze and verify project/source inputs.
  Snapshot,
  /// Invoke compilation for the frozen recipe.
  Compilation,
  /// Admit embedded data, program image and UF2 agreement.
  Validation,
  /// Revalidate selected hardware identity and board consent.
  DeviceSelection,
  /// Invoke the explicit device-write operation.
  Transfer,
  /// Read back and compare admitted image bytes/ranges.
  Verification,
  /// Request reboot; application startup is not observed by this stage.
  Reboot
 }
 /// Prerequisite setup phase, aggregated across individual tool steps.
 SetupStage {
  /// Inspect existing tools and determine required actions.
  Check,
  /// Acquire pinned installation content.
  Download,
  /// Verify acquired content before execution/install.
  Verify,
  /// Run the approved installation step.
  Install,
  /// Recheck installed prerequisites and versions.
  Validate
 }
 /// Read-only host/device probing phase.
 ProbeStage {
  /// Check current host prerequisites.
  Prerequisites,
  /// Detect candidate USB devices.
  DeviceDetection
 }
 /// Explicit owned-file cleanup phase.
 CleanupStage {
  /// Validate ownership/protection and remove selected attempts.
  Cleanup
 }
 /// Stage state; only pending/running are nonterminal.
 StageState {
  /// Stage has not begun.
  Pending,
  /// Stage currently owns active work.
  Running,
  /// Stage completed its defined checks.
  Succeeded,
  /// Stage failed; reason/exit provides available context.
  Failed,
  /// Cancellation ended the stage.
  Cancelled,
  /// Stage was deliberately omitted; summaries require a reason.
  Skipped
 }
 /// Conservative device-write evidence, independent of job success/reboot completion.
 WriteOutcome {
  /// No device write was started.
  NotStarted,
  /// Write invocation is active; final bytes are not established.
  InProgress,
  /// Transfer completed but readback has not established equality.
  TransferredUnverified,
  /// Verification stage confirmed admitted bytes at verification time.
  Verified,
  /// Partial effects cannot be determined, including interrupted writes.
  Unknown
 }
 /// Event purpose, independent of severity and output stream.
 EventKind {
  /// Bounded textual log chunk.
  Log,
  /// Quantified progress update when supported.
  Progress,
  /// Stage lifecycle transition.
  Stage,
  /// Diagnostic context or anomaly.
  Diagnostic
 }
 /// Event display severity; no automatic authority follows from a level.
 Level {
  /// Ordinary informational observation.
  Info,
  /// Recoverable or incomplete condition requiring attention.
  Warning,
  /// Failure observation.
  Error
 }
 /// Source of an emitted event.
 Source {
  /// Captured child standard output.
  Stdout,
  /// Captured child standard error; stream alone does not imply failure.
  Stderr,
  /// Helper-generated lifecycle or diagnostic observation.
  Internal
 }
 /// Counter unit for a bounded progress observation.
 ProgressUnit {
  /// Bytes processed, not necessarily persisted or verified.
  Bytes,
  /// Discrete work items.
  Items,
  /// Integer percent with a required total of exactly 100.
  Percent
 }
 /// Terminal job reason, distinct from stage-specific state and device-write evidence.
 TerminalCause {
  /// Every required stage completed successfully under its operation contract.
  Succeeded,
  /// Operation failed without a more specific terminal category.
  Failed,
  /// An acknowledged cancellation request ended the operation.
  Cancelled,
  /// A bounded operation deadline expired.
  Deadline,
  /// Owned process/resource cleanup could not be proven complete.
  OwnershipCleanupFatal
 }
 /// Closed prerequisite identity; declaration order is the canonical tool-summary order.
 ToolId {
  /// Rust toolchain manager executable.
  Rustup,
  /// Repository-pinned Rust compiler/toolchain.
  RustToolchain,
  /// Rust source formatter component.
  Rustfmt,
  /// Rust lint component.
  Clippy,
  /// Compiler-matched Rust library sources.
  RustSrc,
  /// Compiler-matched LLVM utilities component.
  LlvmTools,
  /// Cortex-M33 hard-float Thumb standard-library target.
  ThumbTarget,
  /// Browser Wasm standard-library target.
  WasmTarget,
  /// Host Python interpreter prerequisite.
  Python,
  /// Windows C/C++ linker and SDK prerequisites.
  MsvcSdk,
  /// Rust/Wasm web asset build tool.
  Trunk,
  /// Matching Wasm binding generator executable.
  WasmBindgen,
  /// Wasm optimizer/tooling prerequisite.
  Binaryen,
  /// Raspberry Pi boot-ROM inspection/transfer utility.
  Picotool,
  /// Required cached Cargo dependency material.
  CargoCache,
  /// Native Studio helper executable.
  NativeHelper
 }
}
/// All sixteen prerequisite identities in canonical summary order.
pub const TOOLS: [ToolId; 16] = [
    ToolId::Rustup,
    ToolId::RustToolchain,
    ToolId::Rustfmt,
    ToolId::Clippy,
    ToolId::RustSrc,
    ToolId::LlvmTools,
    ToolId::ThumbTarget,
    ToolId::WasmTarget,
    ToolId::Python,
    ToolId::MsvcSdk,
    ToolId::Trunk,
    ToolId::WasmBindgen,
    ToolId::Binaryen,
    ToolId::Picotool,
    ToolId::CargoCache,
    ToolId::NativeHelper,
];
/// Aggregate setup phase order, distinct from each tool's executable step sequence.
pub const SETUP_STAGES: [SetupStage; 5] = [
    SetupStage::Check,
    SetupStage::Download,
    SetupStage::Verify,
    SetupStage::Install,
    SetupStage::Validate,
];
/// Namespaced stage identifier; Serde carries explicit `scope` and `name` fields.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "scope", content = "name", rename_all = "snake_case", deny_unknown_fields)]
pub enum StageId {
    /// Build or flash stage.
    Firmware(
        /// Exact firmware phase.
        FirmwareStage,
    ),
    /// Host prerequisite setup stage.
    Setup(
        /// Exact setup phase.
        SetupStage,
    ),
    /// Read-only prerequisite/device scan stage.
    Probe(
        /// Exact inspection phase.
        ProbeStage,
    ),
    /// Owned-file cleanup stage.
    Cleanup(
        /// Exact cleanup phase.
        CleanupStage,
    ),
}
impl OperationKind {
    /// Return the required ordered stage IDs; repeated calls allocate independent small vectors.
    pub fn layout(self) -> Vec<StageId> {
        use FirmwareStage::*;
        match self {
            Self::Build => {
                [Prerequisites, Snapshot, Compilation, Validation]
                    .map(StageId::Firmware)
                    .to_vec()
            }
            Self::Flash => {
                [
                    Prerequisites,
                    Validation,
                    DeviceSelection,
                    Transfer,
                    Verification,
                    Reboot,
                ]
                .map(StageId::Firmware)
                .to_vec()
            }
            Self::Setup => SETUP_STAGES.map(StageId::Setup).to_vec(),
            Self::PrerequisiteCheck => vec![StageId::Probe(ProbeStage::Prerequisites)],
            Self::DeviceScan => vec![StageId::Probe(ProbeStage::DeviceDetection)],
            Self::Cleanup => vec![StageId::Cleanup(CleanupStage::Cleanup)],
        }
    }
}
impl StageState {
    /// Return true for succeeded, failed, cancelled or skipped; pending/running remain nonterminal.
    pub fn terminal(self) -> bool { !matches!(self, Self::Pending | Self::Running) }
}
/// Stage-scoped progress with a known nonzero total; unavailable progress is represented by `None` upstream.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Progress {
    /// Completed count, inclusive range zero through `total`.
    pub completed: u64,
    /// Nonzero expected count; exactly 100 for percent units.
    pub total: u64,
    /// Meaning shared by both counters.
    pub unit: ProgressUnit,
}
impl Progress {
    /// Return success for ordered counters and valid percent denominator; otherwise `Schema`.
    pub fn validate(&self) -> Result<(), Error> {
        if self.total == 0 || self.completed > self.total || (self.unit == ProgressUnit::Percent && self.total != 100) {
            Err(Error::Schema)
        } else {
            Ok(())
        }
    }
}
/// Bounded job event; sequence is assigned by its owner, and JSON escaping counts toward the byte cap.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Event {
    /// Strictly positive job sequence below `u64::MAX`; zero/MAX are not event IDs.
    pub sequence: u64,
    /// Optional observed milliseconds since Unix epoch; `None` preserves unavailable wall time.
    pub utc_ms: Option<u64>,
    /// Monotonic elapsed milliseconds relative to the job's origin, as published by its owner.
    pub elapsed_ms: u64,
    /// Owning stage; batch validation requires membership in the operation layout.
    pub stage: StageId,
    /// Log/progress/stage/diagnostic purpose.
    pub kind: EventKind,
    /// Display severity independent of child output stream.
    pub level: Level,
    /// Child stream or internal source.
    pub source: Source,
    /// At most 1,024 UTF-8 text bytes; the stricter full-event cap may allow fewer.
    pub text: Text<1024>,
    /// Quantified progress if a meaningful nonzero total is available.
    pub progress: Option<Progress>,
    /// Optional setup tool; forbidden on non-setup stages.
    pub tool: Option<ToolId>,
    /// Exact setup step, when known; must agree with `stage` and `tool`.
    pub setup_step: Option<crate::setup::SetupStepId>,
}
impl Wire for Event {
    /// Entire event JSON cap, including text escaping and envelope fields.
    const MAX_BYTES: usize = 1024;

    /// Return success for sequence, optional progress/setup bindings and serialized size invariants.
    /// Ordering relative to other events is validated by [`EventBatch`].
    fn validate(&self) -> Result<(), Error> {
        if self.sequence == 0
            || self.sequence == u64::MAX
            || (self.tool.is_some() && !matches!(self.stage, StageId::Setup(_)))
        {
            return Err(Error::Schema);
        }
        if let Some(p) = self.progress {
            p.validate()?;
        }
        if let Some(step) = self.setup_step {
            step.validate()?;
            if self.stage != StageId::Setup(step.stage) || self.tool != Some(step.tool) {
                return Err(Error::Schema);
            }
        }
        serialized_bound(self, Self::MAX_BYTES)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// One setup tool's state within an aggregate setup stage.
pub struct ToolSummary {
    /// Closed tool identity; enclosing summaries require strict canonical order.
    pub tool: ToolId,
    /// Observed tool-step state in this aggregate phase.
    pub state: StageState,
    /// Optional bounded diagnostic/skip explanation.
    pub reason: Option<Text<512>>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// Stage lifecycle snapshot; timestamps are job-relative milliseconds and may be unavailable.
pub struct StageSummary {
    /// Exact position identity in the operation's required layout.
    pub stage: StageId,
    /// Current or terminal state.
    pub state: StageState,
    /// Explanation; enclosing validation requires one for skipped stages.
    pub reason: Option<Text<512>>,
    /// Job-relative start time when observed; `None` does not mean zero.
    pub started_ms: Option<u64>,
    /// Job-relative end time when observed; must not precede a present start time.
    pub ended_ms: Option<u64>,
    /// Setup-only tool summaries, unique and ordered by [`ToolId`].
    pub tools: BoundedVec<ToolSummary, 16>,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// Sticky accounting anomalies reported by the job owner; all-false is the initial state.
pub struct Anomalies {
    /// A requested UTC timestamp could not be obtained or represented.
    pub utc_unavailable: bool,
    /// Elapsed-time observation could not follow the expected monotonic accounting.
    pub elapsed_anomaly: bool,
    /// An accounting counter could not advance without overflow.
    pub counter_overflow: bool,
    /// Event numbering reached its reserved terminal boundary.
    pub event_sequence_exhausted: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// One Windows process exit code represented as the same 32 bits in unsigned/signed form.
pub struct RawExit {
    /// Original unsigned process exit bits, including NTSTATUS-style failures.
    pub windows_u32: u32,
    /// Two's-complement signed interpretation of exactly `windows_u32`.
    pub interpreted_i32: i32,
}
impl RawExit {
    /// Return success only when both representations have identical bits.
    pub fn validate(self) -> Result<(), Error> {
        if self.interpreted_i32 == self.windows_u32 as i32 {
            Ok(())
        } else {
            Err(Error::Schema)
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// Authoritative in-session job snapshot with bounded stages and conservative write evidence.
/// A closed stream fixes `final_next`; success additionally requires operation-specific stage/artifact checks.
pub struct JobSummary {
    /// Session owning this job; historical copies do not authorize operations.
    pub session_id: SessionId,
    /// Unique identity within the owning session.
    pub job_id: JobId,
    /// Operation defining the exact stage list and success criteria.
    pub operation: OperationKind,
    /// At most eight stage summaries in exactly [`OperationKind::layout`] order.
    pub stages: BoundedVec<StageSummary, 8>,
    /// Cancellation has been requested; this does not imply immediate child/process cleanup.
    pub cancel_requested: bool,
    /// Last relevant child exit observation, if available.
    pub raw_exit: Option<RawExit>,
    /// Flash write evidence; non-flash operations must retain `NotStarted`.
    pub write_outcome: WriteOutcome,
    /// Admitted artifact; required for successful builds, not inferred from files on disk.
    pub artifact_id: Option<ArtifactId>,
    /// Selected device identity if this job has one.
    pub device_id: Option<DeviceId>,
    /// Explicitly retried prior job correlation, without automatic-resume authority.
    pub retry_of: Option<JobId>,
    /// Terminal reason when established; required for a closed event stream.
    pub terminal_cause: Option<TerminalCause>,
    /// Fixed exclusive event-sequence upper bound after closure; never zero.
    pub final_next: Option<u64>,
    /// No further events can be published; exactly equivalent to `final_next.is_some()`.
    pub events_closed: bool,
    /// Must remain false: transfer/reboot tooling does not observe application execution.
    pub running_firmware_observed: bool,
    /// Captured child output is incomplete; successful stages do not erase this evidence.
    pub output_incomplete: bool,
    /// Chunks lost during output ingress or exhausted event sequencing; excludes ordinary event eviction.
    pub output_dropped_chunks: u64,
    /// Number of output bytes discarded, independent of retained event count.
    pub output_dropped_bytes: u64,
    /// Clock/counter anomalies retained for interpretation of reported numbers.
    pub anomalies: Anomalies,
    /// Exact currently running setup step; forbidden outside active setup jobs.
    pub active_setup_step: Option<crate::setup::SetupStepId>,
}
impl Wire for JobSummary {
    /// Complete job-summary JSON cap in bytes.
    const MAX_BYTES: usize = 65536;

    /// Return success for layout, lifecycle, timing, setup-step and conservative write/result invariants.
    /// Non-setup jobs allow at most one running stage. Closed jobs require terminal stages;
    /// successful builds require an artifact and successful flashes require verified bytes.
    /// This validates a snapshot, not the entire transition history or the truth of external observations.
    fn validate(&self) -> Result<(), Error> {
        if self.operation.layout() != self.stages.iter().map(|s| s.stage).collect::<Vec<_>>()
            || self.running_firmware_observed
        {
            return Err(Error::Schema);
        }
        if self.events_closed != self.final_next.is_some()
            || self.final_next == Some(0)
            || (self.events_closed
                && (self.terminal_cause.is_none() || self.stages.iter().any(|s| !s.state.terminal())))
        {
            return Err(Error::Schema);
        }
        if self.operation != OperationKind::Setup
            && self.stages.iter().filter(|s| s.state == StageState::Running).count() > 1
        {
            return Err(Error::Schema);
        }
        if let Some(step) = self.active_setup_step {
            step.validate()?;
            if self.operation != OperationKind::Setup
                || self.events_closed
                || !self.stages.iter().any(|s| {
                    s.stage == StageId::Setup(step.stage)
                        && s.state == StageState::Running
                        && s.tools
                            .iter()
                            .any(|t| t.tool == step.tool && t.state == StageState::Running)
                })
            {
                return Err(Error::Schema);
            }
        }
        for s in self.stages.iter() {
            if !matches!(s.stage, StageId::Setup(_)) && !s.tools.is_empty() {
                return Err(Error::Schema);
            }
            if s.tools.windows(2).any(|w| w[0].tool >= w[1].tool) {
                return Err(Error::Schema);
            }
            if s.state == StageState::Skipped && s.reason.is_none() {
                return Err(Error::Schema);
            }
            if let (Some(a), Some(b)) = (s.started_ms, s.ended_ms)
                && a > b
            {
                return Err(Error::Schema);
            }
        }
        if let Some(exit) = self.raw_exit {
            exit.validate()?;
        }
        if self.operation != OperationKind::Flash && self.write_outcome != WriteOutcome::NotStarted {
            return Err(Error::Schema);
        }
        if self.terminal_cause == Some(TerminalCause::Succeeded)
            && (!self.events_closed
                || self.cancel_requested
                || self.stages.iter().any(|s| {
                    s.state != StageState::Succeeded
                        && !(self.operation == OperationKind::Setup && s.state == StageState::Skipped)
                })
                || (self.operation == OperationKind::Build && self.artifact_id.is_none())
                || (self.operation == OperationKind::Flash && self.write_outcome != WriteOutcome::Verified))
        {
            return Err(Error::Schema);
        }
        if self.terminal_cause == Some(TerminalCause::Cancelled) && !self.cancel_requested {
            return Err(Error::Schema);
        }
        if self.write_outcome == WriteOutcome::Verified
            && !self
                .stages
                .iter()
                .any(|s| s.stage == StageId::Firmware(FirmwareStage::Verification) && s.state == StageState::Succeeded)
        {
            return Err(Error::Schema);
        }
        if self.write_outcome == WriteOutcome::TransferredUnverified
            && !self
                .stages
                .iter()
                .any(|s| s.stage == StageId::Firmware(FirmwareStage::Transfer) && s.state == StageState::Succeeded)
        {
            return Err(Error::Schema);
        }
        serialized_bound(self, Self::MAX_BYTES)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// Cursor and batch limit for replaying retained job events.
pub struct PollRequest {
    /// Last sequence already received; zero requests from the beginning of retained history.
    pub after: u64,
    /// Maximum requested records, inclusive range 1 through 128.
    pub limit: u16,
}
impl PollRequest {
    /// Return success for a bounded batch count; cursor availability is resolved by the job owner.
    pub fn validate(self) -> Result<(), Error> {
        if (1..=128).contains(&self.limit) {
            Ok(())
        } else {
            Err(Error::Bounds)
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// Retained event window plus one coherent job summary; gaps may exist because retention is bounded.
pub struct EventBatch {
    /// Owning session, required to match the summary.
    pub session_id: SessionId,
    /// Owning job, required to match the summary.
    pub job_id: JobId,
    /// First retained sequence or the next sequence when the retained window is empty; nonzero.
    pub oldest: u64,
    /// Exclusive upper sequence bound; equals `summary.final_next` once closed.
    pub next: u64,
    /// Number of previously emitted events dropped from bounded retention.
    pub dropped: u64,
    /// At most 128 events in strictly increasing sequence, each within `[oldest, next)`.
    pub records: BoundedVec<Event, 128>,
    /// Job state corresponding to this response and its cursor bounds.
    pub summary: JobSummary,
}
impl Wire for EventBatch {
    /// Complete batch JSON cap in bytes, including the summary.
    const MAX_BYTES: usize = 262144;

    /// Return success for summary/identity/cursor coherence, ordered event membership and byte bounds.
    /// Missing sequences are allowed; no contiguity or external timestamp validity is inferred.
    fn validate(&self) -> Result<(), Error> {
        self.summary.validate()?;
        if self.session_id != self.summary.session_id
            || self.job_id != self.summary.job_id
            || self.oldest == 0
            || self.oldest > self.next
            || (self.summary.events_closed && self.summary.final_next != Some(self.next))
        {
            return Err(Error::Schema);
        }
        let mut prev = None;
        for e in self.records.iter() {
            e.validate()?;
            if e.sequence < self.oldest
                || e.sequence >= self.next
                || prev.is_some_and(|p| p >= e.sequence)
                || !self.summary.operation.layout().contains(&e.stage)
            {
                return Err(Error::Schema);
            }
            prev = Some(e.sequence);
        }
        serialized_bound(self, Self::MAX_BYTES)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// At most sixteen current-session jobs, each appearing once.
pub struct JobList {
    /// Session to which every contained summary must belong.
    pub session_id: SessionId,
    /// Unique job summaries; order is producer-defined, not validated as chronological.
    pub jobs: BoundedVec<JobSummary, 16>,
}
impl Wire for JobList {
    /// Aggregate JSON cap allowing sixteen maximal summaries plus envelope overhead.
    const MAX_BYTES: usize = 16 * 65536 + 1024;

    /// Return success for individually valid summaries, matching sessions, unique job IDs and byte cap.
    fn validate(&self) -> Result<(), Error> {
        for (i, j) in self.jobs.iter().enumerate() {
            j.validate()?;
            if j.session_id != self.session_id || self.jobs[..i].iter().any(|p| p.job_id == j.job_id) {
                return Err(Error::Schema);
            }
        }
        serialized_bound(self, Self::MAX_BYTES)
    }
}
/// Return `value` milliseconds since Unix epoch as `f64` only within JavaScript Date's positive range.
/// The admitted maximum is below 2^53, so conversion is exact. The actual constructed Date must still
/// have finite `getTime()`; this helper neither constructs a Date nor observes the host clock.
pub fn browser_utc_ms(value: u64) -> Option<f64> {
    if value <= 8_640_000_000_000_000 {
        Some(value as f64)
    } else {
        None
    }
}
