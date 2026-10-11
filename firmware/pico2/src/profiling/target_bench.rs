//! Opt-in boot experiment. No DMA/panel operations or watchdog feeding.
use core::hint::black_box;
use core::sync::atomic::Ordering;

use dashboard_pico2::display::FrameBuffer;
use dashboard_pico2::display::framebuffer::FRAME_BYTES;
use dashboard_pico2::microbench::workload::{Fixture, Kernel};
use dashboard_pico2::microbench::{self, Budget, Counter, Error, ITERATIONS, Measurement, Timebase, TimerPort};
use embassy_rp::{clocks, pac};
use embassy_time::Instant;

struct HardwareTimer;
impl TimerPort for HardwareTimer {
    fn read(&self) -> Timebase {
        let tick = pac::TICKS.timer0_ctrl().read();
        Timebase {
            paused: pac::TIMER0.pause().read().pause(),
            system_source: pac::TIMER0.source().read().clk_sys() != pac::timer::vals::ClkSys::TICK,
            tick_enabled: tick.enable(),
            tick_running: tick.running(),
            tick_cycles: pac::TICKS.timer0_cycles().read().0,
            reference_hz: clocks::clk_ref_freq(),
            debug_pause: pac::TIMER0.dbgpause().read().0,
        }
    }

    fn write_debug_pause(
        &mut self,
        value: u32,
    ) {
        pac::TIMER0.dbgpause().write_value(pac::timer::regs::Dbgpause(value));
        cortex_m::asm::dsb();
    }
}

fn controls() -> (u32, u32) {
    // Privileged Secure core0, Armv8-M DWT_CTRL and DEMCR. No singleton is
    // stolen and no shared counter is reset/disabled by this experiment.
    unsafe {
        (
            core::ptr::read_volatile(0xE0001000 as *const u32),
            core::ptr::read_volatile(0xE000EDFC as *const u32),
        )
    }
}
fn start() -> Counter {
    let (control, demcr) = controls();
    let wall_us = Instant::now().as_micros();
    let cycles = cortex_m::peripheral::DWT::cycle_count();
    Counter {
        wall_us,
        cycles,
        control,
        demcr,
    }
}
fn end() -> Counter {
    let cycles = cortex_m::peripheral::DWT::cycle_count();
    let wall_us = Instant::now().as_micros();
    let (control, demcr) = controls();
    Counter {
        wall_us,
        cycles,
        control,
        demcr,
    }
}
fn context() -> Result<(), Error> {
    if pac::SIO.cpuid().read() != 0
        || !crate::tasks::maintenance::interrupt_ready()
        || !cortex_m::register::control::read().npriv().is_privileged()
    {
        return Err(Error::Context);
    }
    let wd = pac::WATCHDOG.ctrl().read();
    let tick = pac::TICKS.watchdog_ctrl().read();
    if !wd.enable()
        || wd.time() < 5_000_000
        || wd.pause_dbg0()
        || wd.pause_dbg1()
        || wd.pause_jtag()
        || !tick.enable()
        || !tick.running()
        || pac::TICKS.watchdog_cycles().read().0 != clocks::clk_ref_freq() / 1_000_000
    {
        return Err(Error::Watchdog);
    }
    Ok(())
}

#[derive(Clone, Copy)]
struct Sample {
    kind: Kernel,
    before: Counter,
    after: Counter,
    checksum: u32,
    status: u8,
}

/// Retain a named symbol for optimized-code review of every finite iteration.
#[inline(never)]
fn batch(
    fixture: &mut Fixture,
    kind: Kernel,
    frame: &mut FrameBuffer,
) {
    for _ in 0..ITERATIONS {
        black_box(&mut *fixture).run(black_box(kind), black_box(&mut *frame));
    }
    // Include outstanding CPU memory writes, never a peripheral transfer.
    cortex_m::asm::dsb();
}

pub fn run(frame: &mut FrameBuffer) {
    let mut samples = [None; 4];
    let address = frame.as_ptr() as usize;
    let cpu = clocks::clk_sys_freq();
    let watchdog_before = pac::WATCHDOG.ctrl().read().time();
    let pause_before = pac::TIMER0.dbgpause().read().0;
    let core1_before = crate::tasks::recovery::ACQUISITION_PROGRESS.load(Ordering::Relaxed);
    let (dwt, demcr) = controls();
    let result = (|| {
        context()?;
        if cfg!(debug_assertions)
            || address < 0x20000000
            || !address.is_multiple_of(4)
            || address.checked_add(FRAME_BYTES).is_none_or(|end| end > 0x20080000)
        {
            return Err(Error::Context);
        }
        microbench::with_running_timer(&mut HardwareTimer, |timer| {
            let mut budget = Budget::new(Instant::now().as_micros());
            let before = start();
            for value in 0..1024 {
                black_box(value);
                cortex_m::asm::nop();
            }
            Measurement::check(before, end(), cpu)?;
            let timebase = timer.read();
            for (index, kind) in Kernel::ALL.into_iter().enumerate() {
                frame.fill(0);
                let mut fixture = Fixture::new();
                context()?;
                budget.admit(Instant::now().as_micros(), ITERATIONS)?;
                let before = start();
                batch(&mut fixture, kind, frame);
                let after = end();
                let checksum = fixture.checksum(kind, frame)?;
                let status = (|| {
                    Measurement::check(before, after, cpu)?;
                    if checksum != kind.expected_checksum() {
                        return Err(Error::Checksum);
                    }
                    if timer.read() != timebase {
                        return Err(Error::Timebase);
                    }
                    context()?;
                    budget.finish(Instant::now().as_micros())
                })();
                samples[index] = Some(Sample {
                    kind,
                    before,
                    after,
                    checksum,
                    status: status.err().map_or(0, |e| e as u8),
                });
                status?;
            }
            Ok(())
        })
    })();
    // All formatting/RTT is after timing and restoration. No benchmark result
    // enters the production sensor model, frame pipeline or completion count.
    defmt::info!(
        "BENCHBUILD1,{},{},{},{},{},{}",
        env!("BENCH_COMPILER"),
        env!("BENCH_PROFILE"),
        env!("BENCH_OPT_LEVEL"),
        env!("BENCH_DEBUG"),
        env!("BENCH_TARGET"),
        env!("BENCH_FEATURES")
    );
    defmt::info!(
        "BENCHCLOCK1,{},{},{},{},{},{}",
        cpu,
        clocks::clk_peri_freq(),
        clocks::clk_ref_freq(),
        crate::vreg_setting_mv().unwrap_or(0),
        crate::display_spi_config().frequency,
        crate::get_actual_spi_freq(clocks::clk_peri_freq())
    );
    defmt::info!(
        "BENCHENV1,{},{},{},{},{},{},{},{}",
        address as u32,
        core1_before,
        crate::tasks::recovery::ACQUISITION_PROGRESS.load(Ordering::Relaxed),
        pause_before,
        pac::TIMER0.dbgpause().read().0,
        watchdog_before,
        dwt,
        demcr
    );
    defmt::info!(
        "BENCHXIP1,{},{},{},{}",
        pac::XIP_CTRL.ctrl().read().0,
        pac::QMI.mem(0).timing().read().0,
        pac::QMI.mem(0).rfmt().read().0,
        pac::QMI.mem(0).rcmd().read().0
    );
    for s in samples.into_iter().flatten() {
        defmt::info!(
            "BENCH1,{},{},{},{},{},{},{}",
            s.kind as u8,
            ITERATIONS,
            s.before.wall_us,
            s.after.wall_us,
            s.after.cycles.wrapping_sub(s.before.cycles),
            s.checksum,
            s.status
        );
    }
    defmt::info!(
        "BENCHEND1,{},{}",
        result.err().map_or(0, |e| e as u8),
        samples.iter().flatten().count() as u32
    );
}
