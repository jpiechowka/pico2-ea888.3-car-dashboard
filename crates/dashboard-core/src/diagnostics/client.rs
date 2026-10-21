//! Single-owner read transaction state. TX tickets include request generation
//! and a unique serial, so old or duplicate UART completions cannot authorize RX.
use super::CanFrame;
use super::isotp::{IsoError, IsoTpRx, Payload, RxEvent};
use super::request::{Reply, ReplyError, Request};

/// Initial-response application budget in milliseconds after UART completion; not ECU-qualified.
pub const RESPONSE_MS: u64 = 500;
/// Response allowance in milliseconds after each accepted NRC0x78, capped by TOTAL_MS.
pub const PENDING_MS: u64 = 1_000;
/// Inclusive absolute transaction budget in milliseconds from request admission.
pub const TOTAL_MS: u64 = 5_000;
/// Inclusive UART completion allowance in milliseconds for each request/FC write.
pub const TX_MS: u64 = 100;
/// Maximum accepted NRC0x78 responses per request; the fourth fails.
pub const MAX_PENDING: u8 = 3;

/// Owner/API failure distinct from a completed transaction's protocol failure event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClientError {
    /// Non-eleven-bit or equal transmit/receive identifiers.
    Address,
    /// Initial epoch zero or replacement epoch not strictly newer.
    Epoch,
    /// Regressed clock or unrepresentable total deadline; reversal invalidates transport.
    Clock,
    /// A request is already active; no new request is admitted.
    Busy,
    /// TX token is absent, duplicate or belongs to older work; no clock/state change.
    StaleTx,
    /// Request-generation or TX-serial counter cannot advance without reuse.
    Exhausted,
    /// Transport is invalidated; a newly qualified epoch is required.
    WireFault,
}
/// Terminal transaction failure reported with the original request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Failure {
    /// A UART write exceeded its completion allowance.
    TxTimeout,
    /// No matching first reply began within the response allowance.
    ResponseTimeout,
    /// Absolute request budget expired, regardless of pending replies.
    TotalTimeout,
    /// More than MAX_PENDING response-pending replies arrived.
    PendingLimit,
    /// Response exceeded fixed reassembly storage; overflow FC has completed.
    Oversized,
    /// Reassembly, flow-control or transport-clock failure.
    Transport(
        /// Exact ISO-TP error.
        IsoError,
    ),
    /// Malformed matching application reply.
    Reply(
        /// Exact service/identifier/length classification.
        ReplyError,
    ),
}
/// Unforgeable completion identity unique across requests and transport epochs of this client.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TxToken {
    /// Owning nonzero request generation, retained across epoch changes.
    generation: u64,
    /// Nonzero serial for this UART write, including flow-control writes.
    serial: u64,
}
/// One CAN record to transmit; owner must report this exact token only after UART idle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TxAction {
    /// Completion identity invalidated by transaction termination or wire faults.
    pub token: TxToken,
    /// Known-DLC classical CAN frame, not proof of actual CAN delivery.
    pub frame: CanFrame,
}
/// Correlated positive reply with service/identifier headers removed.
#[derive(Debug, PartialEq, Eq)]
pub struct Response {
    /// Request whose positive service and identifier matched.
    pub request: Request,
    /// Nonzero request generation suitable for sample ordering.
    pub generation: u64,
    /// Qualified transport-boundary generation supplied by the owner.
    pub epoch: u64,
    /// Exact eleven-bit responder identifier.
    pub ecu: u16,
    /// Monotonic milliseconds at complete reply admission.
    pub received_at_ms: u64,
    /// Application data after positive service and PID/DID, still requiring decoder validation.
    pub bytes: Payload,
}
/// Client progress or terminal publication; storage is bounded and owned.
#[derive(Debug, PartialEq, Eq)]
#[allow(clippy::large_enum_variant)] // fixed response storage, no allocator
pub enum Event {
    /// No application transition: idle, irrelevant input or accepted TX completion.
    Ignored,
    /// More reassembly data or a later response after NRC0x78 is required.
    Pending,
    /// An FC/overflow record must be serialized before receive progress continues.
    Transmit(
        /// Record and unique UART completion token.
        TxAction,
    ),
    /// Complete positive reply; transaction is now idle.
    Positive(
        /// Correlated application data and receive metadata.
        Response,
    ),
    /// Terminal matching negative response other than NRC0x78.
    Negative {
        /// Original request.
        request: Request,
        /// ECU negative-response code, without interpretation as supported/unsupported.
        nrc: u8,
    },
    /// Terminal failure; outstanding UART work also invalidates transport.
    Failed {
        /// Original request.
        request: Request,
        /// Failure that ended this transaction.
        reason: Failure,
    },
}
/// Meaning of the one outstanding UART write.
#[derive(Clone, Copy)]
enum TxKind {
    /// Starts response reception after completion.
    Request,
    /// Permits the next consecutive-frame block after completion.
    FlowControl,
    /// Completes the oversized-response failure after sending overflow FC.
    Overflow,
}
/// One write whose completion has not yet been accepted.
#[derive(Clone, Copy)]
struct PendingTx {
    /// Exact identity required by tx_completed.
    token: TxToken,
    /// State transition authorized by successful completion.
    kind: TxKind,
    /// Inclusive absolute UART completion deadline, monotonic milliseconds.
    deadline: u64,
}
/// Correlation and deadline state for one admitted request.
#[derive(Clone, Copy)]
struct Transaction {
    /// Read service and PID/DID to match.
    request: Request,
    /// Unique request sequence, never reset by changing epoch.
    generation: u64,
    /// Inclusive absolute deadline in milliseconds, never extended by NRC0x78.
    hard_deadline: u64,
    /// Inclusive initial/pending-response deadline in milliseconds.
    response_deadline: u64,
    /// Original request UART completion was accepted.
    sent: bool,
    /// A matching first frame started reassembly; ISO-TP deadlines now govern progress.
    reply_started: bool,
    /// Accepted NRC0x78 count, bounded by MAX_PENDING.
    pending_count: u8,
}

/// Single-owner transaction engine. Caller supplies qualified CAN records and UART
/// completion; no ECU, bridge, timing or physical-delivery qualification is inferred.
pub struct Client {
    /// Eleven-bit request/flow-control CAN identifier.
    tx_id: u16,
    /// Different eleven-bit responder identifier.
    rx_id: u16,
    /// Nonzero owner-issued transport generation.
    epoch: u64,
    /// Last issued request generation; exhaustion never wraps.
    generation: u64,
    /// Last issued UART ticket serial; exhaustion never wraps.
    tx_serial: u64,
    /// Last accepted monotonic millisecond observation.
    last_ms: u64,
    /// Sticky need for an independently qualified newer epoch.
    wire_fault: bool,
    /// Bounded reassembly for the active response.
    rx: IsoTpRx,
    /// Current request or None while idle.
    active: Option<Transaction>,
    /// Sole UART write awaiting exact completion.
    pending_tx: Option<PendingTx>,
}
impl Client {
    /// Returns an idle client bound to distinct eleven-bit `tx_id`/`rx_id` and nonzero
    /// `epoch`, or Address/Epoch without issuing traffic. Initial monotonic clock is zero.
    pub fn new(
        tx_id: u16,
        rx_id: u16,
        epoch: u64,
    ) -> Result<Self, ClientError> {
        if tx_id > 0x7FF || rx_id > 0x7FF || tx_id == rx_id {
            return Err(ClientError::Address);
        }
        if epoch == 0 {
            return Err(ClientError::Epoch);
        }
        Ok(Self {
            tx_id,
            rx_id,
            epoch,
            generation: 0,
            tx_serial: 0,
            last_ms: 0,
            wire_fault: false,
            rx: IsoTpRx::new(),
            active: None,
            pending_tx: None,
        })
    }

    /// Returns whether a request is active, including outstanding UART writes.
    pub fn active(&self) -> bool { self.active.is_some() }

    /// Returns the currently admitted transport-boundary generation.
    pub const fn epoch(&self) -> u64 { self.epoch }

    /// Returns the fixed eleven-bit responder identifier.
    pub const fn response_id(&self) -> u16 { self.rx_id }

    /// Returns whether transport ambiguity forbids another request until change_epoch.
    pub const fn requires_new_epoch(&self) -> bool { self.wire_fault }

    /// Known byte loss or an abandoned UART write invalidates all local tokens.
    pub fn invalidate_transport(&mut self) {
        self.clear();
        self.wire_fault = true;
    }

    /// Clears active request, UART ticket and reassembly without changing clock, counters or wire fault.
    fn clear(&mut self) {
        self.active = None;
        self.pending_tx = None;
        self.rx.abort();
    }

    /// Admits monotonic `now` in milliseconds. Returns success and updates last time;
    /// reversal clears the transaction, latches wire fault and returns Clock.
    fn clock(
        &mut self,
        now: u64,
    ) -> Result<(), ClientError> {
        if now < self.last_ms {
            self.clear();
            self.wire_fault = true;
            return Err(ClientError::Clock);
        }
        self.last_ms = now;
        Ok(())
    }

    /// Only a newly qualified transport boundary may clear a wire failure.
    /// Keep counters monotonic across epochs, including sample generations.
    /// `epoch` must strictly exceed the current value; `now` is monotonic milliseconds.
    /// Returns success after aborting local work and clearing wire fault, or Epoch without
    /// mutation / Clock with transport invalidation. Caller must first drain/discard old I/O.
    pub fn change_epoch(
        &mut self,
        epoch: u64,
        now: u64,
    ) -> Result<(), ClientError> {
        if epoch <= self.epoch {
            return Err(ClientError::Epoch);
        }
        self.clock(now)?;
        self.clear();
        self.epoch = epoch;
        self.wire_fault = false;
        Ok(())
    }

    /// Returns a new UART action for `frame`, owning request `generation`, and `kind`
    /// at checked monotonic `now` in milliseconds. Replaces the pending slot; callers
    /// enforce serialization. Serial exhaustion clears work and latches wire fault.
    fn queue(
        &mut self,
        frame: CanFrame,
        generation: u64,
        kind: TxKind,
        now: u64,
    ) -> Result<TxAction, ClientError> {
        let Some(serial) = self.tx_serial.checked_add(1) else {
            self.clear();
            self.wire_fault = true;
            return Err(ClientError::Exhausted);
        };
        self.tx_serial = serial;
        let token = TxToken { generation, serial };
        self.pending_tx = Some(PendingTx {
            token,
            kind,
            deadline: now.saturating_add(TX_MS),
        });
        Ok(TxAction { token, frame })
    }

    /// Admits read `request` at monotonic `now` in milliseconds and returns its UART
    /// action. Requires idle, usable transport. Busy/wire-fault checks precede time;
    /// clock reversal invalidates transport, deadline/generation overflow rejects admission,
    /// and UART-serial exhaustion additionally latches wire fault. No I/O is performed.
    pub fn start(
        &mut self,
        request: Request,
        now: u64,
    ) -> Result<TxAction, ClientError> {
        if self.wire_fault {
            return Err(ClientError::WireFault);
        }
        if self.active() {
            return Err(ClientError::Busy);
        }
        self.clock(now)?;
        let hard_deadline = now.checked_add(TOTAL_MS).ok_or(ClientError::Clock)?;
        let generation = self.generation.checked_add(1).ok_or(ClientError::Exhausted)?;
        let frame = request.frame(self.tx_id).map_err(|_| ClientError::Address)?;
        let tx = self.queue(frame, generation, TxKind::Request, now)?;
        self.generation = generation;
        self.active = Some(Transaction {
            request,
            generation,
            hard_deadline,
            response_deadline: hard_deadline,
            sent: false,
            reply_started: false,
            pending_count: 0,
        });
        Ok(tx)
    }

    /// Ends the active transaction and returns Failed with `reason`; outstanding UART
    /// work latches wire fault. Panics if the internal caller has no active transaction.
    fn fail(
        &mut self,
        reason: Failure,
    ) -> Event {
        let request = self.active.expect("failure without transaction").request;
        // Queued UART bytes may still be transmitting. Only a fresh qualified
        // boundary may allow another record after abandoning their completion.
        self.wire_fault |= self.pending_tx.is_some();
        self.clear();
        Event::Failed { request, reason }
    }

    /// Checks monotonic `now` in milliseconds. Returns Ignored or a terminal failure;
    /// deadlines are inclusive, checked UART then total then first-response then ISO-TP.
    /// Clock reversal returns ClientError and invalidates transport even while idle.
    pub fn tick(
        &mut self,
        now: u64,
    ) -> Result<Event, ClientError> {
        self.clock(now)?;
        let Some(active) = self.active else {
            return Ok(Event::Ignored);
        };
        if self.pending_tx.is_some_and(|tx| now > tx.deadline) {
            return Ok(self.fail(Failure::TxTimeout));
        }
        if now > active.hard_deadline {
            return Ok(self.fail(Failure::TotalTimeout));
        }
        if active.sent && !active.reply_started && now > active.response_deadline {
            return Ok(self.fail(Failure::ResponseTimeout));
        }
        if let Err(error) = self.rx.poll_timeout(now) {
            return Ok(self.fail(Failure::Transport(error)));
        }
        Ok(Event::Ignored)
    }

    /// Serializes the only outstanding UART write. Call after BUSY is clear;
    /// it does not assert CAN acknowledgement or physical L_Data.confirm.
    /// `token` must exactly match the outstanding ticket; `now` is monotonic milliseconds.
    /// Returns StaleTx before touching clock/state for old/duplicate tokens, otherwise
    /// checks deadlines and returns the resulting progress/failure event. Request completion
    /// starts response reception; CTS completion opens a block; overflow completion fails oversized.
    pub fn tx_completed(
        &mut self,
        token: TxToken,
        now: u64,
    ) -> Result<Event, ClientError> {
        let Some(tx) = self.pending_tx.filter(|tx| tx.token == token) else {
            return Err(ClientError::StaleTx);
        };
        let event = self.tick(now)?;
        if event != Event::Ignored {
            return Ok(event);
        }
        self.pending_tx = None;
        let active = self.active.as_mut().expect("TX without transaction");
        match tx.kind {
            TxKind::Request => {
                active.sent = true;
                active.response_deadline = now.saturating_add(RESPONSE_MS).min(active.hard_deadline);
                if let Err(error) = self.rx.begin(self.rx_id, self.epoch, now, active.hard_deadline) {
                    return Ok(self.fail(Failure::Transport(error)));
                }
            }
            TxKind::FlowControl => {
                if let Err(error) = self.rx.flow_control_sent(now) {
                    return Ok(self.fail(Failure::Transport(error)));
                }
            }
            TxKind::Overflow => return Ok(self.fail(Failure::Oversized)),
        }
        Ok(Event::Ignored)
    }

    /// Restarts first-response reassembly at checked `now` in milliseconds, retaining
    /// the active absolute deadline. Returns transport admission status; panics without active work.
    fn restart_response(
        &mut self,
        now: u64,
    ) -> Result<(), IsoError> {
        self.rx.abort();
        self.active.as_mut().unwrap().reply_started = false;
        self.rx
            .begin(self.rx_id, self.epoch, now, self.active.unwrap().hard_deadline)
    }

    /// Processes qualified `frame` from `epoch` at monotonic `now` in milliseconds.
    /// Returns progress, FC transmission or terminal reply/failure after deadline checking.
    /// Wrong responder/epoch and provably foreign service/identifier prefixes are ignored
    /// without renewing deadlines; malformed matching data fails. At most three NRC0x78
    /// replies extend the response window, never the total budget. Client errors report
    /// clock/identity exhaustion; pending UART ambiguity requires a new qualified epoch.
    pub fn receive(
        &mut self,
        frame: &CanFrame,
        epoch: u64,
        now: u64,
    ) -> Result<Event, ClientError> {
        let event = self.tick(now)?;
        if event != Event::Ignored {
            return Ok(event);
        }
        let Some(active) = self.active else {
            return Ok(Event::Ignored);
        };
        if !active.sent || frame.id() != self.rx_id || epoch != self.epoch {
            return Ok(Event::Ignored);
        }
        let data = frame.data();
        // Proven foreign SF/FF replies must not affect an active reassembly,
        // its pending FC ticket, or any deadline. Invalid framing still reaches
        // the transport's fail-closed checks below.
        let prefix = match data.first().copied() {
            Some(length @ 1..=7) if data.len() > usize::from(length) => Some(&data[1..=usize::from(length)]),
            Some(pci) if pci >> 4 == 1 && data.len() == 8 => Some(&data[2..]),
            _ => None,
        };
        if let Some(prefix) = prefix {
            match active.request.reply(prefix) {
                Err(ReplyError::Service | ReplyError::Identifier) => return Ok(Event::Ignored),
                Err(error) => return Ok(self.fail(Failure::Reply(error))),
                _ => {}
            }
        }
        match self.rx.on_frame(frame, epoch, now) {
            Err(error) => Ok(self.fail(Failure::Transport(error))),
            Ok(RxEvent::Ignored) => Ok(Event::Ignored),
            Ok(RxEvent::Pending) => Ok(Event::Pending),
            Ok(RxEvent::FlowControl(bytes)) => {
                self.active.as_mut().unwrap().reply_started = true;
                let frame = CanFrame::new(self.tx_id, &bytes).unwrap();
                self.queue(frame, active.generation, TxKind::FlowControl, now)
                    .map(Event::Transmit)
            }
            Ok(RxEvent::Overflow(bytes)) => {
                self.active.as_mut().unwrap().reply_started = true;
                let frame = CanFrame::new(self.tx_id, &bytes).unwrap();
                self.queue(frame, active.generation, TxKind::Overflow, now)
                    .map(Event::Transmit)
            }
            Ok(RxEvent::Complete(payload)) => {
                match active.request.reply(payload.as_slice()) {
                    Err(ReplyError::Service | ReplyError::Identifier) => {
                        if let Err(error) = self.restart_response(now) {
                            return Ok(self.fail(Failure::Transport(error)));
                        }
                        Ok(Event::Ignored)
                    }
                    Err(error) => Ok(self.fail(Failure::Reply(error))),
                    Ok(Reply::Pending) => {
                        if active.pending_count >= MAX_PENDING {
                            return Ok(self.fail(Failure::PendingLimit));
                        }
                        let current = self.active.as_mut().unwrap();
                        current.pending_count += 1;
                        current.response_deadline = now.saturating_add(PENDING_MS).min(current.hard_deadline);
                        if let Err(error) = self.restart_response(now) {
                            return Ok(self.fail(Failure::Transport(error)));
                        }
                        Ok(Event::Pending)
                    }
                    Ok(Reply::Negative(nrc)) => {
                        self.clear();
                        Ok(Event::Negative {
                            request: active.request,
                            nrc,
                        })
                    }
                    Ok(Reply::Positive(bytes)) => {
                        let response = Response {
                            request: active.request,
                            generation: active.generation,
                            epoch: self.epoch,
                            ecu: self.rx_id,
                            received_at_ms: now,
                            bytes: Payload::from_slice(bytes).unwrap(),
                        };
                        self.clear();
                        Ok(Event::Positive(response))
                    }
                }
            }
        }
    }
}
