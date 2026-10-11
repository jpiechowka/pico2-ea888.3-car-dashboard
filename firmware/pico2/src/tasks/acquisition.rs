use core::sync::atomic::Ordering;

#[cfg(not(feature = "longan-rx12"))]
use dashboard_pico2::acquisition::AcquisitionModel;
use dashboard_pico2::acquisition::DashboardSnapshot;
#[cfg(feature = "longan-rx12")]
use dashboard_pico2::diagnostics::service::{LinkSnapshot, Vehicle};
use dashboard_pico2::stack_samples::StackSamples;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;
use embassy_sync::watch::Watch;
use embassy_time::{Duration, Instant, Timer};

use crate::log_info;
use crate::tasks::{CORE1_STACK_USED_KB, CORE1_UTIL_PERCENT};

pub static DASHBOARD_VALUES: Watch<CriticalSectionRawMutex, DashboardSnapshot, 1> = Watch::new();
// Only the newest requested generation matters. The acquisition owner echoes
// its acknowledgement in the same snapshot as the reset statistics.
pub static RESET_STATISTICS: Signal<CriticalSectionRawMutex, u64> = Signal::new();
#[cfg(feature = "longan-rx12")]
pub static VEHICLE_LINK: Watch<CriticalSectionRawMutex, LinkSnapshot, 1> = Watch::new();

pub struct Resources {
    #[cfg(feature = "longan-rx12")]
    pub uart: crate::drivers::longan::Resources,
}

#[embassy_executor::task]
pub async fn acquisition_task(
    start_time: Instant,
    stack_bottom: u32,
    stack_top: u32,
    resources: Resources,
) {
    let sender = DASHBOARD_VALUES.dyn_sender();
    log_info!(
        "Acquisition: {}",
        if cfg!(feature = "demo") {
            "DEMO"
        } else if cfg!(feature = "longan-rx12") {
            "VEHICLE raw UART, unqualified"
        } else {
            "VEHICLE unavailable"
        }
    );

    let mut last_util_calc = Instant::now();
    let mut total_work_cycles = 0u32;
    #[cfg(not(feature = "longan-rx12"))]
    let Resources {} = resources;
    #[cfg(not(feature = "longan-rx12"))]
    let mut model = if cfg!(feature = "demo") {
        AcquisitionModel::demo()
    } else {
        AcquisitionModel::new()
    };
    #[cfg(feature = "longan-rx12")]
    let mut model = Vehicle::new(
        crate::drivers::longan::HardwarePort::new(resources.uart),
        dashboard_pico2::config::vehicle::PROFILE,
        Instant::now().as_millis(),
    )
    .expect("invalid vehicle profile");
    #[cfg(feature = "longan-rx12")]
    let link_sender = VEHICLE_LINK.dyn_sender();
    #[cfg(feature = "longan-rx12")]
    let mut previous_link = None;
    let mut previous_snapshot = None;
    let mut last_publication = Instant::now();
    let interval_ms = if cfg!(feature = "longan-rx12") {
        1
    } else if cfg!(feature = "demo") {
        10
    } else {
        100
    };
    let mut stack_samples = StackSamples::new(stack_bottom, stack_top).expect("invalid core1 stack bounds");

    loop {
        let work_start = crate::profiling::read();

        if let Some(generation) = RESET_STATISTICS.try_take() {
            model.reset_statistics(generation);
        }
        #[cfg(not(feature = "longan-rx12"))]
        if cfg!(feature = "demo") {
            let t = start_time.elapsed().as_millis() as f32 / 1000.0;
            model.ingest_batch(
                [
                    (0.3 + 2.2 * micromath::F32(t * 0.5).sin().0.abs()).min(2.0),
                    60.0 + 55.0 * micromath::F32(t * 0.3).sin().0,
                    88.0 + 7.0 * micromath::F32(t * 0.4).sin().0,
                    75.0 + 40.0 * micromath::F32(t * 0.35).sin().0,
                    30.0 + 40.0 * micromath::F32(t * 0.25).sin().0,
                    200.0 + 1000.0 * micromath::F32(t * 0.04).sin().0.abs(),
                    12.0 + 2.5 * micromath::F32(t * 0.15).sin().0,
                    (14.0 + 4.0 * micromath::F32(t * 0.45).sin().0) / 14.7,
                ],
                Instant::now().as_millis(),
            );
        }
        #[cfg(feature = "longan-rx12")]
        {
            let _ = start_time;
            let now = Instant::now().as_millis();
            // Always Ready: exposes this executor's waker to bounded HAL polls
            // without suspending between the segmented cycle measurements.
            core::future::poll_fn(|cx| {
                model.step(cx, now);
                core::task::Poll::Ready(())
            })
            .await;
        }
        // Statistics include every accepted sample before Watch can coalesce.
        // At most 100 Hz publication; no repeated 3 KB critical-section copy
        // when the backend has neither new data, faults nor a reset ack.
        if previous_snapshot.is_none() || last_publication.elapsed() >= Duration::from_millis(10) {
            let snapshot = model.snapshot();
            let identity = (snapshot.sequence, snapshot.reset_generation);
            if previous_snapshot != Some(identity) {
                sender.send(snapshot);
                previous_snapshot = Some(identity);
            }
            #[cfg(feature = "longan-rx12")]
            {
                let link = model.link();
                if previous_link != Some(link) {
                    link_sender.send(link);
                    previous_link = Some(link);
                }
            }
            last_publication = Instant::now();
        }
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

        // Never run a burst of overdue catch-up ticks after scheduling delays.
        Timer::after_millis(interval_ms).await;
    }
}
