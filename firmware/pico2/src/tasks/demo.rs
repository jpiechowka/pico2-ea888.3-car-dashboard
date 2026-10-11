use core::sync::atomic::Ordering;

use dashboard_pico2::acquisition::{AcquisitionModel, DashboardSnapshot};
use dashboard_pico2::stack_samples::StackSamples;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;
use embassy_sync::watch::Watch;
use embassy_time::{Duration, Instant, Timer};

use crate::log_info;
use crate::tasks::{CORE1_STACK_USED_KB, CORE1_UTIL_PERCENT};

pub static DEMO_VALUES: Watch<CriticalSectionRawMutex, DashboardSnapshot, 1> = Watch::new();
// Only the newest requested generation matters. The acquisition owner echoes
// its acknowledgement in the same snapshot as the reset statistics.
pub static RESET_STATISTICS: Signal<CriticalSectionRawMutex, u64> = Signal::new();

#[embassy_executor::task]
pub async fn demo_values_task(
    start_time: Instant,
    stack_bottom: u32,
    stack_top: u32,
) {
    let sender = DEMO_VALUES.dyn_sender();
    log_info!("Demo task started");

    let mut last_util_calc = Instant::now();
    let mut total_work_cycles = 0u32;
    let mut model = AcquisitionModel::new();
    let mut stack_samples = StackSamples::new(stack_bottom, stack_top).expect("invalid core1 stack bounds");

    loop {
        let work_start = crate::profiling::read();

        let elapsed_ms = start_time.elapsed().as_millis() as u32;
        let t = elapsed_ms as f32 / 1000.0;

        if let Some(generation) = RESET_STATISTICS.try_take() {
            model.reset_statistics(generation);
        }
        model.ingest_batch(
            [
                (0.3 + 2.2 * micromath::F32(t * 0.5).sin().0.abs()).min(2.0),
                60.0 + 55.0 * micromath::F32(t * 0.3).sin().0,
                88.0 + 7.0 * micromath::F32(t * 0.4).sin().0,
                75.0 + 40.0 * micromath::F32(t * 0.35).sin().0,
                30.0 + 40.0 * micromath::F32(t * 0.25).sin().0,
                200.0 + 1000.0 * micromath::F32(t * 0.04).sin().0.abs(),
                12.0 + 2.5 * micromath::F32(t * 0.15).sin().0,
                14.0 + 4.0 * micromath::F32(t * 0.45).sin().0,
            ],
            Instant::now().as_millis(),
        );
        // Statistics include every accepted sample before Watch can coalesce.
        sender.send(model.snapshot());
        crate::tasks::recovery::ACQUISITION_PROGRESS.fetch_add(1, Ordering::Relaxed);

        let work_end = crate::profiling::read();
        total_work_cycles = total_work_cycles.wrapping_add(crate::profiling::elapsed(work_start, work_end));

        if last_util_calc.elapsed() >= Duration::from_secs(1) {
            let util = crate::profiling::calc_util_percent(
                total_work_cycles,
                last_util_calc.elapsed().as_micros().min(u32::MAX as u64) as u32,
            );
            CORE1_UTIL_PERCENT.store(util, Ordering::Relaxed);

            // Sampled depth only; never read live stack memory or claim a HWM.
            let observed = stack_samples
                .observe(dashboard_pico2::memory::stack_pointer())
                .expect("core1 SP outside its stack");
            CORE1_STACK_USED_KB.store(observed.div_ceil(1024), Ordering::Relaxed);

            total_work_cycles = 0;
            last_util_calc = Instant::now();
        }

        Timer::after_millis(10).await;
    }
}
