//! One outstanding command, checked epochs and immutable transaction deadlines.
use heapless::Vec;

use super::elm::{self, Line, Payload};
use crate::diagnostics::request::{Reply, Request};

/// Maximum accepted bytes in one ingress notification.
pub const MAX_NOTIFICATION: usize = 244;
/// Maximum total notification bytes consumed by one transaction, including whitespace.
pub const MAX_TRANSACTION_BYTES: usize = 1024;
/// Maximum caller-selected transaction budget in milliseconds; pending replies never extend it.
pub const MAX_DEADLINE_MS: u64 = 5_000;

/// Explicit traffic authority; real adapter sessions never issue ECU reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Initialize and identify an adapter only; reject every diagnostic read command.
    AdapterOnly,
    /// Separate synthetic test service permits bounded fixture reads after initialization.
    SyntheticPeer,
}

/// One ordered adapter configuration command; successful OK/prompt advances exactly one step.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Init {
    /// ATE0: disable command echo, while the parser tolerates one initial exact echo.
    EchoOff,
    /// ATL0: disable generated linefeeds.
    LinefeedsOff,
    /// ATS1: retain spaces required by the raw CAN grammar.
    SpacesOn,
    /// ATH1: include CAN identifier headers.
    HeadersOn,
    /// ATD1: include explicit DLC digits.
    DlcOn,
    /// ATM0: disable automatic protocol persistence in the adapter.
    MemoryOff,
    /// ATSP6: select 11-bit CAN at 500 kbit/s.
    Can11Bit500K,
    /// ATCEA: restore normal CAN addressing.
    NormalAddressing,
    /// ATCAF0: disable adapter ISO-TP formatting, retaining raw CAN/PCI bytes.
    RawCan,
    /// ATCFC1: let the adapter provide ISO-TP flow control.
    AutomaticFlowControl,
    /// ATSH7E0: select the engine request CAN identifier for synthetic fixture reads.
    EngineHeader,
    /// ATCRA7E8: filter raw CAN replies to the expected engine response identifier.
    EngineFilter,
}
/// Mandatory initialization order before any synthetic read; each command needs an OK and prompt.
pub const INITIALIZATION: [Init; 12] = [
    Init::EchoOff,
    Init::LinefeedsOff,
    Init::SpacesOn,
    Init::HeadersOn,
    Init::DlcOn,
    Init::MemoryOff,
    Init::Can11Bit500K,
    Init::NormalAddressing,
    Init::RawCan,
    Init::AutomaticFlowControl,
    Init::EngineHeader,
    Init::EngineFilter,
];

/// Privately constructed synthetic read contract; carries no real-vehicle qualification.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QualifiedRead {
    /// Read-only service and echoed PID/DID expected in the response.
    request: Request,
    /// Exact positive-response data width after service/identifier prefix.
    width: u8,
}
impl QualifiedRead {
    /// Qualification is for a synthetic test peer only, never a vehicle.
    /// Returns the fixture contract for `pid`: supported-page 00/20/40 use four bytes,
    /// temperature 05/0F/5C one, module voltage 42 two; other PIDs return Unqualified.
    pub fn synthetic_obd(pid: u8) -> Result<Self, Error> {
        let width = match pid {
            0 | 0x20 | 0x40 => 4,
            5 | 0x0F | 0x5C => 1,
            0x42 => 2,
            _ => return Err(Error::Unqualified),
        };
        Ok(Self {
            request: Request::Obd(pid),
            width,
        })
    }

    /// Echo/width fixture. No DID conversion or vehicle source is supplied.
    /// Returns a synthetic read of `did` requiring `width` data bytes in 1..=61;
    /// zero/oversized width returns Length. DID bytes are echoed, not semantically qualified.
    pub fn synthetic_did(
        did: u16,
        width: u8,
    ) -> Result<Self, Error> {
        if width == 0 || usize::from(width) > elm::PAYLOAD_CAPACITY - 3 {
            return Err(Error::Length);
        }
        Ok(Self {
            request: Request::ReadDid(did),
            width,
        })
    }

    /// Returns `self`'s read-only service/identifier request without changing its authority.
    pub const fn request(self) -> Request { self.request }
}

/// One transaction type admitted by mode and initialization state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    /// Perform the next ordered initialization command.
    Initialize(
        /// Required current initialization step.
        Init,
    ),
    /// Request ATI adapter identity; allowed independently of initialization completion.
    Identity,
    /// Execute a bounded read only in fully initialized synthetic-peer mode.
    Read(
        /// Synthetic request and exact positive data width.
        QualifiedRead,
    ),
}
/// Exact write acknowledgment/completion identity, nonreused across connection epochs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Token {
    /// Strictly increasing connection epoch supplied by the owner.
    epoch: u64,
    /// Checked transaction serial, retained across reconnects and never wrapped.
    serial: u64,
}
/// Owned command bytes and identity returned to the GATT writer.
#[derive(Debug)]
pub struct Transmission {
    /// Token that must be returned once the complete GATT write finishes.
    pub token: Token,
    /// ASCII command with final CR, at most 32 bytes; diagnostic requests include raw PCI/padding.
    pub bytes: Vec<u8, 32>,
}
/// Recognized adapter text error, distinct from a valid diagnostic response.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdapterError {
    /// Adapter returned `?` for an unrecognized command.
    UnknownCommand,
    /// Adapter returned STOPPED.
    Stopped,
    /// Adapter returned CAN ERROR.
    Can,
    /// Adapter returned BUS ERROR or BUS INIT: ERROR.
    Bus,
    /// Adapter returned BUFFER FULL.
    BufferFull,
    /// Adapter returned UNABLE TO CONNECT.
    UnableToConnect,
}
/// Prompt-completed response; negative/pending/no-data/error outcomes are never samples.
#[derive(Debug, PartialEq, Eq)]
pub enum Response {
    /// Initialization returned exact OK.
    Ok,
    /// Identity line matched an allowed adapter prefix; does not prove genuine hardware.
    Identity(
        /// Original bounded printable identity line.
        Line,
    ),
    /// Synthetic read echoed exact service/identifier and expected data width.
    Diagnostic(
        /// Full diagnostic response including service/identifier prefix.
        Payload,
    ),
    /// Diagnostic negative response with service echo and exact framing.
    Negative(
        /// Raw negative response code other than pending 0x78.
        u8,
    ),
    /// Prompt arrived after response-pending, invalidating the session rather than extending budget.
    Pending,
    /// Adapter returned NO DATA; does not publish a numeric zero.
    NoData,
    /// Adapter reported a recognized failure line.
    AdapterError(
        /// Specific adapter error classification.
        AdapterError,
    ),
}
/// One response finalized by a prompt, with immutable request identity and actual receipt times.
#[derive(Debug, PartialEq, Eq)]
pub struct Completion {
    /// Exact transaction token originally issued for its command bytes.
    pub token: Token,
    /// Command that produced this response.
    pub command: Command,
    /// Authority mode under which the transaction was admitted.
    pub mode: Mode,
    /// Parsed response; only synthetic Diagnostic is a successful diagnostic fixture reply.
    pub response: Response,
    /// Boot-relative admission time in milliseconds.
    pub started_ms: u64,
    /// Boot-relative milliseconds of notification containing the final prompt.
    pub completed_ms: u64,
}
/// Session admission/protocol error; clock/exhaustion are terminal for the instance.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// No current connected epoch is valid.
    Disconnected,
    /// Connection epoch is stale/nonincreasing or notification belongs to another epoch.
    Epoch,
    /// Another command is already outstanding.
    Busy,
    /// Real-adapter mode or unsupported synthetic PID prohibits a read.
    Unqualified,
    /// Initialization step is out of order or synthetic reads began too early.
    Initialization,
    /// TX acknowledgment token is stale, absent or already acknowledged.
    Token,
    /// Ingress notification counter skipped/repeated a value; queued data cannot be trusted.
    NotificationLoss,
    /// Budget, expected width or raw diagnostic framing length is invalid.
    Length,
    /// Notification/transaction/payload work exceeded its bounded capacity.
    Overflow,
    /// Monotonic clock regressed; instance cannot reconnect afterward.
    Clock,
    /// Immutable transaction deadline reached or passed.
    Timeout,
    /// Epoch-independent serial, notification sequence or deadline arithmetic exhausted.
    Exhausted,
    /// Unexpected echo/status/prompt ordering or notification before TX acknowledgment.
    Unexpected,
    /// ASCII/CAN/ISO-TP parser rejected the reply.
    Parse(
        /// Specific bounded parser failure.
        elm::Error,
    ),
    /// Service/identifier/negative-response shape disagreed, or pending limit was exceeded.
    Reply,
}

/// Single active transaction context; only one mutable owner can advance its receive state.
struct Active {
    /// Admitted command and its synthetic read width if applicable.
    command: Command,
    /// Epoch/serial identity required by TX acknowledgment.
    token: Token,
    /// Boot-relative start time in milliseconds.
    started_ms: u64,
    /// Exclusive absolute deadline in milliseconds, never extended by received data.
    deadline_ms: u64,
    /// Complete GATT write has been acknowledged locally.
    sent: bool,
    /// The optional exact command echo has already been consumed.
    echo: bool,
    /// Total notification bytes seen for this command, including delimiters/padding.
    bytes_seen: usize,
    /// Number of response-pending messages received, capped at three.
    pending_count: u8,
}

/// Bounded single-flight ASCII session; transport owner supplies fresh epochs and ingress counts.
pub struct Session {
    /// Immutable adapter-only versus separate synthetic-peer authority.
    mode: Mode,
    /// Last connected epoch, retained across invalidation.
    epoch: u64,
    /// Last issued TX serial, retained across connections and checked for exhaustion.
    serial: u64,
    /// Last accepted notification ingress sequence in the current epoch.
    sequence: u64,
    /// Latest accepted boot-relative observation time in milliseconds.
    last_ms: u64,
    /// Current epoch is usable; invalidation clears this before any reconnect.
    connected: bool,
    /// Irrecoverable clock/exhaustion error preventing reconnect on this instance.
    terminal: Option<Error>,
    /// Count of successfully prompted initialization commands in the fixed sequence.
    initialized: usize,
    /// Outstanding command context, absent outside a transaction.
    active: Option<Active>,
    /// Incremental ASCII framing for the current transaction.
    parser: elm::Parser,
    /// Parsed response awaiting prompt; Pending may be replaced by a later final response.
    response: Option<Response>,
    /// Current raw ISO-TP payload assembly, bounded to 64 bytes.
    payload: Payload,
    /// Expected ISO-TP payload length from SF/FF, reset on new/pending reply.
    total: usize,
    /// Next expected consecutive-frame sequence nibble, wrapping modulo 16.
    next_frame: u8,
}
impl Session {
    /// Returns a disconnected session with immutable authority `mode`, zero identities
    /// and no parser history. Owner must establish a fresh transport before connecting it.
    pub const fn new(mode: Mode) -> Self {
        Self {
            mode,
            epoch: 0,
            serial: 0,
            sequence: 0,
            last_ms: 0,
            connected: false,
            terminal: None,
            initialized: 0,
            active: None,
            parser: elm::Parser::new(),
            response: None,
            payload: Payload::new(),
            total: 0,
            next_frame: 1,
        }
    }

    /// The owner must reconnect, discard old RX/TX work and establish a fresh
    /// notification stream before calling this. A larger number alone is no flush.
    /// Returns unit after activating strictly newer nonzero `epoch` at boot-relative
    /// `now_ms`, clearing transaction/initialization state and resetting notification count.
    /// Stale epoch returns Epoch without mutation; backward time invalidates and latches
    /// Clock. Existing terminal clock/exhaustion failures reject all reconnect attempts.
    pub fn connect(
        &mut self,
        epoch: u64,
        now_ms: u64,
    ) -> Result<(), Error> {
        if let Some(error) = self.terminal {
            return Err(error);
        }
        if epoch <= self.epoch {
            return Err(Error::Epoch);
        }
        if now_ms < self.last_ms {
            return self.fail(Error::Clock);
        }
        self.invalidate();
        self.epoch = epoch;
        self.sequence = 0;
        self.last_ms = now_ms;
        self.connected = true;
        Ok(())
    }

    /// Admits `command` in `self` at boot-relative `now_ms` with immutable `budget_ms`
    /// in 1..=5000, returning owned ASCII bytes and a checked token. Allows one active
    /// command, enforces initialization order and confines all reads to synthetic mode.
    /// Admission errors retain current work except temporal/exhaustion failures, which
    /// invalidate the session. A successful begin resets parsing but does not acknowledge TX.
    pub fn begin(
        &mut self,
        command: Command,
        now_ms: u64,
        budget_ms: u64,
    ) -> Result<Transmission, Error> {
        self.tick(now_ms)?;
        if self.active.is_some() {
            return Err(Error::Busy);
        }
        if self.sequence == u64::MAX {
            return self.fail(Error::Exhausted);
        }
        if let Command::Read(_) = command {
            if self.mode != Mode::SyntheticPeer {
                return Err(Error::Unqualified);
            }
            if self.initialized != INITIALIZATION.len() {
                return Err(Error::Initialization);
            }
        }
        if let Command::Initialize(init) = command
            && INITIALIZATION.get(self.initialized) != Some(&init)
        {
            return Err(Error::Initialization);
        }
        if budget_ms == 0 || budget_ms > MAX_DEADLINE_MS {
            return Err(Error::Length);
        }
        let Some(deadline_ms) = now_ms.checked_add(budget_ms) else {
            return self.fail(Error::Exhausted);
        };
        let Some(serial) = self.serial.checked_add(1) else {
            return self.fail(Error::Exhausted);
        };
        self.serial = serial;
        let token = Token {
            epoch: self.epoch,
            serial,
        };
        let bytes = encode(command);
        self.parser = elm::Parser::new();
        self.response = None;
        self.payload.clear();
        self.total = 0;
        self.next_frame = 1;
        self.active = Some(Active {
            command,
            token,
            started_ms: now_ms,
            deadline_ms,
            sent: false,
            echo: false,
            bytes_seen: 0,
            pending_count: 0,
        });
        Ok(Transmission { token, bytes })
    }

    /// Acknowledges the complete GATT write, never ECU receipt. The owner must
    /// retain/queue early notifications until this callback, or invalidate.
    /// Returns unit after marking exact active unsent `token` sent at boot-relative
    /// `now_ms`; wrong/already-sent tokens return Token before advancing the clock.
    /// Temporal failure invalidates the session. This is not proof of ECU receipt.
    pub fn sent(
        &mut self,
        token: Token,
        now_ms: u64,
    ) -> Result<(), Error> {
        if !self.active.as_ref().is_some_and(|a| a.token == token && !a.sent) {
            return Err(Error::Token);
        }
        self.tick(now_ms)?;
        self.active.as_mut().unwrap().sent = true;
        Ok(())
    }

    /// Sequence counts notifications at ingress, before a bounded queue can
    /// drop them. The first notification of each connection is sequence one.
    /// Consumes `bytes` for exact `epoch` and next ingress `sequence` at boot-relative
    /// `now_ms`. Returns `None` while accumulating, or one Completion only after the prompt.
    /// Old epochs return Epoch without mutation. Lost sequence, early TX, foreign/malformed
    /// reply, non-CR/LF post-prompt data, or bounds/deadline errors invalidate the session.
    /// Empty notifications still consume sequence. Idle CR/LF is accepted; failed reply
    /// outcomes return a completion but disconnect logically and reset initialization.
    /// No partial positive payload escapes; caller must invalidate on cancelled/partial TX.
    pub fn notify(
        &mut self,
        epoch: u64,
        sequence: u64,
        bytes: &[u8],
        now_ms: u64,
    ) -> Result<Option<Completion>, Error> {
        // Old callbacks cannot advance this session's clock or cancel its work.
        if epoch != self.epoch {
            return Err(Error::Epoch);
        }
        self.tick(now_ms)?;
        let Some(next) = self.sequence.checked_add(1) else {
            return self.fail(Error::Exhausted);
        };
        if sequence != next {
            return self.fail(Error::NotificationLoss);
        }
        self.sequence = sequence;
        if bytes.len() > MAX_NOTIFICATION {
            return self.fail(Error::Overflow);
        }
        if self.active.is_none() {
            if bytes.iter().all(|b| matches!(*b, b'\r' | b'\n')) {
                return Ok(None);
            }
            return self.fail(Error::Unexpected);
        }
        let active = self.active.as_mut().unwrap();
        if !active.sent {
            return self.fail(Error::Unexpected);
        }
        active.bytes_seen += bytes.len();
        if active.bytes_seen > MAX_TRANSACTION_BYTES {
            return self.fail(Error::Overflow);
        }
        let mut prompt = false;
        for byte in bytes {
            if prompt {
                if !matches!(*byte, b'\r' | b'\n') {
                    return self.fail(Error::Unexpected);
                }
                continue;
            }
            let event = match self.parser.push(*byte) {
                Ok(event) => event,
                Err(error) => return self.fail(Error::Parse(error)),
            };
            match event {
                elm::Event::None => {}
                elm::Event::Line(line) => {
                    if let Err(error) = self.line(&line) {
                        return self.fail(error);
                    }
                }
                elm::Event::Prompt => prompt = true,
            }
        }
        if !prompt {
            return Ok(None);
        }
        let Some(response) = self.response.take() else {
            return self.fail(Error::Parse(elm::Error::Incomplete));
        };
        let active = self.active.take().unwrap();
        if matches!(response, Response::Ok) && matches!(active.command, Command::Initialize(_)) {
            self.initialized += 1;
        }
        if !matches!(response, Response::Ok | Response::Identity(_) | Response::Diagnostic(_)) {
            self.invalidate();
        }
        Ok(Some(Completion {
            token: active.token,
            command: active.command,
            mode: self.mode,
            response,
            started_ms: active.started_ms,
            completed_ms: now_ms,
        }))
    }

    /// Advances `self`'s boot-relative `now_ms`, returning unit while connected and before
    /// the active deadline. Disconnected returns Disconnected; reversed time or deadline
    /// equality invalidates the session. Clock is terminal, timeout permits a fresh epoch.
    pub fn tick(
        &mut self,
        now_ms: u64,
    ) -> Result<(), Error> {
        if !self.connected {
            return Err(Error::Disconnected);
        }
        if now_ms < self.last_ms {
            return self.fail(Error::Clock);
        }
        self.last_ms = now_ms;
        if self.active.as_ref().is_some_and(|a| now_ms >= a.deadline_ms) {
            return self.fail(Error::Timeout);
        }
        Ok(())
    }

    /// Disconnect, flash park, notification loss, partial TX or cancelled work.
    /// Resume requires a strictly newer connection epoch and initialization.
    /// Clears `self`'s connection, active work, initialization and parser/response payload,
    /// returning unit. Retains epoch/serial/clock and terminal fault so stale work cannot
    /// regain authority; owner must still disconnect/drain the physical transport separately.
    pub fn invalidate(&mut self) {
        self.connected = false;
        self.initialized = 0;
        self.active = None;
        self.response = None;
        self.payload.clear();
        self.parser = elm::Parser::new();
    }

    /// Returns whether `self` currently admits work in a valid logical connection epoch.
    pub const fn connected(&self) -> bool { self.connected }

    /// Returns whether `self` has one outstanding command awaiting prompt completion.
    pub fn busy(&self) -> bool { self.active.is_some() }

    /// Returns `self`'s next ordered initialization step, or `None` after all twelve succeed.
    /// This reports progress even when disconnected; it does not authorize transmission.
    pub fn next_initialization(&self) -> Option<Init> { INITIALIZATION.get(self.initialized).copied() }

    /// Invalidates `self` and returns the supplied `error` for any expected output type;
    /// Clock/Exhausted also latch terminal state that blocks all later connect calls.
    fn fail<T>(
        &mut self,
        error: Error,
    ) -> Result<T, Error> {
        if matches!(error, Error::Exhausted | Error::Clock) {
            self.terminal = Some(error);
        }
        self.invalidate();
        Err(error)
    }

    /// Consumes complete ASCII `line` for `self`'s active command, returning unit after
    /// optional exact echo, status, identity or raw CAN admission. Rejects duplicate/
    /// reordered responses; Pending clears assembly for a possible final reply.
    /// Errors may partially update receive state and must be followed by invalidation.
    /// Panics if called without active work; `notify` establishes that precondition.
    fn line(
        &mut self,
        line: &[u8],
    ) -> Result<(), Error> {
        let active = self.active.as_mut().unwrap();
        let encoded = encode(active.command);
        if line == &encoded[..encoded.len() - 1] {
            if active.echo || self.response.is_some() || !self.payload.is_empty() {
                return Err(Error::Unexpected);
            }
            active.echo = true;
            return Ok(());
        }
        if matches!(self.response, Some(Response::Pending)) {
            self.response = None;
            self.payload.clear();
            self.total = 0;
            self.next_frame = 1;
        } else if self.response.is_some() {
            return Err(Error::Unexpected);
        }
        let status = match line {
            b"NO DATA" => Some(Response::NoData),
            b"?" => Some(Response::AdapterError(AdapterError::UnknownCommand)),
            b"STOPPED" => Some(Response::AdapterError(AdapterError::Stopped)),
            b"CAN ERROR" => Some(Response::AdapterError(AdapterError::Can)),
            b"BUS ERROR" | b"BUS INIT: ERROR" => Some(Response::AdapterError(AdapterError::Bus)),
            b"BUFFER FULL" => Some(Response::AdapterError(AdapterError::BufferFull)),
            b"UNABLE TO CONNECT" => Some(Response::AdapterError(AdapterError::UnableToConnect)),
            _ => None,
        };
        if let Some(status) = status {
            if !self.payload.is_empty() {
                return Err(Error::Unexpected);
            }
            self.response = Some(status);
            return Ok(());
        }
        match active.command {
            Command::Initialize(_) if line == b"OK" => self.response = Some(Response::Ok),
            Command::Identity
                if line.starts_with(b"ELM327 ")
                    || line.starts_with(b"STN")
                    || line.starts_with(b"OBDLink ")
                    || line.starts_with(b"vLinker ") =>
            {
                self.response = Some(Response::Identity(Line::from_slice(line).map_err(|_| Error::Length)?));
            }
            Command::Read(read) => self.receive_frame(read, line)?,
            _ => return Err(Error::Unexpected),
        }
        Ok(())
    }

    /// Parses raw CAN `line` for synthetic `read` into `self`'s bounded ISO-TP assembly,
    /// returning unit after validating source 0x7E8, PCI/sequence, service/identifier echo
    /// and exact positive width. Allows at most three pending responses without extending
    /// deadline. Errors can leave partial assembly and require session invalidation.
    /// Active command must exist for pending accounting; caller guarantees this invariant.
    fn receive_frame(
        &mut self,
        read: QualifiedRead,
        line: &[u8],
    ) -> Result<(), Error> {
        let frame = elm::frame(line).map_err(Error::Parse)?;
        if frame.id() != 0x7E8 {
            return Err(Error::Parse(elm::Error::Foreign));
        }
        let data = frame.data();
        let Some(&pci) = data.first() else {
            return Err(Error::Length);
        };
        match pci >> 4 {
            0 => {
                let len = usize::from(pci & 15);
                if !self.payload.is_empty() || !(1..=7).contains(&len) || data.len() < len + 1 {
                    return Err(Error::Length);
                }
                self.total = len;
                self.payload
                    .extend_from_slice(&data[1..=len])
                    .map_err(|_| Error::Overflow)?;
            }
            1 => {
                if !self.payload.is_empty() || data.len() != 8 {
                    return Err(Error::Length);
                }
                self.total = (usize::from(pci & 15) << 8) | usize::from(data[1]);
                if !(8..=elm::PAYLOAD_CAPACITY).contains(&self.total) {
                    return Err(Error::Overflow);
                }
                self.payload
                    .extend_from_slice(&data[2..])
                    .map_err(|_| Error::Overflow)?;
            }
            2 => {
                if self.payload.is_empty() || self.payload.len() >= self.total || pci & 15 != self.next_frame {
                    return Err(Error::Parse(elm::Error::Sequence));
                }
                let len = (self.total - self.payload.len()).min(7);
                if data.len() < len + 1 {
                    return Err(Error::Length);
                }
                self.payload
                    .extend_from_slice(&data[1..=len])
                    .map_err(|_| Error::Overflow)?;
                self.next_frame = (self.next_frame + 1) & 15;
            }
            _ => return Err(Error::Parse(elm::Error::Frame)),
        }
        if self.payload.len() == self.total {
            self.response = Some(match read.request.reply(&self.payload).map_err(|_| Error::Reply)? {
                Reply::Positive(bytes) => {
                    if bytes.len() != usize::from(read.width) {
                        return Err(Error::Length);
                    }
                    Response::Diagnostic(core::mem::take(&mut self.payload))
                }
                Reply::Negative(code) => Response::Negative(code),
                Reply::Pending => {
                    let active = self.active.as_mut().unwrap();
                    active.pending_count += 1;
                    if active.pending_count > 3 {
                        return Err(Error::Reply);
                    }
                    Response::Pending
                }
            });
        }
        Ok(())
    }
}

/// Returns ASCII bytes for `command` with final CR; synthetic reads reuse read-only
/// request PCI/service/identifier encoding at 0x7E0 and output uppercase hex with padding.
/// Fixed initialization/identity strings and eight-byte CAN requests fit capacity 32;
/// internal unwraps rely on those compile-time command/identifier bounds.
fn encode(command: Command) -> Vec<u8, 32> {
    let mut bytes = Vec::new();
    let text: &[u8] = match command {
        Command::Identity => b"ATI",
        Command::Initialize(init) => {
            match init {
                Init::EchoOff => b"ATE0",
                Init::LinefeedsOff => b"ATL0",
                Init::SpacesOn => b"ATS1",
                Init::HeadersOn => b"ATH1",
                Init::DlcOn => b"ATD1",
                Init::MemoryOff => b"ATM0",
                Init::Can11Bit500K => b"ATSP6",
                Init::NormalAddressing => b"ATCEA",
                Init::RawCan => b"ATCAF0",
                Init::AutomaticFlowControl => b"ATCFC1",
                Init::EngineHeader => b"ATSH7E0",
                Init::EngineFilter => b"ATCRA7E8",
            }
        }
        Command::Read(read) => {
            // Reuse the established read-only PCI/service/identifier encoding.
            let frame = read.request.frame(0x7E0).unwrap();
            for byte in frame.data() {
                bytes.push(b"0123456789ABCDEF"[usize::from(byte >> 4)]).unwrap();
                bytes.push(b"0123456789ABCDEF"[usize::from(byte & 15)]).unwrap();
            }
            b""
        }
    };
    bytes.extend_from_slice(text).unwrap();
    bytes.push(b'\r').unwrap();
    bytes
}

#[cfg(test)]
/// Synthetic protocol/state regressions; never connect to an adapter or publish vehicle data.
mod tests {
    use super::*;

    /// Returns a synthetic session at epoch one after all twelve OK/prompt replies and
    /// its last ingress sequence; panics if the fixture initialization fails.
    fn initialized() -> (Session, u64) {
        let mut session = Session::new(Mode::SyntheticPeer);
        session.connect(1, 0).unwrap();
        let mut sequence = 0;
        for init in INITIALIZATION {
            let tx = session.begin(Command::Initialize(init), 0, 500).unwrap();
            session.sent(tx.token, 0).unwrap();
            sequence += 1;
            assert_eq!(
                session.notify(1, sequence, b"OK\r>", 0).unwrap().unwrap().response,
                Response::Ok
            );
        }
        (session, sequence)
    }
    /// Starts synthetic PID05 in initialized `session` at 1 ms and acknowledges TX at
    /// 2 ms; returns its transmission after asserting exact raw request bytes.
    /// Panics if fixture admission/encoding or acknowledgment fails.
    fn coolant(session: &mut Session) -> Transmission {
        let tx = session
            .begin(Command::Read(QualifiedRead::synthetic_obd(5).unwrap()), 1, 500)
            .unwrap();
        assert_eq!(tx.bytes.as_slice(), b"0201050000000000\r");
        session.sent(tx.token, 2).unwrap();
        tx
    }

    #[test]
    /// Returns unit after checking every notification split preserves final-prompt timing
    /// and exact PID/source decoding; panics on premature publication or changed payload.
    fn every_notification_split_requires_final_prompt_and_exact_source() {
        let reply = b"7E8 8 03 41 05 82 00 00 00 00\r\n>";
        for split in 1..reply.len() {
            let (mut session, seq) = initialized();
            coolant(&mut session);
            assert_eq!(session.notify(1, seq + 1, &reply[..split], 10).unwrap(), None);
            let done = session.notify(1, seq + 2, &reply[split..], 20).unwrap().unwrap();
            assert_eq!(done.completed_ms, 20);
            assert_eq!(done.started_ms, 1);
            let Response::Diagnostic(data) = done.response else {
                panic!("missing data")
            };
            assert_eq!(data.as_slice(), &[0x41, 5, 130]);
            assert_eq!(
                crate::diagnostics::obd::decode(5, &data[2..]).unwrap(),
                crate::diagnostics::obd::Decoded::Quantity(crate::samples::Quantity::CoolantC(90.0))
            );
        }
    }

    #[test]
    /// Returns unit after rejecting OBD/DID reads in adapter-only mode while admitting
    /// ATI with one echo; panics if a synthetic fixture gains real-adapter read authority.
    fn real_adapter_cannot_transmit_any_ecu_request() {
        let mut session = Session::new(Mode::AdapterOnly);
        session.connect(1, 0).unwrap();
        for request in [
            QualifiedRead::synthetic_obd(0).unwrap(),
            QualifiedRead::synthetic_did(0x11CD, 1).unwrap(),
        ] {
            assert_eq!(
                session.begin(Command::Read(request), 0, 500).unwrap_err(),
                Error::Unqualified
            );
        }
        let tx = session.begin(Command::Identity, 0, 500).unwrap();
        session.sent(tx.token, 0).unwrap();
        let done = session.notify(1, 1, b"ATI\rELM327 v2.3\r>", 10).unwrap().unwrap();
        assert!(matches!(done.response, Response::Identity(_)));
    }

    #[test]
    /// Returns unit after checking timeout/loss/disconnect/reversal invalidation, newer-epoch
    /// recovery and terminal clock failure; panics if stale callbacks mutate a new session.
    fn timeout_loss_disconnect_and_clock_regression_invalidate() {
        for failure in 0..4 {
            let (mut session, seq) = initialized();
            coolant(&mut session);
            match failure {
                0 => assert_eq!(session.tick(501), Err(Error::Timeout)),
                1 => assert_eq!(session.notify(1, seq + 2, b"OK\r>", 10), Err(Error::NotificationLoss)),
                2 => session.invalidate(),
                _ => assert_eq!(session.tick(1), Err(Error::Clock)),
            }
            assert!(!session.connected());
            assert!(session.notify(1, seq + 3, b"7E8 4 03 41 05 82\r>", 502).is_err());
            assert!(session.begin(Command::Identity, 503, 500).is_err());
            if failure == 3 {
                assert_eq!(session.connect(2, 504), Err(Error::Clock));
                continue;
            }
            assert_eq!(session.connect(1, 504), Err(Error::Epoch));
            session.connect(2, 504).unwrap();
            assert_eq!(session.notify(1, 1, b"bad", 0), Err(Error::Epoch));
            assert!(session.connected());
        }
    }

    #[test]
    /// Returns unit after rejecting wrong CAN/PID/width, unexpected echoes, post-prompt
    /// data and incomplete multiframes; panics if malformed evidence completes a sample.
    fn malformed_foreign_echo_and_post_prompt_bytes_never_publish() {
        for reply in [
            b"7E9 4 03 41 05 82\r>".as_slice(),
            b"7E8 4 03 41 0F 82\r>",
            b"7E8 5 04 41 05 82 01\r>",
            b"0105\r>",
            b"7E8 4 03 41 05 82\r>garbage",
            b"7E8 4 03 41 05 82\r>>",
            b"7E8 8 10 0A 62 12 34 01 02 03\r>",
        ] {
            let (mut session, seq) = initialized();
            coolant(&mut session);
            assert!(session.notify(1, seq + 1, reply, 10).is_err(), "{:?}", reply);
            assert!(!session.connected());
        }
    }

    #[test]
    /// Returns unit after classifying no-data, CAN-error, negative and pending outcomes
    /// while invalidating their sessions; panics if any becomes a successful sample.
    fn no_data_errors_and_negative_responses_are_not_samples() {
        for (reply, expected) in [
            (b"NO DATA\r>".as_slice(), Response::NoData),
            (b"CAN ERROR\r>", Response::AdapterError(AdapterError::Can)),
            (b"7E8 4 03 7F 01 22\r>", Response::Negative(0x22)),
            (b"7E8 4 03 7F 01 78\r>", Response::Pending),
        ] {
            let (mut session, seq) = initialized();
            coolant(&mut session);
            assert_eq!(
                session.notify(1, seq + 1, reply, 10).unwrap().unwrap().response,
                expected
            );
            assert!(!session.connected());
        }
    }

    #[test]
    /// Returns unit after reassembling a synthetic DID first/consecutive-frame pair
    /// with exact request and reply bytes; panics on PCI/padding or payload drift.
    fn raw_multiframe_reassembles_with_adapter_owned_flow_control() {
        let (mut session, seq) = initialized();
        let tx = session
            .begin(Command::Read(QualifiedRead::synthetic_did(0x1234, 7).unwrap()), 1, 500)
            .unwrap();
        assert_eq!(tx.bytes.as_slice(), b"0322123400000000\r");
        session.sent(tx.token, 2).unwrap();
        assert_eq!(
            session
                .notify(1, seq + 1, b"7E8 8 10 0A 62 12 34 01 02 03\r", 10)
                .unwrap(),
            None
        );
        let done = session
            .notify(1, seq + 2, b"7E8 8 21 04 05 06 07 00 00 00\r>", 20)
            .unwrap()
            .unwrap();
        let Response::Diagnostic(data) = done.response else {
            panic!("missing multiframe data")
        };
        assert_eq!(data.as_slice(), &[0x62, 0x12, 0x34, 1, 2, 3, 4, 5, 6, 7]);
    }

    #[test]
    /// Returns unit after accepting pending then final response before the original
    /// deadline and rejecting equality; panics if pending extends the transaction budget.
    fn pending_can_precede_final_reply_but_never_extend_total_budget() {
        let (mut session, seq) = initialized();
        coolant(&mut session);
        assert_eq!(session.notify(1, seq + 1, b"7E8 4 03 7F 01 78\r", 400).unwrap(), None);
        let done = session
            .notify(1, seq + 2, b"7E8 4 03 41 05 82\r>", 500)
            .unwrap()
            .unwrap();
        assert!(matches!(done.response, Response::Diagnostic(_)));
        let (mut session, seq) = initialized();
        coolant(&mut session);
        session.notify(1, seq + 1, b"7E8 4 03 7F 01 78\r", 400).unwrap();
        assert_eq!(
            session.notify(1, seq + 2, b"7E8 4 03 41 05 82\r>", 501),
            Err(Error::Timeout)
        );
    }

    #[test]
    /// Returns unit after checking stale TX tokens/notification epochs cannot advance
    /// a new transaction's clock or corrupt it; panics if its valid reply cannot complete.
    fn old_tx_tokens_and_notification_epochs_cannot_mutate_new_request() {
        let (mut session, seq) = initialized();
        let first = coolant(&mut session);
        assert_eq!(session.begin(Command::Identity, 3, 500).unwrap_err(), Error::Busy);
        session.notify(1, seq + 1, b"7E8 4 03 41 05 82\r>", 10).unwrap();
        let next = session.begin(Command::Identity, 11, 500).unwrap();
        assert_eq!(session.sent(first.token, 900), Err(Error::Token));
        assert_eq!(session.notify(2, 1, b"bad", 900), Err(Error::Epoch));
        session.sent(next.token, 12).unwrap();
        assert!(session.notify(1, seq + 2, b"ELM327 v2.3\r>", 13).unwrap().is_some());
    }

    #[test]
    /// Returns unit after forcing serial/deadline/sequence exhaustion and checking terminal
    /// rejection; panics if an identity or absolute deadline can wrap into valid work.
    fn checked_identity_and_time_exhaustion_remain_terminal() {
        let mut session = Session::new(Mode::AdapterOnly);
        session.connect(1, 0).unwrap();
        session.serial = u64::MAX;
        assert_eq!(session.begin(Command::Identity, 0, 1).unwrap_err(), Error::Exhausted);
        assert_eq!(session.connect(2, 1), Err(Error::Exhausted));
        let mut session = Session::new(Mode::AdapterOnly);
        session.connect(1, u64::MAX - 1).unwrap();
        assert_eq!(
            session.begin(Command::Identity, u64::MAX - 1, 2).unwrap_err(),
            Error::Exhausted
        );
        let mut session = Session::new(Mode::AdapterOnly);
        session.connect(1, 0).unwrap();
        session.sequence = u64::MAX;
        assert_eq!(session.notify(1, 0, b"", 0), Err(Error::Exhausted));
    }

    #[test]
    /// Returns unit after injecting an ingress sequence gap at every reply split;
    /// panics if any partial reply survives loss to become a completion.
    fn lost_notification_at_every_split_cannot_complete() {
        let reply = b"7E8 4 03 41 05 82\r>";
        for split in 0..reply.len() {
            let (mut session, seq) = initialized();
            coolant(&mut session);
            session.notify(1, seq + 1, &reply[..split], 3).unwrap();
            assert_eq!(
                session.notify(1, seq + 3, &reply[split..], 4),
                Err(Error::NotificationLoss)
            );
            assert!(!session.connected());
        }
    }

    #[test]
    /// Returns unit after checking line, notification and cumulative transaction caps,
    /// including whitespace-only input; panics if excess work leaves the session connected.
    fn bounded_notification_line_and_transaction_work() {
        for bytes in [&[b'A'; 65][..], &[b'\r'; MAX_NOTIFICATION + 1][..]] {
            let (mut session, seq) = initialized();
            coolant(&mut session);
            assert!(session.notify(1, seq + 1, bytes, 10).is_err());
            assert!(!session.connected());
        }
        let (mut session, seq) = initialized();
        coolant(&mut session);
        for i in 1..=4 {
            session.notify(1, seq + i, &[b'\r'; 244], 10).unwrap();
        }
        assert_eq!(session.notify(1, seq + 5, &[b'\r'; 49], 10), Err(Error::Overflow));
    }

    #[test]
    /// Returns unit after reassembling every synthetic positive width 1..=61 under
    /// seventeen deterministic fragmentations; panics on early prompt or payload mismatch.
    fn all_bounded_payload_lengths_and_seeded_fragmentations() {
        use core::fmt::Write;
        for width in 1..=61 {
            let mut payload = Vec::<u8, 64>::new();
            payload.extend_from_slice(&[0x62, 0x12, 0x34]).unwrap();
            for value in 0..width {
                payload.push(value).unwrap();
            }
            let mut wire = heapless::String::<512>::new();
            if payload.len() <= 7 {
                write!(wire, "7E8 8 {:02X}", payload.len()).unwrap();
                for byte in &payload {
                    write!(wire, " {byte:02X}").unwrap();
                }
                for _ in payload.len() + 1..8 {
                    write!(wire, " 00").unwrap();
                }
                write!(wire, "\r").unwrap();
            } else {
                write!(wire, "7E8 8 10 {:02X}", payload.len()).unwrap();
                for byte in &payload[..6] {
                    write!(wire, " {byte:02X}").unwrap();
                }
                write!(wire, "\r").unwrap();
                for (index, chunk) in payload[6..].chunks(7).enumerate() {
                    write!(wire, "7E8 8 {:02X}", 0x21 + index).unwrap();
                    for byte in chunk {
                        write!(wire, " {byte:02X}").unwrap();
                    }
                    for _ in chunk.len() + 1..8 {
                        write!(wire, " 00").unwrap();
                    }
                    write!(wire, "\r").unwrap();
                }
            }
            write!(wire, ">").unwrap();
            for seed in 1..=17_u32 {
                let (mut session, mut sequence) = initialized();
                let tx = session
                    .begin(
                        Command::Read(QualifiedRead::synthetic_did(0x1234, width).unwrap()),
                        1,
                        500,
                    )
                    .unwrap();
                session.sent(tx.token, 1).unwrap();
                let mut offset = 0;
                let mut random = seed;
                let mut complete = None;
                while offset < wire.len() {
                    random = random.wrapping_mul(1664525).wrapping_add(1013904223);
                    let end = (offset + (random as usize % 23) + 1).min(wire.len());
                    sequence += 1;
                    complete = session.notify(1, sequence, &wire.as_bytes()[offset..end], 2).unwrap();
                    if end != wire.len() {
                        assert_eq!(complete, None);
                    }
                    offset = end;
                }
                assert_eq!(complete.unwrap().response, Response::Diagnostic(payload.clone()));
            }
        }
    }

    #[test]
    /// Returns unit after rejecting oversized, missing-first, bad-sequence and duplicate
    /// ISO-TP frames; panics if any invalid assembly leaves the session connected.
    fn incomplete_duplicate_bad_sequence_and_oversize_multiframes_fail_closed() {
        for reply in [
            b"7E8 8 10 41 62 12 34 01 02 03\r>".as_slice(),
            b"7E8 8 10 0A 62 12 34 01 02 03\r7E8 8 22 04 05 06 07 00 00 00\r>",
            b"7E8 8 10 0A 62 12 34 01 02 03\r7E8 8 10 0A 62 12 34 01 02 03\r>",
            b"7E8 8 21 04 05 06 07 00 00 00\r>",
            b"7E8 8 10 0A 62 12 34 01 02 03\r7E8 8 21 04 05 06 07 00 00 00\r7E8 8 21 04 05 06 07 00 00 00\r>",
        ] {
            let (mut session, seq) = initialized();
            let tx = session
                .begin(Command::Read(QualifiedRead::synthetic_did(0x1234, 7).unwrap()), 1, 500)
                .unwrap();
            session.sent(tx.token, 2).unwrap();
            assert!(session.notify(1, seq + 1, reply, 10).is_err());
            assert!(!session.connected());
        }
    }

    #[test]
    /// Returns unit after checking initialization order/read gating, adapter rejection
    /// and the fourth pending response; panics if failures can retain an active epoch.
    fn initialization_failure_and_four_pending_replies_close_epoch() {
        let mut session = Session::new(Mode::SyntheticPeer);
        session.connect(1, 0).unwrap();
        assert_eq!(
            session
                .begin(Command::Initialize(Init::EngineHeader), 0, 500)
                .unwrap_err(),
            Error::Initialization
        );
        assert_eq!(
            session
                .begin(Command::Read(QualifiedRead::synthetic_obd(5).unwrap()), 0, 500)
                .unwrap_err(),
            Error::Initialization
        );
        let tx = session.begin(Command::Initialize(Init::EchoOff), 0, 500).unwrap();
        session.sent(tx.token, 0).unwrap();
        assert!(matches!(
            session.notify(1, 1, b"?\r>", 0).unwrap().unwrap().response,
            Response::AdapterError(_)
        ));
        assert!(!session.connected());
        let (mut session, seq) = initialized();
        coolant(&mut session);
        for i in 1..=3 {
            assert_eq!(session.notify(1, seq + i, b"7E8 4 03 7F 01 78\r", 10), Ok(None));
        }
        assert_eq!(
            session.notify(1, seq + 4, b"7E8 4 03 7F 01 78\r", 10),
            Err(Error::Reply)
        );
    }

    #[test]
    /// Returns unit after accepting one exact echo, rejecting duplicates and rejecting
    /// notifications before TX acknowledgment; panics if owner queue discipline is bypassed.
    fn echoes_are_consumed_once_and_early_notifications_require_owner_queue() {
        let (mut session, seq) = initialized();
        coolant(&mut session);
        assert!(
            session
                .notify(1, seq + 1, b"0201050000000000\r7E8 4 03 41 05 82\r>", 10)
                .unwrap()
                .is_some()
        );
        let (mut session, seq) = initialized();
        coolant(&mut session);
        assert_eq!(
            session.notify(1, seq + 1, b"0201050000000000\r0201050000000000\r", 10),
            Err(Error::Unexpected)
        );
        let mut session = Session::new(Mode::AdapterOnly);
        session.connect(1, 0).unwrap();
        session.begin(Command::Identity, 0, 100).unwrap();
        assert_eq!(session.notify(1, 1, b"ELM327 v2.3\r>", 1), Err(Error::Unexpected));
    }

    #[test]
    /// Returns unit after completing at the final ingress sequence and rejecting another
    /// transmission; panics if next-command admission permits sequence reuse.
    fn notification_counter_exhaustion_blocks_next_transmission() {
        let mut session = Session::new(Mode::AdapterOnly);
        session.connect(1, 0).unwrap();
        session.sequence = u64::MAX - 1;
        let tx = session.begin(Command::Identity, 0, 100).unwrap();
        session.sent(tx.token, 0).unwrap();
        session.notify(1, u64::MAX, b"ELM327 v2.3\r>", 1).unwrap();
        assert_eq!(session.begin(Command::Identity, 2, 100).unwrap_err(), Error::Exhausted);
    }

    #[test]
    /// Returns unit after checking big-endian millivolt decoding and exact negative reply
    /// service/width; panics on byte-order drift or accepting malformed negative evidence.
    fn voltage_preserves_big_endian_millivolts_and_negative_width_is_exact() {
        let (mut session, seq) = initialized();
        let tx = session
            .begin(Command::Read(QualifiedRead::synthetic_obd(0x42).unwrap()), 1, 500)
            .unwrap();
        assert_eq!(tx.bytes.as_slice(), b"0201420000000000\r");
        session.sent(tx.token, 2).unwrap();
        let done = session
            .notify(1, seq + 1, b"7E8 8 04 41 42 35 E8 00 00 00\r>", 3)
            .unwrap()
            .unwrap();
        let Response::Diagnostic(data) = done.response else {
            panic!("missing voltage")
        };
        assert_eq!(
            crate::diagnostics::obd::decode(0x42, &data[2..]).unwrap(),
            crate::diagnostics::obd::Decoded::Quantity(crate::samples::Quantity::ModuleVolts(13.8))
        );
        for bytes in [b"7E8 4 03 7F 22 78\r>".as_slice(), b"7E8 5 04 7F 01 78 00\r>"] {
            let (mut session, seq) = initialized();
            coolant(&mut session);
            assert_eq!(session.notify(1, seq + 1, bytes, 3), Err(Error::Reply));
        }
    }
}
