//! One bounded UART write, software flush, then hardware BUSY-clear.
//! Contains no DMA or borrowed user buffer. The owner preserves this state
//! across cancelled, side-effect-free Pending buffered-UART polls.
/// Maximum copied command bytes, below the reviewed bridge's 30-byte input array.
pub const MAX_BYTES: usize = 24;
/// Inclusive total write/flush/BUSY-clear budget in milliseconds, including abort drain.
pub const DEADLINE_MS: u64 = 100;

/// Admission or progress failure; clock/count/timeout faults permanently quarantine this pump.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// Empty command or more than MAX_BYTES; no command admitted.
    Length,
    /// A prior command or abort drain is still active.
    Busy,
    /// Completion does not match the current phase; transfer remains unchanged.
    Phase,
    /// Monotonic time regressed; pump quarantined.
    Clock,
    /// Original transfer age exceeded DEADLINE_MS; pump quarantined.
    Timeout,
    /// Successful write reported zero or more than the offered suffix; pump quarantined.
    WriteCount,
    /// A prior terminal fault prevents all further work on this instance.
    Quarantined,
}
/// Next bounded owner action; merely obtaining it does not advance the transfer.
#[derive(Debug, PartialEq, Eq)]
pub enum Work<'a> {
    /// No transfer or drain is active.
    Idle,
    /// Copy this suffix into the UART's software buffer, then report the actual count.
    Write(
        /// Pump-owned unsent bytes, valid for this borrow only.
        &'a [u8],
    ),
    /// Wait for the software transmit ring to empty.
    Flush,
    /// Wait until the hardware UART BUSY flag is clear.
    WaitIdle,
}
/// Terminal result after software flush and owner-observed hardware idle.
#[derive(Debug, PartialEq, Eq)]
pub enum Completion<T> {
    /// Full command drained; returns its original caller token.
    Sent(
        /// Original token, never returned for an aborted command.
        T,
    ),
    /// Abort drain completed; the original token was discarded.
    Aborted,
}
/// Ordered progress from buffer copying through hardware completion.
#[derive(Clone, Copy)]
enum Phase {
    /// Copying the not-yet-queued suffix.
    Write,
    /// Awaiting empty software TX ring.
    Flush,
    /// Awaiting hardware BUSY clear.
    WaitIdle,
}
/// One copied command and its immutable overall deadline origin.
struct Transfer<T> {
    /// Caller completion identity, cleared immediately on abort.
    token: Option<T>,
    /// Next operation the port owner must complete.
    phase: Phase,
    /// Admission time in monotonic milliseconds, retained across aborts.
    started: u64,
    /// Bytes already copied into the software transmit ring.
    at: usize,
    /// Original command length in 1..=MAX_BYTES.
    len: usize,
}
/// Exclusive UART-write progress with owned bytes and permanent fault quarantine.
/// Dropping a port poll is permitted only if Pending copied no bytes; dropping this
/// pump cannot stop bytes already queued in hardware.
pub struct Pump<T> {
    /// Bounded command storage; only the active transfer prefix is meaningful.
    bytes: [u8; MAX_BYTES],
    /// Active full transmission or abort drain; None means no acknowledged work.
    transfer: Option<Transfer<T>>,
    /// Last successful clock observation in milliseconds.
    last_ms: u64,
    /// Sticky ambiguity after loss/timeout; no local reset API clears it.
    quarantined: bool,
}
impl<T: Copy> Default for Pump<T> {
    /// Returns a fresh idle pump, as `new`.
    fn default() -> Self { Self::new() }
}
impl<T: Copy> Pump<T> {
    /// Returns an idle, unquarantined pump with initial clock zero.
    pub const fn new() -> Self {
        Self {
            bytes: [0; MAX_BYTES],
            transfer: None,
            last_ms: 0,
            quarantined: false,
        }
    }

    /// Returns whether a command or abort drain still requires port work.
    pub fn active(&self) -> bool { self.transfer.is_some() }

    /// Returns the sticky fault state; idle alone does not imply usable hardware.
    pub const fn quarantined(&self) -> bool { self.quarantined }

    /// Forgets local transfer/token and permanently refuses new work; queued bytes may still transmit.
    pub fn quarantine(&mut self) {
        // Already queued bytes may still leave hardware, so never enqueue a new
        // command or pretend the port has been resynchronized after a timeout.
        self.quarantined = true;
        self.transfer = None;
    }

    /// Quarantines the pump and returns `error` for any caller result type.
    fn fail<R>(
        &mut self,
        error: Error,
    ) -> Result<R, Error> {
        self.quarantine();
        Err(error)
    }

    /// Checks monotonic `now` in milliseconds and the inclusive original transfer deadline.
    /// Returns success while usable; reversal/expiry quarantines, existing quarantine persists.
    fn check(
        &mut self,
        now: u64,
    ) -> Result<(), Error> {
        if self.quarantined {
            return Err(Error::Quarantined);
        }
        if now < self.last_ms {
            return self.fail(Error::Clock);
        }
        self.last_ms = now;
        if self.transfer.as_ref().is_some_and(|tx| now - tx.started > DEADLINE_MS) {
            return self.fail(Error::Timeout);
        }
        Ok(())
    }

    /// Copies nonempty `bytes` of at most MAX_BYTES and binds completion `token` at
    /// monotonic `now` in milliseconds. Returns success after admission; busy/length/
    /// quarantine rejection preserves state, while clock reversal quarantines it.
    pub fn start(
        &mut self,
        token: T,
        bytes: &[u8],
        now: u64,
    ) -> Result<(), Error> {
        if self.quarantined {
            return Err(Error::Quarantined);
        }
        if self.active() {
            return Err(Error::Busy);
        }
        if bytes.is_empty() || bytes.len() > MAX_BYTES {
            return Err(Error::Length);
        }
        self.check(now)?;
        self.bytes[..bytes.len()].copy_from_slice(bytes);
        self.transfer = Some(Transfer {
            token: Some(token),
            phase: Phase::Write,
            started: now,
            at: 0,
            len: bytes.len(),
        });
        Ok(())
    }

    /// Returns the next port operation after checking monotonic `now` in milliseconds;
    /// no bytes or phase are advanced. Deadline/clock failure quarantines the pump.
    pub fn work(
        &mut self,
        now: u64,
    ) -> Result<Work<'_>, Error> {
        self.check(now)?;
        Ok(match &self.transfer {
            None => Work::Idle,
            Some(tx) => {
                match tx.phase {
                    Phase::Write => Work::Write(&self.bytes[tx.at..tx.len]),
                    Phase::Flush => Work::Flush,
                    Phase::WaitIdle => Work::WaitIdle,
                }
            }
        })
    }

    /// Acknowledges `count` bytes copied from the offered suffix at monotonic `now`
    /// in milliseconds. Returns success, entering Flush when all bytes are copied.
    /// Wrong phase preserves progress; zero/excess count, reversal or expiry quarantines.
    pub fn wrote(
        &mut self,
        count: usize,
        now: u64,
    ) -> Result<(), Error> {
        self.check(now)?;
        let Some(tx) = &mut self.transfer else {
            return Err(Error::Phase);
        };
        if !matches!(tx.phase, Phase::Write) {
            return Err(Error::Phase);
        }
        if count == 0 || count > tx.len - tx.at {
            return self.fail(Error::WriteCount);
        }
        tx.at += count;
        if tx.at == tx.len {
            tx.phase = Phase::Flush;
        }
        Ok(())
    }

    /// Records software-ring flush at monotonic `now` in milliseconds. Returns success
    /// entering WaitIdle, or a phase/clock/deadline error; this is not hardware completion.
    pub fn flushed(
        &mut self,
        now: u64,
    ) -> Result<(), Error> {
        self.check(now)?;
        let Some(tx) = &mut self.transfer else {
            return Err(Error::Phase);
        };
        if !matches!(tx.phase, Phase::Flush) {
            return Err(Error::Phase);
        }
        tx.phase = Phase::WaitIdle;
        Ok(())
    }

    /// The owner calls only after observing hardware BUSY == false. The pump
    /// cannot itself observe or prove electrical transmission or a CAN ACK.
    /// `now` is monotonic milliseconds. Returns Sent with the original token or Aborted,
    /// clearing the transfer, only from WaitIdle within deadline; otherwise returns an error.
    pub fn wire_idle(
        &mut self,
        now: u64,
    ) -> Result<Completion<T>, Error> {
        self.check(now)?;
        if !matches!(
            &self.transfer,
            Some(Transfer {
                phase: Phase::WaitIdle,
                ..
            })
        ) {
            return Err(Error::Phase);
        }
        Ok(match self.transfer.take().unwrap().token {
            Some(token) => Completion::Sent(token),
            None => Completion::Aborted,
        })
    }

    /// Cancel only bytes not yet copied. Preserve the original deadline while
    /// draining queued bytes; repeated aborts cannot prolong a stalled write.
    /// `now` is monotonic milliseconds. Returns success after removing the completion
    /// token and moving Write to Flush (idle is harmless), or a clock/deadline/quarantine error.
    pub fn abort(
        &mut self,
        now: u64,
    ) -> Result<(), Error> {
        self.check(now)?;
        if let Some(tx) = &mut self.transfer {
            tx.token = None;
            if matches!(tx.phase, Phase::Write) {
                tx.phase = Phase::Flush;
            }
        }
        Ok(())
    }
}
