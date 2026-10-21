//! RP2350 POWMAN always-on counter and single alarm, in nominal milliseconds.
//! Clock calibration is caller-supplied; this driver writes only integer kHz and clears the fractional field.
//! IRQ ownership and its software completion latch stay on the constructing core. The async wait does
//! not enter sleep itself: executor/power-management code chooses WFI or DORMANT separately.
//!
//! [RP2350 datasheet](https://datasheets.raspberrypi.com/rp2350/rp2350-datasheet.pdf), sections 6.4 and 12.10,
//! defines password-protected writes, clock selection and wake controls. DORMANT operation requires a
//! clock retained in that power state, normally LPOSC, plus appropriate Secure register access.
//! This API does not configure the full power transition or prove wake/restart behavior.
//! Starting after a stop reloads the programmed SET_TIME value; it does not automatically resume the stopped count.

use core::future::poll_fn;
use core::marker::PhantomData;
use core::sync::atomic::{AtomicBool, Ordering, compiler_fence};
use core::task::Poll;

use embassy_hal_internal::Peri;
use embassy_hal_internal::interrupt::InterruptExt;
use embassy_sync::waitqueue::AtomicWaker;
use embassy_time::Duration;

pub use crate::datetime::{DateTime, DayOfWeek, Error as DateTimeError};
use crate::{interrupt, pac};

/// Required password in the upper half of protected register writes.
const POWMAN_PASSWORD: u32 = 0x5AFE << 16;

/// Single task waiting for interrupt-latched alarm completion.
static WAKER: AtomicWaker = AtomicWaker::new();
/// IRQ-produced completion latch, distinct from the hardware W1C alarm flag.
static ALARM_OCCURRED: AtomicBool = AtomicBool::new(false);

/// Updates only the timer interrupt enable in `powman` to `enabled`, preserving other sources and adding password.
fn set_timer_interrupt(
    powman: pac::powman::Powman,
    enabled: bool,
) {
    powman.inte().modify(|w| {
        w.0 = (w.0 & 0xFFFF) | POWMAN_PASSWORD;
        w.set_timer(enabled);
    });
}

/// Clears hardware W1C alarm in `powman` and software `occurred` under one local interrupt exclusion.
fn clear_alarm_state(
    powman: pac::powman::Powman,
    occurred: &AtomicBool,
) {
    critical_section::with(|_| {
        powman.timer().modify(|w| {
            w.0 = (w.0 & 0x0000FFFF) | POWMAN_PASSWORD;
            w.set_alarm(true); // W1C
        });
        occurred.store(false, Ordering::SeqCst);
    });
}

/// Returns `now` plus `duration_ms` in milliseconds, or AlarmOverflow without wrapping.
fn alarm_after(
    now: u64,
    duration_ms: u64,
) -> Result<u64, Error> {
    now.checked_add(duration_ms).ok_or(Error::AlarmOverflow)
}

/// Checks integer `clock_freq_khz` for `source` before any register writes; panics when unrepresentable.
fn validate_clock_frequency(
    source: ClockSource,
    clock_freq_khz: u32,
) {
    let valid = match source {
        ClockSource::Lposc => (1..=63).contains(&clock_freq_khz),
        ClockSource::Xosc => (2..=65535).contains(&clock_freq_khz),
    };
    assert!(valid, "AON clock is outside its source-specific kHz range");
}

/// Returns `millis` as an Embassy duration; panics if its tick representation cannot fit.
fn counter_duration(millis: u64) -> Duration {
    Duration::try_from_millis(millis).expect("AON counter exceeds Embassy duration range")
}

/// Alarm wake mode configuration
///
/// Controls which low-power wake mechanisms are enabled for the alarm.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum AlarmWakeMode {
    /// Wake from WFI/WFE only (interrupt-based via POWMAN_IRQ_TIMER)
    ///
    /// This is the default and most common mode. The alarm triggers an interrupt
    /// that wakes the CPU from light sleep (WFI/WFE). Works with both XOSC and LPOSC.
    WfiOnly,

    /// Wake from DORMANT mode only (hardware power-up via PWRUP_ON_ALARM)
    ///
    /// The alarm wakes the chip from deep sleep (DORMANT) by triggering a hardware
    /// power-up event. No interrupt is used since the CPU clock is stopped in DORMANT.
    ///
    /// **Requirements:**
    /// - Must use LPOSC clock source (XOSC is powered down in DORMANT)
    /// - Requires Secure privilege level (TIMER register is Secure-only)
    /// - May fail silently in Non-secure contexts
    DormantOnly,

    /// Wake from both WFI/WFE and DORMANT modes
    ///
    /// Enables both interrupt-based wake (WFI/WFE) and hardware power-up wake (DORMANT).
    /// Subject to the same requirements as DormantOnly for DORMANT wake support.
    Both,

    /// Alarm fires but doesn't wake (manual polling only)
    ///
    /// The alarm flag is set in hardware but no wake mechanisms are enabled.
    /// Use `alarm_fired()` to manually poll the alarm status.
    Disabled,
}

/// AON Timer configuration
#[derive(Clone, Copy)]
pub struct Config {
    /// Clock source for the timer
    pub clock_source: ClockSource,
    /// Integer source frequency in kHz: LPOSC 1..=63, XOSC 2..=65535. Fractional calibration is zeroed.
    pub clock_freq_khz: u32,
    /// Alarm wake mode
    ///
    /// Controls which low-power wake mechanisms are enabled for alarms.
    /// See [`AlarmWakeMode`] for details on each mode.
    pub alarm_wake_mode: AlarmWakeMode,
}

impl Default for Config {
    /// Returns 12 MHz XOSC calibration with interrupt-only wake; no hardware writes.
    fn default() -> Self {
        Self {
            clock_source: ClockSource::Xosc,
            clock_freq_khz: 12000, // 12 MHz XOSC
            alarm_wake_mode: AlarmWakeMode::WfiOnly,
        }
    }
}

/// Clock source for the AON Timer
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ClockSource {
    /// Crystal oscillator (more accurate, requires external crystal)
    Xosc,
    /// Low-power oscillator (less accurate, ~32 kHz, available in all power modes)
    Lposc,
}

/// AON Timer errors
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Error {
    /// The alarm time is in the past
    AlarmInPast,
    /// Adding a relative duration would overflow the counter.
    AlarmOverflow,
    /// DateTime conversion error
    DateTime(
        /// Calendar validation or supported epoch-range failure.
        DateTimeError,
    ),
}

/// AON Timer driver
pub struct AonTimer<'d> {
    /// Prevents Send/Sync: the IRQ and completion latch belong to the constructing core.
    _phantom: PhantomData<&'d mut *mut ()>,
    /// Admitted clock calibration and currently selected wake routing.
    config: Config,
}

impl<'d> AonTimer<'d> {
    /// Claims `_inner` and binds `_irq` to this core; returns a driver configured from `config`.
    /// Writes clock calibration, disables alarm/wake routing, clears completion and enables the NVIC line.
    /// Does not stop an already-running timer or set its counter; construct while stopped or while the
    /// selected calibration source is unused. Panics before writes for source-specific invalid kHz values.
    pub fn new(
        _inner: Peri<'d, crate::peripherals::POWMAN>,
        _irq: impl interrupt::typelevel::Binding<interrupt::typelevel::POWMAN_IRQ_TIMER, InterruptHandler> + 'd,
        config: Config,
    ) -> Self {
        validate_clock_frequency(config.clock_source, config.clock_freq_khz);
        let powman = pac::POWMAN;

        match config.clock_source {
            ClockSource::Xosc => {
                powman.xosc_freq_khz_int().write(|w| {
                    w.0 = (config.clock_freq_khz & 0xFFFF) | POWMAN_PASSWORD;
                    *w
                });
                powman.xosc_freq_khz_frac().write(|w| {
                    w.0 = POWMAN_PASSWORD;
                    *w
                });
            }
            ClockSource::Lposc => {
                powman.lposc_freq_khz_int().write(|w| {
                    w.0 = (config.clock_freq_khz & 0xFFFF) | POWMAN_PASSWORD;
                    *w
                });
                powman.lposc_freq_khz_frac().write(|w| {
                    w.0 = POWMAN_PASSWORD;
                    *w
                });
            }
        }

        let mut timer = Self {
            _phantom: PhantomData,
            config,
        };
        timer.disable_alarm_interrupt();
        timer.disable_alarm();
        timer.disable_dormant_wake();
        timer.clear_alarm();
        interrupt::POWMAN_IRQ_TIMER.unpend();
        unsafe { interrupt::POWMAN_IRQ_TIMER.enable() };
        timer
    }

    /// Start the timer
    ///
    /// RUN rising reloads the SET_TIME registers. To resume from a stopped count, explicitly program it first.
    pub fn start(&mut self) {
        let powman = pac::POWMAN;
        powman.timer().modify(|w| {
            w.0 = (w.0 & 0x0000FFFF) | POWMAN_PASSWORD;
            w.set_alarm(false); // Preserve the W1C status.
            match self.config.clock_source {
                ClockSource::Lposc => w.set_use_lposc(true),
                ClockSource::Xosc => w.set_use_xosc(true),
            }
            w.set_run(true);
            *w
        });
    }

    /// Stop the timer
    ///
    /// The timer will stop counting but retain its current value.
    pub fn stop(&mut self) {
        let powman = pac::POWMAN;
        powman.timer().modify(|w| {
            w.0 = (w.0 & 0x0000FFFF) | POWMAN_PASSWORD;
            w.set_alarm(false);
            w.set_run(false);
            *w
        });
    }

    /// Returns the hardware RUN control bit, not confirmation that the selected oscillator is progressing.
    pub fn is_running(&self) -> bool {
        let powman = pac::POWMAN;
        powman.timer().read().run()
    }

    /// Returns the current absolute counter value in nominal milliseconds.
    ///
    /// This reads the 64-bit counter value with rollover protection.
    /// The value includes the programmed starting offset; it is not necessarily boot-relative or Unix time.
    pub fn now(&self) -> u64 {
        let powman = pac::POWMAN;
        // Read with rollover protection: read upper, lower, upper again
        loop {
            let upper1 = powman.read_time_upper().read();
            let lower = powman.read_time_lower().read();
            let upper2 = powman.read_time_upper().read();

            // If upper didn't change, we got a consistent read
            if upper1 == upper2 {
                return ((upper1 as u64) << 32) | (lower as u64);
            }
            // Otherwise retry (rollover occurred)
        }
    }

    /// Programs `value_ms` as the initial millisecond count loaded on the next start.
    ///
    /// This allows you to initialize the counter to any value (e.g., milliseconds since epoch,
    /// or 0 to start counting from boot).
    ///
    /// Panics if RUN is set. Four register writes are sequential; the timer must remain stopped.
    pub fn set_counter(
        &mut self,
        value_ms: u64,
    ) {
        if self.is_running() {
            panic!("timer must be stopped before setting counter");
        }
        let powman = pac::POWMAN;
        powman.set_time_15to0().write(|w| {
            w.0 = ((value_ms & 0xFFFF) as u32) | POWMAN_PASSWORD;
            *w
        });
        powman.set_time_31to16().write(|w| {
            w.0 = (((value_ms >> 16) & 0xFFFF) as u32) | POWMAN_PASSWORD;
            *w
        });
        powman.set_time_47to32().write(|w| {
            w.0 = (((value_ms >> 32) & 0xFFFF) as u32) | POWMAN_PASSWORD;
            *w
        });
        powman.set_time_63to48().write(|w| {
            w.0 = (((value_ms >> 48) & 0xFFFF) as u32) | POWMAN_PASSWORD;
            *w
        });
    }

    /// Sets an absolute millisecond `alarm_ms`, clearing previous completion and applying configured wake routing.
    ///
    /// The alarm will fire when the counter reaches this value.
    /// Returns AlarmInPast for a deadline at or before the sampled current value, leaving the old alarm intact.
    /// Returns Ok after programming; time can advance during setup, so this is not a scheduling-latency guarantee.
    ///
    /// The wake behavior depends on the configured `alarm_wake_mode`:
    /// - `WfiOnly`: Alarm triggers interrupt wake from WFI/WFE
    /// - `DormantOnly`: Alarm triggers power-up from DORMANT mode
    /// - `Both`: Alarm triggers both interrupt and power-up wake
    /// - `Disabled`: Alarm flag is set but no wake occurs
    pub fn set_alarm(
        &mut self,
        alarm_ms: u64,
    ) -> Result<(), Error> {
        let current_ms = self.now();
        if alarm_ms <= current_ms {
            return Err(Error::AlarmInPast);
        }

        self.disable_alarm();
        self.set_alarm_value(alarm_ms);
        self.clear_alarm();

        match self.config.alarm_wake_mode {
            AlarmWakeMode::WfiOnly => {
                self.disable_dormant_wake();
                self.enable_alarm_interrupt();
            }
            AlarmWakeMode::DormantOnly => {
                self.enable_dormant_wake();
                self.disable_alarm_interrupt();
            }
            AlarmWakeMode::Both => {
                self.enable_dormant_wake();
                self.enable_alarm_interrupt();
            }
            AlarmWakeMode::Disabled => {
                self.disable_dormant_wake();
                self.disable_alarm_interrupt();
            }
        }

        self.enable_alarm();

        Ok(())
    }

    /// Enable the alarm interrupt (INTE.TIMER)
    ///
    /// This allows the alarm to trigger POWMAN_IRQ_TIMER and wake from WFI/WFE.
    fn enable_alarm_interrupt(&mut self) { set_timer_interrupt(pac::POWMAN, true); }

    /// Disable the alarm interrupt (INTE.TIMER)
    pub fn disable_alarm_interrupt(&mut self) { set_timer_interrupt(pac::POWMAN, false); }

    /// Writes absolute millisecond `value` through four registers; caller must keep alarm comparison disabled.
    #[inline(always)]
    fn set_alarm_value(
        &mut self,
        value: u64,
    ) {
        let powman = pac::POWMAN;
        powman.alarm_time_15to0().write(|w| {
            w.0 = ((value & 0xFFFF) as u32) | POWMAN_PASSWORD;
            *w
        });
        powman.alarm_time_31to16().write(|w| {
            w.0 = (((value >> 16) & 0xFFFF) as u32) | POWMAN_PASSWORD;
            *w
        });
        powman.alarm_time_47to32().write(|w| {
            w.0 = (((value >> 32) & 0xFFFF) as u32) | POWMAN_PASSWORD;
            *w
        });
        powman.alarm_time_63to48().write(|w| {
            w.0 = (((value >> 48) & 0xFFFF) as u32) | POWMAN_PASSWORD;
            *w
        });
    }

    /// Returns the hardware alarm flag only; the IRQ handler clears it while setting the separate async latch.
    pub fn alarm_fired(&self) -> bool {
        let powman = pac::POWMAN;
        powman.timer().read().alarm()
    }

    /// Clears hardware status and invalidates any pending software completion; does not disable comparison.
    pub fn clear_alarm(&mut self) { clear_alarm_state(pac::POWMAN, &ALARM_OCCURRED); }

    /// Disables comparison without clearing hardware/software completion or interrupt/wake routing.
    pub fn disable_alarm(&mut self) {
        let powman = pac::POWMAN;
        powman.timer().modify(|w| {
            w.0 = (w.0 & 0x0000FFFF) | POWMAN_PASSWORD;
            w.set_alarm(false);
            w.set_alarm_enab(false);
            *w
        });
    }

    /// Enables comparison at the current programmed deadline without clearing previous completion.
    pub fn enable_alarm(&mut self) {
        let powman = pac::POWMAN;
        powman.timer().modify(|w| {
            w.0 = (w.0 & 0x0000FFFF) | POWMAN_PASSWORD;
            w.set_alarm(false);
            w.set_alarm_enab(true);
            *w
        });
    }

    /// Enable DORMANT mode wake on alarm
    ///
    /// Sets the TIMER.PWRUP_ON_ALARM bit to allow the alarm to wake the chip from
    /// DORMANT (deep sleep) mode. This is a hardware power-up event, distinct from
    /// interrupt-based WFI/WFE wake.
    ///
    /// **Security Note**: The TIMER register is Secure-only per the RP2350 datasheet.
    /// This method may fail silently or have no effect when called from Non-secure contexts.
    ///
    /// **Clock Source**: DORMANT wake requires LPOSC as the clock source, since XOSC
    /// is powered down in DORMANT mode.
    pub fn enable_dormant_wake(&mut self) {
        let powman = pac::POWMAN;
        powman.timer().modify(|w| {
            w.0 = (w.0 & 0x0000FFFF) | POWMAN_PASSWORD;
            w.set_alarm(false);
            w.set_pwrup_on_alarm(true);
            *w
        });
    }

    /// Disable DORMANT mode wake on alarm
    ///
    /// Clears the TIMER.PWRUP_ON_ALARM bit. The alarm will no longer wake the chip
    /// from DORMANT mode, but can still wake from WFI/WFE via interrupts.
    ///
    /// **Security Note**: The TIMER register is Secure-only per the RP2350 datasheet.
    /// This method may fail silently or have no effect when called from Non-secure contexts.
    pub fn disable_dormant_wake(&mut self) {
        let powman = pac::POWMAN;
        powman.timer().modify(|w| {
            w.0 = (w.0 & 0x0000FFFF) | POWMAN_PASSWORD;
            w.set_alarm(false);
            w.set_pwrup_on_alarm(false);
            *w
        });
    }

    /// Set the alarm wake mode
    ///
    /// Configures which low-power wake mechanisms are enabled for the alarm.
    /// This immediately updates the hardware configuration.
    ///
    /// # Arguments
    /// * `mode` - The desired wake mode (see [`AlarmWakeMode`])
    ///
    /// # Security Note
    /// Setting modes that involve DORMANT wake (DormantOnly or Both) requires
    /// Secure privilege level. These modes may fail silently in Non-secure contexts.
    pub fn set_wake_mode(
        &mut self,
        mode: AlarmWakeMode,
    ) {
        match mode {
            AlarmWakeMode::WfiOnly => {
                self.enable_alarm_interrupt();
                self.disable_dormant_wake();
            }
            AlarmWakeMode::DormantOnly => {
                self.disable_alarm_interrupt();
                self.enable_dormant_wake();
            }
            AlarmWakeMode::Both => {
                self.enable_alarm_interrupt();
                self.enable_dormant_wake();
            }
            AlarmWakeMode::Disabled => {
                self.disable_alarm_interrupt();
                self.disable_dormant_wake();
            }
        }
        self.config.alarm_wake_mode = mode;
    }

    /// Set an alarm to fire after a duration from now
    ///
    /// Uses `duration` truncated to whole milliseconds; zero/sub-millisecond or setup-expired deadlines return
    /// AlarmInPast. Returns Ok on programming, AlarmOverflow if addition overflows, or AlarmInPast from the second
    /// counter check.
    pub fn set_alarm_after(
        &mut self,
        duration: Duration,
    ) -> Result<(), Error> {
        let current_ms = self.now();
        let alarm_ms = alarm_after(current_ms, duration.as_millis())?;
        self.set_alarm(alarm_ms)
    }

    /// Returns the absolute counter converted to Embassy Duration, including any configured epoch offset.
    /// Panics before overflow if the counter exceeds the Duration tick range (u64::MAX/1000 milliseconds
    /// with the one-microsecond time driver). Use now() to retain the full u64 millisecond domain.
    pub fn elapsed(&self) -> Duration { counter_duration(self.now()) }

    /// Programs the counter from UTC `dt` as Unix-epoch milliseconds; returns Ok after valid conversion.
    /// No-dependency mode ignores supplied weekday; invalid calendar fields and unsupported years return errors.
    ///
    /// # Errors
    /// Returns error if DateTime is before 1970-01-01.
    ///
    /// # Panics
    /// Panics if timer is running.
    pub fn set_datetime(
        &mut self,
        dt: DateTime,
    ) -> Result<(), DateTimeError> {
        #[cfg(feature = "chrono")]
        let millis = crate::datetime::timestamp_millis(&dt)?;
        #[cfg(not(feature = "chrono"))]
        let millis = dt.timestamp_millis()?;

        self.set_counter(millis);
        Ok(())
    }

    /// Returns the counter interpreted as Unix-epoch DateTime, or a conversion/range error.
    /// No-dependency DateTime drops subsecond remainder; this interpretation assumes the caller set a Unix origin.
    ///
    /// # Errors
    /// Returns error if counter value cannot be represented as valid DateTime.
    pub fn now_as_datetime(&self) -> Result<DateTime, DateTimeError> {
        let millis = self.now();

        #[cfg(feature = "chrono")]
        return crate::datetime::from_timestamp_millis(millis);
        #[cfg(not(feature = "chrono"))]
        return DateTime::from_timestamp_millis(millis);
    }

    /// Sets an absolute UTC alarm from `dt`; returns Ok after conversion and future-deadline admission.
    ///
    /// # Errors
    /// Returns error if DateTime conversion fails or alarm time is in the past.
    pub fn set_alarm_at_datetime(
        &mut self,
        dt: DateTime,
    ) -> Result<(), Error> {
        #[cfg(feature = "chrono")]
        let alarm_ms = crate::datetime::timestamp_millis(&dt).map_err(Error::DateTime)?;
        #[cfg(not(feature = "chrono"))]
        let alarm_ms = dt.timestamp_millis().map_err(Error::DateTime)?;

        self.set_alarm(alarm_ms)
    }

    /// Consumes one interrupt-latched completion, then clears hardware/software alarm state.
    /// An already latched IRQ completes immediately. Hardware status alone does not complete this future:
    /// DormantOnly/Disabled modes can wait forever without an IRQ. No sleep instruction is issued here.
    /// Cancellation leaves alarm routing/completion unchanged; another wait can consume the later IRQ.
    pub async fn wait_for_alarm(&mut self) {
        poll_fn(|cx| {
            WAKER.register(cx.waker());

            if ALARM_OCCURRED.swap(false, Ordering::SeqCst) {
                self.clear_alarm();

                compiler_fence(Ordering::SeqCst);
                Poll::Ready(())
            } else {
                Poll::Pending
            }
        })
        .await;
    }
}

/// Interrupt handler for AON Timer alarms
pub struct InterruptHandler {
    /// Prevents external construction; type is used only for interrupt binding.
    _empty: (),
}

impl interrupt::typelevel::Handler<interrupt::typelevel::POWMAN_IRQ_TIMER> for InterruptHandler {
    /// Clears an asserted hardware alarm, disables its comparison/interrupt, latches completion and wakes the waiter.
    ///
    /// # Safety
    /// Must run as the bound POWMAN timer handler on the owning core with exclusive driver register ownership.
    #[inline(always)]
    unsafe fn on_interrupt() {
        let powman = crate::pac::POWMAN;

        if !powman.ints().read().timer() {
            return;
        }
        set_timer_interrupt(powman, false);

        powman.timer().modify(|w| {
            w.0 = (w.0 & 0x0000FFFF) | POWMAN_PASSWORD;
            w.set_alarm_enab(false);
            w.set_alarm(true);
            *w
        });

        ALARM_OCCURRED.store(true, Ordering::SeqCst);
        WAKER.wake();
    }
}

#[cfg(test)]
/// Password/W1C register-model, clock bounds and deadline-overflow regressions.
mod tests {
    use super::*;

    extern crate std;

    /// Exact millisecond values remain monotonic through the final representable duration; larger values panic.
    #[test]
    fn elapsed_duration_rejects_tick_overflow_without_wrapping() {
        // This retained HAL test context uses the same one-microsecond tick as its hardware time driver.
        assert_eq!(embassy_time::TICK_HZ, 1_000_000);
        let maximum_millis = u64::MAX / 1000;
        assert_eq!(counter_duration(0).as_ticks(), 0);
        assert_eq!(counter_duration(123).as_ticks(), 123_000);
        assert_eq!(counter_duration(maximum_millis).as_ticks(), maximum_millis * 1000);
        for millis in [maximum_millis + 1, u64::MAX] {
            assert!(std::panic::catch_unwind(|| counter_duration(millis)).is_err());
        }
    }

    /// Admits each source's actual field endpoints and rejects zero, truncation and overflow values.
    #[test]
    fn clock_frequency_bounds_match_each_source_register() {
        for (source, valid, invalid) in [
            (ClockSource::Lposc, [1, 32, 63], [0, 64, 65535, u32::MAX]),
            (ClockSource::Xosc, [2, 12000, 65535], [0, 1, 65536, u32::MAX]),
        ] {
            for khz in valid {
                validate_clock_frequency(source, khz);
            }
            for khz in invalid {
                assert!(
                    std::panic::catch_unwind(|| validate_clock_frequency(source, khz)).is_err(),
                    "invalid frequency admitted: {source:?} {khz} kHz"
                );
            }
        }
    }

    /// Adds the required write password while preserving unrelated interrupt enable bits.
    #[test]
    fn alarm_interrupt_writes_include_password_and_preserve_other_sources() {
        let mut words = [0u32; 256];
        let regs = unsafe { pac::powman::Powman::from_ptr(words.as_mut_ptr().cast()) };
        regs.inte().write(|w| w.0 = 1);
        set_timer_interrupt(regs, true);
        assert_eq!(regs.inte().read().0 >> 16, 0x5AFE);
        assert!(regs.inte().read().timer());
        assert_eq!(regs.inte().read().0 & 1, 1);
        set_timer_interrupt(regs, false);
        assert_eq!(regs.inte().read().0 >> 16, 0x5AFE);
        assert!(!regs.inte().read().timer());
    }

    /// Invalidates old software completion and emits a password-protected W1C hardware write.
    #[test]
    fn clearing_old_alarm_invalidates_its_software_completion() {
        let mut words = [0u32; 256];
        let regs = unsafe { pac::powman::Powman::from_ptr(words.as_mut_ptr().cast()) };
        let occurred = AtomicBool::new(true);
        clear_alarm_state(regs, &occurred);
        assert!(!occurred.load(Ordering::SeqCst));
        assert!(regs.timer().read().alarm());
        assert_eq!(regs.timer().read().0 >> 16, 0x5AFE);
    }

    /// Returns ordinary deadlines and rejects addition past u64::MAX.
    #[test]
    fn relative_alarm_overflow_is_an_error() {
        assert_eq!(alarm_after(10, 20), Ok(30));
        assert!(alarm_after(u64::MAX, 1).is_err());
    }
}
