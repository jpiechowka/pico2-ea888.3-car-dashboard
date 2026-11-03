//! Explicit one-shot engine collection over fixed raw CAN; this module performs no I/O.
//!
//! The owner confirms collection, reserves evidence, and validates each affine permit
//! immediately before submission. Raw protocol00, 7E0/7E8 and host-owned ISO-TP are
//! fixed. Adapter firmware behavior and physical CAN metadata remain external trust
//! boundaries. One envelope per receive call is supported, including fragmented
//! envelopes across calls; coalesced envelopes deliberately fail closed.
//! Framing follows the pinned [CANyonero constructors](https://github.com/Automotive-Swift/Swift-CANyonero/blob/78091bb5599c4f5e3002fbd5700b826a625e8b04/Sources/libCANyonero/Protocol.cpp#L295-L339).

use super::Epoch;
use super::reads::{ReadId, Reply};
use crate::diagnostics::CanFrame;
use crate::diagnostics::isotp::{IsoTpRx, RxEventRef};

/// Whole collection allowance in milliseconds, never extended by responses.
const RUN_MS: u64 = 80_000;
/// Per-command/diagnostic allowance in milliseconds, including submission.
const STEP_MS: u64 = 2_000;
/// Maximum common envelope bytes, including its four-byte header.
const WIRE_MAX: usize = 245;

/// One statically selected command; remote values cannot create a new variant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    /// Adapter-reported supply query, not ECU voltage.
    Voltage,
    /// Raw CAN00, classical500k, zero separation nibbles.
    Open,
    /// Exact7E0/7E8/7FF arbitration, zero extensions.
    Arbitration,
    /// One compiled service22 entry.
    Read(
        /// Closed manifest identity; never an arbitrary DID.
        ReadId,
    ),
    /// Correlated receive CTS only, block4/STmin5ms/zero padding.
    FlowControl(
        /// Outstanding manifest read whose correlated FF/block authorized this CTS.
        ReadId,
    ),
    /// Close only this attempt's freshly returned handle after an unambiguous result.
    Close,
}

/// First terminal failure. None of these grants cleanup-command or retry authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// Wrong operation identity; rejected without mutating the live owner.
    Stale,
    /// Operation or completion arrived in an incompatible state.
    State,
    /// Evidence reservation was unavailable before outbound admission.
    Capacity,
    /// Monotonic time moved backwards or deadline addition overflowed.
    Clock,
    /// An immutable stage, ISO-TP or total deadline expired.
    Timeout,
    /// Caller explicitly stopped or observed transport failure; disconnect is required.
    Cancelled,
    /// Malformed envelope, suffix, unsupported type or unmatched response.
    Frame,
    /// Original adapter negative type, with bounded raw envelope retained.
    Remote(
        /// Exact E0..E5/EF type byte, without inferred ECU semantics.
        u8,
    ),
    /// Unmatched CAN identity, malformed ISO-TP, unsafe first-frame prefix or sequence.
    Diagnostic,
    /// More than three pending responses arrived within the same fixed deadline.
    PendingLimit,
}

/// Local submission evidence; no variant proves adapter processing or CAN acceptance.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SendState {
    /// Permit exists but its writer has not started submission.
    NotAttempted,
    /// Submission began; cancellation/error may leave partial or complete wire effects.
    InFlightUnknown,
    /// The complete application SDU was submitted locally.
    LocallyComplete,
}

/// Small retained evidence for the latest issued command, independent of later faults.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Submission {
    /// Exact compiled command for this attempt.
    pub command: Command,
    /// Local submission observation, never a physical delivery assertion.
    pub state: SendState,
    /// Boot-relative milliseconds immediately before submission, if attempted.
    pub attempted_at_ms: Option<u64>,
    /// Boot-relative local completion milliseconds, if observed.
    pub completed_at_ms: Option<u64>,
}

/// Nonforgeable local completion identity; copying it does not duplicate a permit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Token {
    /// Explicit connection/operation identity.
    epoch: Epoch,
    /// Nonwrapping serial within this single-use owner.
    serial: u16,
}

/// Affine exact request; no public constructor or arbitrary payload input exists.
#[derive(Debug)]
pub struct Request {
    /// Completion identity retained by the caller before moving this owner into I/O.
    token: Token,
    /// Complete header/body, bounded by the largest fixed arbitration frame.
    bytes: [u8; 19],
    /// Initialized request prefix in bytes.
    len: u8,
}
impl Request {
    /// Returns the exact immutable application frame; extracting it performs no I/O.
    pub fn bytes(&self) -> &[u8] { &self.bytes[..usize::from(self.len)] }

    /// Returns the identity checked immediately before writing and on local completion.
    pub const fn token(&self) -> Token { self.token }
}

/// Raw read result; positive data remain unqualified even when widths match.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReadStatus {
    /// Complete correlated positive payload, available through read_data().
    Positive,
    /// Complete exact negative reply with original NRC.
    Negative(
        /// Exact non-pending NRC; no automatic repair or retry follows.
        u8,
    ),
    /// F186 did not contain exactly01; subsequent reads are prohibited.
    SessionNotDefault,
}

/// Small synchronous observation; the caller copies evidence before requesting more work.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    /// More envelope/ISO-TP bytes are required; no new request is authorized.
    Pending,
    /// Adapter supply in raw millivolts; zero is a valid reported wire value.
    Voltage(
        /// Adapter-reported supply in millivolts, including zero.
        u16,
    ),
    /// Fresh automotive handle, not a reusable saved capability.
    Opened(
        /// Fresh B0 handle; zero has no special meaning in this raw-channel contract.
        u8,
    ),
    /// Fixed arbitration was acknowledged by the pending generic OK only.
    Configured,
    /// A correlated bounded first frame or block boundary permits one fixed CTS.
    FlowControlNeeded,
    /// One read result is retained until the next permit is issued.
    ReadComplete {
        /// Compiled read whose exact response echo was admitted.
        id: ReadId,
        /// Raw outcome; no variant qualifies a dashboard measurement.
        status: ReadStatus,
    },
    /// Matching CLOSE reply arrived; caller still must disconnect and confirm link cleanup.
    Finished,
}

/// What the borrowed evidence prefix represents; no transport traffic is replayable from it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EvidenceKind {
    /// Last complete or current partial CANyonero envelope, including its header.
    Envelope,
    /// Complete reassembled UDS service/identifier/data bytes.
    DiagnosticPayload,
}

/// Private sequencing state; ready is distinct from having issued an affine permit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    /// No explicit authorization has arrived.
    Idle,
    /// Exactly this compiled operation may issue a permit after evidence reservation.
    Ready(
        /// Sole next compiled command, contingent on an evidence reservation.
        Command,
    ),
    /// One permit is outstanding; no receive or second permit is admitted.
    Sending(
        /// Command bound to the outstanding affine permit.
        Command,
    ),
    /// Local submission completed and this reply alone is expected.
    Awaiting(
        /// Locally submitted command whose exact response is expected.
        Command,
    ),
    /// Matching close observed; physical cleanup remains the owner's responsibility.
    Finished,
    /// First terminal failure retained; no further application bytes allowed.
    Failed,
}

/// Pure bounded collection owner. Construct once per explicit confirmed operation.
/// The owner preserves unique epochs even across new instances. Requires a fresh channel
/// and external link cleanup; cannot prove adapter semantics or authenticate a peer.
pub struct Session {
    /// Current explicit generation, absent before start.
    epoch: Option<Epoch>,
    /// Ordered command and reply state.
    phase: Phase,
    /// First terminal failure, never replaced by cleanup observations.
    fault: Option<Error>,
    /// Last accepted monotonic milliseconds.
    last_ms: u64,
    /// Immutable whole-run deadline in milliseconds.
    run_deadline: u64,
    /// Current command/read deadline in milliseconds; FC does not extend it.
    step_deadline: u64,
    /// Sole automotive handle from this run's exact B0 response.
    handle: Option<u8>,
    /// Last nonwrapping permit serial.
    serial: u16,
    /// Latest permit's retained local submission evidence.
    submission: Option<Submission>,
    /// Shared reviewed ISO-TP receiver; no second transport engine is implemented.
    rx: IsoTpRx,
    /// True after admitting a correlated FF, until response completion/reset.
    segmented: bool,
    /// Number of accepted NRC78 replies for the current fixed request.
    pending: u8,
    /// Declared UDS response length including service/echo, zero until first-frame admission.
    response_bytes: u8,
    /// One shared envelope/reassembled-evidence allocation.
    wire: [u8; WIRE_MAX],
    /// Current incomplete envelope prefix length, zero between envelopes.
    frame_len: usize,
    /// Retained raw evidence length; next receive/permit may replace it.
    evidence_len: usize,
    /// Meaning of the retained prefix.
    evidence_kind: EvidenceKind,
    /// Last completed read identity/status, while its data remain borrowed.
    result: Option<(ReadId, ReadStatus)>,
}
impl Default for Session {
    /// Returns an idle owner with no admitted request or hardware resource.
    fn default() -> Self { Self::new() }
}
impl Session {
    /// Returns an idle owner without performing I/O or allocating a hardware resource.
    pub const fn new() -> Self {
        Self {
            epoch: None,
            phase: Phase::Idle,
            fault: None,
            last_ms: 0,
            run_deadline: 0,
            step_deadline: 0,
            handle: None,
            serial: 0,
            submission: None,
            rx: IsoTpRx::new(),
            segmented: false,
            pending: 0,
            response_bytes: 0,
            wire: [0; WIRE_MAX],
            frame_len: 0,
            evidence_len: 0,
            evidence_kind: EvidenceKind::Envelope,
            result: None,
        }
    }

    /// Admits explicit nonzero `epoch` at monotonic `now_ms` with `can_store` reservation.
    /// One instance starts once. Failure is terminal; no request is returned or transmitted.
    pub fn start(
        &mut self,
        epoch: Epoch,
        now_ms: u64,
        can_store: bool,
    ) -> Result<(), Error> {
        if self.phase != Phase::Idle || epoch.connection == 0 || epoch.operation == 0 {
            return self.fail(Error::State);
        }
        if !can_store {
            return self.fail(Error::Capacity);
        }
        let Some(deadline) = now_ms.checked_add(RUN_MS) else {
            return self.fail(Error::Clock);
        };
        self.epoch = Some(epoch);
        self.last_ms = now_ms;
        self.run_deadline = deadline;
        self.step_deadline = deadline;
        self.phase = Phase::Ready(Command::Voltage);
        Ok(())
    }

    /// Returns whether a next-command permit is pending. The owner must reject any already
    /// queued SDU before issuing it; this observation does not validate clock or generation.
    pub const fn ready_for_request(&self) -> bool { matches!(self.phase, Phase::Ready(_)) }

    /// Rejects already queued `bytes` observed while Ready for current `epoch` at `now_ms`.
    /// Retains at most245 incoming bytes as an envelope prefix, then faults without issuing
    /// any permit, including CLOSE. Wrong epochs preserve state; clock/fault checks precede
    /// copying. Calling outside Ready faults State. Empty or oversized SDUs also fault Frame.
    /// Returns Frame after retaining the prefix, or the earlier epoch/time/state rejection;
    /// this rejection path never returns success.
    pub fn reject_queued(
        &mut self,
        epoch: Epoch,
        now_ms: u64,
        bytes: &[u8],
    ) -> Result<(), Error> {
        self.check(epoch, now_ms)?;
        if !self.ready_for_request() {
            return self.fail(Error::State);
        }
        let count = bytes.len().min(WIRE_MAX);
        self.wire[..count].copy_from_slice(&bytes[..count]);
        self.frame_len = 0;
        self.evidence_len = count;
        self.evidence_kind = EvidenceKind::Envelope;
        self.result = None;
        self.fail(Error::Frame)
    }

    /// Issues at most one exact permit for `epoch` at `now_ms`, only if `can_store`
    /// reserves evidence for this command and its bounded reply. None means waiting/finished.
    /// Issuing a non-CTS permit invalidates borrowed previous-result bytes. CTS retains the
    /// latest received envelope for interrupted-reassembly evidence, not a full transcript. Failure stops all
    /// subsequent commands, including CLOSE; the owner must disconnect the transport.
    /// Returns Some owned permit when ready, None while awaiting or finished, or an admission error.
    pub fn next_request(
        &mut self,
        epoch: Epoch,
        now_ms: u64,
        can_store: bool,
    ) -> Result<Option<Request>, Error> {
        self.check(epoch, now_ms)?;
        let Phase::Ready(command) = self.phase else {
            return Ok(None);
        };
        if !can_store {
            return self.fail(Error::Capacity);
        }
        let Some(serial) = self.serial.checked_add(1) else {
            return self.fail(Error::State);
        };
        if !matches!(command, Command::FlowControl(_)) {
            let Some(deadline) = now_ms.checked_add(STEP_MS) else {
                return self.fail(Error::Clock);
            };
            self.step_deadline = deadline.min(self.run_deadline);
        }
        self.serial = serial;
        self.phase = Phase::Sending(command);
        self.submission = Some(Submission {
            command,
            state: SendState::NotAttempted,
            attempted_at_ms: None,
            completed_at_ms: None,
        });
        self.frame_len = 0;
        if !matches!(command, Command::FlowControl(_)) {
            self.evidence_len = 0;
            self.evidence_kind = EvidenceKind::Envelope;
        }
        self.result = None;
        Ok(Some(self.encode(Token { epoch, serial }, command)?))
    }

    /// Validates `token` against this live sending phase and `now_ms` immediately before I/O.
    /// Stale tokens leave live state unchanged; faults/deadlines revoke even previously issued permits.
    /// Returns unit only for a current sending permit, otherwise the stale, state or timing error.
    pub fn validate(
        &mut self,
        token: Token,
        now_ms: u64,
    ) -> Result<(), Error> {
        if self.epoch != Some(token.epoch) || token.serial != self.serial {
            return Err(Error::Stale);
        }
        self.check(token.epoch, now_ms)?;
        if !matches!(self.phase, Phase::Sending(_)) {
            return self.fail(Error::State);
        }
        Ok(())
    }

    /// Records submission start for validated `token` at `now_ms`, before polling the writer.
    /// A second start is terminal. Timeout/cancellation retains InFlightUnknown, never unsent.
    /// Returns unit after recording the attempt, or a validation/state error without starting I/O.
    pub fn mark_attempt(
        &mut self,
        token: Token,
        now_ms: u64,
    ) -> Result<(), Error> {
        self.validate(token, now_ms)?;
        let Some(mut observation) = self.submission else {
            return self.fail(Error::State);
        };
        if observation.state != SendState::NotAttempted {
            return self.fail(Error::State);
        }
        observation.state = SendState::InFlightUnknown;
        observation.attempted_at_ms = Some(now_ms);
        self.submission = Some(observation);
        Ok(())
    }

    /// Records whole-SDU local completion for `token` at `now_ms`. This is not a CAN ACK.
    /// The observation survives a simultaneous deadline fault; late completion cannot authorize
    /// another command. The caller never retries a partial/unknown submission.
    /// Returns unit after entering the reply phase, or a token/state/timing/ISO-TP error;
    /// an admitted completion timestamp remains recorded even when the timing check fails.
    pub fn sent(
        &mut self,
        token: Token,
        now_ms: u64,
    ) -> Result<(), Error> {
        if self.epoch != Some(token.epoch) || token.serial != self.serial {
            return Err(Error::Stale);
        }
        let Phase::Sending(command) = self.phase else {
            return self.fail(Error::State);
        };
        let Some(mut observation) = self.submission else {
            return self.fail(Error::State);
        };
        if observation.state != SendState::InFlightUnknown {
            return self.fail(Error::State);
        }
        observation.state = SendState::LocallyComplete;
        observation.completed_at_ms = Some(now_ms);
        self.submission = Some(observation);
        self.check(token.epoch, now_ms)?;
        match command {
            Command::Read(_) => {
                self.pending = 0;
                self.response_bytes = 0;
                self.segmented = false;
                self.rx.abort();
                if self
                    .rx
                    .begin(0x7E8, token.epoch.operation, now_ms, self.step_deadline)
                    .is_err()
                {
                    return self.fail(Error::Diagnostic);
                }
                self.phase = Phase::Awaiting(command);
            }
            Command::FlowControl(id) => {
                if self.rx.flow_control_sent(now_ms).is_err() {
                    return self.fail(Error::Diagnostic);
                }
                self.phase = Phase::Awaiting(Command::Read(id));
            }
            _ => self.phase = Phase::Awaiting(command),
        }
        Ok(())
    }

    /// Feeds one nonempty receive fragment `bytes` for `epoch` at `now_ms`. One envelope may span
    /// calls; any suffix/coalesced envelope fails before further authority is exposed.
    /// Exact type/handle/address/length/echo checks precede ISO-TP FC. Raw evidence remains
    /// borrowed until the next receive or permit. Errors are terminal and never grant CLOSE.
    /// Returns a pending, correlated reply or completion event, or an admission/protocol error.
    pub fn receive(
        &mut self,
        epoch: Epoch,
        now_ms: u64,
        bytes: &[u8],
    ) -> Result<Event, Error> {
        self.check(epoch, now_ms)?;
        let Phase::Awaiting(command) = self.phase else {
            return self.fail(Error::State);
        };
        if bytes.is_empty() {
            return self.fail(Error::Frame);
        }
        if self.frame_len == 0 {
            self.evidence_len = 0;
            self.evidence_kind = EvidenceKind::Envelope;
        }
        let room = WIRE_MAX - self.frame_len;
        let count = bytes.len().min(room);
        self.wire[self.frame_len..self.frame_len + count].copy_from_slice(&bytes[..count]);
        self.frame_len += count;
        self.evidence_len = self.frame_len;
        if count != bytes.len() || self.wire[0] != 0x1F {
            return self.fail(Error::Frame);
        }
        if self.frame_len < 4 {
            return Ok(Event::Pending);
        }
        let total = 4 + usize::from(u16::from_be_bytes([self.wire[2], self.wire[3]]));
        if total > WIRE_MAX || self.frame_len > total {
            return self.fail(Error::Frame);
        }
        if self.frame_len < total {
            return Ok(Event::Pending);
        }
        self.frame_len = 0;
        let kind = self.wire[1];
        if matches!(kind, 0xE0..=0xE5 | 0xEF) {
            return self.fail(Error::Remote(kind));
        }
        match command {
            Command::Voltage if kind == 0x92 && total == 6 => {
                let voltage = u16::from_be_bytes([self.wire[4], self.wire[5]]);
                self.phase = Phase::Ready(Command::Open);
                Ok(Event::Voltage(voltage))
            }
            Command::Open if kind == 0xB0 && total == 5 => {
                let handle = self.wire[4];
                self.handle = Some(handle);
                self.phase = Phase::Ready(Command::Arbitration);
                Ok(Event::Opened(handle))
            }
            Command::Arbitration if kind == 0x80 && total == 4 => {
                self.phase = Phase::Ready(Command::Read(ReadId::ALL[0]));
                Ok(Event::Configured)
            }
            Command::Read(id) if kind == 0xB2 => self.receive_can(id, now_ms, total),
            Command::Close if kind == 0xB1 && total == 5 && Some(self.wire[4]) == self.handle => {
                self.phase = Phase::Finished;
                Ok(Event::Finished)
            }
            _ => self.fail(Error::Frame),
        }
    }

    /// Checks `epoch` and immutable deadlines at `now_ms` without issuing any command.
    /// Returns unit if current and within its active budgets, or the first timing/fault rejection;
    /// a clean finished session remains accepted without refreshing deadlines.
    pub fn tick(
        &mut self,
        epoch: Epoch,
        now_ms: u64,
    ) -> Result<(), Error> {
        self.check(epoch, now_ms)
    }

    /// Stops this `epoch` at `now_ms`; retained first fault/submission/raw bytes survive.
    /// No automotive CLOSE is authorized; caller disconnects CoC/link and confirms cleanup.
    /// Returns Cancelled after stopping, or an earlier epoch/timing/fault error; never returns success.
    pub fn cancel(
        &mut self,
        epoch: Epoch,
        now_ms: u64,
    ) -> Result<(), Error> {
        self.check(epoch, now_ms)?;
        self.fail(Error::Cancelled)
    }

    /// Returns the first terminal failure, if any; not a vehicle diagnosis.
    pub const fn fault(&self) -> Option<Error> { self.fault }

    /// Returns true after a matching CLOSE or a terminal fault; physical cleanup is separate.
    pub const fn finished(&self) -> bool { matches!(self.phase, Phase::Finished | Phase::Failed) }

    /// Returns latest issued-command submission evidence, including uncertain partial effects.
    pub const fn submission(&self) -> Option<Submission> { self.submission }

    /// Returns accepted NRC78 count for this read, at most four after PendingLimit failure.
    pub const fn pending_count(&self) -> u8 { self.pending }

    /// Returns the admitted SF/FF UDS length including service/echo, at most67 bytes.
    /// None means no correlated first frame was admitted for the current read.
    pub const fn response_bytes(&self) -> Option<u8> {
        if self.response_bytes == 0 {
            None
        } else {
            Some(self.response_bytes)
        }
    }

    /// Returns retained raw bytes and their representation, without copying or replay authority.
    pub fn evidence(&self) -> (EvidenceKind, &[u8]) { (self.evidence_kind, &self.wire[..self.evidence_len]) }

    /// Returns the last positive data bytes after service/DID echo, or an empty slice for
    /// negatives/no result. SessionNotDefault retains its received positive bytes as evidence.
    pub fn read_data(&self) -> &[u8] {
        match self.result {
            Some((_, ReadStatus::Positive | ReadStatus::SessionNotDefault)) if self.evidence_len >= 3 => {
                &self.wire[3..self.evidence_len]
            }
            _ => &[],
        }
    }

    /// Returns current absolute stage/total deadline milliseconds while active, otherwise None.
    pub const fn deadline_ms(&self) -> Option<u64> {
        if matches!(self.phase, Phase::Idle | Phase::Finished | Phase::Failed) {
            None
        } else {
            Some(self.step_deadline)
        }
    }

    /// Constructs only literal setup/cleanup or the compiled read/CTS frame for private `command`.
    /// `token` binds completion. Missing fresh handle faults before returning a permit.
    fn encode(
        &mut self,
        token: Token,
        command: Command,
    ) -> Result<Request, Error> {
        let mut bytes = [0; 19];
        bytes[0] = 0x1F;
        let len = match command {
            Command::Voltage => {
                bytes[1] = 0x12;
                4
            }
            Command::Open => {
                bytes[..10].copy_from_slice(&[0x1F, 0x30, 0, 6, 0, 0, 7, 0xA1, 0x20, 0]);
                10
            }
            _ => {
                let Some(handle) = self.handle else {
                    return self.fail(Error::State);
                };
                bytes[4] = handle;
                match command {
                    Command::Arbitration => {
                        bytes[1] = 0x34;
                        bytes[3] = 15;
                        bytes[5..].copy_from_slice(&[0, 0, 7, 0xE0, 0, 0, 0, 7, 0xE8, 0, 0, 7, 0xFF, 0]);
                        19
                    }
                    Command::Read(id) => {
                        bytes[1] = 0x33;
                        bytes[3] = 9;
                        bytes[5] = 3;
                        bytes[6..9].copy_from_slice(&id.request());
                        13
                    }
                    Command::FlowControl(_) => {
                        bytes[1] = 0x33;
                        bytes[3] = 9;
                        bytes[5..13].copy_from_slice(&[0x30, 4, 5, 0, 0, 0, 0, 0]);
                        13
                    }
                    Command::Close => {
                        bytes[1] = 0x31;
                        bytes[3] = 1;
                        5
                    }
                    _ => return self.fail(Error::State),
                }
            }
        };
        Ok(Request { token, bytes, len })
    }

    /// Admits fixed B2 identity and correlated SF/FF before shared ISO-TP can request FC.
    /// `id` is the outstanding compiled read; `now_ms` is the checked monotonic receive time.
    /// `total` is a completely admitted envelope length; never indexes outside its prefix.
    /// Returns receive progress, correlated FC authority or the completed read, otherwise an error.
    /// Completed replies replace envelope evidence with the full bounded diagnostic payload.
    fn receive_can(
        &mut self,
        id: ReadId,
        now_ms: u64,
        total: usize,
    ) -> Result<Event, Error> {
        if !(11..=18).contains(&total)
            || Some(self.wire[4]) != self.handle
            || self.wire[5..9] != [0, 0, 7, 0xE8]
            || self.wire[9] != 0
        {
            return self.fail(Error::Diagnostic);
        }
        let data = &self.wire[10..total];
        if !self.segmented {
            let request = id.request();
            match data[0] >> 4 {
                0 => {
                    let len = usize::from(data[0] & 15);
                    if !(3..=7).contains(&len) || data.len() < len + 1 {
                        return self.fail(Error::Diagnostic);
                    }
                    let payload = &data[1..=len];
                    if id.classify(payload).is_err() {
                        return self.fail(Error::Diagnostic);
                    }
                    self.response_bytes = len as u8;
                }
                1 => {
                    if data.len() != 8 {
                        return self.fail(Error::Diagnostic);
                    }
                    let len = (usize::from(data[0] & 15) << 8) | usize::from(data[1]);
                    if !(8..=67).contains(&len)
                        || data[2..5] != [0x62, request[1], request[2]]
                        || id == ReadId::ActiveSession
                    {
                        return self.fail(Error::Diagnostic);
                    }
                    self.segmented = true;
                    self.response_bytes = len as u8;
                }
                _ => return self.fail(Error::Diagnostic),
            }
        }
        let frame = CanFrame::new(0x7E8, data).map_err(|_| Error::Diagnostic)?;
        // ISO-TP epoch is stable across CTS permits, which have distinct writer serials.
        let rx_epoch = self.epoch.map(|e| e.operation).ok_or(Error::State)?;
        // Borrow the shared parser's completed bytes; no256-byte owned event crosses this frame.
        let event = self.rx.on_frame_ref(&frame, rx_epoch, now_ms);
        match event {
            Ok(RxEventRef::Pending) => Ok(Event::Pending),
            Ok(RxEventRef::FlowControl([0x30, 4, 5, 0, 0, 0, 0, 0])) => {
                self.phase = Phase::Ready(Command::FlowControl(id));
                Ok(Event::FlowControlNeeded)
            }
            Ok(RxEventRef::Complete(payload)) => {
                self.segmented = false;
                let status = match id.classify(payload) {
                    Ok(Reply::Pending) => {
                        self.pending += 1;
                        if self.pending > 3 {
                            return self.fail(Error::PendingLimit);
                        }
                        if self.rx.begin(0x7E8, rx_epoch, now_ms, self.step_deadline).is_err() {
                            return self.fail(Error::Diagnostic);
                        }
                        return Ok(Event::Pending);
                    }
                    Ok(Reply::Negative(code)) => ReadStatus::Negative(code),
                    Ok(Reply::Positive { .. }) if id == ReadId::ActiveSession && payload != [0x62, 0xF1, 0x86, 1] => {
                        ReadStatus::SessionNotDefault
                    }
                    Ok(Reply::Positive { .. }) => ReadStatus::Positive,
                    Err(_) => return self.fail(Error::Diagnostic),
                };
                self.wire[..payload.len()].copy_from_slice(payload);
                self.evidence_len = payload.len();
                self.evidence_kind = EvidenceKind::DiagnosticPayload;
                self.result = Some((id, status));
                self.phase = Phase::Ready(
                    if (id == ReadId::ActiveSession && status != ReadStatus::Positive)
                        || id.index() + 1 == ReadId::ALL.len()
                    {
                        Command::Close
                    } else {
                        Command::Read(ReadId::ALL[id.index() + 1])
                    },
                );
                Ok(Event::ReadComplete { id, status })
            }
            _ => self.fail(Error::Diagnostic),
        }
    }

    /// Checks exact `epoch`, monotonic `now_ms`, immutable budgets and active ISO-TP timing.
    /// Stale identities never alter clock/fault state. Finished calls emit no new authority.
    /// Returns unit for a healthy active or clean finished session; otherwise returns the
    /// stale identity or retained first fault, latching newly observed clock/deadline failures.
    // All permit/receive transitions share the same small timing/identity guard.
    #[inline(never)]
    fn check(
        &mut self,
        epoch: Epoch,
        now_ms: u64,
    ) -> Result<(), Error> {
        if self.epoch != Some(epoch) {
            return Err(Error::Stale);
        }
        if let Some(error) = self.fault {
            return Err(error);
        }
        if self.phase == Phase::Finished {
            return Ok(());
        }
        if now_ms < self.last_ms {
            return self.fail(Error::Clock);
        }
        self.last_ms = now_ms;
        if now_ms >= self.run_deadline || now_ms >= self.step_deadline {
            return self.fail(Error::Timeout);
        }
        if self.rx.poll_timeout(now_ms).is_err() {
            return self.fail(Error::Timeout);
        }
        Ok(())
    }

    /// Latches `error`, aborts receive authority and rejects every future command.
    /// Retains submission and bounded raw evidence; no adapter cleanup bytes are generated.
    /// Returns Err with the first latched failure and never constructs a successful T value.
    fn fail<T>(
        &mut self,
        error: Error,
    ) -> Result<T, Error> {
        let first = *self.fault.get_or_insert(error);
        self.phase = Phase::Failed;
        self.rx.abort();
        Err(first)
    }
}

#[cfg(test)]
/// Production-sequencing regressions with literal synthetic wire traces and no device I/O.
mod tests {
    use super::*;
    /// Fixed synthetic operation identity, never a real peer.
    const EPOCH: Epoch = Epoch {
        connection: 1,
        operation: 1,
        maintenance: 0,
    };
    /// Detects queued pre-CTS/pre-next-read traffic advancing authority or losing its raw prefix.
    #[test]
    fn queued_receive_before_a_ready_permit_is_terminal_and_retained() {
        for flow_control in [false, true] {
            let mut session = if flow_control { identity_read() } else { configured() };
            assert!(!session.ready_for_request());
            if flow_control {
                receive_can(&mut session, &[0x10, 10, 0x62, 0xF1, 0x9E, 1, 2, 3]).unwrap();
            } else {
                receive_can(&mut session, &[4, 0x62, 0xF1, 0x86, 1]).unwrap();
            }
            assert!(session.ready_for_request());
            let bytes = [0x1F, 0xB2, 0, 11, 0x2A, 0, 0, 7, 0xE8, 0, 0x21, 4, 5, 6, 7];
            let wrong = Epoch { operation: 2, ..EPOCH };
            assert_eq!(session.reject_queued(wrong, u64::MAX, &bytes), Err(Error::Stale));
            assert!(session.ready_for_request());
            assert_eq!(session.reject_queued(EPOCH, 10, &bytes), Err(Error::Frame));
            assert!(!session.ready_for_request());
            assert_eq!(session.evidence(), (EvidenceKind::Envelope, &bytes[..]));
            assert!(session.next_request(EPOCH, 10, true).is_err());
            assert_eq!(session.reject_queued(EPOCH, 10, &[0]), Err(Error::Frame));
            assert_eq!(session.evidence().1, bytes);
        }
        let mut session = Session::new();
        session.start(EPOCH, 10, true).unwrap();
        let bytes = [0xA5; 246];
        assert_eq!(session.reject_queued(EPOCH, 10, &bytes), Err(Error::Frame));
        assert_eq!(session.evidence().1, &bytes[..245]);
        assert!(session.next_request(EPOCH, 10, true).is_err());
    }

    /// Detects erasing the last received FF/CF merely because a CTS permit was issued or completed.
    #[test]
    fn cts_timeout_preserves_last_received_envelope_and_declared_length() {
        for after_block in [false, true] {
            let mut session = identity_read();
            receive_can(&mut session, &[0x10, 67, 0x62, 0xF1, 0x9E, 1, 2, 3]).unwrap();
            if after_block {
                send(&mut session, &[0x1F, 0x33, 0, 9, 0x2A, 0x30, 4, 5, 0, 0, 0, 0, 0]);
                for sequence in 1..=4 {
                    receive_can(&mut session, &[0x20 | sequence, 4, 5, 6, 7, 8, 9, 10]).unwrap();
                }
            }
            let expected = if after_block {
                [0x1F, 0xB2, 0, 14, 0x2A, 0, 0, 7, 0xE8, 0, 0x24, 4, 5, 6, 7, 8, 9, 10]
            } else {
                [
                    0x1F, 0xB2, 0, 14, 0x2A, 0, 0, 7, 0xE8, 0, 0x10, 67, 0x62, 0xF1, 0x9E, 1, 2, 3,
                ]
            };
            assert_eq!(session.evidence().1, expected);
            send(&mut session, &[0x1F, 0x33, 0, 9, 0x2A, 0x30, 4, 5, 0, 0, 0, 0, 0]);
            assert_eq!(session.evidence(), (EvidenceKind::Envelope, &expected[..]));
            assert_eq!(session.response_bytes(), Some(67));
            assert_eq!(session.tick(EPOCH, 2010), Err(Error::Timeout));
            assert_eq!(session.evidence().1, expected);
            assert_eq!(session.response_bytes(), Some(67));
            assert!(session.next_request(EPOCH, 2010, true).is_err());
        }
    }

    /// Detects missing admission or widening the first operation beyond the fixed voltage query.
    #[test]
    fn explicit_start_only_grants_fixed_voltage_query() {
        let mut session = Session::new();
        session.start(EPOCH, 10, true).unwrap();
        let request = session.next_request(EPOCH, 10, true).unwrap().unwrap();
        assert_eq!(request.bytes(), &[0x1F, 0x12, 0, 0]);
        assert!(session.next_request(EPOCH, 10, true).unwrap().is_none());
    }

    /// Runs the real permit and completion boundary, asserting an independently specified wire frame.
    /// Advances `session` at synthetic10ms; `expected` contains the exact complete command bytes.
    /// Panics if admission, byte equality or local completion differs from the fixture.
    fn send(
        session: &mut Session,
        expected: &[u8],
    ) {
        let request = session.next_request(EPOCH, 10, true).unwrap().unwrap();
        assert_eq!(request.bytes(), expected);
        let token = request.token();
        session.mark_attempt(token, 10).unwrap();
        session.validate(token, 10).unwrap();
        session.sent(token, 10).unwrap();
    }

    /// Admits the real setup sequence, detecting protocol05, broad arbitration or handle substitution.
    /// Returns a session awaiting the first F186 reply; panics on any unexpected setup result.
    fn configured() -> Session {
        let mut session = Session::new();
        session.start(EPOCH, 10, true).unwrap();
        send(&mut session, &[0x1F, 0x12, 0, 0]);
        assert_eq!(
            session.receive(EPOCH, 10, &[0x1F, 0x92, 0, 2, 0x35, 0xE8]),
            Ok(Event::Voltage(13800))
        );
        send(&mut session, &[0x1F, 0x30, 0, 6, 0, 0, 7, 0xA1, 0x20, 0]);
        assert_eq!(
            session.receive(EPOCH, 10, &[0x1F, 0xB0, 0, 1, 0x2A]),
            Ok(Event::Opened(0x2A))
        );
        send(
            &mut session,
            &[
                0x1F, 0x34, 0, 15, 0x2A, 0, 0, 7, 0xE0, 0, 0, 0, 7, 0xE8, 0, 0, 7, 0xFF, 0,
            ],
        );
        assert_eq!(session.receive(EPOCH, 10, &[0x1F, 0x80, 0, 0]), Ok(Event::Configured));
        send(&mut session, &[0x1F, 0x33, 0, 9, 0x2A, 3, 0x22, 0xF1, 0x86, 0, 0, 0, 0]);
        session
    }

    /// Detects uncorrelated completion, default-session bypass and accidentally changing the first follow-up DID.
    #[test]
    fn default_session_reply_gates_next_literal_read() {
        let mut session = configured();
        let event = session.receive(
            EPOCH,
            10,
            &[
                0x1F, 0xB2, 0, 14, 0x2A, 0, 0, 7, 0xE8, 0, 4, 0x62, 0xF1, 0x86, 1, 0, 0, 0,
            ],
        );
        assert_eq!(
            event,
            Ok(Event::ReadComplete {
                id: ReadId::ActiveSession,
                status: ReadStatus::Positive
            })
        );
        assert_eq!(session.read_data(), &[1]);
        send(&mut session, &[0x1F, 0x33, 0, 9, 0x2A, 3, 0x22, 0xF1, 0x9E, 0, 0, 0, 0]);
    }

    /// Returns a real session awaiting the first identity read after exact default-session admission.
    fn identity_read() -> Session {
        let mut session = configured();
        session
            .receive(
                EPOCH,
                10,
                &[
                    0x1F, 0xB2, 0, 14, 0x2A, 0, 0, 7, 0xE8, 0, 4, 0x62, 0xF1, 0x86, 1, 0, 0, 0,
                ],
            )
            .unwrap();
        send(&mut session, &[0x1F, 0x33, 0, 9, 0x2A, 3, 0x22, 0xF1, 0x9E, 0, 0, 0, 0]);
        session
    }

    /// Wraps synthetic DLC1..8 `data` in the fixed matching response envelope for `session`.
    /// Returns its receive event/error at10ms; oversized fixture data panics while copying.
    fn receive_can(
        session: &mut Session,
        data: &[u8],
    ) -> Result<Event, Error> {
        let mut bytes = [0u8; 18];
        bytes[..10].copy_from_slice(&[0x1F, 0xB2, 0, (6 + data.len()) as u8, 0x2A, 0, 0, 7, 0xE8, 0]);
        bytes[10..10 + data.len()].copy_from_slice(data);
        session.receive(EPOCH, 10, &bytes[..10 + data.len()])
    }

    /// Detects any later ECU read when active session was negative/nondefault, including zero.
    #[test]
    fn session_denial_only_allows_matching_fresh_handle_close() {
        for state in [0, 2, 3, 0xFF] {
            let mut session = configured();
            assert_eq!(
                receive_can(&mut session, &[4, 0x62, 0xF1, 0x86, state]),
                Ok(Event::ReadComplete {
                    id: ReadId::ActiveSession,
                    status: ReadStatus::SessionNotDefault
                })
            );
            send(&mut session, &[0x1F, 0x31, 0, 1, 0x2A]);
            assert_eq!(
                session.receive(EPOCH, 10, &[0x1F, 0xB1, 0, 1, 0x2A]),
                Ok(Event::Finished)
            );
            assert!(session.next_request(EPOCH, 10, true).unwrap().is_none());
        }
        let mut session = configured();
        assert_eq!(
            receive_can(&mut session, &[3, 0x7F, 0x22, 0x31]),
            Ok(Event::ReadComplete {
                id: ReadId::ActiveSession,
                status: ReadStatus::Negative(0x31)
            })
        );
        assert!(session.read_data().is_empty());
        send(&mut session, &[0x1F, 0x31, 0, 1, 0x2A]);
        assert_eq!(session.receive(EPOCH, 10, &[0x1F, 0xB1, 0, 1, 0x2B]), Err(Error::Frame));
        assert!(session.next_request(EPOCH, 10, true).is_err());
    }

    /// Detects arbitrary-DID/service/header/padding drift, skipped/duplicate entries and cleanup retries.
    #[test]
    fn entire_negative_collection_has_exact_closed_outbound_trace() {
        let mut session = configured();
        receive_can(&mut session, &[4, 0x62, 0xF1, 0x86, 1]).unwrap();
        let dids: [u16; 27] = [
            0xF19E, 0xF1A2, 0xF187, 0xF189, 0xF191, 0xF1A3, 0xF197, 0xF1AD, 0x39C0, 0x13CA, 0x202F, 0x11CD, 0x1001,
            0x14A6, 0x10C0, 0x2006, 0x200A, 0x200B, 0x200C, 0x200D, 0x2029, 0x202A, 0x2065, 0x293E, 0x293F, 0x29BF,
            0xF40F,
        ];
        for (index, did) in dids.into_iter().enumerate() {
            let [hi, lo] = did.to_be_bytes();
            send(&mut session, &[0x1F, 0x33, 0, 9, 0x2A, 3, 0x22, hi, lo, 0, 0, 0, 0]);
            assert_eq!(
                receive_can(&mut session, &[3, 0x7F, 0x22, 0x31]),
                Ok(Event::ReadComplete {
                    id: ReadId::ALL[index + 1],
                    status: ReadStatus::Negative(0x31)
                })
            );
        }
        send(&mut session, &[0x1F, 0x31, 0, 1, 0x2A]);
        assert_eq!(
            session.receive(EPOCH, 10, &[0x1F, 0xB1, 0, 1, 0x2A]),
            Ok(Event::Finished)
        );
        assert!(session.finished());
        assert!(session.next_request(EPOCH, 10, true).unwrap().is_none());
    }

    /// Detects FC generated by a foreign/oversized/uncorrelated FF, unsupported flag or generic ACK.
    #[test]
    fn hostile_receive_bytes_cannot_authorize_flow_control_or_cleanup() {
        let valid = [
            0x1F, 0xB2, 0, 14, 0x2A, 0, 0, 7, 0xE8, 0, 0x10, 10, 0x62, 0xF1, 0x9E, 1, 2, 3,
        ];
        for (offset, value) in [
            (0, 0),
            (1, 0xB3),
            (2, 1),
            (3, 15),
            (4, 0x2B),
            (5, 0x80),
            (8, 0xE9),
            (9, 1),
            (10, 0x20),
            (11, 68),
            (11, 7),
            (12, 0x63),
            (13, 0xF2),
            (14, 0x9F),
        ] {
            let mut session = identity_read();
            let mut bytes = valid;
            bytes[offset] = value;
            // Declared-long envelopes may wait for more bytes, but cannot issue a permit.
            let result = session.receive(EPOCH, 10, &bytes);
            if result == Ok(Event::Pending) {
                session.tick(EPOCH, 2010).unwrap_err();
            } else {
                assert!(result.is_err(), "offset {offset}");
            }
            assert!(session.next_request(EPOCH, 10, true).is_err(), "offset {offset}");
            assert!(session.fault().is_some());
        }
        let mut session = identity_read();
        assert_eq!(session.receive(EPOCH, 10, &[0x1F, 0x80, 0, 0]), Err(Error::Frame));
        assert!(session.next_request(EPOCH, 10, true).is_err());
    }

    /// Detects early CF admission, incorrect fixed CTS bytes and failure to retain a complete short final CF.
    #[test]
    fn bounded_correlated_multiframe_requires_local_cts_completion() {
        let mut early = identity_read();
        assert_eq!(
            receive_can(&mut early, &[0x10, 10, 0x62, 0xF1, 0x9E, 1, 2, 3]),
            Ok(Event::FlowControlNeeded)
        );
        assert!(receive_can(&mut early, &[0x21, 4, 5, 6, 7]).is_err());
        assert!(early.next_request(EPOCH, 10, true).is_err());

        let mut session = identity_read();
        assert_eq!(
            receive_can(&mut session, &[0x10, 10, 0x62, 0xF1, 0x9E, 1, 2, 3]),
            Ok(Event::FlowControlNeeded)
        );
        send(&mut session, &[0x1F, 0x33, 0, 9, 0x2A, 0x30, 4, 5, 0, 0, 0, 0, 0]);
        assert_eq!(
            receive_can(&mut session, &[0x21, 4, 5, 6, 7]),
            Ok(Event::ReadComplete {
                id: ReadId::OdxIdentifier,
                status: ReadStatus::Positive
            })
        );
        assert_eq!(session.read_data(), &[1, 2, 3, 4, 5, 6, 7]);
        assert_eq!(
            session.evidence(),
            (
                EvidenceKind::DiagnosticPayload,
                &[0x62, 0xF1, 0x9E, 1, 2, 3, 4, 5, 6, 7][..]
            )
        );
    }

    /// Detects unbounded FC blocks, oversize buffers and lost identity across multiple CTS permits.
    #[test]
    fn maximum_response_has_three_fixed_cts_and_sixty_four_data_bytes() {
        let mut session = identity_read();
        assert_eq!(
            receive_can(&mut session, &[0x10, 67, 0x62, 0xF1, 0x9E, 0xA5, 0xA5, 0xA5]),
            Ok(Event::FlowControlNeeded)
        );
        assert_eq!(session.response_bytes(), Some(67));
        let mut remaining = 61;
        let mut sequence = 1;
        let mut controls = 0;
        loop {
            send(&mut session, &[0x1F, 0x33, 0, 9, 0x2A, 0x30, 4, 5, 0, 0, 0, 0, 0]);
            controls += 1;
            for _ in 0..4 {
                let count = remaining.min(7);
                let mut cf = [0xA5; 8];
                cf[0] = 0x20 | sequence;
                sequence += 1;
                remaining -= count;
                let event = receive_can(&mut session, &cf[..count + 1]).unwrap();
                if remaining == 0 {
                    assert_eq!(
                        event,
                        Event::ReadComplete {
                            id: ReadId::OdxIdentifier,
                            status: ReadStatus::Positive
                        }
                    );
                    assert_eq!(controls, 3);
                    assert_eq!(session.read_data(), &[0xA5; 64]);
                    return;
                }
                assert_eq!(
                    event,
                    if sequence % 4 == 1 {
                        Event::FlowControlNeeded
                    } else {
                        Event::Pending
                    }
                );
            }
        }
    }

    /// Detects automatic retransmission, timeout extension and treating NRC78 as success.
    #[test]
    fn pending_is_bounded_waiting_without_new_permits() {
        let mut session = configured();
        for _ in 0..3 {
            assert_eq!(receive_can(&mut session, &[3, 0x7F, 0x22, 0x78]), Ok(Event::Pending));
            assert!(session.next_request(EPOCH, 10, true).unwrap().is_none());
            assert_eq!(session.deadline_ms(), Some(2010));
        }
        assert_eq!(
            receive_can(&mut session, &[3, 0x7F, 0x22, 0x78]),
            Err(Error::PendingLimit)
        );
        assert_eq!(session.pending_count(), 4);
        assert!(session.next_request(EPOCH, 10, true).is_err());
    }

    /// Detects lost unknown-send evidence and stale completion/permit reuse after cancellation.
    #[test]
    fn cancellation_capacity_and_token_errors_never_retry() {
        for attempted in [false, true] {
            let mut session = Session::new();
            session.start(EPOCH, 10, true).unwrap();
            let request = session.next_request(EPOCH, 10, true).unwrap().unwrap();
            let token = request.token();
            if attempted {
                session.mark_attempt(token, 10).unwrap();
            }
            assert_eq!(session.cancel(EPOCH, 11), Err(Error::Cancelled));
            assert_eq!(
                session.submission().unwrap().state,
                if attempted {
                    SendState::InFlightUnknown
                } else {
                    SendState::NotAttempted
                }
            );
            assert!(session.validate(token, 12).is_err());
            assert!(session.sent(token, 12).is_err());
            assert!(session.next_request(EPOCH, 12, true).is_err());
        }
        let mut session = Session::new();
        assert_eq!(session.start(EPOCH, 10, false), Err(Error::Capacity));
        assert!(session.start(EPOCH, 10, true).is_err());
        let mut session = Session::new();
        session.start(EPOCH, 10, true).unwrap();
        assert_eq!(session.next_request(EPOCH, 10, false).unwrap_err(), Error::Capacity);
        assert!(session.next_request(EPOCH, 10, true).is_err());
        let mut session = configured();
        let wrong = Epoch { operation: 2, ..EPOCH };
        assert_eq!(session.receive(wrong, u64::MAX, &[0]), Err(Error::Stale));
        assert_eq!(session.fault(), None);
        receive_can(&mut session, &[4, 0x62, 0xF1, 0x86, 1]).unwrap();
    }

    /// Detects suffix acceptance, loss of fragmented raw evidence, deadline equality or clock wrap.
    #[test]
    fn framing_and_time_fail_closed_with_original_evidence() {
        for split in 1..6 {
            let mut session = Session::new();
            session.start(EPOCH, 10, true).unwrap();
            send(&mut session, &[0x1F, 0x12, 0, 0]);
            let bytes = [0x1F, 0x92, 0, 2, 0, 0];
            assert_eq!(session.receive(EPOCH, 10, &bytes[..split]), Ok(Event::Pending));
            assert_eq!(session.receive(EPOCH, 10, &bytes[split..]), Ok(Event::Voltage(0)));
        }
        let mut session = configured();
        let raw = [0x1F, 0xB2, 0, 14, 0x2A, 0, 0];
        assert_eq!(session.receive(EPOCH, 10, &raw), Ok(Event::Pending));
        assert_eq!(session.tick(EPOCH, 2010), Err(Error::Timeout));
        assert_eq!(session.evidence(), (EvidenceKind::Envelope, &raw[..]));
        assert_eq!(session.cancel(EPOCH, 2011), Err(Error::Timeout));
        let mut session = configured();
        assert_eq!(
            session.receive(
                EPOCH,
                10,
                &[
                    0x1F, 0xB2, 0, 14, 0x2A, 0, 0, 7, 0xE8, 0, 4, 0x62, 0xF1, 0x86, 1, 0, 0, 0, 0x1F
                ]
            ),
            Err(Error::Frame)
        );
        assert!(session.next_request(EPOCH, 10, true).is_err());
        let mut session = configured();
        assert_eq!(session.tick(EPOCH, 9), Err(Error::Clock));
        let mut session = Session::new();
        assert_eq!(session.start(EPOCH, u64::MAX, true), Err(Error::Clock));
    }

    /// Detects a same-token second completion, stale-token mutation and lost late local completion evidence.
    #[test]
    fn writer_completion_cannot_restore_expired_or_consumed_authority() {
        let mut session = Session::new();
        session.start(EPOCH, 10, true).unwrap();
        let request = session.next_request(EPOCH, 10, true).unwrap().unwrap();
        let token = request.token();
        session.mark_attempt(token, 10).unwrap();
        assert_eq!(session.sent(token, 2010), Err(Error::Timeout));
        assert_eq!(session.submission().unwrap().state, SendState::LocallyComplete);
        assert_eq!(session.submission().unwrap().completed_at_ms, Some(2010));
        assert!(session.next_request(EPOCH, 2011, true).is_err());

        let mut session = Session::new();
        session.start(EPOCH, 10, true).unwrap();
        let request = session.next_request(EPOCH, 10, true).unwrap().unwrap();
        let token = request.token();
        session.mark_attempt(token, 10).unwrap();
        session.sent(token, 10).unwrap();
        session.receive(EPOCH, 10, &[0x1F, 0x92, 0, 2, 0, 0]).unwrap();
        let second = session.next_request(EPOCH, 10, true).unwrap().unwrap();
        assert_eq!(session.sent(token, u64::MAX), Err(Error::Stale));
        assert_eq!(session.fault(), None);
        assert_eq!(session.submission().unwrap().state, SendState::NotAttempted);
        session.mark_attempt(second.token(), 10).unwrap();
        session.sent(second.token(), 10).unwrap();
        assert_eq!(session.sent(second.token(), 10), Err(Error::State));
        assert!(session.next_request(EPOCH, 10, true).is_err());
    }

    /// Detects permitting a second CTS after cancellation, accepting duplicate FF/SF, or bad CF sequence.
    #[test]
    fn segmented_errors_revoke_all_followup_bytes() {
        for invalid in [
            [0x22, 4, 5, 6, 7, 0, 0, 0],
            [0x10, 10, 0x62, 0xF1, 0x9E, 1, 2, 3],
            [4, 0x62, 0xF1, 0x9E, 1, 0, 0, 0],
        ] {
            let mut session = identity_read();
            receive_can(&mut session, &[0x10, 10, 0x62, 0xF1, 0x9E, 1, 2, 3]).unwrap();
            send(&mut session, &[0x1F, 0x33, 0, 9, 0x2A, 0x30, 4, 5, 0, 0, 0, 0, 0]);
            assert_eq!(receive_can(&mut session, &invalid), Err(Error::Diagnostic));
            assert!(session.next_request(EPOCH, 10, true).is_err());
        }
        let mut session = identity_read();
        receive_can(&mut session, &[0x10, 10, 0x62, 0xF1, 0x9E, 1, 2, 3]).unwrap();
        let request = session.next_request(EPOCH, 10, true).unwrap().unwrap();
        session.mark_attempt(request.token(), 10).unwrap();
        assert_eq!(session.cancel(EPOCH, 10), Err(Error::Cancelled));
        assert_eq!(session.submission().unwrap().state, SendState::InFlightUnknown);
        assert!(session.sent(request.token(), 10).is_err());
        assert!(session.next_request(EPOCH, 10, true).is_err());
    }

    /// Detects treating handle zero as unavailable or substituting a fixed handle after fresh OPEN.
    #[test]
    fn fresh_zero_handle_is_scoped_like_every_other_handle() {
        let mut session = Session::new();
        session.start(EPOCH, 10, true).unwrap();
        send(&mut session, &[0x1F, 0x12, 0, 0]);
        session.receive(EPOCH, 10, &[0x1F, 0x92, 0, 2, 0, 0]).unwrap();
        send(&mut session, &[0x1F, 0x30, 0, 6, 0, 0, 7, 0xA1, 0x20, 0]);
        session.receive(EPOCH, 10, &[0x1F, 0xB0, 0, 1, 0]).unwrap();
        send(
            &mut session,
            &[0x1F, 0x34, 0, 15, 0, 0, 0, 7, 0xE0, 0, 0, 0, 7, 0xE8, 0, 0, 7, 0xFF, 0],
        );
        session.receive(EPOCH, 10, &[0x1F, 0x80, 0, 0]).unwrap();
        send(&mut session, &[0x1F, 0x33, 0, 9, 0, 3, 0x22, 0xF1, 0x86, 0, 0, 0, 0]);
        assert_eq!(
            session.receive(
                EPOCH,
                10,
                &[0x1F, 0xB2, 0, 10, 0, 0, 0, 7, 0xE8, 0, 3, 0x7F, 0x22, 0x31]
            ),
            Ok(Event::ReadComplete {
                id: ReadId::ActiveSession,
                status: ReadStatus::Negative(0x31)
            })
        );
        send(&mut session, &[0x1F, 0x31, 0, 1, 0]);
        assert_eq!(session.receive(EPOCH, 10, &[0x1F, 0xB1, 0, 1, 0]), Ok(Event::Finished));
    }

    /// Detects overflowing a receive buffer, retrying remote errors or generating overflow FC.
    #[test]
    fn bounded_raw_faults_preserve_prefix_and_never_send_overflow() {
        let mut session = identity_read();
        let mut bytes = [0x55; 246];
        bytes[..4].copy_from_slice(&[0x1F, 0xB2, 0, 242]);
        assert_eq!(session.receive(EPOCH, 10, &bytes), Err(Error::Frame));
        assert_eq!(session.evidence().1, &bytes[..245]);
        assert!(session.next_request(EPOCH, 10, true).is_err());
        for kind in [0xE0, 0xE1, 0xE2, 0xE3, 0xE4, 0xE5, 0xEF] {
            let mut session = configured();
            let bytes = [0x1F, kind, 0, 2, 0xAA, 0xBB];
            assert_eq!(session.receive(EPOCH, 10, &bytes), Err(Error::Remote(kind)));
            assert_eq!(session.evidence().1, bytes);
            assert!(session.next_request(EPOCH, 10, true).is_err());
        }
        let mut session = identity_read();
        assert_eq!(
            receive_can(&mut session, &[0x11, 0, 0x62, 0xF1, 0x9E, 1, 2, 3]),
            Err(Error::Diagnostic)
        );
        assert!(session.next_request(EPOCH, 10, true).is_err());
    }
}
