//! One bounded UART write, software flush, then hardware BUSY-clear.
//! Contains no DMA or borrowed user buffer. The owner preserves this state
//! across cancelled, side-effect-free Pending buffered-UART polls.
pub const MAX_BYTES: usize = 24; // below the reviewed bridge's 30-byte input array
pub const DEADLINE_MS: u64 = 100;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Length,
    Busy,
    Phase,
    Clock,
    Timeout,
    WriteCount,
    Quarantined,
}
#[derive(Debug, PartialEq, Eq)]
pub enum Work<'a> {
    Idle,
    Write(&'a [u8]),
    Flush,
    WaitIdle,
}
#[derive(Debug, PartialEq, Eq)]
pub enum Completion<T> {
    Sent(T),
    Aborted,
}
#[derive(Clone, Copy)]
enum Phase {
    Write,
    Flush,
    WaitIdle,
}
struct Transfer<T> {
    token: Option<T>,
    phase: Phase,
    started: u64,
    at: usize,
    len: usize,
}
pub struct Pump<T> {
    bytes: [u8; MAX_BYTES],
    transfer: Option<Transfer<T>>,
    last_ms: u64,
    quarantined: bool,
}
impl<T: Copy> Default for Pump<T> {
    fn default() -> Self { Self::new() }
}
impl<T: Copy> Pump<T> {
    pub const fn new() -> Self {
        Self {
            bytes: [0; MAX_BYTES],
            transfer: None,
            last_ms: 0,
            quarantined: false,
        }
    }

    pub fn active(&self) -> bool { self.transfer.is_some() }

    pub const fn quarantined(&self) -> bool { self.quarantined }

    pub fn quarantine(&mut self) {
        // Already queued bytes may still leave hardware, so never enqueue a new
        // command or pretend the port has been resynchronized after a timeout.
        self.quarantined = true;
        self.transfer = None;
    }

    fn fail<R>(
        &mut self,
        error: Error,
    ) -> Result<R, Error> {
        self.quarantine();
        Err(error)
    }

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
