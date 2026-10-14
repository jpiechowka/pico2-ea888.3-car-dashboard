//! Bounded, local Studio diagnostics. Helper job logs have their own store.
use std::collections::VecDeque;
use std::io::{self, Write};

use serde::Serialize;

const EVENT_CAPACITY: usize = 512;
const COALESCER_CAPACITY: usize = 64;
const JSONL_CAPACITY: usize = 2 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Category {
    Editor,
    Validation,
    Storage,
    ImportExport,
    Simulation,
    Rendering,
    HostTools,
    Device,
}

impl Category {
    /// Human UI text; Serialize retains the stable diagnostic tags above.
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
pub enum Severity {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

/// Application-owned tag. Construction rejects user text and oversized keys.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct EventCode(&'static str);

impl EventCode {
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

    pub fn get(self) -> &'static str { self.0 }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct EventSeq(u64);
impl EventSeq {
    pub fn get(self) -> u64 { self.0 }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct DiagnosticCount {
    pub value: u64,
    /// Set only after the actual count can no longer be represented exactly.
    pub saturated: bool,
}
impl DiagnosticCount {
    pub fn increment(&mut self) {
        if let Some(value) = self.value.checked_add(1) {
            self.value = value;
        } else {
            self.saturated = true;
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct EventCounters {
    pub inserted: DiagnosticCount,
    pub dropped: DiagnosticCount,
    pub repeated: DiagnosticCount,
    pub console_suppressed: DiagnosticCount,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum EventStatus {
    Accepting,
    Exhausted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventError {
    InvalidCode,
    SequenceExhausted,
}

pub struct EventInput<'a> {
    pub category: Category,
    pub severity: Severity,
    pub code: &'static str,
    pub subject: Option<&'a str>,
    pub message: &'a str,
    pub elapsed_ms: u64,
    /// Unix milliseconds, serialized by Rust; never used for admission/order.
    pub wall_clock_ms: Option<i64>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Event {
    pub seq: EventSeq,
    pub category: Category,
    pub severity: Severity,
    pub code: EventCode,
    pub subject: Option<String>,
    pub subject_truncated: bool,
    pub message: String,
    pub message_truncated: bool,
    pub elapsed_ms: u64,
    pub last_elapsed_ms: u64,
    pub wall_clock_ms: Option<i64>,
    pub last_wall_clock_ms: Option<i64>,
    pub count: DiagnosticCount,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecordKind {
    Inserted,
    Repeated,
}

/// The sole console policy decision. Adapters emit only Emit outcomes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConsoleDisposition {
    Emit,
    RetainedOnly,
    Coalesced,
    RateLimited,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RecordOutcome {
    pub seq: EventSeq,
    pub kind: RecordKind,
    pub console: ConsoleDisposition,
}

pub trait EventSink {
    fn record(
        &mut self,
        input: EventInput<'_>,
    ) -> Result<RecordOutcome, EventError>;
}

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

pub struct EventFilter {
    category: Option<Category>,
    minimum: Severity,
    query: String,
    query_truncated: bool,
    limit: usize,
}

impl Default for EventFilter {
    fn default() -> Self { Self::new("") }
}
impl EventFilter {
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

    pub fn minimum(
        mut self,
        minimum: Severity,
    ) -> Self {
        self.minimum = minimum;
        self
    }

    pub fn category(
        mut self,
        category: Category,
    ) -> Self {
        self.category = Some(category);
        self
    }

    pub fn limit(
        mut self,
        limit: usize,
    ) -> Self {
        self.limit = limit.min(EVENT_CAPACITY);
        self
    }

    pub fn query_truncated(&self) -> bool { self.query_truncated }

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
    records: Vec<Event>,
    pub status: EventStatus,
    pub counters: EventCounters,
    /// Oldest/newest of the whole retained store, independent of the filter.
    pub oldest: Option<EventSeq>,
    pub newest: Option<EventSeq>,
    pub retained: usize,
    pub matching: usize,
    pub omitted: usize,
}

impl EventSnapshot {
    pub fn records(&self) -> &[Event] { &self.records }

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
pub enum JsonlError {
    TooLarge,
    Serialization,
}

struct JsonlWriter {
    bytes: Vec<u8>,
    too_large: bool,
}
impl JsonlWriter {
    fn new() -> Self {
        Self {
            bytes: Vec::with_capacity(JSONL_CAPACITY),
            too_large: false,
        }
    }
}
impl Write for JsonlWriter {
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

    fn flush(&mut self) -> io::Result<()> { Ok(()) }
}

/// One store also owns the coalescing and rolling console policy.
pub struct EventStore {
    records: VecDeque<Event>,
    keys: VecDeque<EventSeq>,
    next_seq: Option<u64>,
    counters: EventCounters,
    high_water_ms: u64,
    debug_console: bool,
    emitted: VecDeque<u64>,
    rate_summary: Option<EventSeq>,
}

impl Default for EventStore {
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
    pub fn status(&self) -> EventStatus {
        if self.next_seq.is_some() {
            EventStatus::Accepting
        } else {
            EventStatus::Exhausted
        }
    }

    pub fn counters(&self) -> EventCounters { self.counters }

    pub fn set_debug_console(
        &mut self,
        enabled: bool,
    ) {
        self.debug_console = enabled;
    }

    pub fn get(
        &self,
        seq: EventSeq,
    ) -> Option<&Event> {
        self.records.iter().find(|e| e.seq == seq)
    }

    pub fn contains(
        &self,
        seq: EventSeq,
    ) -> bool {
        self.get(seq).is_some()
    }

    pub fn clear(&mut self) {
        self.records.clear();
        self.keys.clear();
        self.rate_summary = None;
    }

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
    use std::io::Write;

    use super::*;

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
    fn jsonl_writer_checks_exact_two_mib_boundary_before_appending() {
        let mut writer = JsonlWriter::new();
        writer.write_all(&vec![b'x'; 2 * 1024 * 1024]).unwrap();
        assert_eq!(writer.bytes.len(), 2 * 1024 * 1024);
        assert!(writer.write_all(b"\n").is_err());
        assert_eq!(writer.bytes.len(), 2 * 1024 * 1024);
        assert!(writer.too_large);
    }
}
