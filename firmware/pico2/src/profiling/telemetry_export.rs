//! Versioned nonblocking RTT records. Every row carries its snapshot identity.
//! A collector must reject incomplete groups and report sequence gaps.
use dashboard_pico2::metrics::Distribution;

pub fn distribution(
    sequence: u64,
    kind: u8,
    metric: u8,
    value: Distribution,
) {
    let lower = |bounds: Option<dashboard_pico2::metrics::Bounds>| bounds.map_or(0, |b| b.lower);
    let upper = |bounds: Option<dashboard_pico2::metrics::Bounds>| bounds.map_or(0, |b| b.upper);
    defmt::info!(
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
    );
}

#[cfg(feature = "longan-rx12")]
pub fn diagnostics(
    sequence: u64,
    published_ms: u64,
    value: dashboard_pico2::diagnostics::session::Telemetry,
) {
    defmt::info!(
        "DIAG1,{},{},{},{},{}",
        sequence,
        value.ecu,
        value.epoch,
        value.at_ms,
        published_ms
    );
    for channel in value.channels {
        let c = channel.counters;
        defmt::info!(
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
        );
        distribution(sequence, 1, channel.pid, channel.reply_ms);
    }
    defmt::info!("END1,1,{}", sequence);
}
