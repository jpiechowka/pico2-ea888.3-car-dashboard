//! Completed-transfer distributions over nonoverlapping windows of at least
//! thirty seconds. One owner, no allocation, no hardware or lock operations.
use super::telemetry::{FrameTrace, TelemetryError};
use crate::input_trace::InputCounters;
use crate::metrics::{Distribution, Histogram};

/// Minimum elapsed microseconds before a report closes the current window.
pub const WINDOW_US: u64 = 30_000_000;
/// Number of timing distributions in the versioned export order.
pub const METRICS: usize = 9;
// Metric indices are the versioned RTT format: interval, render, queue,
// transfer, blocked, sample age, oldest input age, latest consume age, overlap.
/// Snapshot of a closed observation window, including idle windows with no frames.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WindowReport {
    /// Nonzero monotonically increasing report ID; never wraps.
    pub sequence: u64,
    /// Inclusive observation-window start in monotonic microseconds.
    pub start_us: u64,
    /// Report timestamp in microseconds; also the next window's start.
    pub end_us: u64,
    /// Accepted completed traces in this window, not render attempts.
    pub frames: u32,
    /// Sum of accepted RGB565 payload bytes, excluding bus commands.
    pub payload_bytes: u64,
    /// Sum of observed active render cycles, or `None` if any are absent/overflow.
    pub active_cycles: Option<u64>,
    /// Saturating count of rejected frame/report operations in this window.
    pub rejected: u64,
    /// Microsecond distributions: completion interval, render, queue, transfer,
    /// blocked, sample age, oldest input age, latest consume age, render/transfer overlap.
    pub metrics: [Distribution; METRICS],
    /// Lifetime counters, not per-window deltas. The collector detects reset.
    pub inputs: InputCounters,
}

/// Single-owner accumulator with fixed histogram storage and checked counts.
pub struct FrameWindow {
    /// Current window's start in monotonic microseconds.
    start_us: u64,
    /// Most recent accepted completion or report-call timestamp.
    last_us: u64,
    /// Last exported report ID, zero before first export.
    sequence: u64,
    /// Last accepted trace, retained across windows to preserve idle intervals.
    previous: Option<FrameTrace>,
    /// Last represented input batch ID, retained across windows to reject replay.
    last_input: u64,
    /// Accepted traces since the last successful report.
    frames: u32,
    /// Checked RGB565 payload byte sum for the current window.
    payload_bytes: u64,
    /// Checked cycle sum; once unavailable it stays absent until window reset.
    active_cycles: Option<u64>,
    /// Saturating current-window rejected-operation count.
    rejected: u64,
    /// Fixed logarithmic timing histograms in [`WindowReport::metrics`] order.
    metrics: [Histogram<976>; METRICS],
}

impl FrameWindow {
    /// Returns an empty accumulator starting at monotonic `start_us` microseconds.
    /// No prior transfer/input identity or physical timing is inferred.
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

    /// Adds validated `trace` if time, generation and input identity advance.
    /// Returns success or clock/metadata/counter error; errors increment only
    /// the saturating rejection count, preserving distributions and frame totals.
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

    /// Preflights `trace` then commits its timing samples and totals. Returns
    /// clock/metadata/counter errors without partial histogram mutation; absent
    /// or overflowing cycle sums only invalidate cycle availability. The internal
    /// histogram assertion cannot fail while the preflight frame-count bound holds.
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

    /// Closes a window at monotonic `now_us` with supplied lifetime `inputs`
    /// counters. Returns `None` before 30 seconds, otherwise a report and resets
    /// window totals while retaining prior frame/input identities. Reversed time
    /// or report-ID overflow returns an error and increments rejections; a valid
    /// call advances the observation clock even when no report is emitted.
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
/// Counter exhaustion regressions for atomic admission and cycle availability.
mod tests {
    use super::*;
    use crate::display::pipeline::{BufferId, FrameToken};
    use crate::display::telemetry::RenderTrace;
    /// Returns unit after checking count/byte overflow leaves bins unchanged,
    /// cycle overflow preserves timing, and report IDs never wrap; panics on drift.
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
