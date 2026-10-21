//! The shared encoder is exercised by the host collector integration test.
/// Emits bounded protocol lines for frame-window `value` through defmt; logs an
/// encoding error rather than publishing an invented or partial success marker.
pub fn frames(value: dashboard_pico2::display::window::WindowReport) {
    if dashboard_pico2::telemetry_protocol::frames(value, |line| defmt::info!("{}", line)).is_err() {
        defmt::error!("Frame telemetry encoding failed");
    }
}
#[cfg(feature = "longan-rx12")]
/// Encodes diagnostic `value` under stream `sequence` and boot-relative
/// `published_ms`; logs an encoder error. The sink does not measure UART/ECU time.
pub fn diagnostics(
    sequence: u64,
    published_ms: u64,
    value: dashboard_pico2::diagnostics::session::Telemetry,
) {
    if dashboard_pico2::telemetry_protocol::diagnostics(sequence, published_ms, value, |line| defmt::info!("{}", line))
        .is_err()
    {
        defmt::error!("Diagnostic telemetry encoding failed");
    }
}
