//! Watchdog
//!
//! The watchdog is a countdown timer that can restart parts of the chip if it reaches zero. This can be used to restart
//! the processor if software gets stuck in an infinite loop. The programmer must periodically write a value to the
//! watchdog to stop it from reaching zero.
//!
//! Credit: based on `rp-hal` implementation (also licensed Apache+MIT)

use core::marker::PhantomData;

use embassy_time::Duration;

use crate::peripherals::WATCHDOG;
use crate::{Peri, pac};

/// The reason for a system reset from the watchdog.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ResetReason {
    /// The reset was forced.
    Forced,
    /// The hardware TIMER reason bit is set. On RP2350 a ROM reboot can also
    /// produce this reason; it does not alone prove a missed application feed.
    TimedOut,
}

/// Watchdog peripheral
pub struct Watchdog {
    /// Owns the unique WATCHDOG peripheral token without retaining an MMIO pointer.
    phantom: PhantomData<WATCHDOG>,
}

impl Watchdog {
    /// Create a new `Watchdog`
    /// Consumes `_watchdog` and returns the sole handle without starting or feeding it.
    pub fn new(_watchdog: Peri<'static, WATCHDOG>) -> Self { Self { phantom: PhantomData } }

    /// Start tick generation on clk_tick which is driven from clk_ref.
    ///
    /// # Arguments
    ///
    /// * `cycles` - Total number of tick cycles before the next tick is generated. It is expected to be the frequency
    ///   in MHz of clk_ref.
    /// `cycles` divides the reference clock into watchdog ticks; this RP2040-only
    /// method enables generation but does not measure the reference frequency.
    #[cfg(feature = "rp2040")]
    pub fn enable_tick_generation(
        &mut self,
        cycles: u8,
    ) {
        let watchdog = pac::WATCHDOG;
        watchdog.tick().write(|w| {
            w.set_enable(true);
            w.set_cycles(cycles.into())
        });
    }

    /// Defines whether or not the watchdog timer should be paused when processor(s) are in debug mode
    /// or when JTAG is accessing bus fabric
    /// `pause` sets all three debug/JTAG pause controls together.
    pub fn pause_on_debug(
        &mut self,
        pause: bool,
    ) {
        let watchdog = pac::WATCHDOG;
        watchdog.ctrl().modify(|w| {
            w.set_pause_dbg0(pause);
            w.set_pause_dbg1(pause);
            w.set_pause_jtag(pause);
        })
    }

    /// Writes raw countdown `counter` units to LOAD; callers apply chip-specific
    /// tick conversion and range checks before this register boundary.
    fn load_counter(
        &self,
        counter: u32,
    ) {
        let watchdog = pac::WATCHDOG;
        watchdog.load().write_value(pac::watchdog::regs::Load(counter));
    }

    /// Sets countdown enable to `bit`, preserving the other CTRL settings.
    fn enable(
        &self,
        bit: bool,
    ) {
        let watchdog = pac::WATCHDOG;
        watchdog.ctrl().modify(|w| w.set_enable(bit))
    }

    /// Writes the retained upstream RP2040 reset-selection mask. RP2350 has a
    /// different PSM bit layout; this repository's RecoveryWatchdog immediately
    /// corrects WDSEL after start and never calls start/trigger_reset again.
    /// Do not infer an RP2350 full-chip reset guarantee from this helper.
    fn configure_wdog_reset_triggers(&self) {
        let psm = pac::PSM;
        psm.wdsel().write_value(pac::psm::regs::Wdsel(
            0x0001FFFF & !(0x01 << 0usize) & !(0x01 << 1usize),
        ));
    }

    /// Feed the watchdog timer
    /// Reloads `timeout` in microseconds using the configured 1 MHz tick; this
    /// does not enable a stopped watchdog. Panics above 0xffffff/2 microseconds
    /// on RP2040 or 0xffffff on RP235x. Zero is accepted without special treatment.
    pub fn feed(
        &mut self,
        timeout: Duration,
    ) {
        #[cfg(feature = "rp2040")]
        /// Maximum microseconds after compensating RP2040-E1's double decrement.
        const MAX_PERIOD: u32 = 0xFFFFFF / 2;
        #[cfg(feature = "_rp235x")]
        /// RP235x's full 24-bit countdown range in 1 MHz tick microseconds.
        const MAX_PERIOD: u32 = 0xFFFFFF;

        let timeout_us = timeout.as_micros();
        if timeout_us > (MAX_PERIOD) as u64 {
            panic!("Period cannot exceed {} microseconds", MAX_PERIOD);
        }
        let timeout_us = timeout_us as u32;

        // Due to a logic error, the watchdog decrements by 2 and
        // the load value must be compensated; see RP2040-E1
        // This errata is fixed in the RP235x
        let load_value = if cfg!(feature = "rp2040") {
            timeout_us * 2
        } else {
            timeout_us
        };

        self.load_counter(load_value)
    }

    /// Start the watchdog timer
    /// Disables countdown, applies this HAL's reset mask, loads `initial_timeout`
    /// and reenables it. The timeout has [`Self::feed`]'s limits; on panic the
    /// timer remains disabled. RP2350 callers must correct the reset mask.
    pub fn start(
        &mut self,
        initial_timeout: Duration,
    ) {
        self.enable(false);
        self.configure_wdog_reset_triggers();
        self.feed(initial_timeout);
        self.enable(true);
    }

    /// Stop the watchdog timer
    /// Clears enable without altering reset selection, scratch data or tick setup.
    pub fn stop(&mut self) { self.enable(false); }

    /// Trigger a system reset
    /// Writes this HAL's reset mask, disables debug pause, enables and triggers
    /// the timer. The Rust function may return before reset; the retained upstream
    /// reset-selection helper is RP2040-specific and is not used by the RP2350 wrapper.
    pub fn trigger_reset(&mut self) {
        self.configure_wdog_reset_triggers();
        self.pause_on_debug(false);
        self.enable(true);
        let watchdog = pac::WATCHDOG;
        watchdog.ctrl().write(|w| {
            w.set_trigger(true);
        })
    }

    /// Store data in scratch register
    /// Writes `value` to `index` 0..=7, panicking otherwise. Retention and ROM use
    /// vary by reset path; RP2350 ROM reboot parameters/vectors can consume scratch.
    pub fn set_scratch(
        &mut self,
        index: usize,
        value: u32,
    ) {
        let watchdog = pac::WATCHDOG;
        match index {
            0 => watchdog.scratch0().write(|w| *w = value),
            1 => watchdog.scratch1().write(|w| *w = value),
            2 => watchdog.scratch2().write(|w| *w = value),
            3 => watchdog.scratch3().write(|w| *w = value),
            4 => watchdog.scratch4().write(|w| *w = value),
            5 => watchdog.scratch5().write(|w| *w = value),
            6 => watchdog.scratch6().write(|w| *w = value),
            7 => watchdog.scratch7().write(|w| *w = value),
            _ => panic!("Invalid watchdog scratch index"),
        }
    }

    /// Read data from scratch register
    /// Returns raw register `index` 0..=7, panicking otherwise; does not validate
    /// a record or establish which boot/reset path last wrote it.
    pub fn get_scratch(
        &mut self,
        index: usize,
    ) -> u32 {
        let watchdog = pac::WATCHDOG;
        match index {
            0 => watchdog.scratch0().read(),
            1 => watchdog.scratch1().read(),
            2 => watchdog.scratch2().read(),
            3 => watchdog.scratch3().read(),
            4 => watchdog.scratch4().read(),
            5 => watchdog.scratch5().read(),
            6 => watchdog.scratch6().read(),
            7 => watchdog.scratch7().read(),
            _ => panic!("Invalid watchdog scratch index"),
        }
    }

    /// Get the reason for the last system reset, if it was caused by the watchdog.
    /// Returns Forced before TimedOut if both bits are present, or None for neither.
    /// On RP2350 TIMER may also indicate deliberate ROM reboot; inspect additional
    /// boot evidence before attributing an application failure.
    pub fn reset_reason(&self) -> Option<ResetReason> {
        let watchdog = pac::WATCHDOG;
        let reason = watchdog.reason().read();
        if reason.force() {
            Some(ResetReason::Forced)
        } else if reason.timer() {
            Some(ResetReason::TimedOut)
        } else {
            None
        }
    }
}
