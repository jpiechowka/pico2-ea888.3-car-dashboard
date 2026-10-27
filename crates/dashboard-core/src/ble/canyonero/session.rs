//! Single-flight identity lifecycle; hardware ownership stays with the caller.

use super::{DecodeStatus, FrameDecoder};

/// Selected-peer connection budget in milliseconds.
const CONNECT_MS: u64 = 8_000;
/// Fresh standard inventory and MTU-exchange budget in milliseconds.
const DISCOVERY_MS: u64 = 30_000;
/// Transport admission budget in milliseconds, allowing four bounded three-second reads.
const PARAMETERS_MS: u64 = 15_000;
/// CoC establishment budget in milliseconds.
const OPEN_MS: u64 = 5_000;
/// INFO submission budget in milliseconds, including all backpressure.
const SEND_MS: u64 = 2_000;
/// Whole reply assembly budget in milliseconds; received bytes do not extend it.
const REPLY_MS: u64 = 2_000;
/// Normal link cleanup budget in milliseconds.
const CLEANUP_MS: u64 = 5_000;

/// Identity of one operation on one link; owner-assigned counters never wrap.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Epoch {
    /// Nonzero connection generation, strictly increasing for each start.
    pub connection: u64,
    /// Nonzero operation generation, strictly increasing for each start.
    pub operation: u64,
    /// Maintenance generation, nondecreasing across starts.
    pub maintenance: u64,
}

/// Current lifecycle stage, not a claim of transport interoperability.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    /// No attempt has been started.
    Idle,
    /// Owner is connecting to its explicitly selected peer.
    Connecting,
    /// Owner is discovering fresh service endpoints.
    Discovering,
    /// Owner is validating and recording source/observed transport parameters.
    TransportParameters,
    /// Owner is opening the selected CoC.
    OpeningCoc,
    /// One INFO request may be taken and sent, including backpressure time.
    SendingInfo,
    /// Whole INFO request was submitted and a reply is pending.
    AwaitingInfo,
    /// Owner must finish channel/link cleanup; identity is not yet success.
    Closing,
    /// Attempt ended; inspect outcome and cleanup separately.
    Finished,
}

/// Confirmed hardware-owner cleanup observation; submission alone is insufficient.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CleanupResult {
    /// All operation I/O is stopped and the connection is confirmed normally idle.
    Clean,
    /// Cleanup failed or link termination was abnormal.
    Failed,
}

/// Latest cleanup observation, independent of the first operation fault.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CleanupOutcome {
    /// No cleanup confirmation has arrived.
    Pending,
    /// Owner confirmed normal idle completion, possibly after a timeout fault.
    Clean,
    /// Owner reported abnormal or failed cleanup.
    Failed,
    /// Deadline elapsed without a cleanup observation; late confirmation may replace this.
    TimedOut,
}

/// Hardware error namespace; the optional code retains its original domain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransportDomain {
    /// Controller or HCI status/reason.
    Hci,
    /// GATT/ATT discovery or access error.
    Att,
    /// L2CAP refusal, channel or credit error.
    L2cap,
    /// Host adapter failure without a standardized wire status.
    Local,
}

/// Transport failure evidence supplied by the I/O owner, without invented codes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TransportFault {
    /// Namespace in which code is interpreted.
    pub domain: TransportDomain,
    /// Actual status when observed; None means unavailable.
    pub code: Option<u16>,
    /// True if some INFO bytes might already have been submitted before failure.
    pub partial_send: bool,
}

/// First failure reason; all reasons prohibit another request on this attempt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reason {
    /// Decoder rejected the received frame or end-of-stream truncation.
    Protocol(
        /// Exact first decoder error.
        super::DecodeError,
    ),
    /// Current whole-stage deadline was reached, including equality.
    TimedOut,
    /// Supplied monotonic milliseconds decreased within this attempt.
    TimeReversed,
    /// A new absolute deadline could not be represented in u64 milliseconds.
    ClockOverflow,
    /// Operator cancelled this attempt.
    Cancelled,
    /// Maintenance parking invalidated this attempt.
    Parked,
    /// Owner reported an I/O or link failure.
    Transport(
        /// Original domain, optional code and partial-send evidence.
        TransportFault,
    ),
    /// Same-epoch event violated the ordered lifecycle.
    UnexpectedEvent,
    /// A zero-byte CoC SDU arrived; this is distinct from stream EOF.
    UnexpectedEmptySdu,
    /// Stream ended before both a complete reply and whole-send completion were observed.
    Eof,
    /// Normal cleanup was not confirmed by its deadline.
    CleanupTimedOut,
    /// Owner reported cleanup failure or abnormal termination.
    CleanupFailed,
}

/// Stable first fault with the stage/time at which it occurred.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fault {
    /// Stage before transition into cleanup.
    pub stage: Stage,
    /// First failure, never replaced by later cleanup errors.
    pub reason: Reason,
    /// Supplied monotonic milliseconds of observation, including a backward value if faulty.
    pub at_ms: u64,
}

/// Final application outcome; transport and cleanup details remain separately inspectable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// One validated INFO and confirmed normal cleanup, with no earlier fault.
    Success,
    /// The first fault, even if normal cleanup later succeeds.
    Failed(
        /// First fault retained from the attempt.
        Fault,
    ),
}

/// Owner events; stale epochs cannot mutate state, clocks, bytes or outcome.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    /// Selected connection completed; fresh discovery starts.
    Connected,
    /// Fresh endpoint discovery completed; parameter admission starts.
    Discovered,
    /// Transport parameters were admitted and their provenance recorded by the owner.
    ParametersReady,
    /// Selected channel opened; one INFO request can be taken.
    ChannelOpened,
    /// All four INFO bytes were successfully submitted as one SDU.
    InfoSent,
    /// Operator cancellation; no subsequent request or reply is admitted.
    Cancel,
    /// Maintenance invalidation; reconnect is required after normal cleanup.
    Park,
    /// Terminal transport failure, including partial send or link loss.
    TransportFailed(
        /// Actual owner-observed failure details.
        TransportFault,
    ),
    /// Ordered receive stream ended; a partial frame fails explicitly.
    Eof,
    /// Actual cleanup observation, not merely close submission.
    CleanupComplete(
        /// Confirmed cleanup result.
        CleanupResult,
    ),
    /// Check whole-stage deadlines without extending them.
    Tick,
}

/// Rejection of a caller operation; Faulted means an attempt fault was recorded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rejected {
    /// Epoch differs from the currently owned attempt; no mutation occurred.
    StaleEpoch,
    /// Attempt is not in a stage that accepts this call.
    NotActive,
    /// A previous attempt still owns the link or awaits normal cleanup.
    CleanupUnconfirmed,
    /// New counters are zero, reused, decreased, maintenance went backwards, or start time decreased.
    InvalidEpoch,
    /// Previous connection or operation generation reached u64::MAX.
    EpochExhausted,
    /// This attempt already issued its one outbound request.
    RequestAlreadyTaken,
    /// A first fault was recorded or already prevents the operation.
    Faulted,
}

/// Affine outbound permit; only the four-byte INFO primitive can be constructed.
/// Owners must submit its bytes once in one SDU; extracting bytes is not I/O.
#[derive(Debug)]
pub struct InfoRequest {
    /// Operation identity returned with completion; no public constructor.
    epoch: Epoch,
}

impl InfoRequest {
    /// Returns the exact epoch that must accompany successful or failed submission.
    pub const fn epoch(&self) -> Epoch { self.epoch }

    /// Returns the only supported application request, with a zero-length body.
    pub const fn bytes(&self) -> &'static [u8; 4] { &[0x1F, 0x11, 0, 0] }
}

/// One active operation and one decoder, retaining raw/partial evidence until restart.
/// Call start again only after confirmed normal cleanup, with newer generations.
/// I/O adapters must also isolate peers/handles and stop their tasks; tokens alone
/// do not cancel hardware or prove that a submitted close has completed.
pub struct Session {
    /// Last attempt identity, retained after finish to reject reuse.
    epoch: Option<Epoch>,
    /// Current lifecycle stage.
    stage: Stage,
    /// First fault, independent of later cleanup observations.
    fault: Option<Fault>,
    /// Latest confirmed cleanup observation or deadline outcome.
    cleanup: CleanupOutcome,
    /// Last accepted monotonic timestamp in milliseconds.
    last_ms: u64,
    /// Absolute whole-stage deadline in milliseconds, never extended by bytes.
    deadline_ms: u64,
    /// True after the sole request permit was issued.
    request_taken: bool,
    /// Single raw receive allocation, also backing borrowed identity fields.
    decoder: FrameDecoder,
}

impl Default for Session {
    /// Returns an idle session with no attempt, request, clock or received evidence.
    fn default() -> Self { Self::new() }
}

impl Session {
    /// Returns an idle session without starting discovery, I/O or an operation clock.
    pub const fn new() -> Self {
        Self {
            epoch: None,
            stage: Stage::Idle,
            fault: None,
            cleanup: CleanupOutcome::Pending,
            last_ms: 0,
            deadline_ms: 0,
            request_taken: false,
            decoder: FrameDecoder::new(),
        }
    }

    /// Starts a selected-peer attempt with `epoch` and monotonic `now_ms` milliseconds.
    /// Returns Connecting on admission or an explicit rejection/fault.
    /// Requires nonzero increasing connection/operation counters and nondecreasing
    /// maintenance generation. Restart also requires prior confirmed normal cleanup.
    /// Rejected admission leaves evidence unchanged; admitted clock overflow faults
    /// the new attempt. The owner must preserve counter uniqueness across Session instances.
    pub fn start(
        &mut self,
        epoch: Epoch,
        now_ms: u64,
    ) -> Result<Stage, Rejected> {
        if self.stage != Stage::Idle && (self.stage != Stage::Finished || self.cleanup != CleanupOutcome::Clean) {
            return Err(Rejected::CleanupUnconfirmed);
        }
        if let Some(previous) = self.epoch {
            if previous.connection == u64::MAX || previous.operation == u64::MAX {
                return Err(Rejected::EpochExhausted);
            }
            if epoch.connection <= previous.connection
                || epoch.operation <= previous.operation
                || epoch.maintenance < previous.maintenance
            {
                return Err(Rejected::InvalidEpoch);
            }
            if now_ms < self.last_ms {
                return Err(Rejected::InvalidEpoch);
            }
        }
        if epoch.connection == 0 || epoch.operation == 0 {
            return Err(Rejected::InvalidEpoch);
        }
        self.epoch = Some(epoch);
        self.stage = Stage::Connecting;
        self.fault = None;
        self.cleanup = CleanupOutcome::Pending;
        self.last_ms = now_ms;
        self.request_taken = false;
        self.decoder = FrameDecoder::new();
        self.set_deadline(now_ms, CONNECT_MS)?;
        Ok(self.stage)
    }

    /// Applies one owner `event` for `epoch` at monotonic `now_ms` milliseconds.
    /// Returns the resulting stage or a rejection; recorded faults remain inspectable.
    /// Stale events never change state or evidence. Deadline equality is expired.
    /// Each stage has its own fixed budget. Actual cleanup may arrive late and updates cleanup evidence,
    /// but cannot erase a timeout or turn any failed attempt into success.
    pub fn event(
        &mut self,
        epoch: Epoch,
        now_ms: u64,
        event: Event,
    ) -> Result<Stage, Rejected> {
        self.check_epoch(epoch)?;
        if let Event::CleanupComplete(result) = event {
            return self.confirm_cleanup(now_ms, result);
        }
        self.check_time(now_ms)?;
        match event {
            Event::Tick => return Ok(self.stage),
            Event::Cancel => {
                self.fail(Reason::Cancelled, now_ms);
                return Ok(self.stage);
            }
            Event::Park => {
                self.fail(Reason::Parked, now_ms);
                return Ok(self.stage);
            }
            Event::TransportFailed(error) => {
                self.fail(Reason::Transport(error), now_ms);
                return Ok(self.stage);
            }
            Event::Eof => {
                if self.stage == Stage::Closing && self.decoder.has_identity() && self.fault.is_none() {
                    return Ok(self.stage);
                }
                let reason = match self.decoder.finish() {
                    Err(error) => Reason::Protocol(error),
                    Ok(_) => Reason::Eof,
                };
                self.fail(reason, now_ms);
                return Err(Rejected::Faulted);
            }
            _ => {}
        }
        if self.fault.is_some() {
            return Err(Rejected::Faulted);
        }
        let (next, budget) = match (self.stage, event) {
            (Stage::Connecting, Event::Connected) => (Stage::Discovering, Some(DISCOVERY_MS)),
            (Stage::Discovering, Event::Discovered) => (Stage::TransportParameters, Some(PARAMETERS_MS)),
            (Stage::TransportParameters, Event::ParametersReady) => (Stage::OpeningCoc, Some(OPEN_MS)),
            (Stage::OpeningCoc, Event::ChannelOpened) => (Stage::SendingInfo, Some(SEND_MS)),
            (Stage::SendingInfo, Event::InfoSent) if self.request_taken => {
                if self.decoder.has_identity() {
                    (Stage::Closing, Some(CLEANUP_MS))
                } else {
                    (Stage::AwaitingInfo, Some(REPLY_MS))
                }
            }
            _ => {
                self.fail(Reason::UnexpectedEvent, now_ms);
                return Err(Rejected::Faulted);
            }
        };
        self.stage = next;
        if let Some(budget) = budget {
            self.set_deadline(now_ms, budget)?;
        }
        Ok(self.stage)
    }

    /// Issues the sole INFO permit for `epoch` at monotonic `now_ms` milliseconds.
    /// Returns the permit or a rejection without issuing a second request.
    /// Only SendingInfo may issue it. A second call never yields another request;
    /// transport send failure/cancellation must enter cleanup, never retry the bytes.
    pub fn take_request(
        &mut self,
        epoch: Epoch,
        now_ms: u64,
    ) -> Result<InfoRequest, Rejected> {
        self.check_epoch(epoch)?;
        self.check_time(now_ms)?;
        if self.fault.is_some() {
            return Err(Rejected::Faulted);
        }
        if self.request_taken {
            return Err(Rejected::RequestAlreadyTaken);
        }
        if self.stage != Stage::SendingInfo {
            return Err(Rejected::NotActive);
        }
        self.request_taken = true;
        Ok(InfoRequest { epoch })
    }

    /// Feeds nonempty ordered CoC receive `bytes` for `epoch` at `now_ms` milliseconds.
    /// Returns framing progress or rejection, recording admitted input errors as the first fault.
    /// A reply racing successful send completion is retained, but cannot complete
    /// the operation before InfoSent. Valid INFO enters Closing; any later suffix
    /// before cleanup revokes success. Empty SDU, malformed input or unexpected
    /// phase faults immediately. Failed/stale input never mutates the decoder.
    pub fn receive(
        &mut self,
        epoch: Epoch,
        now_ms: u64,
        bytes: &[u8],
    ) -> Result<DecodeStatus, Rejected> {
        self.check_epoch(epoch)?;
        self.check_time(now_ms)?;
        if self.fault.is_some() {
            return Err(Rejected::Faulted);
        }
        if !self.request_taken || !matches!(self.stage, Stage::SendingInfo | Stage::AwaitingInfo | Stage::Closing) {
            self.fail(Reason::UnexpectedEvent, now_ms);
            return Err(Rejected::Faulted);
        }
        if bytes.is_empty() {
            self.fail(Reason::UnexpectedEmptySdu, now_ms);
            return Err(Rejected::Faulted);
        }
        match self.decoder.push(bytes) {
            Ok(status) => {
                if status == DecodeStatus::Complete && self.stage == Stage::AwaitingInfo {
                    self.stage = Stage::Closing;
                    self.set_deadline(now_ms, CLEANUP_MS)?;
                }
                Ok(status)
            }
            Err(error) => {
                self.fail(Reason::Protocol(error), now_ms);
                Err(Rejected::Faulted)
            }
        }
    }

    /// Returns the current lifecycle stage; Closing is never application success.
    pub const fn stage(&self) -> Stage { self.stage }

    /// Returns the current/last epoch, or None before the first admitted start.
    pub const fn epoch(&self) -> Option<Epoch> { self.epoch }

    /// Returns the first fault with its original stage and observed time, if any.
    pub const fn fault(&self) -> Option<Fault> { self.fault }

    /// Returns cleanup observation separately from the operation's first fault.
    pub const fn cleanup(&self) -> CleanupOutcome { self.cleanup }

    /// Returns absolute deadline milliseconds only while an attempt owns work/cleanup.
    pub const fn deadline_ms(&self) -> Option<u64> {
        match self.stage {
            Stage::Idle | Stage::Finished => None,
            _ => Some(self.deadline_ms),
        }
    }

    /// Returns borrowed received evidence and any structurally valid identity, even after a
    /// separate transport fault. A decoded identity alone is not a successful session.
    pub const fn decoder(&self) -> &FrameDecoder { &self.decoder }

    /// Returns a terminal result only after Finished; success requires both a valid
    /// INFO and confirmed normal cleanup with no previous fault.
    pub fn outcome(&self) -> Option<Outcome> {
        if self.stage != Stage::Finished {
            return None;
        }
        if let Some(fault) = self.fault {
            return Some(Outcome::Failed(fault));
        }
        (self.cleanup == CleanupOutcome::Clean && self.decoder.has_identity()).then_some(Outcome::Success)
    }

    /// Checks exact `epoch` identity without changing any clock, buffer or fault.
    /// Returns unit on a match or StaleEpoch otherwise.
    fn check_epoch(
        &self,
        epoch: Epoch,
    ) -> Result<(), Rejected> {
        if self.epoch == Some(epoch) {
            Ok(())
        } else {
            Err(Rejected::StaleEpoch)
        }
    }

    /// Checks monotonic `now_ms` and the current immutable deadline, faulting expired work.
    /// Returns unit while current or a rejection after recording any time fault.
    /// A cleanup timeout finishes the attempt but does not claim cleanup succeeded.
    fn check_time(
        &mut self,
        now_ms: u64,
    ) -> Result<(), Rejected> {
        if matches!(self.stage, Stage::Idle | Stage::Finished) {
            return Err(Rejected::NotActive);
        }
        if now_ms < self.last_ms {
            self.fail(Reason::TimeReversed, now_ms);
            return Err(Rejected::Faulted);
        }
        self.last_ms = now_ms;
        if now_ms >= self.deadline_ms {
            if self.stage == Stage::Closing {
                self.fail(Reason::CleanupTimedOut, now_ms);
                self.cleanup = CleanupOutcome::TimedOut;
                self.stage = Stage::Finished;
            } else {
                self.fail(Reason::TimedOut, now_ms);
            }
            return Err(Rejected::Faulted);
        }
        Ok(())
    }

    /// Sets deadline to `now_ms` plus `budget_ms`, faulting rather than wrapping milliseconds.
    /// Returns unit on success or Faulted after overflow; overflow ends cleanup as unconfirmed.
    fn set_deadline(
        &mut self,
        now_ms: u64,
        budget_ms: u64,
    ) -> Result<(), Rejected> {
        if let Some(deadline) = now_ms.checked_add(budget_ms) {
            self.deadline_ms = deadline;
            Ok(())
        } else {
            self.fail(Reason::ClockOverflow, now_ms);
            self.stage = Stage::Finished;
            self.cleanup = CleanupOutcome::TimedOut;
            Err(Rejected::Faulted)
        }
    }

    /// Records `reason` once at observed `now_ms`, then requires cleanup with a fixed budget.
    /// Repeated faults never reset the cleanup timer. Overflow ends as unconfirmed cleanup.
    fn fail(
        &mut self,
        reason: Reason,
        now_ms: u64,
    ) {
        if self.fault.is_none() {
            self.fault = Some(Fault {
                stage: self.stage,
                reason,
                at_ms: now_ms,
            });
        }
        if !matches!(self.stage, Stage::Closing | Stage::Finished) {
            self.stage = Stage::Closing;
            if let Some(deadline) = self.last_ms.max(now_ms).checked_add(CLEANUP_MS) {
                self.deadline_ms = deadline;
            } else {
                self.stage = Stage::Finished;
                self.cleanup = CleanupOutcome::TimedOut;
            }
        }
    }

    /// Applies actual owner `result` at `now_ms`, retaining time/failure evidence.
    /// Returns Finished when the observation is admitted, or NotActive in unrelated stages.
    /// Late normal cleanup may unblock a fresh epoch but never erases the first fault.
    fn confirm_cleanup(
        &mut self,
        now_ms: u64,
        result: CleanupResult,
    ) -> Result<Stage, Rejected> {
        let late =
            self.stage == Stage::Finished && matches!(self.cleanup, CleanupOutcome::TimedOut | CleanupOutcome::Failed);
        if self.stage != Stage::Closing && !late {
            return Err(Rejected::NotActive);
        }
        if late {
            if now_ms < self.last_ms {
                self.fail(Reason::TimeReversed, now_ms);
            } else {
                self.last_ms = now_ms;
            }
        } else {
            let _ = self.check_time(now_ms);
        }
        self.cleanup = match result {
            CleanupResult::Clean => CleanupOutcome::Clean,
            CleanupResult::Failed => {
                self.fail(Reason::CleanupFailed, now_ms);
                CleanupOutcome::Failed
            }
        };
        self.stage = Stage::Finished;
        Ok(self.stage)
    }
}
