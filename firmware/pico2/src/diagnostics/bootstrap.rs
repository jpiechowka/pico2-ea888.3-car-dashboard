//! Bounded handshake for a preprovisioned, externally qualified Longan firmware.
//! Never writes EEPROM or discovers baud. Quiet time only guards host commands;
//! only the complete requested data-mode banner establishes an RX12 boundary.

pub const QUIET_MS: u64 = 50;
pub const QUIET_TOTAL_MS: u64 = 1_000;
pub const TX_MS: u64 = 100;
pub const REPLY_MS: u64 = 500;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Clock,
    Exhausted,
    StaleTx,
    QuietTimeout,
    TxTimeout,
    ReplyTimeout,
    UnexpectedReply,
    ReceiveLoss,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Quiet,
    Handshake,
    Online,
    Fault(Error),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    None,
    Boundary,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Token(u64);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Command {
    pub token: Token,
    pub bytes: &'static [u8],
}
#[derive(Clone, Copy)]
enum Step {
    Enter,
    Probe,
    Quit,
}
impl Step {
    const fn command(self) -> &'static [u8] {
        match self {
            Self::Enter => b"+++",
            Self::Probe => b"AT\r\n",
            Self::Quit => b"AT+Q\r\n",
        }
    }

    const fn reply(self) -> &'static [u8] {
        match self {
            Self::Enter => b"ENTER INTO SETTING MODE\r\n",
            Self::Probe => b"OK\r\n",
            Self::Quit => b"OK\r\nENTER DATA MODE\r\n",
        }
    }
}
#[derive(Clone, Copy)]
enum Phase {
    Quiet {
        started: u64,
        last_rx: u64,
    },
    Issue(Step),
    Wait {
        step: Step,
        token: Token,
        started: u64,
        sent: bool,
        matched: usize,
    },
    Online,
    Fault(Error),
}
pub struct Bootstrap {
    phase: Phase,
    serial: u64,
    last_ms: u64,
}
impl Bootstrap {
    pub const fn new(now: u64) -> Self {
        Self {
            phase: Phase::Quiet {
                started: now,
                last_rx: now,
            },
            serial: 0,
            last_ms: now,
        }
    }

    pub const fn status(&self) -> Status {
        match self.phase {
            Phase::Quiet { .. } => Status::Quiet,
            Phase::Issue(_) | Phase::Wait { .. } => Status::Handshake,
            Phase::Online => Status::Online,
            Phase::Fault(error) => Status::Fault(error),
        }
    }

    pub fn fault(
        &mut self,
        error: Error,
    ) {
        // Exhausted identities and a regressed clock must never be resurrected.
        if !matches!(self.phase, Phase::Fault(Error::Clock | Error::Exhausted)) {
            self.phase = Phase::Fault(error);
        }
    }

    fn fail<T>(
        &mut self,
        error: Error,
    ) -> Result<T, Error> {
        self.fault(error);
        Err(error)
    }

    fn check(
        &mut self,
        now: u64,
    ) -> Result<(), Error> {
        if now < self.last_ms {
            return self.fail(Error::Clock);
        }
        self.last_ms = now;
        match self.phase {
            Phase::Fault(error) => Err(error),
            Phase::Quiet { started, .. } if now - started > QUIET_TOTAL_MS => self.fail(Error::QuietTimeout),
            Phase::Wait {
                started, sent: false, ..
            } if now - started > TX_MS => self.fail(Error::TxTimeout),
            Phase::Wait { started, .. } if now - started > REPLY_MS => self.fail(Error::ReplyTimeout),
            _ => Ok(()),
        }
    }

    /// The owner must already have invalidated its transaction/record parser,
    /// drained software TX and BUSY, and successfully discarded local RX.
    pub fn restart_after_discard(
        &mut self,
        now: u64,
    ) -> Result<(), Error> {
        if let Phase::Fault(error @ (Error::Clock | Error::Exhausted)) = self.phase {
            return Err(error);
        }
        if now < self.last_ms {
            return self.fail(Error::Clock);
        }
        self.last_ms = now;
        self.phase = Phase::Quiet {
            started: now,
            last_rx: now,
        };
        Ok(())
    }

    pub fn poll(
        &mut self,
        now: u64,
    ) -> Result<Option<Command>, Error> {
        self.check(now)?;
        if matches!(self.phase, Phase::Quiet { last_rx, .. } if now - last_rx >= QUIET_MS) {
            self.phase = Phase::Issue(Step::Enter);
        }
        let Phase::Issue(step) = self.phase else {
            return Ok(None);
        };
        let Some(serial) = self.serial.checked_add(1) else {
            return self.fail(Error::Exhausted);
        };
        self.serial = serial;
        let token = Token(serial);
        self.phase = Phase::Wait {
            step,
            token,
            started: now,
            sent: false,
            matched: 0,
        };
        Ok(Some(Command {
            token,
            bytes: step.command(),
        }))
    }

    fn advance(&mut self) -> Event {
        if let Phase::Wait {
            step,
            sent: true,
            matched,
            ..
        } = self.phase
            && matched == step.reply().len()
        {
            self.phase = match step {
                Step::Enter => Phase::Issue(Step::Probe),
                Step::Probe => Phase::Issue(Step::Quit),
                Step::Quit => Phase::Online,
            };
            if matches!(self.phase, Phase::Online) {
                return Event::Boundary;
            }
        }
        Event::None
    }

    /// Completion includes UARTFR.BUSY clearing, not just software-ring flush.
    pub fn tx_completed(
        &mut self,
        token: Token,
        now: u64,
    ) -> Result<Event, Error> {
        if !matches!(self.phase, Phase::Wait { token: expected, sent: false, .. } if token == expected) {
            return Err(Error::StaleTx);
        }
        self.check(now)?;
        if let Phase::Wait { sent, .. } = &mut self.phase {
            *sent = true;
        }
        Ok(self.advance())
    }

    /// Call byte-by-byte; on Boundary, route every following byte to RX12.
    /// The runtime drains TX first, retaining early replies in its IRQ buffer.
    pub fn receive(
        &mut self,
        byte: u8,
        now: u64,
    ) -> Result<Event, Error> {
        self.check(now)?;
        match &mut self.phase {
            Phase::Quiet { last_rx, .. } => *last_rx = now,
            Phase::Wait { step, matched, .. } if step.reply().get(*matched) == Some(&byte) => *matched += 1,
            _ => return self.fail(Error::UnexpectedReply),
        }
        Ok(self.advance())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exhausted_command_identity_stays_closed_across_fault_and_restart() {
        let mut boot = Bootstrap::new(0);
        boot.serial = u64::MAX;
        assert_eq!(boot.poll(50), Err(Error::Exhausted));
        boot.fault(Error::ReceiveLoss);
        assert_eq!(boot.restart_after_discard(100), Err(Error::Exhausted));
        assert_eq!(boot.status(), Status::Fault(Error::Exhausted));
    }
}
