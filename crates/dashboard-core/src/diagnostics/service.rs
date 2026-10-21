//! Single transport owner, shared unchanged by firmware and host port replays.
//! No DMA, allocation, automatic baud scan, EEPROM write or guessed RX metadata.
use core::task::{Context, Poll};

use heapless::Deque;

use super::CanFrame;
use super::bootstrap::{self, Bootstrap};
use super::client::{self, TxAction};
use super::longan::{BridgeError, RawRecord, Rx12};
use super::serial_tx::{self, Completion, Pump, Work};
use super::session::{Counters, PreparedSession, Session, SessionError};
use crate::acquisition::DashboardSnapshot;

/// Maximum UART bytes consumed in one service pass; short reads do not prove empty backlog.
pub const RX_WORK_BYTES: usize = 32;
/// Raw observation ring capacity; newer complete records overwrite the oldest.
pub const RAW_TRACE_RECORDS: usize = 16;
/// Host policy, not a bridge guarantee. Incomplete records never get rephased.
/// Inclusive milliseconds from the first byte of an RX12 record before recovery.
pub const PARTIAL_RECORD_MS: u64 = 100;
/// Maximum write/flush/idle progress transitions in one pass.
const TX_WORK_TRANSITIONS: usize = 4;
/// Online milliseconds before resetting exponential recovery backoff.
const STABLE_MS: u64 = 10_000;

/// Port error classification; receive faults invalidate framing, never create samples.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IoError {
    /// UART or software receive storage lost bytes.
    Overrun,
    /// UART stop-bit/framing fault.
    Framing,
    /// UART parity mismatch.
    Parity,
    /// UART break condition.
    Break,
    /// Other port-specific error without a more precise supported classification.
    Other,
}

/// Polls must be bounded. Pending read/write must have no copied-byte effects;
/// the reviewed buffered HAL meets this, generic async IO traits do not promise
/// it. Only this owner writes the port or calls discard. No raw FIFO reads.
pub trait Port {
    /// Polls using `cx` to fill a prefix of nonempty `bytes`. Returns Ready with
    /// 1..=bytes.len() copied bytes or an error, or Pending with zero copied bytes
    /// and no retained slice; dropping the pending operation must be safe.
    fn poll_read(
        &mut self,
        cx: &mut Context<'_>,
        bytes: &mut [u8],
    ) -> Poll<Result<usize, IoError>>;
    /// Polls using `cx` to queue a prefix of nonempty `bytes`. Returns Ready with
    /// 1..=bytes.len() accepted bytes or an error, or side-effect-free Pending.
    /// No caller slice may survive this call; Ready is not hardware completion.
    fn poll_write(
        &mut self,
        cx: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<Result<usize, IoError>>;
    /// Polls with `cx` until the software transmit ring is drained. Returns success,
    /// a port error or Pending; hardware BUSY is checked separately before completion.
    fn poll_flush(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), IoError>>;
    /// Returns the live hardware UART busy flag, independent of software ring occupancy.
    fn busy(&self) -> bool;
    /// Returns a sticky observed receive error without clearing it, or None.
    fn rx_error(&self) -> Option<IoError>;
    /// Discards local RX with bounded work and clears old faults. Returns new discard
    /// failures; success alone does not establish a bridge record boundary.
    fn discard_rx(&mut self) -> Result<(), IoError>;
}

/// External evidence required before RX12's absent DLC/flags can be interpreted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReceiveContract {
    /// Default. Observe records, but never assume their missing DLC/flags.
    Unqualified,
    /// Explicit external qualification: the selected responder's frames are
    /// padded DLC8 standard data, with exact filters and no remote frames.
    /// Selecting this does not perform or prove that hardware qualification.
    PaddedStandardDataDlc8,
}
/// Fixed responder and receive qualification chosen before constructing the owner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Profile {
    /// OBD responder identifier in 0x7E8..=0x7EF; validated at construction.
    pub responder: u16,
    /// External receive metadata guarantee; defaults to observing records only.
    pub receive: ReceiveContract,
}
impl Profile {
    /// Observe-only profile for responder 0x7E8; never admits ECU polling.
    pub const UNQUALIFIED: Self = Self {
        responder: 0x7E8,
        receive: ReceiveContract::Unqualified,
    };
}
/// Public software link phase; none implies physical ECU qualification or health.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkState {
    /// Draining aborted bytes or waiting for retry/discard.
    Recovering,
    /// Acquiring the pre-command quiet guard.
    Quiet,
    /// Matching the fixed enter/probe/quit exchange.
    Handshake,
    /// Data-mode boundary established, but RX12 metadata remains unqualified.
    Unqualified,
    /// Qualified-profile scheduling may run when RX is empty and aligned.
    Polling,
    /// Terminal local fault; this owner cannot recover.
    Quarantined,
    /// Draining for maintenance or fully parked.
    Maintenance,
}
/// Maintenance transition rejected before changing operational state, except Clock quarantines.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PauseError {
    /// Requested pause/resume is invalid in the current owner phase.
    State,
    /// Monotonic time regressed; owner is quarantined.
    Clock,
}
/// Last transport-owner fault, retained through successful recovery for diagnostics.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fault {
    /// Owner clock reversed.
    Clock,
    /// Receive path lost trustworthy byte continuity.
    Rx(
        /// Port receive/discard cause.
        IoError,
    ),
    /// Transmit path failed; queued bytes may still be on the wire.
    Tx(
        /// Port write/flush cause.
        IoError,
    ),
    /// UART pump phase/count/deadline failure.
    TxProgress(
        /// Exact pump classification.
        serial_tx::Error,
    ),
    /// Read reported zero or more than the supplied bounded buffer.
    ReadCount,
    /// Partial RX12 record exceeded the host completion allowance.
    PartialRecordTimeout,
    /// Fixed handshake failed before a trusted boundary.
    Bootstrap(
        /// Exact handshake error.
        bootstrap::Error,
    ),
    /// Raw record framing/identifier/epoch admission failed.
    Record(
        /// Exact RX12 error.
        BridgeError,
    ),
    /// Session admission or transport identity failed.
    Session(
        /// Exact session/client classification.
        SessionError,
    ),
    /// Qualified-profile record contradicts the exact responder/filter contract.
    UnexpectedResponder,
}
/// Coherent copy of current phase and saturating lifetime owner/session counts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LinkSnapshot {
    /// Current operational phase, independent of retained last_fault.
    pub state: LinkState,
    /// Most recently recorded failure, retained after recovery; None before any fault.
    pub last_fault: Option<Fault>,
    /// Saturating owner fault count.
    pub faults: u32,
    /// Saturating fresh quiet/handshake attempts after successful local discard.
    pub handshakes_started: u32,
    /// Saturating count of complete raw records, including unqualified evidence.
    pub raw_records: u32,
    /// Saturating number of oldest raw observations overwritten by the bounded ring.
    pub raw_overwrites: u32,
    /// Aggregate lifetime diagnostic request counters.
    pub diagnostics: Counters,
}
/// Complete raw record evidence, which still lacks DLC/flags/CAN delivery proof.
#[derive(Clone, Copy, Debug)]
pub struct RawObservation {
    /// Monotonic milliseconds when the final record byte was processed.
    pub received_at_ms: u64,
    /// Eleven-bit ID, eight raw bytes and qualified byte-boundary epoch.
    pub record: RawRecord,
}
/// Disjoint completion routing for the single UART pump.
#[derive(Clone, Copy)]
enum Token {
    /// Fixed handshake command completion.
    Bootstrap(
        /// Bootstrap-issued identity.
        bootstrap::Token,
    ),
    /// Request or flow-control completion.
    Diagnostic(
        /// Client-issued identity.
        client::TxToken,
    ),
}
/// Exclusive port owner lifecycle, including maintenance drain and terminal quarantine.
#[derive(Clone, Copy)]
enum OwnerState {
    /// Drain aborted TX, wait bounded backoff, then discard RX and restart handshake.
    Recover {
        /// Recovery start in monotonic milliseconds.
        since: u64,
        /// Minimum backoff milliseconds, zero for initial/resume recovery.
        wait: u64,
    },
    /// Service handshake or qualified record/poll work.
    Running,
    /// Terminal fault, no further port operations.
    Quarantined,
    /// Finish the entire current command before parking, with new admission stopped.
    Draining {
        /// Pause request time in monotonic milliseconds, bounding the drain.
        since: u64,
    },
    /// Software ring and UART observed idle; no port work until explicit resume.
    Parked,
}

/// Sole owner of a non-DMA port, handshake, raw-record parser, UART pump and session.
/// A qualified receive profile is an external premise, not something this service proves.
pub struct Vehicle<'a, P> {
    /// Exclusive bounded port adapter; never shared with another writer/discarder.
    port: P,
    /// Immutable responder and externally selected metadata contract.
    profile: Profile,
    /// Fixed non-provisioning bridge handshake and command tickets.
    bootstrap: Bootstrap,
    /// Byte-boundary parser; faults close it until a complete handshake.
    rx: Rx12,
    /// One copied UART command and its drain/quarantine state.
    tx: Pump<Token>,
    /// Canonical acquisition and bounded OBD scheduler.
    session: Session<'a>,
    /// Port recovery/running/maintenance/quarantine phase.
    owner: OwnerState,
    /// Oldest-to-newest bounded raw evidence ring.
    raw: Deque<RawObservation, RAW_TRACE_RECORDS>,
    /// Saturating complete record count, including overwritten observations.
    raw_records: u32,
    /// Saturating ring overwrite count.
    raw_overwrites: u32,
    /// Saturating count of successful discard-to-handshake restarts.
    handshakes_started: u32,
    /// Saturating count of owner fault transitions.
    faults: u32,
    /// Last cause retained across recovery.
    last_fault: Option<Fault>,
    /// Saturating recovery failure streak; exponent capped and delay limited to 30s.
    failures: u8,
    /// Start of stable online interval, cleared after ten seconds or fault.
    online_since: Option<u64>,
    /// First byte time of incomplete RX12 record, absent at complete boundaries.
    rx_partial_since: Option<u64>,
    /// Last accepted owner monotonic milliseconds.
    last_ms: u64,
}

/// A validated construction input bound to its port, configuration and timestamp.
/// It exposes no live-owner operations. Firmware assembles immediately; delaying
/// assembly preserves the supplied construction timestamp rather than refreshing it.
/// Fields cannot be forged or rebound.
/// ```compile_fail
/// use dashboard_core::diagnostics::service::PreparedVehicle;
/// fn rebind<P>(prepared: &mut PreparedVehicle<'_, P>) { prepared.config = None; }
/// ```
pub struct PreparedVehicle<'a, P> {
    /// Port ownership already bound to this construction input.
    port: P,
    /// Validated responder and selected external metadata premise.
    profile: Profile,
    /// Original construction timestamp in monotonic milliseconds, never refreshed at assembly.
    now: u64,
    /// Closed parser with epoch1 reserved before any handshake.
    rx: Rx12,
    /// Small validated transport fields used to initialize the acquisition owner in place.
    session: PreparedSession,
    /// Optional immutable configuration lifetime shared by the resulting owner.
    config: Option<&'a dashboard_config::ValidatedConfig>,
}
impl<'a, P: Port> PreparedVehicle<'a, P> {
    /// Consume the validated inputs and return the whole owner into its caller's
    /// return destination. Linked caller-plus-callee frames still gate admission.
    /// Returns a fully initialized recovering owner using the original construction time.
    #[inline(never)]
    pub fn assemble(self) -> Vehicle<'a, P> {
        if self.config.is_some() {
            self.assemble_variant::<true>()
        } else {
            self.assemble_variant::<false>()
        }
    }

    /// Returns the initialized owner after statically selecting configured storage;
    /// CONFIGURED must match the bound configuration presence or construction panics.
    #[inline(always)]
    fn assemble_variant<const CONFIGURED: bool>(self) -> Vehicle<'a, P> {
        let mut vehicle = Vehicle {
            port: self.port,
            profile: self.profile,
            bootstrap: Bootstrap::new(self.now),
            rx: self.rx,
            tx: Pump::new(),
            session: self.session.construction_base::<CONFIGURED>(self.config),
            owner: OwnerState::Recover {
                since: self.now,
                wait: 0,
            },
            raw: Deque::new(),
            raw_records: 0,
            raw_overwrites: 0,
            handshakes_started: 0,
            faults: 0,
            last_fault: None,
            failures: 0,
            online_since: None,
            rx_partial_since: None,
            last_ms: self.now,
        };
        vehicle.session.initialize_measurements();
        vehicle
    }
}
impl<'a, P: Port> Vehicle<'a, P> {
    /// Takes exclusive `port` with fixed `profile` at monotonic `now` milliseconds.
    /// Returns an unconfigured recovering owner or invalid responder/address error;
    /// construction itself performs no I/O, handshake or vehicle requests.
    pub fn new(
        port: P,
        profile: Profile,
        now: u64,
    ) -> Result<Self, SessionError> {
        Ok(Self::prepare_inner(port, profile, now, None)?.assemble())
    }

    /// Takes exclusive `port`, fixed `profile`, construction `now` in monotonic
    /// milliseconds and immutable `config`. Returns an initialized recovering owner
    /// or addressing error, with no I/O until service passes.
    pub fn configured(
        port: P,
        profile: Profile,
        now: u64,
        config: &'a dashboard_config::ValidatedConfig,
    ) -> Result<Self, SessionError> {
        Ok(Self::prepare_configured(port, profile, now, config)?.assemble())
    }

    /// Binds exclusive `port`, fixed `profile`, monotonic `now` milliseconds and
    /// immutable `config`. Returns small validated construction inputs or an addressing
    /// error; assembly preserves this timestamp and no I/O occurs here.
    pub fn prepare_configured(
        port: P,
        profile: Profile,
        now: u64,
        config: &'a dashboard_config::ValidatedConfig,
    ) -> Result<PreparedVehicle<'a, P>, SessionError> {
        Self::prepare_inner(port, profile, now, Some(config))
    }

    /// Returns the immutable bound configuration, or None for legacy generic polling policy.
    pub fn configuration(&self) -> Option<&dashboard_config::ValidatedConfig> { self.session.configuration() }

    /// Returns validated inputs bound to `port`, `profile`, monotonic `now` milliseconds
    /// and optional `config`. Reserves closed epoch1; invalid responder/address returns
    /// SessionError before any hardware operation.
    fn prepare_inner(
        port: P,
        profile: Profile,
        now: u64,
        config: Option<&'a dashboard_config::ValidatedConfig>,
    ) -> Result<PreparedVehicle<'a, P>, SessionError> {
        // Reserve epoch1 for the initial unqualified state. The first complete
        // handshake advances past it; capabilities and tokens are never reused.
        let mut rx = Rx12::new();
        rx.fault();
        let session = PreparedSession::new(profile.responder, 1)?;
        Ok(PreparedVehicle {
            port,
            profile,
            now,
            rx,
            session,
            config,
        })
    }

    /// Returns a coherent copy of canonical measurements, quality, statistics and warnings.
    pub fn snapshot(&self) -> DashboardSnapshot { self.session.snapshot() }

    /// Returns lifetime per-PID request/timing summaries; histogram scans are bounded.
    pub fn telemetry(&self) -> super::session::Telemetry { self.session.telemetry() }

    /// Applies producer warning `command` at monotonic `now` milliseconds. Returns
    /// shared acceptance/replay/rejection status; no port I/O follows from this call.
    pub fn apply_warning_command(
        &mut self,
        command: crate::warning_state::WarningCommand,
        now: u64,
    ) -> crate::warning_state::CommandResult {
        self.session.apply_warning_command(command, now)
    }

    /// Applies newer nonzero reset `generation`. Returns whether measurement statistics
    /// reset; raw source/quality, transport counters and capability evidence are preserved.
    pub fn reset_statistics(
        &mut self,
        generation: u64,
    ) -> bool {
        self.session.reset_statistics(generation)
    }

    /// Returns retained raw observations oldest first, including unqualified records.
    pub fn raw(&self) -> impl Iterator<Item = &RawObservation> { self.raw.iter() }

    /// Stop admission, but finish the entire current command including bytes
    /// not yet copied into the UART. `fail`/Pump::abort cannot serve this role.
    /// `now` is monotonic milliseconds. Returns success entering Draining only from
    /// Running/Recover, or State without mutation; clock reversal quarantines and returns Clock.
    pub fn request_pause(
        &mut self,
        now: u64,
    ) -> Result<(), PauseError> {
        if now < self.last_ms {
            self.fail(Fault::Clock, self.last_ms, true);
            return Err(PauseError::Clock);
        }
        if !matches!(self.owner, OwnerState::Running | OwnerState::Recover { .. }) {
            return Err(PauseError::State);
        }
        self.last_ms = now;
        self.owner = OwnerState::Draining { since: now };
        Ok(())
    }

    /// Returns true only after full command/software-ring/hardware drain and session invalidation.
    pub fn paused(&self) -> bool { matches!(self.owner, OwnerState::Parked) }

    /// A park can only end after wire idle. Fresh discard and bootstrap are
    /// performed by subsequent service passes; terminal quarantine survives.
    /// `now` is monotonic milliseconds. Returns success from Parked, entering immediate
    /// Recover; wrong state is unchanged, while reversal quarantines and returns Clock.
    pub fn resume(
        &mut self,
        now: u64,
    ) -> Result<(), PauseError> {
        if now < self.last_ms {
            self.fail(Fault::Clock, self.last_ms, true);
            return Err(PauseError::Clock);
        }
        if !self.paused() {
            return Err(PauseError::State);
        }
        self.last_ms = now;
        self.owner = OwnerState::Recover { since: now, wait: 0 };
        Ok(())
    }

    /// Makes bounded TX/flush progress using `cx` at monotonic `now` milliseconds;
    /// `since` is the pause request time. Parks only after ring empty and hardware idle,
    /// invalidating parser/session. Drain timeout or I/O/session fault quarantines.
    fn drain_for_pause(
        &mut self,
        cx: &mut Context<'_>,
        now: u64,
        since: u64,
    ) {
        self.transmit(cx, now);
        if !matches!(self.owner, OwnerState::Draining { .. }) {
            return;
        }
        if now - since > serial_tx::DEADLINE_MS {
            self.fail(Fault::TxProgress(serial_tx::Error::Timeout), now, true);
            return;
        }
        if self.tx.active() {
            return;
        }
        // Check the software ring even without a current Pump. This is an
        // independent guard against an unexpectedly buffered previous write.
        match self.port.poll_flush(cx) {
            Poll::Pending => return,
            Poll::Ready(Err(error)) => {
                self.fail(Fault::Tx(error), now, true);
                return;
            }
            Poll::Ready(Ok(())) => {}
        }
        if self.port.busy() {
            return;
        }
        self.online_since = None;
        self.rx_partial_since = None;
        self.rx.fault();
        self.bootstrap.fault(bootstrap::Error::ReceiveLoss);
        match self.session.transport_fault(now) {
            Ok(()) => self.owner = OwnerState::Parked,
            Err(error) => self.fail(Fault::Session(error), now, true),
        }
    }

    /// Returns coherent owner phase, retained fault and saturating lifetime counters.
    pub fn link(&self) -> LinkSnapshot {
        let state = match self.owner {
            OwnerState::Recover { .. } => LinkState::Recovering,
            OwnerState::Quarantined => LinkState::Quarantined,
            OwnerState::Draining { .. } | OwnerState::Parked => LinkState::Maintenance,
            OwnerState::Running => {
                match self.bootstrap.status() {
                    bootstrap::Status::Quiet => LinkState::Quiet,
                    bootstrap::Status::Handshake => LinkState::Handshake,
                    bootstrap::Status::Fault(_) => LinkState::Recovering,
                    bootstrap::Status::Online => {
                        match self.profile.receive {
                            ReceiveContract::Unqualified => LinkState::Unqualified,
                            ReceiveContract::PaddedStandardDataDlc8 => LinkState::Polling,
                        }
                    }
                }
            }
        };
        LinkSnapshot {
            state,
            last_fault: self.last_fault,
            faults: self.faults,
            handshakes_started: self.handshakes_started,
            raw_records: self.raw_records,
            raw_overwrites: self.raw_overwrites,
            diagnostics: self.session.counters(),
        }
    }

    /// Records `reason` at checked monotonic `now` milliseconds, invalidates parser/
    /// session and aborts unsent command bytes. `quarantine` requests permanent closure;
    /// session/pump abort failures force it. Otherwise schedules 250ms..30s exponential recovery.
    fn fail(
        &mut self,
        reason: Fault,
        now: u64,
        mut quarantine: bool,
    ) {
        self.last_fault = Some(reason);
        self.faults = self.faults.saturating_add(1);
        self.online_since = None;
        self.rx_partial_since = None;
        self.rx.fault();
        self.bootstrap.fault(bootstrap::Error::ReceiveLoss);
        quarantine |= self.session.transport_fault(now).is_err();
        quarantine |= self.tx.abort(now).is_err();
        if quarantine {
            self.tx.quarantine();
            self.owner = OwnerState::Quarantined;
        } else {
            self.failures = self.failures.saturating_add(1);
            let wait = (250u64 << self.failures.saturating_sub(1).min(7)).min(30_000);
            self.owner = OwnerState::Recover { since: now, wait };
        }
    }

    /// Applies handshake `result` at `now` milliseconds. Boundary opens a new RX epoch
    /// and resets session capability admission; identity/clock/stale-TX errors quarantine,
    /// while ordinary handshake failures enter recovery.
    fn boot_event(
        &mut self,
        result: Result<bootstrap::Event, bootstrap::Error>,
        now: u64,
    ) {
        match result {
            Ok(bootstrap::Event::None) => {}
            Ok(bootstrap::Event::Boundary) => {
                match self.rx.confirm_boundary() {
                    Err(error) => self.fail(Fault::Record(error), now, true),
                    Ok(epoch) => {
                        match self.session.change_epoch(epoch, now) {
                            Err(error) => self.fail(Fault::Session(error), now, true),
                            Ok(()) => self.online_since = Some(now),
                        }
                    }
                }
            }
            Err(error) => {
                self.fail(
                    Fault::Bootstrap(error),
                    now,
                    matches!(
                        error,
                        bootstrap::Error::Clock | bootstrap::Error::Exhausted | bootstrap::Error::StaleTx
                    ),
                )
            }
        }
    }

    /// Applies session `result` at `now` milliseconds, queuing its optional padded
    /// action. WireFault permits fresh-boundary recovery; other session errors quarantine.
    /// Panics if an internal diagnostic action violates the required DLC8 TX contract.
    fn session_event(
        &mut self,
        result: Result<Option<TxAction>, SessionError>,
        now: u64,
    ) {
        if let Err(error) = result {
            self.fail(
                Fault::Session(error),
                now,
                error != SessionError::Client(client::ClientError::WireFault),
            );
            return;
        }
        if self.session.requires_new_epoch() {
            self.fail(
                Fault::Session(SessionError::Client(client::ClientError::WireFault)),
                now,
                false,
            );
            return;
        }
        if let Ok(Some(action)) = result {
            // Diagnostic requests and FCs are explicitly padded DLC8.
            let bytes = action.frame.longan_tx().expect("unpadded diagnostic TX");
            self.enqueue(Token::Diagnostic(action.token), &bytes, now);
        }
    }

    /// Copies `bytes` into the sole pump with routing `token` at `now` milliseconds;
    /// any overlapping/invalid/clock/quarantine admission failure quarantines the owner.
    fn enqueue(
        &mut self,
        token: Token,
        bytes: &[u8],
        now: u64,
    ) {
        if let Err(error) = self.tx.start(token, bytes, now) {
            self.fail(Fault::TxProgress(error), now, true);
        }
    }

    /// Performs at most TX_WORK_TRANSITIONS using port wake context `cx` and checked
    /// monotonic `now` milliseconds. Pending retains no user slice. UART completion
    /// routes exact tickets except during maintenance drain, where new protocol work is suppressed.
    fn transmit(
        &mut self,
        cx: &mut Context<'_>,
        now: u64,
    ) {
        for _ in 0..TX_WORK_TRANSITIONS {
            let result = match self.tx.work(now) {
                Err(error) => {
                    self.fail(Fault::TxProgress(error), now, true);
                    return;
                }
                Ok(Work::Idle) => return,
                Ok(Work::Write(bytes)) => {
                    match self.port.poll_write(cx, bytes) {
                        Poll::Pending => return,
                        Poll::Ready(Err(error)) => {
                            self.fail(Fault::Tx(error), now, true);
                            return;
                        }
                        Poll::Ready(Ok(n)) => self.tx.wrote(n, now),
                    }
                }
                Ok(Work::Flush) => {
                    match self.port.poll_flush(cx) {
                        Poll::Pending => return,
                        Poll::Ready(Err(error)) => {
                            self.fail(Fault::Tx(error), now, true);
                            return;
                        }
                        Poll::Ready(Ok(())) => self.tx.flushed(now),
                    }
                }
                Ok(Work::WaitIdle) => {
                    if self.port.busy() {
                        return;
                    }
                    match self.tx.wire_idle(now) {
                        Err(error) => Err(error),
                        Ok(Completion::Aborted) => return,
                        // The completed action must not schedule another FC or
                        // command while a maintenance drain is in progress.
                        Ok(Completion::Sent(_)) if matches!(self.owner, OwnerState::Draining { .. }) => return,
                        Ok(Completion::Sent(Token::Bootstrap(token))) => {
                            let result = self.bootstrap.tx_completed(token, now);
                            self.boot_event(result, now);
                            return;
                        }
                        Ok(Completion::Sent(Token::Diagnostic(token))) => {
                            let result = self.session.tx_completed(token, now);
                            self.session_event(result, now);
                            return;
                        }
                    }
                }
            };
            if let Err(error) = result {
                self.fail(Fault::TxProgress(error), now, true);
                return;
            }
        }
    }

    /// At checked monotonic `now` milliseconds, waits for pump/hardware idle and retry
    /// delay, then discards RX and starts a fresh handshake. Stuck BUSY or terminal
    /// bootstrap failure quarantines; discard faults retry with bounded backoff.
    fn recover(
        &mut self,
        now: u64,
    ) {
        let OwnerState::Recover { since, wait } = self.owner else {
            return;
        };
        if self.tx.active() {
            return;
        }
        if self.port.busy() {
            if now - since > serial_tx::DEADLINE_MS {
                self.fail(Fault::TxProgress(serial_tx::Error::Timeout), now, true);
            }
            return;
        }
        if now - since < wait {
            return;
        }
        match self.port.discard_rx() {
            Err(error) => self.fail(Fault::Rx(error), now, false),
            Ok(()) => {
                match self.bootstrap.restart_after_discard(now) {
                    Err(error) => self.fail(Fault::Bootstrap(error), now, true),
                    Ok(()) => {
                        self.handshakes_started = self.handshakes_started.saturating_add(1);
                        self.owner = OwnerState::Running;
                    }
                }
            }
        }
    }

    /// Routes ordered `byte` at checked monotonic `now` milliseconds through handshake
    /// or RX12. Retains complete raw records before optional qualified-session admission;
    /// unexpected responder under the qualified profile invalidates the transport.
    fn receive_byte(
        &mut self,
        byte: u8,
        now: u64,
    ) {
        if self.bootstrap.status() != bootstrap::Status::Online {
            let result = self.bootstrap.receive(byte, now);
            self.boot_event(result, now);
            return;
        }
        if self.rx.at_boundary() {
            self.rx_partial_since = Some(now);
        }
        let record = match self.rx.push(byte) {
            Ok(None) => return,
            Err(error) => {
                self.fail(Fault::Record(error), now, error == BridgeError::EpochExhausted);
                return;
            }
            Ok(Some(record)) => record,
        };
        self.rx_partial_since = None;
        self.raw_records = self.raw_records.saturating_add(1);
        if self.raw.is_full() {
            self.raw.pop_front();
            self.raw_overwrites = self.raw_overwrites.saturating_add(1);
        }
        self.raw
            .push_back(RawObservation {
                record,
                received_at_ms: now,
            })
            .unwrap();
        if self.profile.receive == ReceiveContract::PaddedStandardDataDlc8 {
            if record.id() != self.profile.responder {
                self.fail(Fault::UnexpectedResponder, now, false);
                return;
            }
            // This conversion is authorized only by the explicit external
            // receive contract. RX12 content alone cannot establish the DLC.
            let frame = CanFrame::new(record.id(), record.data()).unwrap();
            let result = self.session.receive(&frame, record.epoch(), now);
            self.session_event(result, now);
        }
    }

    /// One bounded nonblocking service pass. Firmware calls at 1 ms intervals
    /// and updates its watchdog progress independently of all CAN responses.
    /// No future, DMA lease or user IO slice survives this call.
    /// `cx` supplies port wakers and `now` monotonic milliseconds; processes bounded
    /// input/TX work then expires measurements, with no warning command.
    pub fn step(
        &mut self,
        cx: &mut Context<'_>,
        now: u64,
    ) {
        self.step_with_warning(cx, now, None);
    }

    /// One coherent producer iteration: samples/faults, command, timed expiry.
    /// `cx` supplies port wakers; `now` is monotonic milliseconds; optional `command`
    /// is applied after inputs and before measurement expiry, even while parked/offline.
    pub fn step_with_warning(
        &mut self,
        cx: &mut Context<'_>,
        now: u64,
        command: Option<crate::warning_state::WarningCommand>,
    ) {
        self.step_inputs(cx, now);
        if let Some(command) = command {
            self.apply_warning_command(command, now);
        }
        self.session.tick_measurements(now);
    }

    /// Performs bounded port/protocol work with `cx` at monotonic `now` milliseconds.
    /// Quarantine/park prevent I/O; reversal quarantines. Reads at most RX_WORK_BYTES,
    /// holds early replies behind TX completion, and admits new requests only after
    /// Pending RX plus a complete record boundary, never merely after a short read.
    fn step_inputs(
        &mut self,
        cx: &mut Context<'_>,
        now: u64,
    ) {
        if matches!(self.owner, OwnerState::Quarantined) {
            return;
        }
        if now < self.last_ms {
            self.fail(Fault::Clock, self.last_ms, true);
            return;
        }
        self.last_ms = now;
        match self.owner {
            OwnerState::Parked => return,
            OwnerState::Draining { since } => {
                self.drain_for_pause(cx, now, since);
                return;
            }
            _ => {}
        }
        if matches!(self.owner, OwnerState::Running)
            && let Some(error) = self.port.rx_error()
        {
            self.fail(Fault::Rx(error), now, false);
        }
        if matches!(self.owner, OwnerState::Running) {
            if self
                .rx_partial_since
                .is_some_and(|since| now - since > PARTIAL_RECORD_MS)
            {
                self.fail(Fault::PartialRecordTimeout, now, false);
            } else if self.bootstrap.status() == bootstrap::Status::Online
                && self.profile.receive == ReceiveContract::PaddedStandardDataDlc8
            {
                let result = self.session.tick_protocol(now);
                self.session_event(result.map(|()| None), now);
            }
        }
        self.transmit(cx, now);
        if matches!(self.owner, OwnerState::Quarantined) {
            return;
        }
        if matches!(self.owner, OwnerState::Recover { .. }) {
            self.recover(now);
            return;
        }
        // Keep early replies in the interrupt buffer until the current wire TX
        // is complete and its token has been acknowledged to the protocol.
        if self.tx.active() {
            return;
        }

        let mut bytes = [0; RX_WORK_BYTES];
        let receive_empty = match self.port.poll_read(cx, &mut bytes) {
            Poll::Pending => true,
            Poll::Ready(Err(error)) => {
                self.fail(Fault::Rx(error), now, false);
                return;
            }
            Poll::Ready(Ok(n)) => {
                if n == 0 || n > bytes.len() {
                    self.fail(Fault::ReadCount, now, true);
                    return;
                }
                if let Some(error) = self.port.rx_error() {
                    self.fail(Fault::Rx(error), now, false);
                    return;
                }
                for &byte in &bytes[..n] {
                    self.receive_byte(byte, now);
                    if !matches!(self.owner, OwnerState::Running) {
                        return;
                    }
                }
                if let Some(error) = self.port.rx_error() {
                    self.fail(Fault::Rx(error), now, false);
                    return;
                }
                false
            }
        };
        if self.online_since.is_some_and(|since| now - since >= STABLE_MS) {
            self.failures = 0;
            self.online_since = None;
        }
        if self.tx.active() {
            return;
        }
        if self.bootstrap.status() == bootstrap::Status::Online {
            // A short read is not evidence that the backlog is empty. Never
            // let a pre-request record straddle admission of a new request.
            if self.profile.receive == ReceiveContract::PaddedStandardDataDlc8 && receive_empty && self.rx.at_boundary()
            {
                let result = self.session.poll_protocol(now);
                self.session_event(result, now);
            }
        } else {
            match self.bootstrap.poll(now) {
                Ok(Some(command)) => self.enqueue(Token::Bootstrap(command.token), command.bytes, now),
                Ok(None) => {}
                Err(error) => self.boot_event(Err(error), now),
            }
        }
    }
}

#[cfg(test)]
/// Terminal owner-fault priority regressions using a port that must never be accessed.
mod tests {
    use super::*;
    /// Construction-only fixture; every port operation deliberately panics.
    struct UnusedPort;
    impl Port for UnusedPort {
        /// Panics if read polling occurs; this fixture returns no bytes or Poll result.
        fn poll_read(
            &mut self,
            _: &mut Context<'_>,
            _: &mut [u8],
        ) -> Poll<Result<usize, IoError>> {
            unreachable!()
        }

        /// Panics if write polling occurs; this fixture returns no write count.
        fn poll_write(
            &mut self,
            _: &mut Context<'_>,
            _: &[u8],
        ) -> Poll<Result<usize, IoError>> {
            unreachable!()
        }

        /// Panics if flush polling occurs; this fixture returns no completion.
        fn poll_flush(
            &mut self,
            _: &mut Context<'_>,
        ) -> Poll<Result<(), IoError>> {
            unreachable!()
        }

        /// Panics if hardware status is queried; no busy value is returned.
        fn busy(&self) -> bool { unreachable!() }

        /// Panics if RX status is queried; no error observation is returned.
        fn rx_error(&self) -> Option<IoError> { unreachable!() }

        /// Panics if discard is attempted; no successful I/O result is returned.
        fn discard_rx(&mut self) -> Result<(), IoError> { unreachable!() }
    }

    #[test]
    /// Verifies preexisting wire-fault recovery cannot downgrade terminal clock/identity failures.
    fn a_wire_fault_latch_cannot_hide_terminal_session_failures() {
        for error in [client::ClientError::Clock, client::ClientError::Exhausted] {
            let mut owner = Vehicle::new(UnusedPort, Profile::UNQUALIFIED, 0).unwrap();
            owner.session.transport_fault(0).unwrap();
            owner.session_event(Err(SessionError::Client(error)), 0);
            assert_eq!(owner.link().state, LinkState::Quarantined);
            assert_eq!(
                owner.link().last_fault,
                Some(Fault::Session(SessionError::Client(error)))
            );
        }
    }
}
