//! Versioned records for nonblocking RTT adapters. Every row carries its snapshot identity.
//! A collector must reject incomplete groups and report sequence gaps.
//! Callbacks own transport/delimiters; these formatters neither detect drops nor enforce nonblocking I/O.
use core::fmt::{self, Write};

use crate::metrics::Distribution;

// Even fourteen maximum-width u64 decimal fields fit in this fixed buffer.
/// Formats `arguments` into at most 320 stack bytes, then borrows the completed
/// line to `emit` without a newline. Returns formatting failure before emitting
/// this line; callback failure/drop cannot be reported through its unit result.
fn emit_line(
    emit: &mut impl FnMut(&str),
    arguments: fmt::Arguments<'_>,
) -> fmt::Result {
    let mut line = heapless::String::<320>::new();
    line.write_fmt(arguments)?;
    emit(line.as_str());
    Ok(())
}

/// Emits the coherent frame window `value` through `emit`: FRAME2, INPUT1,
/// nine indexed METRIC1 rows and END1. Times/distributions are microseconds;
/// active CPU cycles remain `na` when unavailable. `emit` must consume/copy each
/// borrowed line and supply framing. Returns formatting success or an error
/// after any previously emitted rows; group delivery is not atomic or acknowledged.
pub fn frames(
    value: crate::display::window::WindowReport,
    mut emit: impl FnMut(&str),
) -> fmt::Result {
    let sequence = value.sequence;
    emit_line(
        &mut emit,
        format_args!(
            "FRAME2,{},{},{},{},{},{},{}",
            value.sequence,
            value.start_us,
            value.end_us,
            value.frames,
            value.payload_bytes,
            OptionalCycles(value.active_cycles),
            value.rejected
        ),
    )?;
    let c = value.inputs;
    emit_line(
        &mut emit,
        format_args!(
            "INPUT1,{},{},{},{},{},{},{},{}",
            value.sequence,
            c.ignored_events,
            c.no_damage_events,
            c.obscured_events,
            c.invalid,
            c.maintenance_gaps,
            c.encoder_dropped,
            c.encoder_discarded
        ),
    )?;
    for (metric, value) in value.metrics.into_iter().enumerate() {
        distribution(sequence, 0, metric as u8, value, &mut emit)?;
    }
    emit_line(&mut emit, format_args!("END1,0,{}", value.sequence))
}

/// Decimal cycle count preserving unavailable observations as the literal `na`.
struct OptionalCycles(
    /// Observed active cycles, including valid zero, or unavailable clock evidence.
    Option<u64>,
);
impl fmt::Display for OptionalCycles {
    /// Writes decimal cycles or `na` to `f`; returns the formatter's write result.
    fn fmt(
        &self,
        f: &mut fmt::Formatter<'_>,
    ) -> fmt::Result {
        match self.0 {
            Some(value) => value.fmt(f),
            None => f.write_str("na"),
        }
    }
}

/// Emits `value` as one METRIC1 row via `emit`, using caller-issued `sequence`,
/// group `kind` (0 frame, 1 diagnostic) and `metric` (frame index or PID).
/// Units belong to that metric. Missing extrema/bounds encode zero and require
/// the sample count to distinguish empty state. Returns formatting success/error;
/// the borrowed callback line has no newline or delivery acknowledgment.
fn distribution(
    sequence: u64,
    kind: u8,
    metric: u8,
    value: Distribution,
    emit: &mut impl FnMut(&str),
) -> fmt::Result {
    let lower = |bounds: Option<crate::metrics::Bounds>| bounds.map_or(0, |b| b.lower);
    let upper = |bounds: Option<crate::metrics::Bounds>| bounds.map_or(0, |b| b.upper);
    emit_line(
        emit,
        format_args!(
            "METRIC1,{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
            kind,
            sequence,
            metric,
            value.samples,
            value.overflow,
            value.rejected,
            value.min.unwrap_or(0),
            value.max.unwrap_or(0),
            lower(value.p50),
            upper(value.p50),
            lower(value.p95),
            upper(value.p95),
            lower(value.p99),
            upper(value.p99)
        ),
    )
}

/// Emits diagnostic snapshot `value` under caller-issued `sequence` through
/// `emit`: DIAG1, each PID1/METRIC1 pair, then END1. `published_ms` is the
/// consumer publication clock in milliseconds; no ordering against the snapshot
/// clock is checked. Reply distributions use milliseconds. Returns formatting
/// success/error, retaining already emitted rows on error; callback transport
/// drops are unobservable and each borrowed line requires caller framing.
pub fn diagnostics(
    sequence: u64,
    published_ms: u64,
    value: crate::diagnostics::session::Telemetry,
    mut emit: impl FnMut(&str),
) -> fmt::Result {
    emit_line(
        &mut emit,
        format_args!(
            "DIAG1,{},{},{},{},{}",
            sequence, value.ecu, value.epoch, value.at_ms, published_ms
        ),
    )?;
    for channel in value.channels {
        let c = channel.counters;
        emit_line(
            &mut emit,
            format_args!(
                "PID1,{},{},{},{},{},{},{},{},{},{}",
                sequence,
                channel.pid,
                c.requested,
                c.transmitted,
                c.completed,
                c.negative,
                c.timeouts,
                c.malformed,
                c.abandoned,
                c.transport_faults
            ),
        )?;
        distribution(sequence, 1, channel.pid, channel.reply_ms, &mut emit)?;
    }
    emit_line(&mut emit, format_args!("END1,1,{}", sequence))
}
