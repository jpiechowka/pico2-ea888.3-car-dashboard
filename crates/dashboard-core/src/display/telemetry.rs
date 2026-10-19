//! Software-observed transfer timing. This does not measure panel scanout.
use super::framebuffer::FRAME_BYTES;
use super::pipeline::FrameToken;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TelemetryError {
    Clock,
    Metadata,
    Counter,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RenderTrace {
    pub start_us: u64,
    pub end_us: u64,
    pub queued_us: u64,
    pub blocked_us: u64,
    pub sample_us: Option<u64>,
    pub active_cycles: Option<u64>,
    pub input: Option<crate::input_trace::InputTrace>,
}

impl RenderTrace {
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameTrace {
    token: FrameToken,
    render_start_us: u64,
    render_end_us: u64,
    queued_us: u64,
    blocked_us: u64,
    sample_us: Option<u64>,
    // Keep availability independent of every u64 value. Flattening this private
    // storage lets the flag use existing padding in each of the 64 ring entries.
    active_cycles: u64,
    cycles_present: bool,
    input: Option<crate::input_trace::InputTrace>,
    started_us: u64,
    completed_us: u64,
    payload_bytes: u32,
}

impl FrameTrace {
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

    pub fn token(&self) -> FrameToken { self.token }

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

    pub fn started_us(&self) -> u64 { self.started_us }

    pub fn completed_us(&self) -> u64 { self.completed_us }

    pub fn payload_bytes(&self) -> u32 { self.payload_bytes }

    pub fn render_us(&self) -> u64 { self.render_end_us - self.render_start_us }

    pub fn queue_us(&self) -> u64 { self.started_us - self.queued_us }

    pub fn transfer_observed_us(&self) -> u64 { self.completed_us - self.started_us }

    pub fn total_us(&self) -> u64 { self.completed_us - self.render_start_us }

    pub fn sample_age_us(&self) -> Option<u64> { self.sample_us.map(|at| self.completed_us - at) }

    pub fn input_age_us(&self) -> Option<u64> { self.input.map(|input| self.completed_us - input.observed_us) }

    pub fn input_after_consume_us(&self) -> Option<u64> {
        self.input.map(|input| self.completed_us - input.consumed_us)
    }
}

/// Oldest entries are overwritten; rejected stale/duplicate records do not count.
pub struct TraceRing<const N: usize> {
    records: [Option<FrameTrace>; N],
    next: usize,
    len: usize,
    overwritten: u64,
}

impl<const N: usize> Default for TraceRing<N> {
    fn default() -> Self { Self::new() }
}
impl<const N: usize> TraceRing<N> {
    pub const fn new() -> Self {
        assert!(N > 0, "trace capacity must be nonzero");
        Self {
            records: [None; N],
            next: 0,
            len: 0,
            overwritten: 0,
        }
    }

    pub fn len(&self) -> usize { self.len }

    pub fn is_empty(&self) -> bool { self.len == 0 }

    pub fn overwritten(&self) -> u64 { self.overwritten }

    pub fn latest(&self) -> Option<FrameTrace> { self.len.checked_sub(1).and_then(|i| self.get(i)) }

    pub fn get(
        &self,
        oldest_index: usize,
    ) -> Option<FrameTrace> {
        if oldest_index >= self.len {
            return None;
        }
        self.records[((self.next + N - self.len) % N + oldest_index) % N]
    }

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

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrameRate {
    pub current: f32,
    pub average: f32,
}

/// Rate from completed HAL writes, with elapsed-time weighting across windows.
pub struct CompletionRate {
    origin_us: u64,
    last_us: u64,
    last_count: u32,
    window_us: u64,
    window_count: u64,
    total_count: u64,
    latest: Option<(u64, FrameRate)>,
}

impl CompletionRate {
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
    pub fn latest(
        &self,
        now_us: u64,
    ) -> Option<FrameRate> {
        let (at, rate) = self.latest?;
        crate::diagnostic_values::fresh(at, now_us).then_some(rate)
    }

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
