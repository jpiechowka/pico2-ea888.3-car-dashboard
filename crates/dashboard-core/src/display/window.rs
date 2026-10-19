//! Completed-transfer distributions over nonoverlapping windows of at least
//! thirty seconds. One owner, no allocation, no hardware or lock operations.
use super::telemetry::{FrameTrace, TelemetryError};
use crate::input_trace::InputCounters;
use crate::metrics::{Distribution, Histogram};

pub const WINDOW_US: u64 = 30_000_000;
pub const METRICS: usize = 9;
// Metric indices are the versioned RTT format: interval, render, queue,
// transfer, blocked, sample age, oldest input age, latest consume age, overlap.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WindowReport {
    pub sequence: u64,
    pub start_us: u64,
    pub end_us: u64,
    pub frames: u32,
    pub payload_bytes: u64,
    pub active_cycles: Option<u64>,
    pub rejected: u64,
    pub metrics: [Distribution; METRICS],
    /// Lifetime counters, not per-window deltas. The collector detects reset.
    pub inputs: InputCounters,
}

pub struct FrameWindow {
    start_us: u64,
    last_us: u64,
    sequence: u64,
    previous: Option<FrameTrace>,
    last_input: u64,
    frames: u32,
    payload_bytes: u64,
    active_cycles: Option<u64>,
    rejected: u64,
    metrics: [Histogram<976>; METRICS],
}

impl FrameWindow {
    pub const fn new(start_us: u64) -> Self {
        Self {
            start_us,
            last_us: start_us,
            sequence: 0,
            previous: None,
            last_input: 0,
            frames: 0,
            payload_bytes: 0,
            active_cycles: Some(0),
            rejected: 0,
            metrics: [const { Histogram::new() }; METRICS],
        }
    }

    pub fn record(
        &mut self,
        trace: FrameTrace,
    ) -> Result<(), TelemetryError> {
        let result = self.accept(trace);
        if result.is_err() {
            self.rejected = self.rejected.saturating_add(1);
        }
        result
    }

    fn accept(
        &mut self,
        trace: FrameTrace,
    ) -> Result<(), TelemetryError> {
        if trace.completed_us() < self.last_us
            || self.previous.is_some_and(|old| trace.started_us() < old.completed_us())
        {
            return Err(TelemetryError::Clock);
        }
        if self
            .previous
            .is_some_and(|old| trace.token().sequence <= old.token().sequence)
            || trace
                .render()
                .input
                .is_some_and(|input| input.first_id <= self.last_input)
        {
            return Err(TelemetryError::Metadata);
        }
        // Preflight every fallible addition before changing any bin. Each
        // histogram receives at most one sample per accepted frame.
        let frames = self.frames.checked_add(1).ok_or(TelemetryError::Counter)?;
        let bytes = self
            .payload_bytes
            .checked_add(u64::from(trace.payload_bytes()))
            .ok_or(TelemetryError::Counter)?;
        // Missing/overflowing cycle observations do not discard valid transfer
        // counts or timing. The sum remains unavailable for this entire window.
        let cycles = self
            .active_cycles
            .zip(trace.render().active_cycles)
            .and_then(|(total, next)| total.checked_add(next));
        let values = [
            self.previous.map(|old| trace.completed_us() - old.completed_us()),
            Some(trace.render_us()),
            Some(trace.queue_us()),
            Some(trace.transfer_observed_us()),
            Some(trace.render().blocked_us),
            trace.sample_age_us(),
            trace.input_age_us(),
            trace.input_after_consume_us(),
            self.previous.map(|old| {
                trace
                    .render()
                    .end_us
                    .min(old.completed_us())
                    .saturating_sub(trace.render().start_us.max(old.started_us()))
            }),
        ];
        for (histogram, value) in self.metrics.iter_mut().zip(values) {
            if let Some(value) = value {
                assert!(histogram.record(value));
            }
        }
        self.frames = frames;
        self.payload_bytes = bytes;
        self.active_cycles = cycles;
        self.last_us = trace.completed_us();
        self.previous = Some(trace);
        if let Some(input) = trace.render().input {
            self.last_input = input.last_id;
        }
        Ok(())
    }

    pub fn report(
        &mut self,
        now_us: u64,
        inputs: InputCounters,
    ) -> Result<Option<WindowReport>, TelemetryError> {
        if now_us < self.last_us {
            self.rejected = self.rejected.saturating_add(1);
            return Err(TelemetryError::Clock);
        }
        self.last_us = now_us;
        if now_us - self.start_us < WINDOW_US {
            return Ok(None);
        }
        let Some(sequence) = self.sequence.checked_add(1) else {
            self.rejected = self.rejected.saturating_add(1);
            return Err(TelemetryError::Counter);
        };
        let report = WindowReport {
            sequence,
            start_us: self.start_us,
            end_us: now_us,
            frames: self.frames,
            payload_bytes: self.payload_bytes,
            active_cycles: self.active_cycles,
            rejected: self.rejected,
            metrics: core::array::from_fn(|i| self.metrics[i].summary()),
            inputs,
        };
        // Keep previous frame/input identity across windows, including idle
        // windows. A resumed interval includes the entire observed idle gap.
        for histogram in &mut self.metrics {
            histogram.clear();
        }
        self.sequence = sequence;
        self.start_us = now_us;
        self.frames = 0;
        self.payload_bytes = 0;
        self.active_cycles = Some(0);
        self.rejected = 0;
        Ok(Some(report))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::display::pipeline::{BufferId, FrameToken};
    use crate::display::telemetry::RenderTrace;
    #[test]
    fn count_sum_and_export_identity_exhaustion_do_not_partially_mutate() {
        let trace = FrameTrace::finish(
            FrameToken {
                buffer: BufferId::A,
                sequence: 1,
            },
            RenderTrace {
                active_cycles: Some(1),
                ..RenderTrace::instant(0)
            },
            0,
            0,
            2,
        )
        .unwrap();
        let mut window = FrameWindow::new(0);
        for field in 0..2 {
            match field {
                0 => window.frames = u32::MAX,
                _ => window.payload_bytes = u64::MAX,
            }
            assert_eq!(window.record(trace), Err(TelemetryError::Counter));
            assert_eq!(window.metrics[1].summary().samples, 0);
            window.frames = 0;
            window.payload_bytes = 0;
            window.active_cycles = Some(0);
        }
        window.active_cycles = Some(u64::MAX);
        window.record(trace).unwrap();
        assert_eq!(window.active_cycles, None);
        assert_eq!(window.frames, 1);
        assert_eq!(window.metrics[1].summary().samples, 1);
        window.sequence = u64::MAX;
        assert_eq!(
            window.report(WINDOW_US, InputCounters::ZERO),
            Err(TelemetryError::Counter)
        );
        assert_eq!(window.sequence, u64::MAX);
    }
}
