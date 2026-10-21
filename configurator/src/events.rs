//! Bounded, local Studio diagnostics. Helper job logs have their own store.
use std::collections::VecDeque;
use std::io::{self, Write};

use serde::Serialize;

/// Maximum retained event records; eviction removes the oldest record.
const EVENT_CAPACITY: usize = 512;
/// Maximum recent warning/error keys considered for exact coalescing.
const COALESCER_CAPACITY: usize = 64;
/// Maximum serialized snapshot bytes, including JSONL newlines.
const JSONL_CAPACITY: usize = 2 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
/// Stable subsystem tags serialized in diagnostics and mapped explicitly by the optional relay.
pub enum Category {
    /// Editor commands, revisions and draft lifecycle.
    Editor,
    /// Shared schema/acceptance failures and recovery.
    Validation,
    /// Browser persistence and quota/permission outcomes.
    Storage,
    /// File, project and conversion input/output lifecycle.
    ImportExport,
    /// Synthetic playback and simulator controls.
    Simulation,
    /// Preview rendering, canvas and scheduling.
    Rendering,
    /// Helper connection, prerequisites and build tools.
    HostTools,
    /// Selected-device inspection and explicit operation diagnostics.
    Device,
}

impl Category {
    /// Human UI text; Serialize retains the stable diagnostic tags above.
    /// Returns the category's display label.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Editor => "Editor",
            Self::Validation => "Validation",
            Self::Storage => "Storage",
            Self::ImportExport => "Import / export",
            Self::Simulation => "Simulation",
            Self::Rendering => "Rendering",
            Self::HostTools => "Host tools",
            Self::Device => "Device",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
/// Ordered local diagnostic severity, from trace detail through error.
pub enum Severity {
    /// Finest optional debugging detail; console emission requires opt-in and rate admission.
    Trace,
    /// Debugging detail; console emission requires opt-in and rate admission.
    Debug,
    /// Retained informational state; not emitted by the console policy.
    Info,
    /// Attention condition; exact bounded repetitions may be coalesced.
    Warn,
    /// Failed operation; exact bounded repetitions may be coalesced.
    Error,
}

/// Application-owned tag. Construction rejects user text and oversized keys.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct EventCode(
    /// Application-owned ASCII code of 1–32 lowercase/digit/underscore/hyphen/dot bytes.
    &'static str,
);

impl EventCode {
    /// Returns a checked static `code`, or invalid-code error for empty/oversized/disallowed characters.
    pub fn new(code: &'static str) -> Result<Self, EventError> {
        if code.is_empty()
            || code.len() > 32
            || !code
                .bytes()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, b'_' | b'-' | b'.'))
        {
            return Err(EventError::InvalidCode);
        }
        Ok(Self(code))
    }

    /// Returns the exact admitted static code.
    pub fn get(self) -> &'static str { self.0 }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(transparent)]
/// Monotonic store-local event identity; coalescing retains the original sequence.
pub struct EventSeq(
    /// Issued sequence number; zero is used only before private insertion assigns identity.
    u64,
);
impl EventSeq {
    /// Returns the store-local numeric sequence; values never wrap or restart on clear.
    pub fn get(self) -> u64 { self.0 }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
/// Saturating diagnostic count with explicit loss-of-exactness evidence.
pub struct DiagnosticCount {
    /// Exact count until overflow, then retained `u64::MAX`.
    pub value: u64,
    /// Set only after the actual count can no longer be represented exactly.
    pub saturated: bool,
}
impl DiagnosticCount {
    /// Adds one, marking saturation only when an increment exceeds the representable maximum.
    pub fn increment(&mut self) {
        if let Some(value) = self.value.checked_add(1) {
            self.value = value;
        } else {
            self.saturated = true;
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
/// Lifetime accounting retained when visible records are cleared.
pub struct EventCounters {
    /// Records allocated, including internal suppression summaries.
    pub inserted: DiagnosticCount,
    /// Old records evicted by retention capacity.
    pub dropped: DiagnosticCount,
    /// Repeats merged into retained events, including suppression summary repetitions.
    pub repeated: DiagnosticCount,
    /// Debug/Trace console emissions denied by the rolling rate gate.
    pub console_suppressed: DiagnosticCount,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
/// Whether another event identity can be allocated.
pub enum EventStatus {
    /// At least one remaining sequence value is available.
    Accepting,
    /// Final sequence was issued; all further recording, including repeats, is rejected.
    Exhausted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Recording admission failure; rejected input changes no clocks, counters or console slots.
pub enum EventError {
    /// Static code violates its bounded ASCII syntax.
    InvalidCode,
    /// All sequence identities have been issued.
    SequenceExhausted,
}

/// Borrowed local diagnostic input; caller excludes credentials/private payloads before recording.
pub struct EventInput<'a> {
    /// Subsystem responsible for the event.
    pub category: Category,
    /// Retention/coalescing/console severity.
    pub severity: Severity,
    /// Static application tag validated by [`EventCode::new`].
    pub code: &'static str,
    /// Optional local correlation text, retained up to 128 UTF-8 bytes.
    pub subject: Option<&'a str>,
    /// Local diagnostic text, retained up to 512 UTF-8 bytes.
    pub message: &'a str,
    /// Monotonic elapsed milliseconds; backwards input is clamped to the store high-water mark.
    pub elapsed_ms: u64,
    /// Unix milliseconds, serialized by Rust; never used for admission/order.
    pub wall_clock_ms: Option<i64>,
}

#[derive(Clone, Debug, Serialize)]
/// Owned bounded event record, possibly representing repeated exact Warn/Error inputs.
pub struct Event {
    /// First-insertion identity, unchanged by repetition.
    pub seq: EventSeq,
    /// Originating subsystem.
    pub category: Category,
    /// Original severity used by filters and console policy.
    pub severity: Severity,
    /// Validated application-owned tag.
    pub code: EventCode,
    /// Optional UTF-8-safe bounded local correlation text.
    pub subject: Option<String>,
    /// Original subject exceeded the 128-byte retention cap; prevents coalescing.
    pub subject_truncated: bool,
    /// UTF-8-safe local message prefix, at most 512 bytes.
    pub message: String,
    /// Original message exceeded its cap; prevents coalescing.
    pub message_truncated: bool,
    /// First insertion's clamped monotonic elapsed milliseconds.
    pub elapsed_ms: u64,
    /// Latest repetition's clamped monotonic elapsed milliseconds.
    pub last_elapsed_ms: u64,
    /// Optional first insertion Unix milliseconds; never an ordering authority.
    pub wall_clock_ms: Option<i64>,
    /// Optional latest repetition Unix milliseconds, including unavailable `None`.
    pub last_wall_clock_ms: Option<i64>,
    /// Number of merged inputs, with explicit overflow evidence.
    pub count: DiagnosticCount,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Whether recording allocated a new event identity or merged an exact repeat.
pub enum RecordKind {
    /// A new record was retained, possibly evicting the oldest.
    Inserted,
    /// Existing retained warning/error count was incremented.
    Repeated,
}

/// The sole console policy decision. Adapters emit only Emit outcomes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConsoleDisposition {
    /// Adapter should emit this event to the browser console.
    Emit,
    /// Retain locally without console output.
    RetainedOnly,
    /// Repeated warning/error was merged without another console message.
    Coalesced,
    /// Opted-in Debug/Trace exceeded ten emissions per rolling second.
    RateLimited,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Recording result separating retained identity from the sole console-emission decision.
pub struct RecordOutcome {
    /// Inserted or repeated event identity.
    pub seq: EventSeq,
    /// Allocation versus coalescing result.
    pub kind: RecordKind,
    /// Adapter action; emit only [`ConsoleDisposition::Emit`].
    pub console: ConsoleDisposition,
}

/// Boundary accepting local diagnostics without requiring a browser console.
pub trait EventSink {
    /// Records borrowed `input`; returns retained identity/kind/console decision or admission failure.
    fn record(
        &mut self,
        input: EventInput<'_>,
    ) -> Result<RecordOutcome, EventError>;
}

/// Returns the longest valid UTF-8 prefix of `text` within `capacity` bytes and whether any suffix was omitted.
fn bounded(
    text: &str,
    capacity: usize,
) -> (&str, bool) {
    let mut end = text.len().min(capacity);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    (&text[..end], end < text.len())
}

/// Bounded case-insensitive text/category/severity query over an immutable snapshot.
pub struct EventFilter {
    /// Exact subsystem restriction; `None` permits every category.
    category: Option<Category>,
    /// Inclusive severity floor.
    minimum: Severity,
    /// Unicode lowercase query derived from an at-most-128-byte input prefix.
    query: String,
    /// Input query lost a suffix before lowercase conversion.
    query_truncated: bool,
    /// Maximum oldest-first matching records, at most retention capacity.
    limit: usize,
}

impl Default for EventFilter {
    /// Returns an unrestricted filter over all retained records.
    fn default() -> Self { Self::new("") }
}
impl EventFilter {
    /// Returns a filter searching `query` in message/code/subject, truncating to 128 UTF-8 bytes before lowercasing.
    pub fn new(query: &str) -> Self {
        let (query, query_truncated) = bounded(query, 128);
        Self {
            category: None,
            minimum: Severity::Trace,
            query: query.to_lowercase(),
            query_truncated,
            limit: EVENT_CAPACITY,
        }
    }

    /// Returns this filter with inclusive severity floor `minimum`.
    pub fn minimum(
        mut self,
        minimum: Severity,
    ) -> Self {
        self.minimum = minimum;
        self
    }

    /// Returns this filter restricted to `category`.
    pub fn category(
        mut self,
        category: Category,
    ) -> Self {
        self.category = Some(category);
        self
    }

    /// Returns this filter with oldest-first record `limit`, clamped to 512; zero returns no records.
    pub fn limit(
        mut self,
        limit: usize,
    ) -> Self {
        self.limit = limit.min(EVENT_CAPACITY);
        self
    }

    /// Returns whether the original query exceeded its UTF-8 input byte cap.
    pub fn query_truncated(&self) -> bool { self.query_truncated }

    /// Returns whether `event` satisfies severity, category and case-insensitive text predicates.
    fn matches(
        &self,
        event: &Event,
    ) -> bool {
        event.severity >= self.minimum
            && self.category.is_none_or(|c| c == event.category)
            && (self.query.is_empty()
                || event.message.to_lowercase().contains(&self.query)
                || event.code.get().contains(&self.query)
                || event
                    .subject
                    .as_ref()
                    .is_some_and(|s| s.to_lowercase().contains(&self.query)))
    }
}

/// Immutable data for filtering/export. It carries no recording authority.
pub struct EventSnapshot {
    /// Cloned matching records in insertion order, limited by the filter.
    records: Vec<Event>,
    /// Store issuance availability at snapshot time.
    pub status: EventStatus,
    /// Lifetime counters at snapshot time, independent of filter.
    pub counters: EventCounters,
    /// Oldest/newest of the whole retained store, independent of the filter.
    pub oldest: Option<EventSeq>,
    /// Newest identity of the whole retained store, or `None` when empty.
    pub newest: Option<EventSeq>,
    /// Total records in the whole retained store.
    pub retained: usize,
    /// Records satisfying the filter before its result limit.
    pub matching: usize,
    /// Matching records excluded only by the filter result limit.
    pub omitted: usize,
}

impl EventSnapshot {
    /// Returns the snapshot's oldest-first matching retained records.
    pub fn records(&self) -> &[Event] { &self.records }

    /// Returns newline-terminated JSON for snapshot records, bounded to 2 MiB; empty snapshots return empty bytes.
    /// Size/serialization failure returns no partial export, leaving the snapshot unchanged.
    pub fn jsonl(&self) -> Result<Vec<u8>, JsonlError> {
        let mut writer = JsonlWriter::new();
        for event in &self.records {
            if serde_json::to_writer(&mut writer, event).is_err() || writer.write_all(b"\n").is_err() {
                return Err(if writer.too_large {
                    JsonlError::TooLarge
                } else {
                    JsonlError::Serialization
                });
            }
        }
        Ok(writer.bytes)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Bounded local event export failure.
pub enum JsonlError {
    /// Encoded records and newlines exceed 2 MiB.
    TooLarge,
    /// JSON serialization or another writer failure occurred.
    Serialization,
}

/// In-memory writer enforcing the export cap before every append.
struct JsonlWriter {
    /// Successfully appended prefix, never longer than [`JSONL_CAPACITY`].
    bytes: Vec<u8>,
    /// Sticky evidence that an attempted append exceeded the byte cap.
    too_large: bool,
}
impl JsonlWriter {
    /// Returns an empty writer with capacity reserved for the entire bounded export.
    fn new() -> Self {
        Self {
            bytes: Vec::with_capacity(JSONL_CAPACITY),
            too_large: false,
        }
    }
}
impl Write for JsonlWriter {
    /// Appends all `bytes` and returns their count, or marks overflow and errors without appending any of this chunk.
    fn write(
        &mut self,
        bytes: &[u8],
    ) -> io::Result<usize> {
        if bytes.len() > JSONL_CAPACITY - self.bytes.len() {
            self.too_large = true;
            return Err(io::Error::other("event JSONL limit exceeded"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    /// Returns success; all writes already reside in memory and require no flush operation.
    fn flush(&mut self) -> io::Result<()> { Ok(()) }
}

/// One store also owns the coalescing and rolling console policy.
pub struct EventStore {
    /// At most 512 owned records in insertion order.
    records: VecDeque<Event>,
    /// At most 64 coalescible retained sequence identities, evicted independently of records.
    keys: VecDeque<EventSeq>,
    /// Next allocatable sequence, or `None` after the final identity is issued.
    next_seq: Option<u64>,
    /// Lifetime accounting retained across visible clears.
    counters: EventCounters,
    /// Maximum observed elapsed milliseconds, clamping backwards timestamps.
    high_water_ms: u64,
    /// Explicit opt-in for rate-limited Debug/Trace console messages.
    debug_console: bool,
    /// Last admitted Debug/Trace console timestamps; at most ten per rolling second.
    emitted: VecDeque<u64>,
    /// Retained suppression summary identity, or `None` when absent/evicted/cleared.
    rate_summary: Option<EventSeq>,
}

impl Default for EventStore {
    /// Returns an empty store issuing from sequence one, with debug console opt-in disabled.
    fn default() -> Self {
        Self {
            records: VecDeque::with_capacity(EVENT_CAPACITY),
            keys: VecDeque::with_capacity(COALESCER_CAPACITY),
            next_seq: Some(1),
            counters: EventCounters::default(),
            high_water_ms: 0,
            debug_console: false,
            emitted: VecDeque::with_capacity(10),
            rate_summary: None,
        }
    }
}

impl EventStore {
    /// Returns whether sequence issuance remains available; exhaustion also blocks coalescing.
    pub fn status(&self) -> EventStatus {
        if self.next_seq.is_some() {
            EventStatus::Accepting
        } else {
            EventStatus::Exhausted
        }
    }

    /// Returns lifetime insertion/eviction/repeat/console-suppression accounting.
    pub fn counters(&self) -> EventCounters { self.counters }

    /// Sets Debug/Trace console opt-in to `enabled`; retained records and rolling admission history remain unchanged.
    pub fn set_debug_console(
        &mut self,
        enabled: bool,
    ) {
        self.debug_console = enabled;
    }

    /// Returns retained event `seq`, or `None` after eviction/clear or for an unknown identity.
    pub fn get(
        &self,
        seq: EventSeq,
    ) -> Option<&Event> {
        self.records.iter().find(|e| e.seq == seq)
    }

    /// Returns whether event `seq` remains retained.
    pub fn contains(
        &self,
        seq: EventSeq,
    ) -> bool {
        self.get(seq).is_some()
    }

    /// Clears records/coalescing keys/summary; preserves identity exhaustion, lifetime counters, clocks and rate
    /// history.
    pub fn clear(&mut self) {
        self.records.clear();
        self.keys.clear();
        self.rate_summary = None;
    }

    /// Returns cloned oldest-first records admitted by `filter`, with whole-store and prelimit matching counts.
    /// Snapshot construction does not alter recording, coalescing or console policy.
    pub fn snapshot(
        &self,
        filter: &EventFilter,
    ) -> EventSnapshot {
        let matching = self.records.iter().filter(|e| filter.matches(e)).count();
        let records = self
            .records
            .iter()
            .filter(|e| filter.matches(e))
            .take(filter.limit)
            .cloned()
            .collect();
        EventSnapshot {
            records,
            status: self.status(),
            counters: self.counters,
            oldest: self.records.front().map(|e| e.seq),
            newest: self.records.back().map(|e| e.seq),
            retained: self.records.len(),
            matching,
            omitted: matching.saturating_sub(filter.limit),
        }
    }

    /// Allocates identity for `event`, ignoring its placeholder sequence, and returns the issued sequence.
    /// Evicts oldest record/key/summary at capacity; exhaustion returns an error before mutation.
    fn insert(
        &mut self,
        mut event: Event,
    ) -> Result<EventSeq, EventError> {
        let seq = self.next_seq.ok_or(EventError::SequenceExhausted)?;
        self.next_seq = seq.checked_add(1);
        event.seq = EventSeq(seq);
        if self.records.len() == EVENT_CAPACITY {
            if let Some(oldest) = self.records.pop_front() {
                self.keys.retain(|key| *key != oldest.seq);
                if self.rate_summary == Some(oldest.seq) {
                    self.rate_summary = None;
                }
            }
            self.counters.dropped.increment();
        }
        self.records.push_back(event);
        self.counters.inserted.increment();
        Ok(EventSeq(seq))
    }

    /// Increments retained `seq` and updates its last monotonic time plus optional Unix `wall_clock_ms`.
    /// Missing identities do nothing; exactness flags propagate independently for event and lifetime counts.
    fn repeat(
        &mut self,
        seq: EventSeq,
        wall_clock_ms: Option<i64>,
    ) {
        if let Some(event) = self.records.iter_mut().find(|e| e.seq == seq) {
            event.count.increment();
            event.last_elapsed_ms = self.high_water_ms;
            event.last_wall_clock_ms = wall_clock_ms;
            self.counters.repeated.increment();
        }
    }

    /// Inserts/repeats the local console-suppression summary using optional Unix `wall_clock_ms`.
    /// Allocation failure at sequence exhaustion is ignored; summary never re-enters console policy.
    fn suppression_summary(
        &mut self,
        wall_clock_ms: Option<i64>,
    ) {
        if let Some(seq) = self.rate_summary {
            self.repeat(seq, wall_clock_ms);
            return;
        }
        // Direct bounded insertion: Info summary never re-enters console policy.
        let summary = Event {
            seq: EventSeq(0),
            category: Category::Rendering,
            severity: Severity::Info,
            code: EventCode("console.rate_limit"),
            subject: None,
            subject_truncated: false,
            message: "Debug/Trace console rate limit reached".into(),
            message_truncated: false,
            elapsed_ms: self.high_water_ms,
            last_elapsed_ms: self.high_water_ms,
            wall_clock_ms,
            last_wall_clock_ms: wall_clock_ms,
            count: DiagnosticCount {
                value: 1,
                saturated: false,
            },
        };
        self.rate_summary = self.insert(summary).ok();
    }
}

impl EventSink for EventStore {
    /// Validates `input`, clamps monotonic time, retains bounded text, and returns identity/console policy outcome.
    /// Exact untruncated Warn/Error repeats among the last 64 keys coalesce; other records allocate identity.
    /// Debug/Trace console output requires opt-in and at most ten emissions per rolling 1000 ms.
    /// Invalid codes/exhausted identity return errors before mutation; no browser I/O occurs here.
    fn record(
        &mut self,
        input: EventInput<'_>,
    ) -> Result<RecordOutcome, EventError> {
        // Validate before changing clocks, counters, identity or console slots.
        let code = EventCode::new(input.code)?;
        if self.next_seq.is_none() {
            return Err(EventError::SequenceExhausted);
        }
        self.high_water_ms = self.high_water_ms.max(input.elapsed_ms);
        let (message, message_truncated) = bounded(input.message, 512);
        let (subject, subject_truncated) = match input.subject {
            Some(s) => {
                let (s, truncated) = bounded(s, 128);
                (Some(s), truncated)
            }
            None => (None, false),
        };
        let coalescible =
            matches!(input.severity, Severity::Warn | Severity::Error) && !message_truncated && !subject_truncated;
        if coalescible
            && let Some(seq) = self.keys.iter().find_map(|seq| {
                self.get(*seq)
                    .filter(|e| {
                        e.category == input.category
                            && e.severity == input.severity
                            && e.code == code
                            && e.subject.as_deref() == subject
                            && e.message == message
                    })
                    .map(|e| e.seq)
            })
        {
            self.repeat(seq, input.wall_clock_ms);
            return Ok(RecordOutcome {
                seq,
                kind: RecordKind::Repeated,
                console: ConsoleDisposition::Coalesced,
            });
        }
        let event = Event {
            seq: EventSeq(0),
            category: input.category,
            severity: input.severity,
            code,
            subject: subject.map(str::to_owned),
            subject_truncated,
            message: message.into(),
            message_truncated,
            elapsed_ms: self.high_water_ms,
            last_elapsed_ms: self.high_water_ms,
            wall_clock_ms: input.wall_clock_ms,
            last_wall_clock_ms: input.wall_clock_ms,
            count: DiagnosticCount {
                value: 1,
                saturated: false,
            },
        };
        let seq = self.insert(event)?;
        if coalescible {
            if self.keys.len() == COALESCER_CAPACITY {
                self.keys.pop_front();
            }
            self.keys.push_back(seq);
        }
        let console = match input.severity {
            Severity::Warn | Severity::Error => ConsoleDisposition::Emit,
            Severity::Debug | Severity::Trace if self.debug_console => {
                while self
                    .emitted
                    .front()
                    .is_some_and(|time| self.high_water_ms - *time >= 1000)
                {
                    self.emitted.pop_front();
                }
                if self.emitted.len() < 10 {
                    self.emitted.push_back(self.high_water_ms);
                    ConsoleDisposition::Emit
                } else {
                    self.counters.console_suppressed.increment();
                    self.suppression_summary(input.wall_clock_ms);
                    ConsoleDisposition::RateLimited
                }
            }
            _ => ConsoleDisposition::RetainedOnly,
        };
        Ok(RecordOutcome {
            seq,
            kind: RecordKind::Inserted,
            console,
        })
    }
}

#[cfg(test)]
mod tests {
    //! Exhaustion, saturated count evidence and exact JSONL byte boundary regressions.
    use std::io::Write;

    use super::*;

    /// Returns one stable coalescible editor error at monotonic millisecond one.
    fn error() -> EventInput<'static> {
        EventInput {
            category: Category::Editor,
            severity: Severity::Error,
            code: "editor.error",
            subject: None,
            message: "failure",
            elapsed_ms: 1,
            wall_clock_ms: None,
        }
    }

    #[test]
    /// Issuing the final sequence disables even repeats and clearing cannot restore issuance authority.
    fn event_final_sequence_stops_all_admission_and_survives_clear() {
        let mut store = EventStore {
            next_seq: Some(u64::MAX),
            ..EventStore::default()
        };
        let last = store.record(error()).unwrap().seq;
        assert_eq!(last.get(), u64::MAX);
        assert_eq!(store.status(), EventStatus::Exhausted);
        assert_eq!(store.record(error()), Err(EventError::SequenceExhausted));
        assert_eq!(store.counters().repeated.value, 0);
        store.clear();
        assert_eq!(store.record(error()), Err(EventError::SequenceExhausted));
    }

    #[test]
    /// Reaching the maximum remains exact; only a subsequent increment marks count saturation.
    fn event_counts_flag_actual_overflow_instead_of_claiming_exactness() {
        let mut count = DiagnosticCount {
            value: u64::MAX - 1,
            saturated: false,
        };
        count.increment();
        assert_eq!(count.value, u64::MAX);
        assert!(!count.saturated);
        count.increment();
        assert_eq!(count.value, u64::MAX);
        assert!(count.saturated);
        let mut store = EventStore::default();
        let seq = store.record(error()).unwrap().seq;
        store.records.front_mut().unwrap().count = count;
        store.counters.repeated = count;
        store.record(error()).unwrap();
        assert!(store.get(seq).unwrap().count.saturated);
        assert!(store.counters().repeated.saturated);
    }

    #[test]
    /// An exact 2 MiB write succeeds and a subsequent newline fails without extending the retained prefix.
    fn jsonl_writer_checks_exact_two_mib_boundary_before_appending() {
        let mut writer = JsonlWriter::new();
        writer.write_all(&vec![b'x'; 2 * 1024 * 1024]).unwrap();
        assert_eq!(writer.bytes.len(), 2 * 1024 * 1024);
        assert!(writer.write_all(b"\n").is_err());
        assert_eq!(writer.bytes.len(), 2 * 1024 * 1024);
        assert!(writer.too_large);
    }
}
