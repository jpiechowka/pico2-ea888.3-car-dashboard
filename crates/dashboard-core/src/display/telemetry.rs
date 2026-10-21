//! Software-observed transfer timing. This does not measure panel scanout.
use super::framebuffer::FRAME_BYTES;
use super::pipeline::FrameToken;

/// Rejected observation category; unavailable cycle metadata alone is not an error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TelemetryError {
    /// Timestamp order violates the required causal/monotonic ordering.
    Clock,
    /// Identity, payload width, replayed input or blocked-time metadata is invalid.
    Metadata,
    /// A checked counter/sum would overflow or completion count appears reset.
    Counter,
}

/// Immutable render observations in one monotonic microsecond timebase.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RenderTrace {
    /// Render start; sample and consumed input must not be later than this time.
    pub start_us: u64,
    /// Render end, no earlier than start.
    pub end_us: u64,
    /// Submission time, no earlier than render end.
    pub queued_us: u64,
    /// Measured wait portion between render end and queue, in microseconds.
    pub blocked_us: u64,
    /// Source sample timestamp; `None` denotes unavailable sample-age evidence.
    pub sample_us: Option<u64>,
    /// Measured active render cycles; `None` is unavailable, not a zero measurement.
    pub active_cycles: Option<u64>,
    /// Qualified consumed input batch represented by this frame, if any.
    pub input: Option<crate::input_trace::InputTrace>,
}

impl RenderTrace {
    /// Returns a zero-duration render at monotonic `at_us`, with optional sample,
    /// cycle and input observations absent; useful for synthetic/boot frames.
    pub const fn instant(at_us: u64) -> Self {
        Self {
            start_us: at_us,
            end_us: at_us,
            queued_us: at_us,
            blocked_us: 0,
            sample_us: None,
            active_cycles: None,
            input: None,
        }
    }

    /// Returns success for ordered render/sample/input times and a blocked
    /// interval fitting queue delay; otherwise `Clock` or `Metadata`. Pure check.
    pub fn validate(&self) -> Result<(), TelemetryError> {
        if self.start_us > self.end_us
            || self.end_us > self.queued_us
            || self.sample_us.is_some_and(|at| at > self.start_us)
            || self.input.is_some_and(|trace| !trace.valid_before(self.start_us))
        {
            return Err(TelemetryError::Clock);
        }
        if self.blocked_us > self.queued_us - self.end_us {
            return Err(TelemetryError::Metadata);
        }
        Ok(())
    }
}

/// Validated successful software transfer observation; not physical scanout timing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameTrace {
    /// Nonzero render identity associated with this transfer.
    token: FrameToken,
    /// Render start in monotonic microseconds.
    render_start_us: u64,
    /// Render completion in the same clock domain.
    render_end_us: u64,
    /// Submission time, after render completion and before payload start.
    queued_us: u64,
    /// Observed blocking after render, bounded by submission delay.
    blocked_us: u64,
    /// Optional source sample time no later than render start.
    sample_us: Option<u64>,
    // Keep availability independent of every u64 value. Flattening this private
    // storage lets the flag use existing padding in each of the 64 ring entries.
    /// Cycle payload; ignored when `cycles_present` is false, zero remains valid.
    active_cycles: u64,
    /// Availability flag independent of every possible cycle count.
    cycles_present: bool,
    /// Optional input batch with validated identity/timestamps.
    input: Option<crate::input_trace::InputTrace>,
    /// Software time after first polling the payload future, in microseconds.
    started_us: u64,
    /// Software observation after HAL transfer return, in microseconds.
    completed_us: u64,
    /// Positive even RGB565 payload byte count, at most one complete frame.
    payload_bytes: u32,
}

impl FrameTrace {
    /// Returns a validated completed trace for nonzero `token`, `render`,
    /// payload-poll time `started_us`, HAL-return time `completed_us` (microseconds),
    /// and even `payload_bytes` in 2..=153600. Returns `Clock` for reversed timing
    /// or `Metadata` for invalid size/identity/blocking; no hardware is inspected.
    pub fn finish(
        token: FrameToken,
        render: RenderTrace,
        started_us: u64,
        completed_us: u64,
        payload_bytes: u32,
    ) -> Result<Self, TelemetryError> {
        render.validate()?;
        if render.queued_us > started_us || started_us > completed_us {
            return Err(TelemetryError::Clock);
        }
        if token.sequence == 0
            || payload_bytes == 0
            || payload_bytes as usize > FRAME_BYTES
            || !payload_bytes.is_multiple_of(2)
        {
            return Err(TelemetryError::Metadata);
        }
        Ok(Self {
            token,
            render_start_us: render.start_us,
            render_end_us: render.end_us,
            queued_us: render.queued_us,
            blocked_us: render.blocked_us,
            sample_us: render.sample_us,
            active_cycles: render.active_cycles.unwrap_or(0),
            cycles_present: render.active_cycles.is_some(),
            input: render.input,
            started_us,
            completed_us,
            payload_bytes,
        })
    }

    /// Returns the completed render's immutable buffer/generation identity.
    pub fn token(&self) -> FrameToken { self.token }

    /// Returns original render metadata, preserving unavailable cycle observations.
    pub fn render(&self) -> RenderTrace {
        RenderTrace {
            start_us: self.render_start_us,
            end_us: self.render_end_us,
            queued_us: self.queued_us,
            blocked_us: self.blocked_us,
            sample_us: self.sample_us,
            active_cycles: self.cycles_present.then_some(self.active_cycles),
            input: self.input,
        }
    }

    /// Returns the software payload-start observation in monotonic microseconds.
    pub fn started_us(&self) -> u64 { self.started_us }

    /// Returns the software HAL-completion observation in monotonic microseconds.
    pub fn completed_us(&self) -> u64 { self.completed_us }

    /// Returns RGB565 payload bytes, excluding command/window overhead.
    pub fn payload_bytes(&self) -> u32 { self.payload_bytes }

    /// Returns elapsed render microseconds, including any intervening waits/interrupts.
    pub fn render_us(&self) -> u64 { self.render_end_us - self.render_start_us }

    /// Returns microseconds from submission to payload-start observation.
    pub fn queue_us(&self) -> u64 { self.started_us - self.queued_us }

    /// Returns software-observed transfer microseconds; executor latency is included.
    pub fn transfer_observed_us(&self) -> u64 { self.completed_us - self.started_us }

    /// Returns end-to-end microseconds from render start through HAL completion.
    pub fn total_us(&self) -> u64 { self.completed_us - self.render_start_us }

    /// Returns sample age at completion in microseconds, or `None` without a sample.
    pub fn sample_age_us(&self) -> Option<u64> { self.sample_us.map(|at| self.completed_us - at) }

    /// Returns oldest represented input observation age at completion, in
    /// microseconds, or `None` when this frame represents no qualified batch.
    pub fn input_age_us(&self) -> Option<u64> { self.input.map(|input| self.completed_us - input.observed_us) }

    /// Returns microseconds from latest batch consumption to completion, or
    /// `None` when no qualified input batch is represented.
    pub fn input_after_consume_us(&self) -> Option<u64> {
        self.input.map(|input| self.completed_us - input.consumed_us)
    }
}

/// Oldest entries are overwritten; rejected stale/duplicate records do not count.
pub struct TraceRing<const N: usize> {
    /// Fixed storage; initialized entries are always `Some`.
    records: [Option<FrameTrace>; N],
    /// Next write slot modulo nonzero capacity `N`.
    next: usize,
    /// Retained count, at most `N`.
    len: usize,
    /// Saturating lifetime number of overwritten oldest entries.
    overwritten: u64,
}

impl<const N: usize> Default for TraceRing<N> {
    /// Returns an empty ring; panics when capacity `N` is zero.
    fn default() -> Self { Self::new() }
}
impl<const N: usize> TraceRing<N> {
    /// Returns an empty fixed-capacity ring; panics for `N == 0`.
    pub const fn new() -> Self {
        assert!(N > 0, "trace capacity must be nonzero");
        Self {
            records: [None; N],
            next: 0,
            len: 0,
            overwritten: 0,
        }
    }

    /// Returns initialized entry count, at most `N`.
    pub fn len(&self) -> usize { self.len }

    /// Returns true before the first accepted trace.
    pub fn is_empty(&self) -> bool { self.len == 0 }

    /// Returns the saturating count of accepted traces replacing old entries.
    pub fn overwritten(&self) -> u64 { self.overwritten }

    /// Returns a copy of the newest trace, or `None` for an empty ring.
    pub fn latest(&self) -> Option<FrameTrace> { self.len.checked_sub(1).and_then(|i| self.get(i)) }

    /// Returns a copy at `oldest_index` relative to the oldest retained entry,
    /// or `None` outside the retained count.
    pub fn get(
        &self,
        oldest_index: usize,
    ) -> Option<FrameTrace> {
        if oldest_index >= self.len {
            return None;
        }
        self.records[((self.next + N - self.len) % N + oldest_index) % N]
    }

    /// Appends `trace` and returns true, overwriting the oldest when full.
    /// Returns false without mutation for a nonincreasing generation or reversed
    /// completion time; equal completion timestamps are allowed for newer IDs.
    pub fn push(
        &mut self,
        trace: FrameTrace,
    ) -> bool {
        if self
            .latest()
            .is_some_and(|last| trace.token.sequence <= last.token.sequence || trace.completed_us < last.completed_us)
        {
            return false;
        }
        self.records[self.next] = Some(trace);
        self.next = (self.next + 1) % N;
        if self.len == N {
            self.overwritten = self.overwritten.saturating_add(1);
        } else {
            self.len += 1;
        }
        true
    }
}

/// Completed HAL transfers per second; neither value measures physical scanout.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrameRate {
    /// Count delta divided by the latest observation window (at least one second).
    pub current: f32,
    /// Lifetime count delta divided by elapsed time since estimator construction.
    pub average: f32,
}

/// Rate from completed HAL writes, with elapsed-time weighting across windows.
pub struct CompletionRate {
    /// Construction timestamp in monotonic microseconds.
    origin_us: u64,
    /// Most recent accepted sample timestamp.
    last_us: u64,
    /// Most recent accepted wrapping `u32` completion counter.
    last_count: u32,
    /// Start of the current reporting interval in microseconds.
    window_us: u64,
    /// Cumulative accepted count at the current interval's start.
    window_count: u64,
    /// Checked cumulative count delta since construction.
    total_count: u64,
    /// Timestamp/rate of latest complete interval; cleared by rejected observations.
    latest: Option<(u64, FrameRate)>,
}

impl CompletionRate {
    /// Returns an estimator based at `now_us` (monotonic microseconds) and the
    /// current wrapping lifetime `completed` count; prior completions are excluded.
    pub const fn new(
        now_us: u64,
        completed: u32,
    ) -> Self {
        Self {
            origin_us: now_us,
            last_us: now_us,
            last_count: completed,
            window_us: now_us,
            window_count: 0,
            total_count: 0,
            latest: None,
        }
    }

    /// A missing first window, rejected observation or expired rate is unknown.
    /// Returns the cached rate at `now_us` only within the shared two-second
    /// freshness bound and without reversal relative to its timestamp.
    pub fn latest(
        &self,
        now_us: u64,
    ) -> Option<FrameRate> {
        let (at, rate) = self.latest?;
        crate::diagnostic_values::fresh(at, now_us).then_some(rate)
    }

    /// Samples wrapping `completed` count at monotonic `now_us` in microseconds.
    /// Returns `Some(rate)` after >=1 second, `None` while accumulating, or a
    /// clock/counter error. A count delta >half the `u32` range is rejected as a
    /// likely reset; legitimate wraps with smaller deltas are accepted. Errors
    /// clear cached availability but retain prior baselines for a later retry.
    pub fn sample(
        &mut self,
        now_us: u64,
        completed: u32,
    ) -> Result<Option<FrameRate>, TelemetryError> {
        if now_us < self.last_us {
            self.latest = None;
            return Err(TelemetryError::Clock);
        }
        let delta = completed.wrapping_sub(self.last_count);
        // A backwards reset must not masquerade as billions of completed frames.
        if delta > u32::MAX / 2 {
            self.latest = None;
            return Err(TelemetryError::Counter);
        }
        let Some(total_count) = self.total_count.checked_add(u64::from(delta)) else {
            self.latest = None;
            return Err(TelemetryError::Counter);
        };
        self.total_count = total_count;
        self.last_us = now_us;
        self.last_count = completed;
        let window_elapsed = now_us - self.window_us;
        if window_elapsed < 1_000_000 {
            return Ok(None);
        }
        let rate = FrameRate {
            current: (total_count - self.window_count) as f32 * 1_000_000.0 / window_elapsed as f32,
            average: total_count as f32 * 1_000_000.0 / (now_us - self.origin_us) as f32,
        };
        self.window_us = now_us;
        self.window_count = total_count;
        self.latest = Some((now_us, rate));
        Ok(Some(rate))
    }
}
