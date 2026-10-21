//! Private setup transcript schemas and bounded framing; these pure values do not authenticate OS peers.
use serde::{Deserialize, Serialize};

use crate::*;
/// Generate documented closed enums with stable snake_case serde spellings.
macro_rules! enums {($($(#[$ndoc:meta])* $n:ident {$($(#[$vdoc:meta])* $v:ident),+ $(,)?})+) => {$ (
$(#[$ndoc])*
#[derive(Clone,Copy,Debug,PartialEq,Eq,PartialOrd,Ord,Serialize,Deserialize)]
#[serde(rename_all="snake_case")] pub enum $n {$($(#[$vdoc])* $v),+}
)+};}
enums! {
 /// Explicit setup intent; does not itself grant installation or process authority.
 SetupAction {
   /// Fill missing required prerequisites.
   MissingRequired,
   /// Reconcile prerequisites to the repository's pinned versions.
   UpdatePinned
 }
 /// Role of one retained private peer association.
 SetupRole {
   /// Native setup bootstrap allowed one continuation handoff.
   Bootstrap,
   /// Setup continuation with no further delegation.
   Continuation
 }
 /// Peer-requested retirement reason; abnormal closure stops the ledger before acknowledgement.
 SetupCloseDisposition {
   /// Result receipt completed; this alone does not publish an executable.
   Completed,
   /// Command/setup failure.
   Failed,
   /// Explicit cancellation.
   Cancelled,
   /// Process ownership cleanup could not be established.
   CleanupFatal
 }
 /// Closed setup failure vocabulary, without command output or private capability material.
 SetupFailure {
   /// Unsupported or inconsistent message schema.
   InvalidSchema,
   /// Record, byte or collection limit exceeded.
   BoundsExceeded,
   /// Attempt/input context differs from the admitted one.
   ContextMismatch,
   /// Process/image identity differs from the bound peer.
   PeerMismatch,
   /// Private nonce differs from the bound association.
   NonceMismatch,
   /// Duplicate or out-of-order transcript input.
   Replay,
   /// Owning process/lease disappeared.
   OwnerLost,
   /// Transition is invalid for the current transcript or ledger state.
   InvalidState,
   /// Cancellation was requested.
   Cancelled,
   /// A bounded operation or handshake deadline expired.
   Deadline,
   /// Required prerequisite is unavailable.
   PrerequisiteUnavailable,
   /// Pinned bytes/identity failed integrity admission.
   IntegrityMismatch,
   /// An admitted command failed.
   CommandFailed,
   /// Captured output was incomplete.
   OutputIncomplete,
   /// Owned process/worker cleanup failed; overrides earlier ledger stop causes.
   OwnershipCleanupFatal
 }
 /// Native owner's observation of its owned process tree, not a statement about all system processes.
 SetupTreeState {
   /// No owned descendants remain.
   Empty,
   /// Owned descendants are known to remain.
   NotEmpty,
   /// Tree emptiness could not be established.
   Unknown
 }
 /// Whether every associated capture/transport worker has joined.
 SetupWorkerState {
   /// Workers have joined.
   Joined,
   /// At least one worker has not joined.
   Unjoined
 }
 /// Terminal continuation report classification; publication remains separately gated.
 SetupResultState {
   /// Assigned outcomes succeeded/skipped and required identities/cleanup observations are present.
   Succeeded,
   /// Setup failed.
   Failed,
   /// Setup was cancelled.
   Cancelled,
   /// Cleanup is fatal or unresolved; nonterminal step evidence may remain.
   CleanupFatal
 }
 /// Requested publication disposition, not proof that any executable has been replaced or restarted.
 SetupPublication {
   /// No installation change awaits publication.
   NoChange,
   /// Verified output awaits owner-controlled publication.
   PendingPublication,
   /// Published tooling needs an explicit restart before use.
   PendingRestart,
   /// Result cannot be published.
   NotPublishable
 }
 /// Owner of a planned step; the ledger permits only its next assigned executor.
 SetupExecutor {
   /// Original setup owner executes locally.
   OriginalOwner,
   /// First retained peer association.
   Peer0,
   /// One continuation association, available only to native delegation.
   Peer1
 }
 /// Maximum private-peer topology admitted for one setup attempt.
 DelegationShape {
   /// Original owner plus one continuation; no peer1.
   Cold,
   /// Original owner, bootstrap peer0 and at most one continuation peer1.
   Native
 }
 /// Closed semantic command catalogue; actual executable/arguments belong to the native setup adapter.
 SetupCommand {
   /// Query the selected tool's normal version output.
   Version,
   /// Acquire an archive pinned by the repository manifest.
   AcquirePinned,
   /// Verify acquired bytes against their pin.
   VerifyPinned,
   /// Install the verified pinned artifact.
   InstallPinned,
   /// Query verbose rustc identity.
   RustcVerbose,
   /// Query Cargo identity.
   CargoVersion,
   /// Resolve rustc executable candidates.
   WhereRustc,
   /// Resolve Cargo executable candidates.
   WhereCargo,
   /// Install the exact pinned Rust toolchain.
   InstallToolchain,
   /// Check a required rustup component.
   ComponentPresent,
   /// Install a required rustup component.
   InstallComponent,
   /// Check a required cross-compilation target.
   TargetPresent,
   /// Install a required cross-compilation target.
   InstallTarget,
   /// Probe required Python standard-library imports.
   StdlibImports,
   /// Discover the required MSVC compiler and Windows SDK.
   DiscoverMsvcSdk,
   /// Compile/link a native prerequisite smoke target.
   HostLinkSmoke,
   /// Build a Studio prerequisite smoke target.
   StudioBuildSmoke,
   /// Check installed executable content/version identity.
   InstalledIdentity,
   /// Query picotool's short version form.
   VersionShort,
   /// Query picotool's full version form.
   VersionFull,
   /// Inspect load command help without loading a device.
   HelpLoad,
   /// Inspect verify command help without reading a device.
   HelpVerify,
   /// Inspect reboot command help without rebooting a device.
   HelpReboot,
   /// Inspect partition command help without changing a device.
   HelpPartition,
   /// Inspect UF2 command help.
   HelpUf2,
   /// Inspect Cargo dependency cache state.
   CacheInventory,
   /// Fetch locked root-workspace dependencies.
   FetchRoot,
   /// Fetch locked firmware dependencies.
   FetchFirmware,
   /// Fetch locked native-helper dependencies.
   FetchHelper,
   /// Fetch locked configurator dependencies.
   FetchConfigurator,
   /// Fetch locked host-test dependencies.
   FetchHost,
   /// Validate the root dependency cache.
   CacheRoot,
   /// Validate the firmware dependency cache.
   CacheFirmware,
   /// Validate the native-helper dependency cache.
   CacheHelper,
   /// Validate the configurator dependency cache.
   CacheConfigurator,
   /// Validate the host-test dependency cache.
   CacheHost,
   /// Build the selected native-helper executable.
   BuildNativeHelper,
   /// Query and validate the newly built native-helper identity.
   NativeIdentity
 }
}
/// Exact secret spelling; deliberately no Debug/Display. Serialize only through private codec.
#[derive(Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct Nonce(
    /// Exactly 64 lowercase hex bytes; secrecy depends on callers keeping private frames out of logs.
    Text<64>,
);
impl Nonce {
    /// Return a nonce preserving exact `s`; reject anything outside the 64-byte lowercase SHA-256 grammar.
    /// Syntax validation does not establish randomness, freshness or authority.
    pub fn new(s: &str) -> Result<Self, Error> {
        Sha256::new(s)?;
        Ok(Self(Text::new(s)?))
    }

    /// Return the borrowed secret spelling for redaction; callers must not log it.
    pub fn expose_for_redactor(&self) -> &str { self.0.as_str() }
}
impl<'de> Deserialize<'de> for Nonce {
    /// Deserialize from `d` and return a grammar-validated nonce or serde error, without lossy normalization.
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = Text::<64>::deserialize(d)?;
        Self::new(s.as_str()).map_err(serde::de::Error::custom)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// Immutable input digests bound to an attempt; equality checks do not recompute or authenticate the inputs.
pub struct SetupInputContext {
    /// Selected workspace identity digest.
    pub workspace_sha256: Sha256,
    /// Digest of the admitted aggregate setup context.
    pub context_sha256: Sha256,
    /// Prerequisite report digest used to derive the plan.
    pub report_sha256: Sha256,
    /// Selected source-tree identity digest.
    pub source_sha256: Sha256,
    /// Pinned tool manifest digest.
    pub tool_manifest_sha256: Sha256,
    /// Required compiler specification digest.
    pub compiler_spec_sha256: Sha256,
    /// Root Cargo.lock digest.
    pub root_lock_sha256: Sha256,
    /// Firmware Cargo.lock digest.
    pub firmware_lock_sha256: Sha256,
    /// Native-helper Cargo.lock digest.
    pub helper_lock_sha256: Sha256,
    /// Canonical setup-option digest.
    pub options_sha256: Sha256,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// Process and executable identity to compare with independent OS observations; PID alone is insufficient.
pub struct SetupPeerIdentity {
    /// Nonzero operating-system process identifier.
    pub pid: u32,
    /// Process creation time as [FILETIME](https://learn.microsoft.com/en-us/windows/win32/api/minwinbase/ns-minwinbase-filetime)
    /// ticks of 100 ns since 1601-01-01 UTC; preserves identity across PID reuse.
    pub creation_filetime_100ns: u64,
    /// SHA-256 of the bound executable bytes.
    pub image_sha256: Sha256,
    /// Executable filesystem volume identity, not a USB serial.
    pub image_volume_serial: u64,
    /// Executable's 128-bit file identity on that volume.
    pub image_file_id: FileId128,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// Private one-peer admission envelope; contains a secret nonce and intentionally has no Debug implementation.
pub struct SetupLeaseEnvelope {
    /// Private schema revision; only 1 is admitted.
    pub schema_version: u16,
    /// Owner's setup-attempt identity, never reusable for another attempt.
    pub attempt_id: AttemptId,
    /// Explicit setup action bound across continuation.
    pub action: SetupAction,
    /// Expected bootstrap/continuation role.
    pub role: SetupRole,
    /// Retained association index 0 or 1; index 1 must be a continuation.
    pub peer_slot: u8,
    /// Exact accepted setup inputs.
    pub context: SetupInputContext,
    /// Original owner's externally observed process/image identity.
    pub owner: SetupPeerIdentity,
    /// Executable digest expected for this peer.
    pub expected_peer_image_sha256: Sha256,
    /// Per-association secret, distinct from a delegated parent's nonce.
    pub nonce: Nonce,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// Exact admitted tool/stage/command tuple; enum membership alone does not make every combination legal.
pub struct SetupStepId {
    /// Tool whose prerequisite is being handled.
    pub tool: ToolId,
    /// Setup stage containing this command.
    pub stage: SetupStage,
    /// Closed command selector, not free-form shell input.
    pub command: SetupCommand,
}
/// Number of distinct admitted tuples returned by [`setup_steps`].
pub const SETUP_STEP_COUNT: usize = 82;
/// Return the complete 82-tuple allowlist in stable order; it is not a dependency-valid execution plan.
/// A native owner selects/orders a subset and assigns executors before constructing a ledger.
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
    /// Return success only when this exact tuple occurs in the closed allowlist; otherwise `Schema`.
    pub fn validate(self) -> Result<(), Error> {
        if setup_steps().contains(&self) {
            Ok(())
        } else {
            Err(Error::Schema)
        }
    }
}
/// Raw Windows exit bits plus their checked signed interpretation, shared with ordinary job stages.
pub type SetupRawExit = RawExit;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// One step's evidence; validation admits pending/running records for retained cleanup-failure reports.
pub struct SetupStepOutcome {
    /// Exact admitted step tuple.
    pub step: SetupStepId,
    /// Lifecycle state; a successful outcome cannot carry a failure cause.
    pub state: StageState,
    /// Required for Failed/Cancelled, optional for interrupted Skipped records.
    pub failure: Option<SetupFailure>,
    /// Process exit observation if available; absence is not exit code zero.
    pub raw_exit: Option<SetupRawExit>,
    /// Elapsed milliseconds supplied by the owner; this schema does not measure time.
    pub elapsed_ms: u64,
    /// Some process output was lost; command success alone does not repair it.
    pub output_incomplete: bool,
}
impl SetupStepOutcome {
    /// Return success for an admitted tuple, consistent raw exit/failure state and at most 512 JSON bytes.
    /// Executor assignment, ordering and terminal-state requirements are checked by the ledger/result owner.
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
/// Projection of ordered step evidence into one of five public setup stages.
pub struct SetupStageOutcome {
    /// Stage being summarized; array order is checked against [`crate::SETUP_STAGES`].
    pub stage: SetupStage,
    /// Aggregate state; unfinished work prevents a stage from becoming terminal.
    pub state: StageState,
    /// First applicable failure/cancellation/skipped cause retained by aggregation.
    pub failure: Option<SetupFailure>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// Native cleanup/output observations; pure validation cannot verify process-tree or worker reality.
pub struct SetupInternalClosure {
    /// Owned descendant-tree emptiness observation.
    pub tree: SetupTreeState,
    /// Capture/transport worker join observation.
    pub workers: SetupWorkerState,
    /// True when output evidence is incomplete, independent of command exit success.
    pub output_incomplete: bool,
    /// Count of known discarded output chunks; uncertainty remains represented separately.
    pub observed_discarded_chunks: u64,
    /// Count of known discarded output bytes.
    pub observed_discarded_bytes: u64,
    /// A counter could not represent the complete count; maximum is not an exact total.
    pub counter_overflow: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// Bounded continuation evidence; acceptance also needs exact owner ledger/context comparison and real cleanup checks.
pub struct SetupContinuationResult {
    /// Private result revision; only 1 is admitted.
    pub schema_version: u16,
    /// Original setup attempt whose evidence is reported.
    pub attempt_id: AttemptId,
    /// Action inherited from that attempt.
    pub action: SetupAction,
    /// Peer role inherited from its admission envelope.
    pub role: SetupRole,
    /// Association 0 or 1; second peer must be a continuation.
    pub peer_slot: u8,
    /// Exact immutable input bindings.
    pub context: SetupInputContext,
    /// Produced native executable digest; required for success, not proof of publication.
    pub native_exe_sha256: Option<Sha256>,
    /// Resolved tool-set digest; required for successful continuation roles.
    pub resolved_tools_sha256: Option<Sha256>,
    /// Resolved compiler identity digest; required for success.
    pub compiler_identity_sha256: Option<Sha256>,
    /// Five-stage projection; must equal aggregation of the reported steps exactly.
    pub stage_outcomes: [SetupStageOutcome; 5],
    /// At most 82 unique admitted step outcomes, later compared with the peer's assigned ledger slice.
    pub step_outcomes: BoundedVec<SetupStepOutcome, 82>,
    /// Result classification; only CleanupFatal may retain nonterminal steps.
    pub terminal: SetupResultState,
    /// Optional failure cause; forbidden for successful results.
    pub failure: Option<SetupFailure>,
    /// Independent cleanup/output facts; success requires empty owned tree and joined workers.
    pub closure: SetupInternalClosure,
    /// Desired publication disposition; validated owner policy must decide actual effects.
    pub publication: SetupPublication,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case", deny_unknown_fields)]
/// Closed bidirectional private transcript; structural decoding alone does not authorize transitions.
pub enum SetupLeaseMessage {
    /// Peer proves possession of its nonce and reports the externally bound identity.
    Hello {
        /// Exact secret from the admission envelope.
        nonce: Nonce,
        /// Must match the independent expected process/image observation.
        peer: SetupPeerIdentity,
    },
    /// Owner acknowledges the exact association secret.
    HelloAck {
        /// Original envelope nonce; no replacement or normalization.
        nonce: Nonce,
    },
    /// Peer requests the next step; execution waits for owner acknowledgement.
    AdmitStep {
        /// Exact next tuple assigned to this peer.
        step: SetupStepId,
    },
    /// Owner atomically marks the requested step Running.
    StepAdmitted {
        /// Tuple matching the outstanding admission request.
        step: SetupStepId,
    },
    /// Peer reports completion, holding the global receipt barrier until owner acknowledgement.
    StepFinished {
        /// Valid terminal evidence for the active step.
        outcome: SetupStepOutcome,
    },
    /// Owner releases the receipt barrier for the completed step.
    StepReceived {
        /// Tuple matching the outstanding receipt.
        step: SetupStepId,
    },
    /// Bootstrap requests its sole continuation image binding.
    PrepareContinuation {
        /// Expected continuation executable digest.
        image_sha256: Sha256,
    },
    /// Owner allocates the continuation association.
    ContinuationPrepared {
        /// Exactly 1; no third peer or slot recycling.
        peer_slot: u8,
        /// New secret distinct from the bootstrap nonce.
        nonce: Nonce,
    },
    /// Bootstrap supplies the prepared continuation's process/image identity.
    RegisterContinuation {
        /// Identity whose image must match preparation.
        peer: SetupPeerIdentity,
    },
    /// Owner acknowledges the bound continuation identity.
    ContinuationRegistered {
        /// Exactly 1.
        peer_slot: u8,
    },
    /// Peer announces separately transferred result bytes after all assigned steps are terminal.
    ResultReady {
        /// Digest to acknowledge and independently compare with the result bytes.
        result_sha256: Sha256,
        /// Nonzero result length, at most 65536 bytes.
        result_bytes: u32,
    },
    /// Owner acknowledges the announced result digest; actual result admission is separate.
    ResultReceived {
        /// Must match the outstanding ResultReady digest.
        result_sha256: Sha256,
    },
    /// Peer retires; Completed requires result receipt, while abnormal closure stops pending work immediately.
    Close {
        /// Retirement reason retained through the acknowledgement.
        disposition: SetupCloseDisposition,
    },
    /// Owner confirms the exact closure disposition.
    Closed {
        /// Must match the pending Close reason.
        disposition: SetupCloseDisposition,
    },
    /// Owner rejects an outstanding handshake; does not interrupt Idle/Executing via this transition.
    Rejected {
        /// Stop cause latched into the ledger.
        failure: SetupFailure,
    },
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// One length-framed control record; attempt, context, peer and sequence are checked again by the transcript.
pub struct SetupLeaseFrame {
    /// Private frame revision; only 1 is admitted.
    pub schema_version: u16,
    /// Owning setup-attempt identity.
    pub attempt_id: AttemptId,
    /// Accepted aggregate setup-context digest.
    pub context_sha256: Sha256,
    /// Retained association index 0 or 1.
    pub peer_slot: u8,
    /// Nonzero sequence 1..=1024 shared across both directions; transcript additionally counts the envelope.
    pub message_no: u16,
    /// Closed typed control payload, potentially containing secrets.
    pub body: SetupLeaseMessage,
}
impl Wire for SetupLeaseEnvelope {
    /// JSON body byte cap; four-byte framing brings the maximum to 4096.
    const MAX_BYTES: usize = 4092;

    /// Return success for revision/slot/role/owner PID and serialized size; real identity is caller-verified.
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
    /// JSON body byte cap before the four-byte little-endian prefix.
    const MAX_BYTES: usize = 4092;

    /// Return success for bounded schema/sequence/body fields; transition order/nonces are checked separately.
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
    /// Result JSON cap; large results use a separate bounded transfer, not a private control frame.
    const MAX_BYTES: usize = 65536;

    /// Return success for unique valid outcomes, exact stage aggregation, success identities/cleanup and size.
    /// Does not authenticate digests, validate publication policy or compare the owner's assigned steps.
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
/// Validate/serialize `value` and return a little-endian length prefix plus at most 4092 JSON bytes.
/// Serialization allocates before the body-size check; input types must retain their own bounded fields.
/// Errors publish no partial frame. Private nonce-bearing bytes must never enter public logs or HTTP routes.
pub fn encode_private<T: Wire>(value: &T) -> Result<Vec<u8>, Error> {
    let body = encode(value)?;
    if body.len() > 4092 {
        return Err(Error::Bounds);
    }
    let mut frame = (body.len() as u32).to_le_bytes().to_vec();
    frame.extend(body);
    Ok(frame)
}
/// Return a validated `T` from `frame`; reject outer length/prefix mismatch before JSON decoding/allocation.
/// Accept at most 4096 total bytes and require exact framing without trailing bytes; propagate codec errors.
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
