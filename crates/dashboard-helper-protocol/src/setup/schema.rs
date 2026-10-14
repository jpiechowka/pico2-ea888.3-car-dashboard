use serde::{Deserialize, Serialize};

use crate::*;
macro_rules! enums {($($n:ident {$($v:ident),+ $(,)?})+) => {$ (
#[derive(Clone,Copy,Debug,PartialEq,Eq,PartialOrd,Ord,Serialize,Deserialize)]
#[serde(rename_all="snake_case")] pub enum $n {$($v),+}
)+};}
enums! {
 SetupAction {MissingRequired,UpdatePinned}
 SetupRole {Bootstrap,Continuation}
 SetupCloseDisposition {Completed,Failed,Cancelled,CleanupFatal}
 SetupFailure {InvalidSchema,BoundsExceeded,ContextMismatch,PeerMismatch,NonceMismatch,Replay,OwnerLost,InvalidState,Cancelled,Deadline,PrerequisiteUnavailable,IntegrityMismatch,CommandFailed,OutputIncomplete,OwnershipCleanupFatal}
 SetupTreeState {Empty,NotEmpty,Unknown}
 SetupWorkerState {Joined,Unjoined}
 SetupResultState {Succeeded,Failed,Cancelled,CleanupFatal}
 SetupPublication {NoChange,PendingPublication,PendingRestart,NotPublishable}
 SetupExecutor {OriginalOwner,Peer0,Peer1}
 DelegationShape {Cold,Native}
 SetupCommand {Version,AcquirePinned,VerifyPinned,InstallPinned,RustcVerbose,CargoVersion,WhereRustc,WhereCargo,InstallToolchain,ComponentPresent,InstallComponent,TargetPresent,InstallTarget,StdlibImports,DiscoverMsvcSdk,HostLinkSmoke,StudioBuildSmoke,InstalledIdentity,VersionShort,VersionFull,HelpLoad,HelpVerify,HelpReboot,HelpPartition,HelpUf2,CacheInventory,FetchRoot,FetchFirmware,FetchHelper,FetchConfigurator,FetchHost,CacheRoot,CacheFirmware,CacheHelper,CacheConfigurator,CacheHost,BuildNativeHelper,NativeIdentity}
}
/// Exact secret spelling; deliberately no Debug/Display. Serialize only through private codec.
#[derive(Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct Nonce(Text<64>);
impl Nonce {
    pub fn new(s: &str) -> Result<Self, Error> {
        Sha256::new(s)?;
        Ok(Self(Text::new(s)?))
    }

    pub fn expose_for_redactor(&self) -> &str { self.0.as_str() }
}
impl<'de> Deserialize<'de> for Nonce {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = Text::<64>::deserialize(d)?;
        Self::new(s.as_str()).map_err(serde::de::Error::custom)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetupInputContext {
    pub workspace_sha256: Sha256,
    pub context_sha256: Sha256,
    pub report_sha256: Sha256,
    pub source_sha256: Sha256,
    pub tool_manifest_sha256: Sha256,
    pub compiler_spec_sha256: Sha256,
    pub root_lock_sha256: Sha256,
    pub firmware_lock_sha256: Sha256,
    pub helper_lock_sha256: Sha256,
    pub options_sha256: Sha256,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetupPeerIdentity {
    pub pid: u32,
    pub creation_filetime_100ns: u64,
    pub image_sha256: Sha256,
    pub image_volume_serial: u64,
    pub image_file_id: FileId128,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetupLeaseEnvelope {
    pub schema_version: u16,
    pub attempt_id: AttemptId,
    pub action: SetupAction,
    pub role: SetupRole,
    pub peer_slot: u8,
    pub context: SetupInputContext,
    pub owner: SetupPeerIdentity,
    pub expected_peer_image_sha256: Sha256,
    pub nonce: Nonce,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetupStepId {
    pub tool: ToolId,
    pub stage: SetupStage,
    pub command: SetupCommand,
}
pub const SETUP_STEP_COUNT: usize = 82;
pub fn setup_steps() -> Vec<SetupStepId> {
    use SetupCommand::*;
    use SetupStage::*;
    use ToolId::*;
    let mut steps = Vec::new();
    let mut add = |tool, stage, commands: &[SetupCommand]| {
        for &command in commands {
            steps.push(SetupStepId { tool, stage, command });
        }
    };
    for tool in [Rustup, Trunk, WasmBindgen, Binaryen] {
        add(tool, Check, &[Version]);
        add(tool, Download, &[AcquirePinned]);
        add(tool, Verify, &[VerifyPinned]);
        add(tool, Install, &[InstallPinned]);
        add(tool, Validate, &[Version]);
    }
    add(Trunk, Validate, &[StudioBuildSmoke]);
    for stage in [Check, Validate] {
        add(
            RustToolchain,
            stage,
            &[WhereRustc, WhereCargo, RustcVerbose, CargoVersion],
        );
    }
    add(RustToolchain, Install, &[InstallToolchain]);
    for tool in [Rustfmt, Clippy, RustSrc, LlvmTools] {
        add(tool, Check, &[ComponentPresent]);
        add(tool, Install, &[InstallComponent]);
        add(tool, Validate, &[ComponentPresent]);
    }
    for tool in [ThumbTarget, WasmTarget] {
        add(tool, Check, &[TargetPresent]);
        add(tool, Install, &[InstallTarget]);
        add(tool, Validate, &[TargetPresent]);
    }
    for stage in [Check, Validate] {
        add(Python, stage, &[Version, StdlibImports]);
    }
    add(Python, Download, &[AcquirePinned]);
    add(Python, Verify, &[VerifyPinned]);
    add(Python, Install, &[InstallPinned]);
    add(MsvcSdk, Check, &[DiscoverMsvcSdk]);
    add(MsvcSdk, Validate, &[HostLinkSmoke]);
    add(Picotool, Check, &[InstalledIdentity]);
    add(Picotool, Download, &[AcquirePinned]);
    add(Picotool, Verify, &[VerifyPinned]);
    add(Picotool, Install, &[InstallPinned]);
    add(
        Picotool,
        Validate,
        &[
            VersionShort,
            VersionFull,
            HelpLoad,
            HelpVerify,
            HelpReboot,
            HelpPartition,
            HelpUf2,
        ],
    );
    add(CargoCache, Check, &[CacheInventory]);
    add(
        CargoCache,
        Install,
        &[FetchRoot, FetchFirmware, FetchHelper, FetchConfigurator, FetchHost],
    );
    add(
        CargoCache,
        Validate,
        &[CacheRoot, CacheFirmware, CacheHelper, CacheConfigurator, CacheHost],
    );
    add(NativeHelper, Check, &[InstalledIdentity]);
    add(NativeHelper, Install, &[BuildNativeHelper]);
    add(NativeHelper, Validate, &[NativeIdentity]);
    steps
}
impl SetupStepId {
    pub fn validate(self) -> Result<(), Error> {
        if setup_steps().contains(&self) {
            Ok(())
        } else {
            Err(Error::Schema)
        }
    }
}
pub type SetupRawExit = RawExit;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetupStepOutcome {
    pub step: SetupStepId,
    pub state: StageState,
    pub failure: Option<SetupFailure>,
    pub raw_exit: Option<SetupRawExit>,
    pub elapsed_ms: u64,
    pub output_incomplete: bool,
}
impl SetupStepOutcome {
    pub fn validate(&self) -> Result<(), Error> {
        self.step.validate()?;
        if let Some(e) = self.raw_exit {
            e.validate()?;
        }
        if self.state == StageState::Succeeded && self.failure.is_some() {
            return Err(Error::Schema);
        }
        if matches!(self.state, StageState::Failed | StageState::Cancelled) && self.failure.is_none() {
            return Err(Error::Schema);
        }
        serialized_bound(self, 512)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetupStageOutcome {
    pub stage: SetupStage,
    pub state: StageState,
    pub failure: Option<SetupFailure>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetupInternalClosure {
    pub tree: SetupTreeState,
    pub workers: SetupWorkerState,
    pub output_incomplete: bool,
    pub observed_discarded_chunks: u64,
    pub observed_discarded_bytes: u64,
    pub counter_overflow: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetupContinuationResult {
    pub schema_version: u16,
    pub attempt_id: AttemptId,
    pub action: SetupAction,
    pub role: SetupRole,
    pub peer_slot: u8,
    pub context: SetupInputContext,
    pub native_exe_sha256: Option<Sha256>,
    pub resolved_tools_sha256: Option<Sha256>,
    pub compiler_identity_sha256: Option<Sha256>,
    pub stage_outcomes: [SetupStageOutcome; 5],
    pub step_outcomes: BoundedVec<SetupStepOutcome, 82>,
    pub terminal: SetupResultState,
    pub failure: Option<SetupFailure>,
    pub closure: SetupInternalClosure,
    pub publication: SetupPublication,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case", deny_unknown_fields)]
pub enum SetupLeaseMessage {
    Hello { nonce: Nonce, peer: SetupPeerIdentity },
    HelloAck { nonce: Nonce },
    AdmitStep { step: SetupStepId },
    StepAdmitted { step: SetupStepId },
    StepFinished { outcome: SetupStepOutcome },
    StepReceived { step: SetupStepId },
    PrepareContinuation { image_sha256: Sha256 },
    ContinuationPrepared { peer_slot: u8, nonce: Nonce },
    RegisterContinuation { peer: SetupPeerIdentity },
    ContinuationRegistered { peer_slot: u8 },
    ResultReady { result_sha256: Sha256, result_bytes: u32 },
    ResultReceived { result_sha256: Sha256 },
    Close { disposition: SetupCloseDisposition },
    Closed { disposition: SetupCloseDisposition },
    Rejected { failure: SetupFailure },
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetupLeaseFrame {
    pub schema_version: u16,
    pub attempt_id: AttemptId,
    pub context_sha256: Sha256,
    pub peer_slot: u8,
    pub message_no: u16,
    pub body: SetupLeaseMessage,
}
impl Wire for SetupLeaseEnvelope {
    const MAX_BYTES: usize = 4092;

    fn validate(&self) -> Result<(), Error> {
        if self.schema_version != 1
            || self.peer_slot > 1
            || (self.peer_slot == 1 && self.role != SetupRole::Continuation)
            || self.owner.pid == 0
        {
            return Err(Error::Schema);
        }
        serialized_bound(self, Self::MAX_BYTES)
    }
}
impl Wire for SetupLeaseFrame {
    const MAX_BYTES: usize = 4092;

    fn validate(&self) -> Result<(), Error> {
        if self.schema_version != 1 || self.peer_slot > 1 || self.message_no == 0 || self.message_no > 1024 {
            return Err(Error::Schema);
        }
        use SetupLeaseMessage::*;
        match &self.body {
            AdmitStep { step } | StepAdmitted { step } | StepReceived { step } => step.validate()?,
            StepFinished { outcome } => outcome.validate()?,
            Hello { peer, .. } | RegisterContinuation { peer } if peer.pid == 0 => return Err(Error::Schema),
            ContinuationPrepared { peer_slot, .. } | ContinuationRegistered { peer_slot } if *peer_slot != 1 => {
                return Err(Error::Schema);
            }
            ResultReady { result_bytes, .. } if *result_bytes == 0 || *result_bytes > 65536 => {
                return Err(Error::Bounds);
            }
            _ => {}
        }
        serialized_bound(self, Self::MAX_BYTES)
    }
}
impl Wire for SetupContinuationResult {
    const MAX_BYTES: usize = 65536;

    fn validate(&self) -> Result<(), Error> {
        if self.schema_version != 1
            || self.peer_slot > 1
            || (self.peer_slot == 1 && self.role != SetupRole::Continuation)
        {
            return Err(Error::Schema);
        }
        for (i, o) in self.step_outcomes.iter().enumerate() {
            o.validate()?;
            if self.step_outcomes[..i].iter().any(|p| p.step == o.step) {
                return Err(Error::Schema);
            }
        }
        if self.stage_outcomes != super::aggregate(&self.step_outcomes) {
            return Err(Error::Schema);
        }
        for s in &self.stage_outcomes {
            serialized_bound(s, 256)?;
        }
        if self.terminal == SetupResultState::Succeeded
            && (self.failure.is_some()
                || self.native_exe_sha256.is_none()
                || self.compiler_identity_sha256.is_none()
                || (self.role == SetupRole::Continuation && self.resolved_tools_sha256.is_none())
                || self.closure.tree != SetupTreeState::Empty
                || self.closure.workers != SetupWorkerState::Joined
                || self
                    .step_outcomes
                    .iter()
                    .any(|o| !matches!(o.state, StageState::Succeeded | StageState::Skipped) || o.failure.is_some()))
        {
            return Err(Error::Schema);
        }
        if self.terminal != SetupResultState::CleanupFatal && self.step_outcomes.iter().any(|o| !o.state.terminal()) {
            return Err(Error::Schema);
        }
        serialized_bound(self, Self::MAX_BYTES)
    }
}
/// Length checked before allocating/decoding. The four-byte prefix is included in4096.
pub fn encode_private<T: Wire>(value: &T) -> Result<Vec<u8>, Error> {
    let body = encode(value)?;
    if body.len() > 4092 {
        return Err(Error::Bounds);
    }
    let mut frame = (body.len() as u32).to_le_bytes().to_vec();
    frame.extend(body);
    Ok(frame)
}
pub fn decode_private<T: Wire>(frame: &[u8]) -> Result<T, Error> {
    if frame.len() < 4 || frame.len() > 4096 {
        return Err(Error::Bounds);
    }
    let n = u32::from_le_bytes(frame[..4].try_into().map_err(|_| Error::Schema)?) as usize;
    if n > 4092 || n != frame.len() - 4 {
        return Err(Error::Bounds);
    }
    decode(&frame[4..])
}
