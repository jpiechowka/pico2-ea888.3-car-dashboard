//! The process adapter sends only StreamFramer output here. Controls use JobStore directly.
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError};

use dashboard_helper_protocol::Error;

use super::logs::LogInput;
pub const PROCESS_LOG_CAPACITY: usize = 256;
#[derive(Default)]
struct Counters {
    chunks: AtomicU64,
    bytes: AtomicU64,
    overflow: AtomicBool,
}
#[derive(Clone)]
pub struct ProcessLogSender {
    tx: SyncSender<LogInput>,
    loss: Arc<Counters>,
}
pub struct ProcessLogReceiver {
    rx: Receiver<LogInput>,
    loss: Arc<Counters>,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LogLoss {
    pub chunks: u64,
    pub bytes: u64,
    pub counter_overflow: bool,
}
pub fn process_log_channel() -> (ProcessLogSender, ProcessLogReceiver) {
    let (tx, rx) = mpsc::sync_channel(PROCESS_LOG_CAPACITY);
    let loss = Arc::new(Counters::default());
    (
        ProcessLogSender { tx, loss: loss.clone() },
        ProcessLogReceiver { rx, loss },
    )
}
impl ProcessLogSender {
    pub fn try_send(
        &self,
        input: LogInput,
    ) -> Result<bool, Error> {
        if input.text.len() > 1024 {
            return Err(Error::Bounds);
        }
        match self.tx.try_send(input) {
            Ok(()) => Ok(true),
            Err(TrySendError::Full(input)) => {
                add(&self.loss.chunks, 1, &self.loss.overflow);
                add(&self.loss.bytes, input.text.len() as u64, &self.loss.overflow);
                Ok(false)
            }
            Err(TrySendError::Disconnected(_)) => Err(Error::Closed),
        }
    }
}
impl ProcessLogReceiver {
    pub fn try_recv(&self) -> Result<Option<LogInput>, Error> {
        match self.rx.try_recv() {
            Ok(input) => Ok(Some(input)),
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) => Err(Error::Closed),
        }
    }

    /// Drain these deltas into the owner. While senders run, fields can straddle
    /// adjacent samples; after joining senders a final sample accounts for all drops.
    pub fn take_loss(&self) -> LogLoss {
        LogLoss {
            chunks: self.loss.chunks.swap(0, Ordering::AcqRel),
            bytes: self.loss.bytes.swap(0, Ordering::AcqRel),
            counter_overflow: self.loss.overflow.swap(false, Ordering::AcqRel),
        }
    }
}
fn add(
    value: &AtomicU64,
    amount: u64,
    overflow: &AtomicBool,
) {
    if let Ok(old) = value.try_update(Ordering::AcqRel, Ordering::Acquire, |old| {
        Some(old.saturating_add(amount))
    }) && old.checked_add(amount).is_none()
    {
        overflow.store(true, Ordering::Release);
    }
}
