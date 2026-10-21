//! Bounded application log history. RTT losses are a separate transport concern.
use core::cell::RefCell;
use core::fmt;
use core::sync::atomic::{AtomicU32, Ordering};

use embassy_sync::blocking_mutex::Mutex as BlockingMutex;
use embassy_sync::blocking_mutex::raw::RawMutex;
use embassy_sync::mutex::Mutex;
use heapless::{String, Vec};

/// Maximum retained chronological records; the next accepted record evicts the oldest.
pub const LOG_ENTRIES: usize = 128;
/// Maximum UTF-8 message bytes, excluding timestamp/severity stored separately.
pub const LOG_MSG_LEN: usize = 40;
/// Maximum copied history rows in one display snapshot, excluding the protected fault row.
pub const VISIBLE_LINES: usize = 12;

/// A tiny observer of the already-owned snapshot, never another acquisition owner.
/// Baseline capture is quiet. Effective freshness includes elapsed-time staleness.
pub struct QualityTransitions {
    /// Last observed effective quality in SensorId order, including elapsed-time expiry.
    previous: [crate::samples::Quality; 8],
    /// Whether the quiet initial baseline has been captured.
    initialized: bool,
}
impl Default for QualityTransitions {
    /// Returns a quiet, uninitialized quality observer.
    fn default() -> Self { Self::new() }
}
impl QualityTransitions {
    /// Returns an observer whose first snapshot establishes a baseline without logs.
    pub const fn new() -> Self {
        Self {
            previous: [crate::samples::Quality::Unknown; 8],
            initialized: false,
        }
    }

    /// Compares `snapshot` effective qualities at `now_ms` milliseconds with the
    /// previous observation and attempts Info entries in `store` for changes.
    /// Advances the baseline even when logging loses a record; never replaces
    /// explicit fault storage. Caller supplies its already accepted coherent source.
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
/// Returns `true` only when `feedback` is Rejected and an Info append was attempted
/// in `store` at `now_ms` milliseconds; contention/exhaustion may still drop history.
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
/// Bounded UTF-8 prefix with sticky formatting/truncation failure evidence.
pub struct Message {
    /// Valid UTF-8 prefix of at most 40 bytes; no ellipsis is appended.
    text: String<LOG_MSG_LEN>,
    /// A write exceeded capacity or entry formatting otherwise failed.
    truncated: bool,
}
impl Message {
    /// Returns empty text with no truncation evidence.
    pub const fn new() -> Self {
        Self {
            text: String::new(),
            truncated: false,
        }
    }

    /// Returns the retained UTF-8 prefix, including any control characters before display sanitization.
    pub fn as_str(&self) -> &str { &self.text }

    /// Returns whether any formatting/write attempt failed or exceeded capacity.
    pub const fn truncated(&self) -> bool { self.truncated }
}
impl fmt::Write for Message {
    /// Appends the largest UTF-8-aligned prefix of `s` that fits. Returns an error
    /// after a partial write on truncation; subsequent writes return an error without mutation.
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
/// Whether an event may replace the separately protected latest explicit fault.
pub enum Level {
    #[default]
    /// Informational history only, including derived quality/acknowledgment events.
    Info,
    /// Explicit failure retained independently of history eviction/contention.
    Fault,
}
impl Level {
    /// Returns the single-character display prefix `I` or `F`.
    pub const fn prefix(self) -> char {
        match self {
            Self::Info => 'I',
            Self::Fault => 'F',
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
/// Formatted log event before a history sequence is assigned.
pub struct Entry {
    /// Severity controlling the protected fault copy.
    pub level: Level,
    /// Caller-supplied boot-relative milliseconds; not validated for ordering.
    pub timestamp_ms: u64,
    /// Bounded UTF-8 text and explicit truncation evidence.
    pub message: Message,
}
impl Entry {
    /// Empty informational placeholder used only for unused ring slots.
    pub const EMPTY: Self = Self {
        level: Level::Info,
        timestamp_ms: 0,
        message: Message::new(),
    };

    /// Returns an entry with `level`, caller clock `timestamp_ms` in milliseconds
    /// and formatted `args`. Keeps a valid prefix on formatting failure and marks
    /// truncation; user-provided formatting runs here before storage locks.
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
/// Accepted history event paired with a never-reused chronological sequence.
pub struct Record {
    /// One-based append identity, independent of potentially unordered event timestamps.
    pub sequence: u64,
    /// Original bounded event, including severity and full millisecond timestamp.
    pub entry: Entry,
}

/// Owned history snapshot; no storage lock survives its construction.
pub struct Window {
    /// At most 12 records in oldest-to-newest order within the selected window.
    pub entries: Vec<Record, VISIBLE_LINES>,
    /// Latest retained sequence anywhere in the ring, or `None` when empty.
    pub newest: Option<u64>,
    /// Total accepted records evicted from the ring, excluding rejected append attempts.
    pub overwritten: u64,
}

/// Fixed 128-record ring with sequence anchors stable until their records are evicted.
pub struct LogBuffer {
    /// Physical ring slots; only the `count` entries preceding `head` are live.
    entries: [Record; LOG_ENTRIES],
    /// Next overwrite position modulo capacity.
    head: usize,
    /// Retained record count in `0..=128`.
    count: usize,
    /// Last assigned sequence, zero while empty; exhaustion permanently stops admission.
    sequence: u64,
}
impl Default for LogBuffer {
    /// Returns an empty ring with no issued sequences.
    fn default() -> Self { Self::new() }
}
impl LogBuffer {
    /// Returns empty fixed storage whose first accepted record has sequence one.
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
    /// Appends `entry`, evicting the oldest when full. Returns `false` without
    /// mutation at sequence exhaustion, or `true` after assignment and insertion.
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

    /// Returns inclusive oldest/newest retained sequence bounds, or `None` when empty.
    fn bounds(&self) -> Option<(u64, u64)> {
        (self.count != 0).then(|| (self.sequence - (self.count as u64 - 1), self.sequence))
    }

    /// Returns the ending sequence for `anchor` (`None` follows newest), clamped
    /// to retain a full available window; empty history returns `None`.
    fn end(
        &self,
        anchor: Option<u64>,
    ) -> Option<u64> {
        let (oldest, newest) = self.bounds()?;
        let visible = self.count.min(VISIBLE_LINES);
        Some(anchor.unwrap_or(newest).clamp(oldest + (visible as u64 - 1), newest))
    }

    /// None follows the newest entry. Negative rotation moves into history.
    /// Returns the adjusted `anchor` after signed row `delta`, saturating/clamping
    /// to retained history. Returns `None` at newest or on an empty ring; does not mutate storage.
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

    /// Returns an owned chronological window ending at `anchor`; `None` follows
    /// newest, expired/out-of-range anchors clamp, and an empty ring yields no entries.
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
/// Independently sampled saturating loss counters; not atomic with a history window.
pub struct Counters {
    /// Push attempts dropped because the history lock was busy.
    pub contention: u32,
    /// Entries whose formatter failed or exceeded the message byte capacity.
    pub truncated: u32,
    /// Push attempts rejected after sequence exhaustion.
    pub exhausted: u32,
    /// Explicit faults replacing an already occupied protected fault slot.
    pub fault_replacements: u32,
}
/// Saturating-increments `counter` with relaxed atomic ordering; supplies statistics, not synchronization.
fn count(counter: &AtomicU32) {
    let _ = counter.try_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
        Some(value.saturating_add(1))
    });
}

/// Concurrent bounded history plus an independently protected latest explicit fault.
/// `M` must support the caller's IRQ/core context. Formatting occurs outside locks;
/// history uses try-lock only, and fault/history locks are never nested.
pub struct LogStore<M: RawMutex> {
    /// Async-mutex storage accessed only with nonawaiting try-lock operations here.
    history: Mutex<M, LogBuffer>,
    // Separate bounded copy; informational history contention/eviction cannot
    // discard the latest explicit fault. Never nest this with the history lock.
    /// Latest explicit fault, copied under a separate short raw-mutex critical section.
    fault: BlockingMutex<M, RefCell<Option<Entry>>>,
    /// Saturating history-contention append loss count.
    contention: AtomicU32,
    /// Saturating message formatting/truncation count.
    truncated: AtomicU32,
    /// Saturating history sequence-exhaustion loss count.
    exhausted: AtomicU32,
    /// Saturating count of overwrites of the independent fault copy.
    fault_replacements: AtomicU32,
}
impl<M: RawMutex> Default for LogStore<M> {
    /// Returns an empty store with no fault or recorded losses.
    fn default() -> Self { Self::new() }
}
impl<M: RawMutex> LogStore<M> {
    /// Returns empty bounded history, an empty protected fault slot and zero counters.
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

    /// Formats `args` at `timestamp_ms` boot-relative milliseconds and severity
    /// `level`, then attempts history insertion without awaiting its lock.
    /// Fault entries update the independent protected copy even if history is busy
    /// or exhausted. Loss/truncation is counted; timestamps are accepted without ordering checks.
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

    /// Returns an owned window for ending sequence `anchor` (`None` follows latest),
    /// or `None` when history is busy. Read contention does not increment append-loss counters.
    pub fn window(
        &self,
        anchor: Option<u64>,
    ) -> Option<Window> {
        self.history.try_lock().ok().map(|history| history.window(anchor))
    }

    /// Adjusts `anchor` by signed row `delta` under a history try-lock. Returns
    /// `false` without changing it when busy, or `true` after clamping/follow-latest resolution.
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

    /// Returns a short-lock copy of the latest explicit fault, or `None` before one is recorded.
    pub fn fault(&self) -> Option<Entry> { self.fault.lock(|slot| slot.borrow().clone()) }

    /// Independent saturating counters; not a transaction with a history snapshot.
    /// Returns relaxed atomic observations of the four statistics.
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
/// Host regressions for separate fault ownership, saturation and terminal sequence exhaustion.
mod tests {
    use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;

    use super::*;
    #[test]
    /// Holds history busy while checking retained fault, unchanged scroll and saturating loss counters.
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
    /// Confirms exhausted history freezes its last record while protected fault storage still advances.
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
