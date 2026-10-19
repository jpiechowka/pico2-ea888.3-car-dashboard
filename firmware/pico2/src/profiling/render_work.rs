use embassy_time::Instant;

/// UI segments include IRQ cycles but exclude executor work during yields.
pub struct RenderWork {
    total: Option<u64>,
    started: Instant,
    cycle_start: u32,
    cpu_hz: u32,
}
impl RenderWork {
    pub fn new(cpu_hz: u32) -> Self {
        Self {
            total: dashboard_pico2::cpu_cycles::available(0).then_some(0),
            started: Instant::now(),
            cycle_start: super::read(),
            cpu_hz,
        }
    }

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

    pub async fn checkpoint(&mut self) {
        self.pause();
        embassy_futures::yield_now().await;
        self.started = Instant::now();
        self.cycle_start = super::read();
    }

    pub fn finish(mut self) -> Option<u64> {
        self.pause();
        self.total
    }
}
