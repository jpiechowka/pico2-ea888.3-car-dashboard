use serde::{Deserialize, Serialize};

use crate::*;

macro_rules! enums { ($($name:ident { $($v:ident),+ $(,)? })+) => {$ (
    #[derive(Clone,Copy,Debug,PartialEq,Eq,PartialOrd,Ord,Serialize,Deserialize)]
    #[serde(rename_all="snake_case")]
    pub enum $name { $($v),+ }
)+}; }
enums! {
 OperationKind { Build, Flash, Setup, PrerequisiteCheck, DeviceScan, Cleanup }
 FirmwareStage { Prerequisites, Snapshot, Compilation, Validation, DeviceSelection, Transfer, Verification, Reboot }
 SetupStage { Check, Download, Verify, Install, Validate }
 ProbeStage { Prerequisites, DeviceDetection }
 CleanupStage { Cleanup }
 StageState { Pending, Running, Succeeded, Failed, Cancelled, Skipped }
 WriteOutcome { NotStarted, InProgress, TransferredUnverified, Verified, Unknown }
 EventKind { Log, Progress, Stage, Diagnostic }
 Level { Info, Warning, Error }
 Source { Stdout, Stderr, Internal }
 ProgressUnit { Bytes, Items, Percent }
 TerminalCause { Succeeded, Failed, Cancelled, Deadline, OwnershipCleanupFatal }
 ToolId { Rustup, RustToolchain, Rustfmt, Clippy, RustSrc, LlvmTools, ThumbTarget, WasmTarget, Python, MsvcSdk, Trunk, WasmBindgen, Binaryen, Picotool, CargoCache, NativeHelper }
}
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
pub const SETUP_STAGES: [SetupStage; 5] = [
    SetupStage::Check,
    SetupStage::Download,
    SetupStage::Verify,
    SetupStage::Install,
    SetupStage::Validate,
];
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "scope", content = "name", rename_all = "snake_case", deny_unknown_fields)]
pub enum StageId {
    Firmware(FirmwareStage),
    Setup(SetupStage),
    Probe(ProbeStage),
    Cleanup(CleanupStage),
}
impl OperationKind {
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
    pub fn terminal(self) -> bool { !matches!(self, Self::Pending | Self::Running) }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Progress {
    pub completed: u64,
    pub total: u64,
    pub unit: ProgressUnit,
}
impl Progress {
    pub fn validate(&self) -> Result<(), Error> {
        if self.total == 0 || self.completed > self.total || (self.unit == ProgressUnit::Percent && self.total != 100) {
            Err(Error::Schema)
        } else {
            Ok(())
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub sequence: u64,
    pub utc_ms: Option<u64>,
    pub elapsed_ms: u64,
    pub stage: StageId,
    pub kind: EventKind,
    pub level: Level,
    pub source: Source,
    pub text: Text<1024>,
    pub progress: Option<Progress>,
    pub tool: Option<ToolId>,
}
impl Wire for Event {
    const MAX_BYTES: usize = 1024;

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
        serialized_bound(self, Self::MAX_BYTES)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolSummary {
    pub tool: ToolId,
    pub state: StageState,
    pub reason: Option<Text<512>>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StageSummary {
    pub stage: StageId,
    pub state: StageState,
    pub reason: Option<Text<512>>,
    pub started_ms: Option<u64>,
    pub ended_ms: Option<u64>,
    pub tools: BoundedVec<ToolSummary, 16>,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Anomalies {
    pub utc_unavailable: bool,
    pub elapsed_anomaly: bool,
    pub counter_overflow: bool,
    pub event_sequence_exhausted: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawExit {
    pub windows_u32: u32,
    pub interpreted_i32: i32,
}
impl RawExit {
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
pub struct JobSummary {
    pub session_id: SessionId,
    pub job_id: JobId,
    pub operation: OperationKind,
    pub stages: BoundedVec<StageSummary, 8>,
    pub cancel_requested: bool,
    pub raw_exit: Option<RawExit>,
    pub write_outcome: WriteOutcome,
    pub artifact_id: Option<ArtifactId>,
    pub device_id: Option<DeviceId>,
    pub retry_of: Option<JobId>,
    pub terminal_cause: Option<TerminalCause>,
    pub final_next: Option<u64>,
    pub events_closed: bool,
    pub running_firmware_observed: bool,
    pub output_incomplete: bool,
    pub output_dropped_chunks: u64,
    pub output_dropped_bytes: u64,
    pub anomalies: Anomalies,
}
impl Wire for JobSummary {
    const MAX_BYTES: usize = 65536;

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
        if self.stages.iter().filter(|s| s.state == StageState::Running).count() > 1 {
            return Err(Error::Schema);
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
        serialized_bound(self, Self::MAX_BYTES)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PollRequest {
    pub after: u64,
    pub limit: u16,
}
impl PollRequest {
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
pub struct EventBatch {
    pub session_id: SessionId,
    pub job_id: JobId,
    pub oldest: u64,
    pub next: u64,
    pub dropped: u64,
    pub records: BoundedVec<Event, 128>,
    pub summary: JobSummary,
}
impl Wire for EventBatch {
    const MAX_BYTES: usize = 262144;

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
pub struct JobList {
    pub session_id: SessionId,
    pub jobs: BoundedVec<JobSummary, 16>,
}
impl Wire for JobList {
    const MAX_BYTES: usize = 16 * 65536 + 1024;

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
/// Projection admission only. Actual JS Date must additionally have finite getTime().
pub fn browser_utc_ms(value: u64) -> Option<f64> {
    if value <= 8_640_000_000_000_000 {
        Some(value as f64)
    } else {
        None
    }
}
