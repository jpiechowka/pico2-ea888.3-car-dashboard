//! DWT work accumulation excluding cooperative UI yield intervals.
use embassy_time::Instant;

/// UI segments include IRQ cycles but exclude executor work during yields.
pub struct RenderWork {
    /// Accepted cycle sum; a rejected segment permanently changes this to `None`.
    total: Option<u64>,
    /// Monotonic start of the currently instrumented segment.
    started: Instant,
    /// Raw wrapping DWT sample at this segment's start.
    cycle_start: u32,
    /// Configured CPU hertz used for wrap/plausibility checks.
    cpu_hz: u32,
}
impl RenderWork {
    /// Returns a core0 measurement starting now at configured `cpu_hz`; an
    /// unavailable core0 counter starts with no valid result.
    pub fn new(cpu_hz: u32) -> Self {
        Self {
            total: dashboard_pico2::cpu_cycles::available(0).then_some(0),
            started: Instant::now(),
            cycle_start: super::read(),
            cpu_hz,
        }
    }

    /// Adds the current segment's checked cycle difference, invalidating the
    /// entire measurement on bad time/wrap/frequency or sum overflow.
    fn pause(&mut self) {
        let end = super::read();
        let cycles = dashboard_pico2::cpu_math::checked_segment(
            self.cpu_hz,
            self.cycle_start,
            end,
            self.started.elapsed().as_micros(),
        );
        self.total = self
            .total
            .zip(cycles)
            .and_then(|(total, cycles)| total.checked_add(cycles));
    }

    /// Ends the current segment, cooperatively yields, then starts a fresh one.
    /// The measurement must be resumed through completion before further use;
    /// dropping the measurement is permitted but produces no completed total.
    pub async fn checkpoint(&mut self) {
        self.pause();
        embassy_futures::yield_now().await;
        self.started = Instant::now();
        self.cycle_start = super::read();
    }

    /// Consumes this measurement and returns total instrumented cycles including
    /// the final segment, or `None` if any segment or accumulated sum was invalid.
    pub fn finish(mut self) -> Option<u64> {
        self.pause();
        self.total
    }
}
