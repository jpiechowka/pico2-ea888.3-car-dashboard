//! Deterministic browser fixture driver for the same bounded protocol as firmware.
//! No radio I/O, security negotiation or vehicle sample publication occurs here.
use dashboard_config::{BleProfile, BleSettings};
use dashboard_core::ble::link::{LinkFault, LinkSnapshot, LinkState};
use dashboard_core::ble::session::{Command, Error, INITIALIZATION, Mode, QualifiedRead, Response, Session};

pub struct BleSimulator {
    settings: BleSettings,
    session: Session,
    now_ms: u64,
    sequence: u64,
    snapshot: LinkSnapshot,
}
impl BleSimulator {
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

    pub fn snapshot(&self) -> LinkSnapshot { self.snapshot }

    pub fn now_ms(&self) -> u64 { self.now_ms }

    pub fn connected(&self) -> bool { self.session.connected() }

    pub fn busy(&self) -> bool { self.session.busy() }

    fn advance(
        &mut self,
        delta: u64,
    ) -> Result<(), Error> {
        self.now_ms = self.now_ms.checked_add(delta).ok_or(Error::Exhausted)?;
        Ok(())
    }

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

    pub fn begin_reply(&mut self) -> Result<(), Error> {
        let command = if self.settings.profile == BleProfile::TestPeer {
            Command::Read(QualifiedRead::synthetic_obd(5)?)
        } else {
            Command::Identity
        };
        self.start(command)
    }

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

    pub fn lose_notification(&mut self) -> Result<(), Error> {
        self.snapshot.lost_notifications = self.snapshot.lost_notifications.saturating_add(1);
        let result = self
            .session
            .notify(self.snapshot.epoch, self.sequence.saturating_add(2), b">", self.now_ms);
        result.map(|_| ()).map_err(|error| self.fail(error))
    }

    pub fn expire_reply(&mut self) -> Result<(), Error> {
        self.advance(u64::from(self.settings.response_timeout_ms))?;
        self.session.tick(self.now_ms).map_err(|error| self.fail(error))
    }

    /// A rejected old callback must not cancel or complete the current command.
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

    pub fn disconnect(&mut self) {
        self.session.invalidate();
        self.snapshot.state = LinkState::Disabled;
        self.snapshot.last_latency_ms = None;
        self.snapshot.fault = None;
    }
}
