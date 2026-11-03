//! Bounded raw collection evidence; no transport permits, decoding qualification or I/O.
use super::Payload;
use crate::ble::canyonero::CleanupOutcome;
use crate::ble::canyonero::collection::{Error, EvidenceKind, ReadStatus, Submission};
use crate::ble::canyonero::reads::{ReadId, Reply};

/// Maximum retained complete UDS response including its three-byte service/DID prefix.
pub const RAW_MAX: usize = 67;

/// Small capture metadata; raw responses remain separately borrowed records.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Summary {
    /// Collection authorization observation in boot-relative milliseconds.
    pub started_at_ms: u64,
    /// Latest nonreversing update in boot-relative milliseconds.
    pub updated_at_ms: u64,
    /// Terminal observation in boot-relative milliseconds, absent while active.
    pub ended_at_ms: Option<u64>,
    /// Adapter-reported supply in millivolts, not ECU or battery-post voltage.
    pub adapter_mv: Option<u16>,
    /// Fresh OPEN handle observation, not a reusable saved capability.
    pub handle: Option<u8>,
    /// The fixed arbitration configuration received its expected acknowledgment.
    pub configured: bool,
    /// Matching final CLOSE received; physical cleanup is recorded separately.
    pub completed: bool,
    /// First terminal protocol failure, without erasing submission uncertainty.
    pub fault: Option<Error>,
    /// Actual final connection-cleanup outcome; Pending while unfinished.
    pub cleanup: CleanupOutcome,
    /// Last command submission including partial/unknown local write outcome.
    pub last_submission: Option<Submission>,
    /// Meaning of latest terminal raw prefix, absent before explicit publication.
    pub terminal_kind: Option<EvidenceKind>,
    /// Announced terminal prefix length in bytes, at most 245; not a full wire transcript.
    pub terminal_len: u16,
}

/// Borrowed row, independent of standard GATT read records; bytes never become live measurements.
#[derive(Clone, Copy, Debug)]
pub struct RowRef<'a> {
    /// Exact closed manifest entry; the capture also checks canonical ordering.
    pub id: ReadId,
    /// Attempt, local submission and reply times in boot-relative milliseconds.
    /// Absent slots contain zero; presence is independent of every stored value.
    times: [u64; 3],
    /// Low three bits record timestamp presence in `times` order; other bits are zero.
    time_flags: u8,
    /// Correlated raw result, absent before a complete transport response.
    pub status: Option<ReadStatus>,
    /// Accepted pending NRC78 count for this request, from zero through three.
    pub pending: u8,
    /// Full UDS response length announced before chunks, at most 67.
    pub declared_len: u8,
    /// Exact retained UDS prefix; shorter than declared_len explicitly means incomplete evidence.
    pub raw: &'a [u8],
}

/// One bounded row; compact timestamps use explicit presence flags instead of sentinel zero.
#[derive(Clone, Copy, Debug)]
pub struct Row {
    /// Attempt, submission and result times in boot-relative milliseconds.
    times: [u64; 3],
    /// Presence flags for attempt and submission; result presence follows status.
    flags: u8,
    /// Correlated result before raw publication, absent while pending/unattempted.
    status: Option<ReadStatus>,
    /// Accepted NRC78 count, no more than three.
    pending: u8,
    /// Announced complete application length, at most 67.
    declared_len: u8,
    /// Initialized raw prefix length, never larger than declared_len.
    raw_len: u8,
    /// Exact UDS response prefix, never sanitized or decoded in place.
    raw: [u8; RAW_MAX],
}
impl Row {
    /// Empty unattempted row, with no observable timestamp or value.
    const EMPTY: Self = Self {
        times: [0; 3],
        flags: 0,
        status: None,
        pending: 0,
        declared_len: 0,
        raw_len: 0,
        raw: [0; RAW_MAX],
    };

    /// Returns a small borrowed view labelled by owning manifest `id`; performs no read.
    #[inline(always)]
    pub fn view(
        &self,
        id: ReadId,
    ) -> RowRef<'_> {
        RowRef {
            id,
            times: self.times,
            time_flags: self.flags | (u8::from(self.status.is_some()) << 2),
            status: self.status,
            pending: self.pending,
            declared_len: self.declared_len,
            raw: &self.raw[..usize::from(self.raw_len)],
        }
    }
}

/// Small copied cross-core events; no whole evidence array is placed on an event queue.
#[derive(Clone, Copy, Debug)]
pub enum Update {
    /// Initialize exactly once before any collection command can be submitted.
    Start {
        /// Boot-relative authorization time in milliseconds.
        at_ms: u64,
    },
    /// Adapter-local supply reply, not a vehicle measurement.
    Voltage {
        /// Raw millivolts from the adapter.
        millivolts: u16,
        /// Boot-relative reply time.
        at_ms: u64,
    },
    /// Fresh channel-open acknowledgment.
    Opened {
        /// Fresh channel handle; saved handles grant no authority.
        handle: u8,
        /// Boot-relative reply time.
        at_ms: u64,
    },
    /// Exact fixed arbitration acknowledgment.
    Configured {
        /// Boot-relative acknowledgment time.
        at_ms: u64,
    },
    /// Writer is about to start one read; effects may remain unknown if interrupted.
    Attempt {
        /// Compiled read identity.
        id: ReadId,
        /// Boot-relative attempt time.
        at_ms: u64,
    },
    /// Full read SDU locally submitted, without claiming adapter/CAN delivery.
    Submitted {
        /// Compiled read identity.
        id: ReadId,
        /// Boot-relative local completion time.
        at_ms: u64,
    },
    /// Announce a terminal row before publishing its raw bytes in ordered chunks.
    RowHeader {
        /// Compiled read identity.
        id: ReadId,
        /// Boot-relative complete-response time.
        at_ms: u64,
        /// Raw protocol result, never decoded validity.
        status: ReadStatus,
        /// Complete application response length in 1..=67.
        length: u8,
        /// Accepted NRC78 count in 0..=3.
        pending: u8,
    },
    /// Append one exact contiguous raw response prefix fragment.
    Chunk {
        /// Compiled read identity.
        id: ReadId,
        /// Required current prefix length, in 0..=66.
        offset: u8,
        /// One through 64 bytes; truncated chunks are rejected.
        bytes: Payload<64>,
    },
    /// Announces the latest bounded envelope/diagnostic prefix before final cleanup publication.
    TerminalHeader {
        /// Prefix provenance; no interpretation as a complete transcript.
        kind: EvidenceKind,
        /// Prefix length in 0..=245, or0..=67 for a diagnostic payload.
        length: u16,
    },
    /// Appends exact contiguous terminal prefix bytes before End.
    TerminalChunk {
        /// Required current prefix offset in bytes.
        offset: u16,
        /// One through 64 untruncated bytes.
        bytes: Payload<64>,
    },
    /// Terminal collection/cleanup observation; never grants live measurement validity.
    End {
        /// Boot-relative final observation time.
        at_ms: u64,
        /// True only after matching final CLOSE acknowledgment.
        completed: bool,
        /// First terminal protocol failure, if any.
        fault: Option<Error>,
        /// Final cleanup must not remain Pending.
        cleanup: CleanupOutcome,
        /// Latest command submission, including unknown partial effects.
        last_submission: Option<Submission>,
    },
}

/// Optional evidence in one resident allocation; activation never moves the full row array.
/// Borrowing accessors preserve absent/present semantics for renderers and capture serializers.
pub struct Storage {
    /// Whether an explicit Start has initialized the resident evidence for this inspection.
    active: bool,
    /// Resident capacity with no initialized rows while inactive; never exposed until all28 exist.
    evidence: Evidence,
}

impl Default for Storage {
    /// Returns inactive initialized storage; runtime reuse should call clear rather than replace it.
    fn default() -> Self { Self::new() }
}

impl Storage {
    /// Returns inactive storage for initial construction; no collection is authorized or visible.
    pub const fn new() -> Self {
        Self {
            active: false,
            evidence: Evidence::vacant(),
        }
    }

    /// Returns whether an explicit collection Start has been accepted since the last clear.
    #[inline(always)]
    pub const fn is_some(&self) -> bool { self.active }

    /// Returns whether collection evidence is absent, including after clear.
    #[inline(always)]
    pub const fn is_none(&self) -> bool { !self.active }

    /// Returns borrowed evidence only while active; never copies the fixed row or terminal arrays.
    #[inline(always)]
    pub fn as_ref(&self) -> Option<&Evidence> { self.active.then_some(&self.evidence) }

    /// Returns exclusive borrowed evidence only while active; no implicit initialization occurs.
    #[inline(always)]
    pub fn as_mut(&mut self) -> Option<&mut Evidence> { self.active.then_some(&mut self.evidence) }

    /// Starts resident evidence at boot-relative `at_ms`; returns true after an in-place reset.
    /// Returns false without changing evidence when already active, even after terminal End.
    /// Inspection admission separately validates the timestamp against its own start.
    #[inline(always)]
    pub fn start(
        &mut self,
        at_ms: u64,
    ) -> bool {
        if self.active {
            return false;
        }
        self.evidence.reset(at_ms);
        self.active = true;
        true
    }

    /// Clears all retained collection metadata and bytes in place and makes evidence absent.
    /// Repeated calls remain empty; this does not authorize traffic or acknowledge cleanup.
    pub fn clear(&mut self) {
        self.active = false;
        self.evidence.clear();
    }
}

/// One explicit collection with a fixed row for every manifest entry, including unattempted reads.
pub struct Evidence {
    /// Small lifecycle and adapter-local metadata, independent of raw rows.
    pub summary: Summary,
    /// Capacity28, with exactly28 initialized rows whenever Evidence is publicly accessible.
    /// Only private inactive Storage leaves this empty; callers cannot change its length.
    rows: heapless::Vec<Row, 28>,
    /// Latest terminal envelope/payload prefix, shared across failure and cleanup outcomes.
    terminal: [u8; 245],
    /// Number of initialized terminal prefix bytes, never exceeding the announced length.
    terminal_len: u16,
}
impl Evidence {
    /// Returns an empty collection authorized at boot-relative `at_ms`, with all28 rows available.
    /// Runtime Storage activation instead initializes resident rows individually, avoiding this large value.
    pub const fn new(at_ms: u64) -> Self {
        Self {
            summary: Summary::new(at_ms),
            rows: heapless::Vec::from_array([Row::EMPTY; 28]),
            terminal: [0; 245],
            terminal_len: 0,
        }
    }

    /// Returns inaccessible inactive backing with reserved row capacity but no initialized rows.
    /// Only Storage uses this constructor; public Evidence construction always includes all28 rows.
    const fn vacant() -> Self {
        Self {
            summary: Summary::new(0),
            rows: heapless::Vec::new(),
            terminal: [0; 245],
            terminal_len: 0,
        }
    }

    /// Wipes initialized rows/terminal bytes and metadata, then leaves the row capacity empty.
    /// Storage hides this private state before calling; no public borrow can observe missing rows.
    #[inline(never)]
    fn clear(&mut self) {
        self.summary = Summary::new(0);
        self.rows.fill(Row::EMPTY);
        self.rows.clear();
        self.terminal.fill(0);
        self.terminal_len = 0;
    }

    /// Resets this resident buffer for boot-relative `at_ms`, clearing every row and terminal byte.
    /// Only small metadata or one row is assigned at a time; no whole Evidence temporary is formed.
    #[inline(never)]
    fn reset(
        &mut self,
        at_ms: u64,
    ) {
        self.summary = Summary::new(at_ms);
        self.rows.clear();
        self.rows.resize(28, Row::EMPTY).expect("fixed row capacity");
        self.terminal.fill(0);
        self.terminal_len = 0;
    }

    /// Returns the immutable row for closed manifest `id`; no data is copied.
    /// Public construction/Storage activation ensure all28 rows exist before access.
    #[inline(always)]
    pub fn row(
        &self,
        id: ReadId,
    ) -> RowRef<'_> {
        self.rows[id.index()].view(id)
    }

    /// Returns exact retained terminal bytes, possibly shorter than the declared prefix length.
    #[inline(always)]
    pub fn terminal(&self) -> &[u8] { &self.terminal[..usize::from(self.terminal_len)] }

    /// Returns clean collection completion only with all 28 positive, fully retained responses.
    /// Numeric schemas remain unqualified; negative replies, faults, incomplete terminal bytes
    /// and unavailable/failed physical cleanup never grant live green status.
    #[inline(always)]
    pub fn successful(&self) -> bool {
        self.summary.ended_at_ms.is_some()
            && self.summary.completed
            && self.summary.fault.is_none()
            && self.summary.cleanup == CleanupOutcome::Clean
            && usize::from(self.terminal_len) == usize::from(self.summary.terminal_len)
            && ReadId::ALL.iter().all(|&id| {
                let row = self.row(id);
                row.complete() && row.status == Some(ReadStatus::Positive)
            })
            && self.row(ReadId::ActiveSession).raw == [0x62, 0xF1, 0x86, 1]
    }

    /// Applies ordered bounded `update`; false rejects before mutation and never emits traffic.
    /// Returns true only after accepting the observation; false leaves evidence unchanged.
    // Permit scalar reducer state to share the caller's bounded scratch frame.
    #[inline(always)]
    pub fn apply(
        &mut self,
        update: Update,
    ) -> bool {
        if self.summary.ended_at_ms.is_some() {
            return false;
        }
        let at = match update {
            Update::Start { .. } => return false,
            Update::Voltage { at_ms, .. }
            | Update::Opened { at_ms, .. }
            | Update::Configured { at_ms }
            | Update::Attempt { at_ms, .. }
            | Update::Submitted { at_ms, .. }
            | Update::RowHeader { at_ms, .. }
            | Update::End { at_ms, .. } => at_ms,
            Update::Chunk { .. } | Update::TerminalHeader { .. } | Update::TerminalChunk { .. } => {
                self.summary.updated_at_ms
            }
        };
        if at < self.summary.updated_at_ms {
            return false;
        }
        match update {
            Update::Start { .. } => return false,
            Update::TerminalHeader { kind, length } => {
                if self.summary.terminal_kind.is_some()
                    || length > 245
                    || (kind == EvidenceKind::DiagnosticPayload && length > 67)
                {
                    return false;
                }
                self.summary.terminal_kind = Some(kind);
                self.summary.terminal_len = length;
            }
            Update::TerminalChunk { offset, ref bytes } => {
                let end = usize::from(offset) + usize::from(bytes.len);
                if self.summary.terminal_kind.is_none()
                    || offset != self.terminal_len
                    || bytes.len == 0
                    || bytes.len > 64
                    || bytes.truncated
                    || end > usize::from(self.summary.terminal_len)
                {
                    return false;
                }
                self.terminal[usize::from(offset)..end].copy_from_slice(bytes.as_slice());
                self.terminal_len = end as u16;
            }
            Update::Voltage { millivolts, .. } => {
                if self.summary.adapter_mv.is_some() || self.summary.handle.is_some() {
                    return false;
                }
                self.summary.adapter_mv = Some(millivolts);
            }
            Update::Opened { handle, .. } => {
                if self.summary.adapter_mv.is_none() || self.summary.handle.is_some() {
                    return false;
                }
                self.summary.handle = Some(handle);
            }
            Update::Configured { .. } => {
                if self.summary.handle.is_none() || self.summary.configured {
                    return false;
                }
                self.summary.configured = true;
            }
            Update::End {
                completed,
                fault,
                cleanup,
                last_submission,
                ..
            } => {
                if cleanup == CleanupOutcome::Pending {
                    return false;
                }
                let end = EndRef {
                    completed,
                    fault,
                    cleanup,
                    last_submission: last_submission.as_ref(),
                };
                if !self.summary.valid_state(at, at, Some(at), end) {
                    return false;
                }
                self.summary.updated_at_ms = at;
                self.summary.ended_at_ms = Some(at);
                self.summary.completed = completed;
                self.summary.fault = fault;
                self.summary.cleanup = cleanup;
                self.summary.last_submission = last_submission;
                return true;
            }
            Update::Attempt { id, .. }
            | Update::Submitted { id, .. }
            | Update::RowHeader { id, .. }
            | Update::Chunk { id, .. } => {
                if !self.summary.configured {
                    return false;
                }
                let index = id.index();
                if index > 0
                    && (self.row(ReadId::ActiveSession).raw != [0x62, 0xF1, 0x86, 1]
                        || !self.row(ReadId::ALL[index - 1]).complete())
                {
                    return false;
                }
                if self.rows.iter().skip(index + 1).any(|r| r.flags != 0) {
                    return false;
                }
                let row = &self.rows[index];
                if let Update::Chunk { offset, ref bytes, .. } = update {
                    let len = usize::from(bytes.len);
                    let end = usize::from(offset) + len;
                    if row.status.is_none()
                        || offset != row.raw_len
                        || len == 0
                        || len > 64
                        || bytes.truncated
                        || end > usize::from(row.declared_len)
                    {
                        return false;
                    }
                    // Only raw updates need an assembled prospective response. Keep the
                    // single existing classifier and never expose a rejected fragment.
                    let mut raw = [0; RAW_MAX];
                    let mut view = row.view(id);
                    raw[..view.raw.len()].copy_from_slice(view.raw);
                    raw[usize::from(offset)..end].copy_from_slice(bytes.as_slice());
                    view.raw = &raw[..end];
                    if !view.valid_at(&self.summary, at) {
                        return false;
                    }
                    let row = &mut self.rows[index];
                    row.raw[usize::from(offset)..end].copy_from_slice(bytes.as_slice());
                    row.raw_len = end as u8;
                } else {
                    // Metadata updates borrow raw bytes; no complete Row or raw array
                    // is copied while checking the prospective timestamps/status.
                    let mut view = row.view(id);
                    match update {
                        Update::Attempt { .. } => {
                            if row.flags != 0 {
                                return false;
                            }
                            view.time_flags = 1 | (u8::from(view.status.is_some()) << 2);
                            view.times[0] = at;
                        }
                        Update::Submitted { .. } => {
                            if row.flags != 1 || row.status.is_some() {
                                return false;
                            }
                            view.time_flags = 3;
                            view.times[1] = at;
                        }
                        Update::RowHeader {
                            status,
                            length,
                            pending,
                            ..
                        } => {
                            if row.flags != 3 || row.status.is_some() || !(1..=67).contains(&length) || pending > 3 {
                                return false;
                            }
                            view.status = Some(status);
                            view.time_flags = 7;
                            view.times[2] = at;
                            view.declared_len = length;
                            view.pending = pending;
                        }
                        _ => return false,
                    }
                    if !view.valid_at(&self.summary, at) {
                        return false;
                    }
                    let RowRef {
                        times,
                        time_flags,
                        status,
                        pending,
                        declared_len,
                        ..
                    } = view;
                    let row = &mut self.rows[index];
                    row.times = times;
                    row.flags = time_flags & 3;
                    row.status = status;
                    row.pending = pending;
                    row.declared_len = declared_len;
                }
            }
        }
        self.summary.updated_at_ms = at;
        true
    }
}

/// Borrowed prospective terminal fields; setup/raw metadata stays in its resident Summary.
/// Absent submission remains absent and never implies that no bytes were sent.
struct EndRef<'a> {
    /// Matching CLOSE result, checked against the resident configuration observation.
    completed: bool,
    /// First terminal failure, or absent while no failure was recorded.
    fault: Option<Error>,
    /// Actual cleanup observation; Pending is valid only before a terminal timestamp.
    cleanup: CleanupOutcome,
    /// Optional borrowed latest command with unchanged unknown-send semantics.
    last_submission: Option<&'a Submission>,
}

impl Summary {
    /// Returns empty metadata at boot-relative `at_ms`; no timestamps imply transport submission.
    pub(super) const fn new(at_ms: u64) -> Self {
        Self {
            started_at_ms: at_ms,
            updated_at_ms: at_ms,
            ended_at_ms: None,
            adapter_mv: None,
            handle: None,
            configured: false,
            completed: false,
            fault: None,
            cleanup: CleanupOutcome::Pending,
            last_submission: None,
            terminal_kind: None,
            terminal_len: 0,
        }
    }

    /// Returns whether summary timestamps, setup order and final submission are coherent at `captured_at_ms`.
    /// A true result preserves uncertainty; it does not establish default ECU session or clean completion.
    pub fn valid(
        &self,
        captured_at_ms: u64,
    ) -> bool {
        self.valid_state(
            captured_at_ms,
            self.updated_at_ms,
            self.ended_at_ms,
            EndRef {
                completed: self.completed,
                fault: self.fault,
                cleanup: self.cleanup,
                last_submission: self.last_submission.as_ref(),
            },
        )
    }

    /// Checks resident setup/raw bounds with prospective `updated_at_ms`, `ended_at_ms`
    /// and borrowed terminal `end`, all against boot-relative `captured_at_ms`.
    /// Shared by stored-summary admission and End; returns false without changing evidence.
    fn valid_state(
        &self,
        captured_at_ms: u64,
        updated_at_ms: u64,
        ended_at_ms: Option<u64>,
        end: EndRef<'_>,
    ) -> bool {
        if self.started_at_ms > updated_at_ms
            || updated_at_ms > captured_at_ms
            || ended_at_ms.is_some_and(|t| t != updated_at_ms)
            || (ended_at_ms.is_none()
                && (end.cleanup != CleanupOutcome::Pending
                    || end.completed
                    || end.fault.is_some()
                    || end.last_submission.is_some()))
            || (ended_at_ms.is_some() && end.cleanup == CleanupOutcome::Pending)
            || (end.completed && !self.configured)
            || self.terminal_len > 245
            || (self.terminal_kind.is_none() && self.terminal_len != 0)
            || (self.terminal_kind == Some(EvidenceKind::DiagnosticPayload) && self.terminal_len > 67)
            || (self.configured && self.handle.is_none())
            || (self.handle.is_some() && self.adapter_mv.is_none())
        {
            return false;
        }
        if let Some(s) = end.last_submission {
            use crate::ble::canyonero::collection::SendState;
            if s.attempted_at_ms
                .is_some_and(|t| t < self.started_at_ms || t > updated_at_ms)
                || s.completed_at_ms
                    .is_some_and(|t| s.attempted_at_ms.is_none_or(|a| t < a) || t > updated_at_ms)
            {
                return false;
            }
            if !match s.state {
                SendState::NotAttempted => s.attempted_at_ms.is_none() && s.completed_at_ms.is_none(),
                SendState::InFlightUnknown => s.attempted_at_ms.is_some() && s.completed_at_ms.is_none(),
                SendState::LocallyComplete => s.attempted_at_ms.is_some() && s.completed_at_ms.is_some(),
            } {
                return false;
            }
        }
        true
    }
}

impl<'a> RowRef<'a> {
    /// Returns an unvalidated view of `id` and borrowed `raw` bytes without retaining parser storage.
    /// `times` independently records attempt, local submission and reply boot-relative milliseconds;
    /// None differs from every Some value, including zero and u64::MAX. `status`, `pending` and
    /// `declared_len` retain the supplied reply result, NRC78 count and announced byte length.
    /// No bounds or chronology are inferred here; [`Self::valid`] performs semantic admission.
    #[inline(always)]
    pub fn new(
        id: ReadId,
        times: [Option<u64>; 3],
        status: Option<ReadStatus>,
        pending: u8,
        declared_len: u8,
        raw: &'a [u8],
    ) -> Self {
        Self {
            id,
            time_flags: u8::from(times[0].is_some())
                | (u8::from(times[1].is_some()) << 1)
                | (u8::from(times[2].is_some()) << 2),
            times: times.map(|time| time.unwrap_or(0)),
            status,
            pending,
            declared_len,
            raw,
        }
    }

    /// Returns attempt boot milliseconds, or None when no write began; Some(0) is present.
    #[inline(always)]
    pub fn attempted_at_ms(&self) -> Option<u64> { (self.time_flags & 1 != 0).then_some(self.times[0]) }

    /// Returns local submission boot milliseconds, or None when unconfirmed; not CAN acknowledgment.
    #[inline(always)]
    pub fn submitted_at_ms(&self) -> Option<u64> { (self.time_flags & 2 != 0).then_some(self.times[1]) }

    /// Returns reply boot milliseconds, or None before a result; raw chunks may still be incomplete.
    #[inline(always)]
    pub fn completed_at_ms(&self) -> Option<u64> { (self.time_flags & 4 != 0).then_some(self.times[2]) }

    /// Returns true only when a result exists and all declared response bytes were retained.
    /// This is evidence completeness, not numeric validity or vehicle qualification.
    #[inline(always)]
    pub fn complete(&self) -> bool { self.status.is_some() && self.raw.len() == usize::from(self.declared_len) }

    /// Checks row timing and raw-result consistency against `summary`; no bytes are changed.
    /// Returns true for a consistent complete or interrupted row, not vehicle qualification;
    /// false indicates inconsistent bounds, ordering, timestamps or a complete reply/status mismatch.
    pub fn valid(
        &self,
        summary: &Summary,
    ) -> bool {
        self.valid_at(summary, summary.updated_at_ms)
    }

    /// Validates this row against borrowed `summary` using prospective `updated_at_ms` boot time.
    /// Returns the same consistency result as valid with that update time, without copying or
    /// changing metadata; the caller commits the row and summary timestamp only after true.
    fn valid_at(
        &self,
        summary: &Summary,
        updated_at_ms: u64,
    ) -> bool {
        if self.raw.len() > usize::from(self.declared_len)
            || usize::from(self.declared_len) > RAW_MAX
            || self.pending > 3
        {
            return false;
        }
        let Some(attempt) = self.attempted_at_ms() else {
            return self.submitted_at_ms().is_none()
                && self.completed_at_ms().is_none()
                && self.status.is_none()
                && self.raw.is_empty()
                && self.declared_len == 0
                && self.pending == 0;
        };
        if !summary.configured || attempt < summary.started_at_ms || attempt > updated_at_ms {
            return false;
        }
        if self.submitted_at_ms().is_some_and(|t| t < attempt || t > updated_at_ms) {
            return false;
        }
        if self.status.is_none() {
            return self.completed_at_ms().is_none()
                && self.raw.is_empty()
                && self.declared_len == 0
                && self.pending == 0;
        }
        let Some(completed) = self.completed_at_ms() else {
            return false;
        };
        if self.submitted_at_ms().is_none_or(|t| completed < t) || completed > updated_at_ms || self.declared_len == 0 {
            return false;
        }
        if !self.complete() {
            return true;
        }
        match (self.status, self.id.classify(self.raw)) {
            (Some(ReadStatus::Positive), Ok(Reply::Positive { .. })) => true,
            (Some(ReadStatus::Negative(nrc)), Ok(Reply::Negative(actual))) => nrc == actual,
            (Some(ReadStatus::SessionNotDefault), Ok(Reply::Positive { data, .. })) => {
                self.id == ReadId::ActiveSession && data != [1]
            }
            (Some(ReadStatus::SessionNotDefault), Ok(Reply::Negative(_))) => self.id == ReadId::ActiveSession,
            _ => false,
        }
    }
}

#[cfg(test)]
/// Ordered publication and uncertain-send regressions without hardware or filesystem effects.
mod tests {
    use super::*;

    /// Rejected terminal updates must not advance time, hide raw prefixes or partially finish evidence.
    #[test]
    fn invalid_end_keeps_summary_and_terminal_prefix_atomic() {
        use crate::ble::canyonero::collection::{Command, SendState};
        let mut evidence = Evidence::new(10);
        assert!(evidence.apply(Update::TerminalHeader {
            kind: EvidenceKind::DiagnosticPayload,
            length: 4
        }));
        assert!(evidence.apply(Update::TerminalChunk {
            offset: 0,
            bytes: Payload::new(&[0x62, 0xF1])
        }));
        let before = evidence.summary;
        for (completed, cleanup, last_submission) in [
            (false, CleanupOutcome::Pending, None),
            (true, CleanupOutcome::Clean, None),
            (
                false,
                CleanupOutcome::Clean,
                Some(Submission {
                    command: Command::Voltage,
                    state: SendState::LocallyComplete,
                    attempted_at_ms: None,
                    completed_at_ms: Some(11),
                }),
            ),
            (
                false,
                CleanupOutcome::Clean,
                Some(Submission {
                    command: Command::Voltage,
                    state: SendState::InFlightUnknown,
                    attempted_at_ms: Some(12),
                    completed_at_ms: None,
                }),
            ),
        ] {
            assert!(!evidence.apply(Update::End {
                at_ms: 11,
                completed,
                fault: None,
                cleanup,
                last_submission
            }));
            assert_eq!(evidence.summary, before);
            assert_eq!(evidence.terminal(), &[0x62, 0xF1]);
        }
        assert!(evidence.apply(Update::End {
            at_ms: 11,
            completed: false,
            fault: Some(Error::Cancelled),
            cleanup: CleanupOutcome::Clean,
            last_submission: None
        }));
        assert_eq!(evidence.summary.ended_at_ms, Some(11));
        assert_eq!(evidence.summary.updated_at_ms, 11);
        assert_eq!(evidence.terminal(), &[0x62, 0xF1]);
    }

    /// A metadata rejection cannot advance timestamps or disturb a previously accepted partial row.
    #[test]
    fn invalid_row_metadata_keeps_the_complete_resident_row() {
        let mut evidence = Evidence::new(0);
        let id = ReadId::ActiveSession;
        for update in [
            Update::Voltage {
                millivolts: 12000,
                at_ms: 1,
            },
            Update::Opened { handle: 1, at_ms: 2 },
            Update::Configured { at_ms: 3 },
            Update::Attempt { id, at_ms: 4 },
            Update::Submitted { id, at_ms: 5 },
            Update::RowHeader {
                id,
                at_ms: 6,
                status: ReadStatus::Positive,
                length: 4,
                pending: 0,
            },
            Update::Chunk {
                id,
                offset: 0,
                bytes: Payload::new(&[0x62, 0xF1]),
            },
        ] {
            assert!(evidence.apply(update));
        }
        let before = evidence.rows[0];
        let summary = evidence.summary;
        for update in [
            Update::Attempt { id, at_ms: 7 },
            Update::Submitted { id, at_ms: 7 },
            Update::RowHeader {
                id,
                at_ms: 7,
                status: ReadStatus::Negative(0x31),
                length: 3,
                pending: 3,
            },
        ] {
            assert!(!evidence.apply(update));
            assert_eq!(evidence.summary, summary);
            let row = &evidence.rows[0];
            assert_eq!(
                (
                    row.times,
                    row.flags,
                    row.status,
                    row.pending,
                    row.declared_len,
                    row.raw_len
                ),
                (
                    before.times,
                    before.flags,
                    before.status,
                    before.pending,
                    before.declared_len,
                    before.raw_len
                )
            );
            assert_eq!(row.raw, before.raw);
        }
    }

    /// Every legal split preserves exact maximum responses; invalid final classification leaves the prefix intact.
    #[test]
    fn chunk_splits_keep_classifier_results_and_rejected_prefixes() {
        let mut maximum = [0xAB; RAW_MAX];
        maximum[..3].copy_from_slice(&[0x62, 0xF1, 0x86]);
        let cases: &[(&[u8], ReadStatus, bool)] = &[
            (&maximum, ReadStatus::Positive, true),
            (&[0x62, 0xF1, 0x86, 1], ReadStatus::Positive, true),
            (&[0x62, 0xF1, 0x86, 2], ReadStatus::SessionNotDefault, true),
            (&[0x7F, 0x22, 0x31], ReadStatus::Negative(0x31), true),
            (&[0x7F, 0x22, 0x31], ReadStatus::SessionNotDefault, true),
            (&[0x62, 0xF1, 0x86, 1], ReadStatus::SessionNotDefault, false),
            (&[0x62, 0xF1, 0x87, 1], ReadStatus::Positive, false),
            (&[0x7F, 0x22, 0x31], ReadStatus::Negative(0x32), false),
            (&[0x7F, 0x22, 0x78], ReadStatus::Negative(0x78), false),
        ];
        let id = ReadId::ActiveSession;
        for &(response, status, accepted) in cases {
            for split in response.len().saturating_sub(64)..response.len().min(65) {
                let mut evidence = Evidence::new(0);
                for update in [
                    Update::Voltage {
                        millivolts: 12000,
                        at_ms: 1,
                    },
                    Update::Opened { handle: 0, at_ms: 2 },
                    Update::Configured { at_ms: 3 },
                    Update::Attempt { id, at_ms: 4 },
                    Update::Submitted { id, at_ms: 5 },
                    Update::RowHeader {
                        id,
                        at_ms: 6,
                        status,
                        length: response.len() as u8,
                        pending: 3,
                    },
                ] {
                    assert!(evidence.apply(update));
                }
                if split != 0 {
                    assert!(evidence.apply(Update::Chunk {
                        id,
                        offset: 0,
                        bytes: Payload::new(&response[..split])
                    }));
                }
                let summary = evidence.summary;
                let before = evidence.rows[0];
                assert_eq!(
                    evidence.apply(Update::Chunk {
                        id,
                        offset: split as u8,
                        bytes: Payload::new(&response[split..])
                    }),
                    accepted,
                    "{response:?} split {split}"
                );
                if accepted {
                    assert_eq!(evidence.row(id).raw, response);
                    assert!(evidence.row(id).valid(&evidence.summary));
                } else {
                    assert_eq!(evidence.row(id).raw, &response[..split]);
                    assert_eq!(evidence.rows[0].raw, before.raw);
                    assert_eq!(evidence.rows[0].raw_len, before.raw_len);
                    assert_eq!(evidence.summary, summary);
                }
            }
        }
    }

    /// Every timestamp has independent presence, including real zero and maximum boot time.
    #[test]
    fn compact_row_timestamps_preserve_all_presence_patterns_and_extrema() {
        for mask in 0u8..8 {
            for value in [0, u64::MAX] {
                let expected = [1, 2, 4].map(|bit| (mask & bit != 0).then_some(value));
                let row = RowRef::new(ReadId::ActiveSession, expected, None, 0, 0, &[]);
                assert_eq!(
                    [row.attempted_at_ms(), row.submitted_at_ms(), row.completed_at_ms()],
                    expected
                );
            }
        }
        assert!(core::mem::size_of::<RowRef<'_>>() <= 48);
    }

    /// Prospective validation must use the incoming time without modifying the retained summary.
    #[test]
    fn prospective_row_time_does_not_require_a_summary_copy() {
        let mut evidence = Evidence::new(0);
        evidence.summary.configured = true;
        let prior = evidence.summary;
        let row = RowRef::new(ReadId::ActiveSession, [Some(2), None, None], None, 0, 0, &[]);
        assert!(!row.valid(&evidence.summary));
        assert!(!row.valid_at(&evidence.summary, 1));
        assert!(row.valid_at(&evidence.summary, 2));
        assert_eq!(evidence.summary, prior);
    }

    /// A final chunk with a foreign DID must not overwrite the accepted raw prefix or any row metadata.
    #[test]
    fn invalid_completing_chunk_keeps_full_prior_row_and_summary() {
        let mut evidence = Evidence::new(0);
        let id = ReadId::ActiveSession;
        for update in [
            Update::Voltage {
                millivolts: 12000,
                at_ms: 1,
            },
            Update::Opened { handle: 4, at_ms: 2 },
            Update::Configured { at_ms: 3 },
            Update::Attempt { id, at_ms: 4 },
            Update::Submitted { id, at_ms: 5 },
            Update::RowHeader {
                id,
                at_ms: 6,
                status: ReadStatus::Positive,
                length: 4,
                pending: 2,
            },
            Update::Chunk {
                id,
                offset: 0,
                bytes: Payload::new(&[0x62, 0xF1]),
            },
        ] {
            assert!(evidence.apply(update));
        }
        let prior = evidence.rows[id.index()];
        let summary = evidence.summary;
        assert!(!evidence.apply(Update::Chunk {
            id,
            offset: 2,
            bytes: Payload::new(&[0x87, 1])
        }));
        let after = &evidence.rows[id.index()];
        assert_eq!(after.times, prior.times);
        assert_eq!(after.flags, prior.flags);
        assert_eq!(after.status, prior.status);
        assert_eq!(after.pending, prior.pending);
        assert_eq!(after.declared_len, prior.declared_len);
        assert_eq!(after.raw_len, prior.raw_len);
        assert_eq!(after.raw, prior.raw);
        assert_eq!(evidence.summary, summary);
        assert!(evidence.apply(Update::Chunk {
            id,
            offset: 2,
            bytes: Payload::new(&[0x86, 1])
        }));
        assert_eq!(evidence.row(id).raw, &[0x62, 0xF1, 0x86, 1]);
    }

    /// Inactive backing has no initialized rows; public construction and every activation expose all28.
    /// A repeated Start preserves existing evidence, and clear removes access before another start.
    #[test]
    fn inactive_backing_initializes_rows_only_on_start() {
        let mut storage = Storage::new();
        assert!(storage.evidence.rows.is_empty());
        assert!(storage.as_ref().is_none() && storage.as_mut().is_none());
        storage.clear();
        assert!(storage.evidence.rows.is_empty());
        for at_ms in [0, 10] {
            assert!(storage.start(at_ms));
            assert_eq!(storage.evidence.rows.len(), ReadId::ALL.len());
            assert!(!storage.start(at_ms + 1));
            let evidence = storage.as_ref().unwrap();
            assert_eq!(evidence.summary.started_at_ms, at_ms);
            for id in ReadId::ALL {
                let row = evidence.row(id);
                assert!(row.raw.is_empty() && row.status.is_none());
                assert!(row.valid(&evidence.summary));
            }
            storage.clear();
            assert!(storage.evidence.rows.is_empty() && storage.as_ref().is_none());
        }
        /// Const-constructed public evidence at20ms must expose all28 initialized rows.
        const PUBLIC: Evidence = Evidence::new(20);
        assert_eq!(PUBLIC.rows.len(), ReadId::ALL.len());
        for id in ReadId::ALL {
            assert!(PUBLIC.row(id).valid(&PUBLIC.summary));
        }
    }

    #[test]
    /// Rejects out-of-order attempts and incomplete prefix reuse without losing accepted bytes.
    fn row_publication_is_ordered_and_preserves_incomplete_evidence() {
        let mut e = Evidence::new(0);
        assert!(e.apply(Update::Voltage {
            millivolts: 0,
            at_ms: 1
        }));
        assert!(e.apply(Update::Opened { handle: 7, at_ms: 2 }));
        assert!(e.apply(Update::Configured { at_ms: 3 }));
        assert!(!e.apply(Update::Attempt {
            id: ReadId::OdxIdentifier,
            at_ms: 4
        }));
        let id = ReadId::ActiveSession;
        assert!(e.apply(Update::Attempt { id, at_ms: 4 }));
        assert!(e.apply(Update::Submitted { id, at_ms: 5 }));
        assert!(e.apply(Update::RowHeader {
            id,
            at_ms: 6,
            status: ReadStatus::Positive,
            length: 4,
            pending: 1
        }));
        assert!(e.apply(Update::Chunk {
            id,
            offset: 0,
            bytes: Payload::new(&[0x62, 0xF1])
        }));
        assert!(!e.row(id).complete());
        assert!(!e.apply(Update::Attempt {
            id: ReadId::OdxIdentifier,
            at_ms: 7
        }));
        assert!(!e.apply(Update::Chunk {
            id,
            offset: 1,
            bytes: Payload::new(&[0x86, 1])
        }));
        assert!(e.apply(Update::Chunk {
            id,
            offset: 2,
            bytes: Payload::new(&[0x86, 1])
        }));
        assert!(e.row(id).complete());
        assert!(e.row(id).valid(&e.summary));
        assert!(e.apply(Update::Attempt {
            id: ReadId::OdxIdentifier,
            at_ms: 7
        }));
        assert_eq!(e.row(id).raw, &[0x62, 0xF1, 0x86, 1]);
    }
    #[test]
    /// An interrupted local write remains attempted with no invented completion or raw reply.
    fn interrupted_submission_and_nondefault_session_never_advance() {
        let mut e = Evidence::new(0);
        assert!(e.apply(Update::Voltage {
            millivolts: 12000,
            at_ms: 1
        }));
        assert!(e.apply(Update::Opened { handle: 0, at_ms: 2 }));
        assert!(e.apply(Update::Configured { at_ms: 3 }));
        let id = ReadId::ActiveSession;
        assert!(e.apply(Update::Attempt { id, at_ms: 4 }));
        assert!(e.apply(Update::End {
            at_ms: 5,
            completed: false,
            fault: Some(Error::Cancelled),
            cleanup: CleanupOutcome::Clean,
            last_submission: None
        }));
        assert_eq!(e.row(id).attempted_at_ms(), Some(4));
        assert_eq!(e.row(id).submitted_at_ms(), None);
        assert!(!e.row(id).complete());
        assert!(!e.apply(Update::Submitted { id, at_ms: 6 }));
        assert!(e.row(id).valid(&e.summary));
    }
}
