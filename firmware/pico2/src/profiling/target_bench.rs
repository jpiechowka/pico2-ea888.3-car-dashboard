//! Opt-in boot experiment. No DMA/panel operations or watchdog feeding.
use core::hint::black_box;
use core::sync::atomic::Ordering;

use dashboard_pico2::display::FrameBuffer;
use dashboard_pico2::display::framebuffer::FRAME_BYTES;
use dashboard_pico2::microbench::workload::{Fixture, Kernel};
use dashboard_pico2::microbench::{self, Budget, Counter, Error, ITERATIONS, Measurement, Timebase, TimerPort};
use embassy_rp::{clocks, pac};
use embassy_time::Instant;

/// Timer0 register boundary for the temporary benchmark debug-pause guard.
struct HardwareTimer;
impl TimerPort for HardwareTimer {
    /// Returns live Timer0 tick/source/debug-pause metadata without changing the timebase.
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

    /// Writes Timer0 debug-pause `value` and completes peripheral ordering before return.
    fn write_debug_pause(
        &mut self,
        value: u32,
    ) {
        pac::TIMER0.dbgpause().write_value(pac::timer::regs::Dbgpause(value));
        cortex_m::asm::dsb();
    }
}

/// Returns secure core0 DWT_CTRL and DEMCR register snapshots without stealing
/// a singleton or resetting/disabling the shared counter.
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
/// Returns controls, wall microseconds then DWT cycles sampled in start order.
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
/// Returns DWT cycles, wall microseconds then controls sampled in end order,
/// bounding the work interval conservatively against wall time.
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
/// Returns success only on privileged interruptible core0 with an active
/// unpaused 1 MHz watchdog tick and at least five seconds remaining; otherwise
/// returns Context or Watchdog without feeding or altering watchdog state.
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
/// One finite kernel result retained until timer restoration and deferred logging.
struct Sample {
    /// Kernel identity fixing input workload and expected checksum.
    kind: Kernel,
    /// Start-order raw counter/time/control snapshot.
    before: Counter,
    /// End-order raw counter/time/control snapshot.
    after: Counter,
    /// Observed output checksum, compared with the kernel fixture contract.
    checksum: u32,
    /// Zero on accepted sample, otherwise the shared benchmark Error discriminant.
    status: u8,
}

/// Retain a named symbol for optimized-code review of every finite iteration.
#[inline(never)]
/// Runs `kind` for exactly ITERATIONS using mutable `fixture` and `frame`,
/// retaining optimizer-visible effects and completing CPU writes with DSB.
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

/// Mutates exclusively borrowed SRAM `frame` with finite benchmark kernels and
/// emits results only after timing/debug-pause restoration. Rejects debug builds,
/// bad address/alignment, absent clock/voltage metadata or invalid counter/timebase/
/// watchdog evidence; retains partial sample diagnostics. Never feeds watchdog,
/// uses DMA, or increments production display completion/sample counters.
pub fn run(frame: &mut FrameBuffer) {
    let mut samples = [None; 4];
    let address = frame.as_ptr() as usize;
    let cpu = clocks::clk_sys_freq();
    let spi_hz = crate::get_actual_spi_freq(clocks::clk_peri_freq());
    let vreg_mv = crate::vreg_setting_mv();
    let watchdog_before = pac::WATCHDOG.ctrl().read().time();
    let pause_before = pac::TIMER0.dbgpause().read().0;
    let core1_before = crate::tasks::recovery::ACQUISITION_PROGRESS.load(Ordering::Relaxed);
    let (dwt, demcr) = controls();
    let result = (|| {
        context()?;
        // Missing divider metadata rejects the run, never becoming a 0-Hz
        // successful record or silently using the requested SPI frequency.
        spi_hz.ok_or(Error::Frequency)?;
        vreg_mv.ok_or(Error::Context)?;
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
    if let (Some(spi_hz), Some(vreg_mv)) = (spi_hz, vreg_mv) {
        defmt::info!(
            "BENCHCLOCK1,{},{},{},{},{},{}",
            cpu,
            clocks::clk_peri_freq(),
            clocks::clk_ref_freq(),
            vreg_mv,
            crate::display_spi_config().frequency,
            spi_hz
        );
    }
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
