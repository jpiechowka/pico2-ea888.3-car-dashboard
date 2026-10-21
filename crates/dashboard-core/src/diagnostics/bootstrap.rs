//! Bounded handshake for a preprovisioned, externally qualified Longan firmware.
//! Never writes EEPROM or discovers baud. Quiet time only guards host commands;
//! only the complete requested data-mode banner establishes an RX12 boundary.

/// Required UART receive silence in milliseconds before issuing +++; not framing evidence.
pub const QUIET_MS: u64 = 50;
/// Inclusive total quiet-acquisition budget in milliseconds, unaffected by incoming bytes.
pub const QUIET_TOTAL_MS: u64 = 1_000;
/// Inclusive command UART-completion allowance in milliseconds from issue.
pub const TX_MS: u64 = 100;
/// Inclusive reply allowance in milliseconds from command issue, not TX completion.
pub const REPLY_MS: u64 = 500;

/// Handshake failure; Clock and Exhausted are permanently terminal for this instance.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// Monotonic time regressed.
    Clock,
    /// Command identity cannot advance without reuse.
    Exhausted,
    /// Completion token is wrong, duplicate or no longer awaited; state is preserved.
    StaleTx,
    /// Quiet acquisition exceeded QUIET_TOTAL_MS.
    QuietTimeout,
    /// UART command completion exceeded TX_MS.
    TxTimeout,
    /// Expected reply did not complete within REPLY_MS from command issue.
    ReplyTimeout,
    /// Byte differs from the exact expected reply or arrived in an invalid phase.
    UnexpectedReply,
    /// Owner observed missing/untrustworthy receive bytes and invalidated the handshake.
    ReceiveLoss,
}

/// Coarse handshake state; Online proves only the externally qualified banner boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    /// Waiting for a bounded quiet interval before +++.
    Quiet,
    /// Issuing or matching the enter/probe/quit exchange.
    Handshake,
    /// Complete requested data-mode banner and all TX completions observed.
    Online,
    /// No further work until an allowed external discard/restart.
    Fault(
        /// Latched cause, with Clock/Exhausted protected from replacement.
        Error,
    ),
}
/// Incremental handshake result; only Boundary permits subsequent bytes to enter RX12.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    /// No completed data-mode boundary yet.
    None,
    /// Exact final banner and UART completion jointly established the new byte boundary.
    Boundary,
}
/// Unforgeable, monotonically issued command completion identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Token(
    /// Nonzero serial, never reused across restart.
    u64,
);
/// One static handshake command and the identity required at UART completion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Command {
    /// Unique completion ticket for this command.
    pub token: Token,
    /// Exact command bytes, including CRLF where applicable.
    pub bytes: &'static [u8],
}
/// Fixed, non-provisioning handshake order.
#[derive(Clone, Copy)]
enum Step {
    /// Escape from data mode with +++.
    Enter,
    /// Verify command response with AT.
    Probe,
    /// Return to data mode with AT+Q.
    Quit,
}
impl Step {
    /// Returns the exact static command bytes for this step; no EEPROM-setting command is included.
    const fn command(self) -> &'static [u8] {
        match self {
            Self::Enter => b"+++",
            Self::Probe => b"AT\r\n",
            Self::Quit => b"AT+Q\r\n",
        }
    }

    /// Returns the complete expected reply, including line endings and final data-mode banner.
    const fn reply(self) -> &'static [u8] {
        match self {
            Self::Enter => b"ENTER INTO SETTING MODE\r\n",
            Self::Probe => b"OK\r\n",
            Self::Quit => b"OK\r\nENTER DATA MODE\r\n",
        }
    }
}
/// Detailed handshake state with one outstanding command and bounded byte matching.
#[derive(Clone, Copy)]
enum Phase {
    /// Initial/recovery guard interval, not evidence of frame alignment.
    Quiet {
        /// Monotonic milliseconds when this quiet attempt began.
        started: u64,
        /// Monotonic milliseconds of the last received byte, initially started.
        last_rx: u64,
    },
    /// Next poll may issue exactly one command.
    Issue(
        /// Next fixed handshake step.
        Step,
    ),
    /// Command issued; reply bytes and TX completion may arrive in either order.
    Wait {
        /// Command/reply pair being matched.
        step: Step,
        /// Exact pending UART completion identity.
        token: Token,
        /// Issue time in milliseconds, shared by TX and reply deadlines.
        started: u64,
        /// Hardware-idle completion accepted for this command.
        sent: bool,
        /// Matching prefix length of the exact expected reply.
        matched: usize,
    },
    /// Final boundary established; raw bytes must now be routed elsewhere.
    Online,
    /// Handshake closed until external recovery, except terminal errors never recover.
    Fault(
        /// Protected terminal or recoverable failure cause.
        Error,
    ),
}
/// Bounded banner matcher; the owner supplies clock, I/O completion and recovery guarantees.
pub struct Bootstrap {
    /// Current quiet/command/reply/terminal state.
    phase: Phase,
    /// Last command ticket issued; retained across recovery.
    serial: u64,
    /// Last monotonic time observation in milliseconds.
    last_ms: u64,
}
impl Bootstrap {
    /// Returns a quiet-phase handshake starting at monotonic `now` in milliseconds.
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

    /// Returns coarse phase and latched fault without advancing time or issuing commands.
    pub const fn status(&self) -> Status {
        match self.phase {
            Phase::Quiet { .. } => Status::Quiet,
            Phase::Issue(_) | Phase::Wait { .. } => Status::Handshake,
            Phase::Online => Status::Online,
            Phase::Fault(error) => Status::Fault(error),
        }
    }

    /// Latches `error`, preserving an already terminal Clock/Exhausted cause.
    pub fn fault(
        &mut self,
        error: Error,
    ) {
        // Exhausted identities and a regressed clock must never be resurrected.
        if !matches!(self.phase, Phase::Fault(Error::Clock | Error::Exhausted)) {
            self.phase = Phase::Fault(error);
        }
    }

    /// Latches `error` subject to terminal-cause protection and returns Err(error).
    fn fail<T>(
        &mut self,
        error: Error,
    ) -> Result<T, Error> {
        self.fault(error);
        Err(error)
    }

    /// Checks monotonic `now` in milliseconds and inclusive phase deadlines. Returns
    /// success while admissible or latches/reports a clock/deadline error; existing faults persist.
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
    /// `now` is monotonic milliseconds. Returns success after restarting the quiet
    /// guard without resetting ticket serials; Clock/Exhausted remain closed, and reversal latches Clock.
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

    /// Advances quiet/issue work at monotonic `now` in milliseconds. Returns at most
    /// one new command, None while awaiting progress/online, or a latched deadline/
    /// clock/exhaustion error. Repeated polls never issue the same ticket twice.
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

    /// Advances only when both full reply and UART completion exist. Returns Boundary
    /// for final Quit completion, otherwise None; no command is issued here.
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
    /// `token` must be the exact uncompleted command ticket; `now` is monotonic
    /// milliseconds. Returns the next event after deadline checks, or StaleTx before
    /// changing time/state for duplicate/foreign completions.
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
    /// `byte` is the next ordered UART byte and `now` monotonic milliseconds. Returns
    /// Boundary only after the complete final reply plus TX completion; quiet bytes
    /// restart the silence guard, while any unexpected handshake byte latches failure.
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
/// Terminal handshake identity regression tests.
mod tests {
    use super::*;

    #[test]
    /// Verifies exhausted ticket identity survives replacement faults and attempted restart.
    fn exhausted_command_identity_stays_closed_across_fault_and_restart() {
        let mut boot = Bootstrap::new(0);
        boot.serial = u64::MAX;
        assert_eq!(boot.poll(50), Err(Error::Exhausted));
        boot.fault(Error::ReceiveLoss);
        assert_eq!(boot.restart_after_discard(100), Err(Error::Exhausted));
        assert_eq!(boot.status(), Status::Fault(Error::Exhausted));
    }
}
