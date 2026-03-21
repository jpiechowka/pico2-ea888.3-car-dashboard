use core::sync::atomic::Ordering;

use defmt::info;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::watch::Watch;
use embassy_time::{Duration, Instant, Timer};

use crate::tasks::{CORE1_STACK_USED_KB, CORE1_UTIL_PERCENT};

#[derive(Clone, Copy, Default)]
pub struct DemoSensorValues {
    pub boost: f32,
    pub oil_temp: f32,
    pub water_temp: f32,
    pub dsg_temp: f32,
    pub iat_temp: f32,
    pub egt_temp: f32,
    pub batt_voltage: f32,
    pub afr: f32,
}

pub static DEMO_VALUES: Watch<CriticalSectionRawMutex, DemoSensorValues, 2> = Watch::new();

#[embassy_executor::task]
pub async fn demo_values_task(
    start_time: Instant,
    stack_top: u32,
) {
    let sender = DEMO_VALUES.dyn_sender();
    info!("Demo values task started (stack top: 0x{:08x})", stack_top);

    let mut last_util_calc = Instant::now();
    let mut total_work_cycles = 0u32;

    loop {
        let work_start = crate::profiling::read();

        let elapsed_ms = start_time.elapsed().as_millis() as u32;
        let t = elapsed_ms as f32 / 1000.0;

        let values = DemoSensorValues {
            boost: (0.3 + 2.2 * micromath::F32(t * 0.5).sin().0.abs()).min(2.0),
            oil_temp: 60.0 + 55.0 * micromath::F32(t * 0.3).sin().0,
            water_temp: 88.0 + 7.0 * micromath::F32(t * 0.4).sin().0,
            dsg_temp: 75.0 + 40.0 * micromath::F32(t * 0.35).sin().0,
            iat_temp: 30.0 + 40.0 * micromath::F32(t * 0.25).sin().0,
            egt_temp: 200.0 + 1000.0 * micromath::F32(t * 0.04).sin().0.abs(),
            batt_voltage: 12.0 + 2.5 * micromath::F32(t * 0.15).sin().0,
            afr: 14.0 + 4.0 * micromath::F32(t * 0.45).sin().0,
        };

        sender.send(values);

        let work_end = crate::profiling::read();
        total_work_cycles = total_work_cycles.wrapping_add(crate::profiling::elapsed(work_start, work_end));

        if last_util_calc.elapsed() >= Duration::from_secs(1) {
            let util = crate::profiling::calc_util_percent(total_work_cycles, 1_000_000);
            CORE1_UTIL_PERCENT.store(util, Ordering::Relaxed);

            // Calculate stack usage for Core 1
            let stack_ptr: u32;
            unsafe {
                core::arch::asm!("mov {}, sp", out(reg) stack_ptr);
            }
            if stack_ptr < stack_top {
                let used = stack_top - stack_ptr;
                CORE1_STACK_USED_KB.store(used / 1024, Ordering::Relaxed);
            }

            total_work_cycles = 0;
            last_util_calc = Instant::now();
        }

        Timer::after_millis(10).await;
    }
}
