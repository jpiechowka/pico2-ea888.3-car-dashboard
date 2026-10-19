//! Versioned nonblocking RTT records. Every row carries its snapshot identity.
//! A collector must reject incomplete groups and report sequence gaps.
use core::fmt::{self, Write};

use crate::metrics::Distribution;

// Even fourteen maximum-width u64 decimal fields fit in this fixed buffer.
fn emit_line(
    emit: &mut impl FnMut(&str),
    arguments: fmt::Arguments<'_>,
) -> fmt::Result {
    let mut line = heapless::String::<320>::new();
    line.write_fmt(arguments)?;
    emit(line.as_str());
    Ok(())
}

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

struct OptionalCycles(Option<u64>);
impl fmt::Display for OptionalCycles {
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
