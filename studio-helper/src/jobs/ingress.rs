//! The process adapter sends only StreamFramer output here. Controls use JobStore directly.
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError};

use dashboard_helper_protocol::Error;

use super::logs::LogInput;
/// Maximum queued process-output chunks before nonblocking producers count drops.
pub const PROCESS_LOG_CAPACITY: usize = 256;
#[derive(Default)]
/// Shared drop deltas; independent atomics are not a coherent multi-field snapshot.
struct Counters {
    /// Saturating number of dropped chunks since the receiver's last drain.
    chunks: AtomicU64,
    /// Saturating dropped UTF-8 byte count since the last drain.
    bytes: AtomicU64,
    /// Latched arithmetic saturation since the last drain.
    overflow: AtomicBool,
}
#[derive(Clone)]
/// Cloneable process-pipe producer; overflow drops output without blocking child pipes.
pub struct ProcessLogSender {
    /// Bounded queue shared by all producer clones.
    tx: SyncSender<LogInput>,
    /// Shared drop accounting observed by the owning receiver.
    loss: Arc<Counters>,
}
/// Single job-owner consumer of process output and overflow accounting.
pub struct ProcessLogReceiver {
    /// Output queue drained independently of child pipe readers.
    rx: Receiver<LogInput>,
    /// Shared drop accounting; drain again after joining producers for final totals.
    loss: Arc<Counters>,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
/// Loss deltas removed from the process-output ingress counters.
pub struct LogLoss {
    /// Number of omitted output chunks, saturating at `u64::MAX`.
    pub chunks: u64,
    /// Number of omitted UTF-8 bytes, saturating at `u64::MAX`.
    pub bytes: u64,
    /// At least one loss counter saturated during the observed interval.
    pub counter_overflow: bool,
}
/// Returns connected bounded producer/consumer endpoints with initially zero loss.
pub fn process_log_channel() -> (ProcessLogSender, ProcessLogReceiver) {
    let (tx, rx) = mpsc::sync_channel(PROCESS_LOG_CAPACITY);
    let loss = Arc::new(Counters::default());
    (
        ProcessLogSender { tx, loss: loss.clone() },
        ProcessLogReceiver { rx, loss },
    )
}
impl ProcessLogSender {
    /// Queue `input` without waiting. Returns `Ok(true)` when retained,
    /// `Ok(false)` when a full queue dropped and counted it, `Bounds` for text
    /// over 1024 UTF-8 bytes, or `Closed` after receiver disconnection.
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
    /// Returns one queued chunk, `Ok(None)` while empty with live producers,
    /// or `Closed` when disconnected and drained; never waits for output.
    pub fn try_recv(&self) -> Result<Option<LogInput>, Error> {
        match self.rx.try_recv() {
            Ok(input) => Ok(Some(input)),
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) => Err(Error::Closed),
        }
    }

    /// Drain these deltas into the owner. While senders run, fields can straddle
    /// adjacent samples; after joining senders a final sample accounts for all drops.
    /// Returns the removed chunk/byte/saturation deltas, resetting each independently.
    pub fn take_loss(&self) -> LogLoss {
        LogLoss {
            chunks: self.loss.chunks.swap(0, Ordering::AcqRel),
            bytes: self.loss.bytes.swap(0, Ordering::AcqRel),
            counter_overflow: self.loss.overflow.swap(false, Ordering::AcqRel),
        }
    }
}
/// Saturating-add `amount` to `value`, latching `overflow` if arithmetic exceeds
/// `u64::MAX`. Atomic modification permits concurrent producers and receiver drains.
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
