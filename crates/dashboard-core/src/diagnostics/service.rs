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

pub const RX_WORK_BYTES: usize = 32;
pub const RAW_TRACE_RECORDS: usize = 16;
/// Host policy, not a bridge guarantee. Incomplete records never get rephased.
pub const PARTIAL_RECORD_MS: u64 = 100;
const TX_WORK_TRANSITIONS: usize = 4;
const STABLE_MS: u64 = 10_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IoError {
    Overrun,
    Framing,
    Parity,
    Break,
    Other,
}

/// Polls must be bounded. Pending read/write must have no copied-byte effects;
/// the reviewed buffered HAL meets this, generic async IO traits do not promise
/// it. Only this owner writes the port or calls discard. No raw FIFO reads.
pub trait Port {
    fn poll_read(
        &mut self,
        cx: &mut Context<'_>,
        bytes: &mut [u8],
    ) -> Poll<Result<usize, IoError>>;
    fn poll_write(
        &mut self,
        cx: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<Result<usize, IoError>>;
    fn poll_flush(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), IoError>>;
    fn busy(&self) -> bool;
    fn rx_error(&self) -> Option<IoError>;
    fn discard_rx(&mut self) -> Result<(), IoError>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReceiveContract {
    /// Default. Observe records, but never assume their missing DLC/flags.
    Unqualified,
    /// Explicit external qualification: the selected responder's frames are
    /// padded DLC8 standard data, with exact filters and no remote frames.
    /// Selecting this does not perform or prove that hardware qualification.
    PaddedStandardDataDlc8,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Profile {
    pub responder: u16,
    pub receive: ReceiveContract,
}
impl Profile {
    pub const UNQUALIFIED: Self = Self {
        responder: 0x7E8,
        receive: ReceiveContract::Unqualified,
    };
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkState {
    Recovering,
    Quiet,
    Handshake,
    Unqualified,
    Polling,
    Quarantined,
    Maintenance,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PauseError {
    State,
    Clock,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fault {
    Clock,
    Rx(IoError),
    Tx(IoError),
    TxProgress(serial_tx::Error),
    ReadCount,
    PartialRecordTimeout,
    Bootstrap(bootstrap::Error),
    Record(BridgeError),
    Session(SessionError),
    UnexpectedResponder,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LinkSnapshot {
    pub state: LinkState,
    pub last_fault: Option<Fault>,
    pub faults: u32,
    pub handshakes_started: u32,
    pub raw_records: u32,
    pub raw_overwrites: u32,
    pub diagnostics: Counters,
}
#[derive(Clone, Copy, Debug)]
pub struct RawObservation {
    pub received_at_ms: u64,
    pub record: RawRecord,
}
#[derive(Clone, Copy)]
enum Token {
    Bootstrap(bootstrap::Token),
    Diagnostic(client::TxToken),
}
#[derive(Clone, Copy)]
enum OwnerState {
    Recover { since: u64, wait: u64 },
    Running,
    Quarantined,
    Draining { since: u64 },
    Parked,
}

pub struct Vehicle<'a, P> {
    port: P,
    profile: Profile,
    bootstrap: Bootstrap,
    rx: Rx12,
    tx: Pump<Token>,
    session: Session<'a>,
    owner: OwnerState,
    raw: Deque<RawObservation, RAW_TRACE_RECORDS>,
    raw_records: u32,
    raw_overwrites: u32,
    handshakes_started: u32,
    faults: u32,
    last_fault: Option<Fault>,
    failures: u8,
    online_since: Option<u64>,
    rx_partial_since: Option<u64>,
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
    port: P,
    profile: Profile,
    now: u64,
    rx: Rx12,
    session: PreparedSession,
    config: Option<&'a dashboard_config::ValidatedConfig>,
}
impl<'a, P: Port> PreparedVehicle<'a, P> {
    /// Consume the validated inputs and return the whole owner into its caller's
    /// return destination. Linked caller-plus-callee frames still gate admission.
    #[inline(never)]
    pub fn assemble(self) -> Vehicle<'a, P> {
        if self.config.is_some() {
            self.assemble_variant::<true>()
        } else {
            self.assemble_variant::<false>()
        }
    }

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
    pub fn new(
        port: P,
        profile: Profile,
        now: u64,
    ) -> Result<Self, SessionError> {
        Ok(Self::prepare_inner(port, profile, now, None)?.assemble())
    }

    pub fn configured(
        port: P,
        profile: Profile,
        now: u64,
        config: &'a dashboard_config::ValidatedConfig,
    ) -> Result<Self, SessionError> {
        Ok(Self::prepare_configured(port, profile, now, config)?.assemble())
    }

    pub fn prepare_configured(
        port: P,
        profile: Profile,
        now: u64,
        config: &'a dashboard_config::ValidatedConfig,
    ) -> Result<PreparedVehicle<'a, P>, SessionError> {
        Self::prepare_inner(port, profile, now, Some(config))
    }

    pub fn configuration(&self) -> Option<&dashboard_config::ValidatedConfig> { self.session.configuration() }

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

    pub fn snapshot(&self) -> DashboardSnapshot { self.session.snapshot() }

    pub fn telemetry(&self) -> super::session::Telemetry { self.session.telemetry() }

    pub fn apply_warning_command(
        &mut self,
        command: crate::warning_state::WarningCommand,
        now: u64,
    ) -> crate::warning_state::CommandResult {
        self.session.apply_warning_command(command, now)
    }

    pub fn reset_statistics(
        &mut self,
        generation: u64,
    ) -> bool {
        self.session.reset_statistics(generation)
    }

    pub fn raw(&self) -> impl Iterator<Item = &RawObservation> { self.raw.iter() }

    /// Stop admission, but finish the entire current command including bytes
    /// not yet copied into the UART. `fail`/Pump::abort cannot serve this role.
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

    pub fn paused(&self) -> bool { matches!(self.owner, OwnerState::Parked) }

    /// A park can only end after wire idle. Fresh discard and bootstrap are
    /// performed by subsequent service passes; terminal quarantine survives.
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
    pub fn step(
        &mut self,
        cx: &mut Context<'_>,
        now: u64,
    ) {
        self.step_with_warning(cx, now, None);
    }

    /// One coherent producer iteration: samples/faults, command, timed expiry.
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
mod tests {
    use super::*;
    struct UnusedPort;
    impl Port for UnusedPort {
        fn poll_read(
            &mut self,
            _: &mut Context<'_>,
            _: &mut [u8],
        ) -> Poll<Result<usize, IoError>> {
            unreachable!()
        }

        fn poll_write(
            &mut self,
            _: &mut Context<'_>,
            _: &[u8],
        ) -> Poll<Result<usize, IoError>> {
            unreachable!()
        }

        fn poll_flush(
            &mut self,
            _: &mut Context<'_>,
        ) -> Poll<Result<(), IoError>> {
            unreachable!()
        }

        fn busy(&self) -> bool { unreachable!() }

        fn rx_error(&self) -> Option<IoError> { unreachable!() }

        fn discard_rx(&mut self) -> Result<(), IoError> { unreachable!() }
    }

    #[test]
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
