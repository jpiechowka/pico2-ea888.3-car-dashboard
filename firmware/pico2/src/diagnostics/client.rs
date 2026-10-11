//! Single-owner read transaction state. TX tickets include request generation
//! and a unique serial, so old or duplicate UART completions cannot authorize RX.
use super::CanFrame;
use super::isotp::{IsoError, IsoTpRx, Payload, RxEvent};
use super::request::{Reply, ReplyError, Request};

// Explicit application budgets, pending physical ECU/bridge characterization.
pub const RESPONSE_MS: u64 = 500;
pub const PENDING_MS: u64 = 1_000;
pub const TOTAL_MS: u64 = 5_000;
pub const TX_MS: u64 = 100;
pub const MAX_PENDING: u8 = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClientError {
    Address,
    Epoch,
    Clock,
    Busy,
    StaleTx,
    Exhausted,
    WireFault,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Failure {
    TxTimeout,
    ResponseTimeout,
    TotalTimeout,
    PendingLimit,
    Oversized,
    Transport(IsoError),
    Reply(ReplyError),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TxToken {
    generation: u64,
    serial: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TxAction {
    pub token: TxToken,
    pub frame: CanFrame,
}
#[derive(Debug, PartialEq, Eq)]
pub struct Response {
    pub request: Request,
    pub generation: u64,
    pub epoch: u64,
    pub ecu: u16,
    pub received_at_ms: u64,
    pub bytes: Payload,
}
#[derive(Debug, PartialEq, Eq)]
#[allow(clippy::large_enum_variant)] // fixed response storage, no allocator
pub enum Event {
    Ignored,
    Pending,
    Transmit(TxAction),
    Positive(Response),
    Negative { request: Request, nrc: u8 },
    Failed { request: Request, reason: Failure },
}
#[derive(Clone, Copy)]
enum TxKind {
    Request,
    FlowControl,
    Overflow,
}
#[derive(Clone, Copy)]
struct PendingTx {
    token: TxToken,
    kind: TxKind,
    deadline: u64,
}
#[derive(Clone, Copy)]
struct Transaction {
    request: Request,
    generation: u64,
    hard_deadline: u64,
    response_deadline: u64,
    sent: bool,
    reply_started: bool,
    pending_count: u8,
}

pub struct Client {
    tx_id: u16,
    rx_id: u16,
    epoch: u64,
    generation: u64,
    tx_serial: u64,
    last_ms: u64,
    wire_fault: bool,
    rx: IsoTpRx,
    active: Option<Transaction>,
    pending_tx: Option<PendingTx>,
}
impl Client {
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

    pub fn active(&self) -> bool { self.active.is_some() }

    pub const fn epoch(&self) -> u64 { self.epoch }

    pub const fn response_id(&self) -> u16 { self.rx_id }

    fn clear(&mut self) {
        self.active = None;
        self.pending_tx = None;
        self.rx.abort();
    }

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

    fn restart_response(
        &mut self,
        now: u64,
    ) -> Result<(), IsoError> {
        self.rx.abort();
        self.active.as_mut().unwrap().reply_started = false;
        self.rx
            .begin(self.rx_id, self.epoch, now, self.active.unwrap().hard_deadline)
    }

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
