use dashboard_pico2::cycle_segments::CycleSegments;

/// UI segments include IRQ cycles but exclude executor work during yields.
pub struct RenderWork(CycleSegments);
fn read() -> u32 {
    if dashboard_pico2::cpu_cycles::available(0) {
        super::read()
    } else {
        0
    }
}
impl RenderWork {
    pub fn new(cpu_hz: u32) -> Self { Self(CycleSegments::new(read(), cpu_hz.saturating_mul(2))) }

    pub async fn checkpoint(&mut self) {
        self.0.pause(read()).expect("render cycle segment too long");
        embassy_futures::yield_now().await;
        self.0.resume(read()).expect("render cycle segment already running");
    }

    pub fn finish(mut self) -> u64 {
        self.0.pause(read()).expect("final render cycle segment too long");
        self.0.total()
    }
}
