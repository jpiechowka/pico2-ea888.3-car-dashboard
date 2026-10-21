//! Deterministic browser fixture driver for the same bounded protocol as firmware.
//! No radio I/O, security negotiation or vehicle sample publication occurs here.
use dashboard_config::{BleProfile, BleSettings};
use dashboard_core::ble::link::{LinkFault, LinkSnapshot, LinkState};
use dashboard_core::ble::session::{Command, Error, INITIALIZATION, Mode, QualifiedRead, Response, Session};

/// Explicit synthetic session driver with checked logical time and no vehicle sample output.
pub struct BleSimulator {
    /// Captured accepted BLE settings; timing/profile changes require a new simulator.
    settings: BleSettings,
    /// Shared single-command parser/deadline/epoch state, restricted to adapter or synthetic authority.
    session: Session,
    /// Fixture-relative milliseconds advanced by commands and injected notifications, not wall/radio time.
    now_ms: u64,
    /// Checked fixture notification count, reset on each new connection epoch.
    sequence: u64,
    /// Display evidence and cumulative completion/loss counters, independent of physical BLE status.
    snapshot: LinkSnapshot,
}
impl BleSimulator {
    /// Returns a disconnected fixture using `settings`; TestPeer permits synthetic reads, other profiles identity only.
    /// Does not validate settings or simulate radio pairing/encryption/reconnect; callers supply accepted
    /// configuration.
    pub fn new(settings: BleSettings) -> Self {
        let mode = if settings.profile == BleProfile::TestPeer {
            Mode::SyntheticPeer
        } else {
            Mode::AdapterOnly
        };
        Self {
            settings,
            session: Session::new(mode),
            now_ms: 0,
            sequence: 0,
            snapshot: LinkSnapshot::default(),
        }
    }

    /// Returns the latest synthetic link evidence; never a hardware observation.
    pub fn snapshot(&self) -> LinkSnapshot { self.snapshot }

    /// Returns fixture-relative elapsed milliseconds, unrelated to actual radio latency.
    pub fn now_ms(&self) -> u64 { self.now_ms }

    /// Returns whether the shared parser currently has a valid logical fixture connection.
    pub fn connected(&self) -> bool { self.session.connected() }

    /// Returns whether a fixture command is awaiting prompt completion.
    pub fn busy(&self) -> bool { self.session.busy() }

    /// Advances fixture time by `delta` milliseconds; returns Exhausted without changing time on overflow.
    fn advance(
        &mut self,
        delta: u64,
    ) -> Result<(), Error> {
        self.now_ms = self.now_ms.checked_add(delta).ok_or(Error::Exhausted)?;
        Ok(())
    }

    /// Advances by configured command gap, admits `command` with configured whole-response deadline and acknowledges
    /// TX. Returns clock/exhaustion/session admission errors; elapsed time remains advanced on later errors.
    fn start(
        &mut self,
        command: Command,
    ) -> Result<(), Error> {
        self.advance(u64::from(self.settings.poll_gap_ms))?;
        let tx = self
            .session
            .begin(command, self.now_ms, u64::from(self.settings.response_timeout_ms))?;
        self.session.sent(tx.token, self.now_ms)
    }

    /// Delivers fixture `bytes` after advancing notification count and time by one millisecond.
    /// Returns false while assembling, true for allowed prompt completion, or counter/session/reply error with prior
    /// changes retained. Successful completion increments cumulative transactions and records fixture latency; no
    /// payload becomes a vehicle sample.
    fn fragment(
        &mut self,
        bytes: &[u8],
    ) -> Result<bool, Error> {
        self.sequence = self.sequence.checked_add(1).ok_or(Error::Exhausted)?;
        self.advance(1)?;
        let Some(done) = self
            .session
            .notify(self.snapshot.epoch, self.sequence, bytes, self.now_ms)?
        else {
            return Ok(false);
        };
        if !matches!(
            done.response,
            Response::Ok | Response::Identity(_) | Response::Diagnostic(_)
        ) {
            return Err(Error::Reply);
        }
        self.snapshot.completed_transactions = self
            .snapshot
            .completed_transactions
            .checked_add(1)
            .ok_or(Error::Exhausted)?;
        self.snapshot.last_latency_ms = Some((done.completed_ms - done.started_ms) as u32);
        Ok(true)
    }

    /// Invalidates current work, marks Fault and clears latency, returning the original `error` for propagation.
    /// Retains elapsed time, epoch and cumulative counters for inspection.
    fn fail(
        &mut self,
        error: Error,
    ) -> Error {
        self.session.invalidate();
        self.snapshot.state = LinkState::Fault;
        self.snapshot.fault = Some(LinkFault::Protocol(error));
        self.snapshot.last_latency_ms = None;
        error
    }

    /// Returns success after a fresh checked epoch, twelve ordered synthetic OK replies and synthetic adapter identity.
    /// Disabled settings return Disconnected unchanged; other failures invalidate current work and publish a protocol
    /// fault. Resets notification sequence, retains cumulative counters/time, and never performs pairing or real
    /// adapter/ECU I/O.
    pub fn connect(&mut self) -> Result<(), Error> {
        if !self.settings.enabled {
            return Err(Error::Disconnected);
        }
        let result = (|| {
            self.snapshot.epoch = self.snapshot.epoch.checked_add(1).ok_or(Error::Exhausted)?;
            self.sequence = 0;
            self.session.connect(self.snapshot.epoch, self.now_ms)?;
            self.snapshot.state = LinkState::Identifying;
            for init in INITIALIZATION {
                self.start(Command::Initialize(init))?;
                self.fragment(b"OK\r>")?;
            }
            self.start(Command::Identity)?;
            self.fragment(b"ELM327 synthetic\r>")?;
            self.snapshot.state = if self.settings.profile == BleProfile::TestPeer {
                LinkState::SyntheticReady
            } else {
                LinkState::AdapterReady
            };
            self.snapshot.fault = None;
            Ok(())
        })();
        result.map_err(|error| self.fail(error))
    }

    /// Starts synthetic coolant PID05 for TestPeer or identity for adapter profiles; returns shared admission/deadline
    /// errors. Advances command-gap time even if admission later fails; does not itself refresh the display fault
    /// snapshot on error.
    pub fn begin_reply(&mut self) -> Result<(), Error> {
        let command = if self.settings.profile == BleProfile::TestPeer {
            Command::Read(QualifiedRead::synthetic_obd(5)?)
        } else {
            Command::Identity
        };
        self.start(command)
    }

    /// Delivers the profile's fixed three-fragment response; returns success after shared parsing or latches a fault on
    /// error. Requires a matching outstanding fixture command for success; earlier fragments/time/counters are not
    /// rolled back.
    pub fn complete_fragmented(&mut self) -> Result<(), Error> {
        let fragments: &[&[u8]] = if self.settings.profile == BleProfile::TestPeer {
            &[b"7E8 8 03 41", b" 05 82 00 00 00 00\r", b">"]
        } else {
            &[b"ELM327 ", b"synthetic\r", b">"]
        };
        let result = (|| {
            for bytes in fragments {
                self.fragment(bytes)?;
            }
            Ok(())
        })();
        result.map_err(|error| self.fail(error))
    }

    /// Increments saturating observed-loss count and injects a skipped notification sequence at current time.
    /// Returns the shared result, normally NotificationLoss with invalidated work/fault; never repairs sequence
    /// continuity.
    pub fn lose_notification(&mut self) -> Result<(), Error> {
        self.snapshot.lost_notifications = self.snapshot.lost_notifications.saturating_add(1);
        let result = self
            .session
            .notify(self.snapshot.epoch, self.sequence.saturating_add(2), b">", self.now_ms);
        result.map(|_| ()).map_err(|error| self.fail(error))
    }

    /// Advances by one configured response budget and ticks the shared session; returns Timeout for an outstanding
    /// reply. Session failures latch display Fault; local time overflow returns Exhausted before that fault update.
    pub fn expire_reply(&mut self) -> Result<(), Error> {
        self.advance(u64::from(self.settings.response_timeout_ms))?;
        self.session.tick(self.now_ms).map_err(|error| self.fail(error))
    }

    /// Injects previous-epoch prompt and returns the shared result, normally Epoch after at least one connection.
    /// Does not advance fixture time/count or cancel current work; at initial epoch zero, saturation instead tests
    /// disconnected state.
    pub fn stale_reply(&mut self) -> Result<(), Error> {
        self.session
            .notify(
                self.snapshot.epoch.saturating_sub(1),
                self.sequence.saturating_add(1),
                b">",
                self.now_ms,
            )
            .map(|_| ())
    }

    /// Invalidates fixture work and clears displayed fault/latency, retaining elapsed time, epochs and cumulative
    /// counters.
    pub fn disconnect(&mut self) {
        self.session.invalidate();
        self.snapshot.state = LinkState::Disabled;
        self.snapshot.last_latency_ms = None;
        self.snapshot.fault = None;
    }
}
