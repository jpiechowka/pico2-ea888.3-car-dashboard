//! Bounded application log history. RTT losses are a separate transport concern.
use core::cell::RefCell;
use core::fmt;
use core::sync::atomic::{AtomicU32, Ordering};

use embassy_sync::blocking_mutex::Mutex as BlockingMutex;
use embassy_sync::blocking_mutex::raw::RawMutex;
use embassy_sync::mutex::Mutex;
use heapless::{String, Vec};

pub const LOG_ENTRIES: usize = 128;
pub const LOG_MSG_LEN: usize = 40;
pub const VISIBLE_LINES: usize = 12;

/// A tiny observer of the already-owned snapshot, never another acquisition owner.
/// Baseline capture is quiet. Effective freshness includes elapsed-time staleness.
pub struct QualityTransitions {
    previous: [crate::samples::Quality; 8],
    initialized: bool,
}
impl Default for QualityTransitions {
    fn default() -> Self { Self::new() }
}
impl QualityTransitions {
    pub const fn new() -> Self {
        Self {
            previous: [crate::samples::Quality::Unknown; 8],
            initialized: false,
        }
    }

    pub fn log<M: RawMutex>(
        &mut self,
        store: &LogStore<M>,
        snapshot: &crate::acquisition::DashboardSnapshot,
        now_ms: u64,
    ) {
        for id in crate::samples::SensorId::ALL {
            let quality = snapshot.quality(id, now_ms);
            let previous = self.previous[id as usize];
            self.previous[id as usize] = quality;
            if self.initialized && previous != quality {
                // Only Info: a derivative quality transition cannot replace
                // the latest explicit transport/watchdog/transfer fault.
                store.push(
                    Level::Info,
                    now_ms,
                    format_args!("{}: {} -> {}", id.label(), previous.label(), quality.label()),
                );
            }
        }
        self.initialized = true;
    }
}

/// Caller invokes once when its existing request owner consumes a result.
pub fn log_warning_rejection<M: RawMutex>(
    store: &LogStore<M>,
    feedback: crate::warning_state::WarningFeedback,
    now_ms: u64,
) -> bool {
    if feedback != crate::warning_state::WarningFeedback::Rejected {
        return false;
    }
    store.push(Level::Info, now_ms, format_args!("Warning ack rejected"));
    true
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Message {
    text: String<LOG_MSG_LEN>,
    truncated: bool,
}
impl Message {
    pub const fn new() -> Self {
        Self {
            text: String::new(),
            truncated: false,
        }
    }

    pub fn as_str(&self) -> &str { &self.text }

    pub const fn truncated(&self) -> bool { self.truncated }
}
impl fmt::Write for Message {
    fn write_str(
        &mut self,
        s: &str,
    ) -> fmt::Result {
        if self.truncated {
            return Err(fmt::Error);
        }
        let available = LOG_MSG_LEN - self.text.len();
        let mut end = s.len().min(available);
        while !s.is_char_boundary(end) {
            end -= 1;
        }
        self.text.push_str(&s[..end]).map_err(|_| fmt::Error)?;
        self.truncated = end != s.len();
        if self.truncated { Err(fmt::Error) } else { Ok(()) }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Level {
    #[default]
    Info,
    Fault,
}
impl Level {
    pub const fn prefix(self) -> char {
        match self {
            Self::Info => 'I',
            Self::Fault => 'F',
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Entry {
    pub level: Level,
    pub timestamp_ms: u64,
    pub message: Message,
}
impl Entry {
    pub const EMPTY: Self = Self {
        level: Level::Info,
        timestamp_ms: 0,
        message: Message::new(),
    };

    pub fn new(
        level: Level,
        timestamp_ms: u64,
        args: fmt::Arguments<'_>,
    ) -> Self {
        let mut message = Message::new();
        if fmt::write(&mut message, args).is_err() {
            message.truncated = true;
        }
        Self {
            level,
            timestamp_ms,
            message,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Record {
    pub sequence: u64,
    pub entry: Entry,
}

pub struct Window {
    pub entries: Vec<Record, VISIBLE_LINES>,
    pub newest: Option<u64>,
    pub overwritten: u64,
}

pub struct LogBuffer {
    entries: [Record; LOG_ENTRIES],
    head: usize,
    count: usize,
    sequence: u64,
}
impl Default for LogBuffer {
    fn default() -> Self { Self::new() }
}
impl LogBuffer {
    pub const fn new() -> Self {
        Self {
            entries: [const {
                Record {
                    sequence: 0,
                    entry: Entry::EMPTY,
                }
            }; LOG_ENTRIES],
            head: 0,
            count: 0,
            sequence: 0,
        }
    }

    /// Exhaustion is terminal: never alias an earlier sequence or anchor.
    pub fn push(
        &mut self,
        entry: Entry,
    ) -> bool {
        let Some(next) = self.sequence.checked_add(1) else {
            return false;
        };
        self.entries[self.head] = Record { sequence: next, entry };
        self.sequence = next;
        self.head = (self.head + 1) % LOG_ENTRIES;
        self.count = (self.count + 1).min(LOG_ENTRIES);
        true
    }

    fn bounds(&self) -> Option<(u64, u64)> {
        (self.count != 0).then(|| (self.sequence - (self.count as u64 - 1), self.sequence))
    }

    fn end(
        &self,
        anchor: Option<u64>,
    ) -> Option<u64> {
        let (oldest, newest) = self.bounds()?;
        let visible = self.count.min(VISIBLE_LINES);
        Some(anchor.unwrap_or(newest).clamp(oldest + (visible as u64 - 1), newest))
    }

    /// None follows the newest entry. Negative rotation moves into history.
    pub fn scroll(
        &self,
        anchor: Option<u64>,
        delta: i32,
    ) -> Option<u64> {
        let end = self.end(anchor)?;
        let desired = if delta >= 0 {
            end.saturating_add(delta as u64)
        } else {
            end.saturating_sub(u64::from(delta.unsigned_abs()))
        };
        let end = self.end(Some(desired))?;
        (end != self.sequence).then_some(end)
    }

    pub fn window(
        &self,
        anchor: Option<u64>,
    ) -> Window {
        let mut window = Window {
            entries: Vec::new(),
            newest: self.bounds().map(|(_, newest)| newest),
            overwritten: self.sequence.saturating_sub(self.count as u64),
        };
        let Some(end) = self.end(anchor) else {
            return window;
        };
        let visible = self.count.min(VISIBLE_LINES);
        let oldest = self.bounds().unwrap().0;
        let first = end - (visible as u64 - 1);
        let start = (self.head + LOG_ENTRIES - self.count) % LOG_ENTRIES;
        for index in 0..visible {
            let offset = (first - oldest) as usize + index;
            let _ = window
                .entries
                .push(self.entries[(start + offset) % LOG_ENTRIES].clone());
        }
        window
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Counters {
    pub contention: u32,
    pub truncated: u32,
    pub exhausted: u32,
    pub fault_replacements: u32,
}
fn count(counter: &AtomicU32) {
    let _ = counter.try_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
        Some(value.saturating_add(1))
    });
}

pub struct LogStore<M: RawMutex> {
    history: Mutex<M, LogBuffer>,
    // Separate bounded copy; informational history contention/eviction cannot
    // discard the latest explicit fault. Never nest this with the history lock.
    fault: BlockingMutex<M, RefCell<Option<Entry>>>,
    contention: AtomicU32,
    truncated: AtomicU32,
    exhausted: AtomicU32,
    fault_replacements: AtomicU32,
}
impl<M: RawMutex> Default for LogStore<M> {
    fn default() -> Self { Self::new() }
}
impl<M: RawMutex> LogStore<M> {
    pub const fn new() -> Self {
        Self {
            history: Mutex::new(LogBuffer::new()),
            fault: BlockingMutex::new(RefCell::new(None)),
            contention: AtomicU32::new(0),
            truncated: AtomicU32::new(0),
            exhausted: AtomicU32::new(0),
            fault_replacements: AtomicU32::new(0),
        }
    }

    pub fn push(
        &self,
        level: Level,
        timestamp_ms: u64,
        args: fmt::Arguments<'_>,
    ) {
        // Formatting is outside both locks. The formatter stops at its bound.
        let entry = Entry::new(level, timestamp_ms, args);
        if entry.message.truncated() {
            count(&self.truncated);
        }
        if level == Level::Fault {
            let replaced = self
                .fault
                .lock(|slot| slot.borrow_mut().replace(entry.clone()).is_some());
            if replaced {
                count(&self.fault_replacements);
            }
        }
        match self.history.try_lock() {
            Ok(mut history) => {
                if !history.push(entry) {
                    count(&self.exhausted);
                }
            }
            Err(_) => count(&self.contention),
        }
    }

    pub fn window(
        &self,
        anchor: Option<u64>,
    ) -> Option<Window> {
        self.history.try_lock().ok().map(|history| history.window(anchor))
    }

    pub fn scroll(
        &self,
        anchor: &mut Option<u64>,
        delta: i32,
    ) -> bool {
        let Ok(history) = self.history.try_lock() else {
            return false;
        };
        *anchor = history.scroll(*anchor, delta);
        true
    }

    pub fn fault(&self) -> Option<Entry> { self.fault.lock(|slot| slot.borrow().clone()) }

    /// Independent saturating counters; not a transaction with a history snapshot.
    pub fn counters(&self) -> Counters {
        Counters {
            contention: self.contention.load(Ordering::Relaxed),
            truncated: self.truncated.load(Ordering::Relaxed),
            exhausted: self.exhausted.load(Ordering::Relaxed),
            fault_replacements: self.fault_replacements.load(Ordering::Relaxed),
        }
    }
}

#[cfg(all(test, not(target_os = "none")))]
mod tests {
    use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;

    use super::*;
    #[test]
    fn fault_survives_a_busy_history_and_counters_saturate() {
        let store = LogStore::<CriticalSectionRawMutex>::new();
        let guard = store.history.try_lock().unwrap();
        store.push(Level::Fault, 77, format_args!("fault"));
        assert_eq!(store.fault().unwrap().timestamp_ms, 77);
        assert_eq!(store.counters().contention, 1);
        assert!(store.window(None).is_none());
        let mut anchor = Some(3);
        assert!(!store.scroll(&mut anchor, 1));
        assert_eq!(anchor, Some(3));
        drop(guard);
        store.contention.store(u32::MAX, Ordering::Relaxed);
        let _guard = store.history.try_lock().unwrap();
        store.push(Level::Info, 78, format_args!("info"));
        assert_eq!(store.counters().contention, u32::MAX);
    }
    #[test]
    fn sequence_exhaustion_keeps_last_record_and_never_wraps() {
        let store = LogStore::<CriticalSectionRawMutex>::new();
        store.history.try_lock().unwrap().sequence = u64::MAX - 1;
        store.push(Level::Info, 1, format_args!("last"));
        store.push(Level::Fault, 2, format_args!("still retained"));
        let window = store.window(None).unwrap();
        assert_eq!(window.entries[0].sequence, u64::MAX);
        assert_eq!(window.entries[0].entry.message.as_str(), "last");
        assert_eq!(store.counters().exhausted, 1);
        assert_eq!(store.fault().unwrap().timestamp_ms, 2);
    }
}
