//! RP2040 calendar RTC; separate from the RP2350 always-on timer.
//! Register order follows the [SDK RTC driver](https://github.com/raspberrypi/pico-sdk/blob/master/src/rp2_common/hardware_rtc/rtc.c).
mod conversions;
mod filter;

use core::future::poll_fn;
use core::sync::atomic::{AtomicBool, Ordering, compiler_fence};
use core::task::Poll;

use embassy_hal_internal::{Peri, PeripheralType};
use embassy_sync::waitqueue::AtomicWaker;

pub use self::filter::DateTimeFilter;
use crate::clocks::clk_rtc_freq;
pub use crate::datetime::{DateTime, DayOfWeek, Error as DateTimeError};
use crate::interrupt::typelevel::Binding;
use crate::interrupt::{self, InterruptExt};

/// One async alarm waiter for the exclusive RTC singleton.
static WAKER: AtomicWaker = AtomicWaker::new();
/// Latched alarm occurrence; scheduling/disabling does not itself clear this flag.
static ALARM_OCCURRED: AtomicBool = AtomicBool::new(false);

/// A reference to the real time clock of the system
pub struct Rtc<'d, T: Instance> {
    /// Exclusive RTC peripheral ownership.
    inner: Peri<'d, T>,
}

impl<'d, T: Instance> Rtc<'d, T> {
    /// Returns a driver owning `inner` with handler binding `_irq`, sets the1 Hz divider and enables its NVIC line.
    /// Does not load a date or start the clock. Configured RTC frequency must be nonzero and fit the divider.
    pub fn new(
        inner: Peri<'d, T>,
        _irq: impl Binding<interrupt::typelevel::RTC_IRQ, InterruptHandler>,
    ) -> Self {
        // Set the RTC divider
        inner.regs().clkdiv_m1().write(|w| w.set_clkdiv_m1(clk_rtc_freq() - 1));

        // Setup the IRQ
        // Clear any pending interrupts from the RTC_IRQ interrupt and enable it, so we do not have unexpected
        // interrupts after initialization
        interrupt::RTC_IRQ.unpend();
        unsafe { interrupt::RTC_IRQ.enable() };

        Self { inner }
    }

    /// Enable or disable the leap year check. The rp2040 chip will always add a Feb 29th on every year that is
    /// divisible by 4, but this may be incorrect (e.g. on century years). This function allows you to disable this
    /// check.
    ///
    /// `leap_year_check_enabled` selects this check; restore/set_datetime rewrite CTRL and clear the override.
    pub fn set_leap_year_check(
        &mut self,
        leap_year_check_enabled: bool,
    ) {
        self.inner.regs().ctrl().modify(|w| {
            w.set_force_notleapyear(!leap_year_check_enabled);
        });
    }

    /// Loads raw date `ymd` and time `hms`, then starts the clock; neither record is validated.
    /// Busy-waits without a deadline for stop/start acknowledgment; clears other CTRL bits including leap override.
    pub fn restore(
        &mut self,
        ymd: rp_pac::rtc::regs::Rtc1,
        hms: rp_pac::rtc::regs::Rtc0,
    ) {
        // disable RTC while we configure it
        self.inner.regs().ctrl().modify(|w| w.set_rtc_enable(false));
        while self.inner.regs().ctrl().read().rtc_active() {
            core::hint::spin_loop();
        }

        self.inner.regs().setup_0().write(|w| {
            *w = rp_pac::rtc::regs::Setup0(ymd.0);
        });
        self.inner.regs().setup_1().write(|w| {
            *w = rp_pac::rtc::regs::Setup1(hms.0);
        });

        // Load the new datetime and re-enable RTC
        self.inner.regs().ctrl().write(|w| w.set_load(true));
        self.inner.regs().ctrl().write(|w| w.set_rtc_enable(true));
        while !self.inner.regs().ctrl().read().rtc_active() {
            core::hint::spin_loop();
        }
    }

    /// Returns raw date/time words, reading RTC0 before RTC1 as required by the hardware snapshot latch.
    /// Does not require a running or calendar-valid clock.
    pub fn save(&mut self) -> (rp_pac::rtc::regs::Rtc1, rp_pac::rtc::regs::Rtc0) {
        let rtc_0: rp_pac::rtc::regs::Rtc0 = self.inner.regs().rtc_0().read();
        let rtc_1 = self.inner.regs().rtc_1().read();
        (rtc_1, rtc_0)
    }

    /// Returns the hardware RTC_ACTIVE indication, not date validity.
    pub fn is_running(&self) -> bool { self.inner.regs().ctrl().read().rtc_active() }

    /// Loads validated `t`, starts the RTC, and returns Ok or InvalidDateTime before any hardware change.
    /// Waits without a deadline for stop/start; subsecond data and previous CTRL leap override are discarded.
    ///
    /// # Errors
    ///
    /// Will return `RtcError::InvalidDateTime` if the datetime is not a valid range.
    pub fn set_datetime(
        &mut self,
        t: DateTime,
    ) -> Result<(), RtcError> {
        conversions::validate_datetime(&t).map_err(RtcError::InvalidDateTime)?;

        // disable RTC while we configure it
        self.inner.regs().ctrl().modify(|w| w.set_rtc_enable(false));
        while self.inner.regs().ctrl().read().rtc_active() {
            core::hint::spin_loop();
        }

        self.inner.regs().setup_0().write(|w| {
            conversions::write_setup_0(&t, w);
        });
        self.inner.regs().setup_1().write(|w| {
            conversions::write_setup_1(&t, w);
        });

        // Load the new datetime and re-enable RTC
        self.inner.regs().ctrl().write(|w| w.set_load(true));
        self.inner.regs().ctrl().write(|w| w.set_rtc_enable(true));
        while !self.inner.regs().ctrl().read().rtc_active() {
            core::hint::spin_loop();
        }
        Ok(())
    }

    /// Returns the current whole-second snapshot, or NotRunning/InvalidDateTime.
    ///
    /// # Errors
    ///
    /// Plain dates validate only weekday on read; chrono also rejects impossible calendar/time fields.
    pub fn now(&self) -> Result<DateTime, RtcError> {
        if !self.is_running() {
            return Err(RtcError::NotRunning);
        }

        let rtc_0 = self.inner.regs().rtc_0().read();
        let rtc_1 = self.inner.regs().rtc_1().read();

        conversions::datetime_from_registers(rtc_0, rtc_1).map_err(RtcError::InvalidDateTime)
    }

    /// Disables the match comparator and waits without a deadline for acknowledgment.
    /// Leaves the software occurrence latch unchanged. Alarm configured by [`schedule_alarm`].
    ///
    /// [`schedule_alarm`]: #method.schedule_alarm
    pub fn disable_alarm(&mut self) {
        self.inner.regs().irq_setup_0().modify(|s| s.set_match_ena(false));

        while self.inner.regs().irq_setup_0().read().match_active() {
            core::hint::spin_loop();
        }
    }

    /// Replaces the comparator with `filter`, enables RTC interrupts and waits for match activation.
    /// Field ranges are not validated; absent fields match any value. The IRQ disables the comparator,
    /// so recurring alarms must be rearmed. An older software occurrence latch is not cleared here.
    ///
    /// ```no_run
    /// # #[cfg(not(feature = "chrono"))]
    /// fn next_minute(rtc: &mut embassy_rp::rtc::Rtc<'_, embassy_rp::peripherals::RTC>) {
    ///     let now = rtc.now().unwrap();
    ///     rtc.schedule_alarm(
    ///         embassy_rp::rtc::DateTimeFilter::default().minute((now.minute + 1) % 60),
    ///     );
    /// }
    /// ```
    pub fn schedule_alarm(
        &mut self,
        filter: DateTimeFilter,
    ) {
        self.disable_alarm();

        self.inner.regs().irq_setup_0().write(|w| {
            filter.write_setup_0(w);
        });
        self.inner.regs().irq_setup_1().write(|w| {
            filter.write_setup_1(w);
        });

        self.inner.regs().inte().modify(|w| w.set_rtc(true));

        // Set the enable bit and check if it is set
        self.inner.regs().irq_setup_0().modify(|w| w.set_match_ena(true));
        while !self.inner.regs().irq_setup_0().read().match_active() {
            core::hint::spin_loop();
        }
    }

    /// Disables alarm matching to clear its hardware condition; does not consume the software occurrence latch.
    /// The bound IRQ handler already disables matching; [`schedule_alarm`] rearms it.
    ///
    /// [`schedule_alarm`]: #method.schedule_alarm
    pub fn clear_interrupt(&mut self) { self.disable_alarm(); }

    /// Check if an alarm is scheduled.
    ///
    /// This function checks if the RTC alarm is currently active. If it is, it returns the alarm configuration
    /// as a [`DateTimeFilter`]. Returns None when inactive or when an enabled weekday field is invalid.
    pub fn alarm_scheduled(&self) -> Option<DateTimeFilter> {
        // Check if alarm is active
        if !self.inner.regs().irq_setup_0().read().match_active() {
            return None;
        }

        // Get values from both alarm registers
        let irq_0 = self.inner.regs().irq_setup_0().read();
        let irq_1 = self.inner.regs().irq_setup_1().read();

        // Create a DateTimeFilter and populate it based on which fields are enabled
        let mut filter = DateTimeFilter::default();

        if irq_0.year_ena() {
            filter.year = Some(irq_0.year());
        }

        if irq_0.month_ena() {
            filter.month = Some(irq_0.month());
        }

        if irq_0.day_ena() {
            filter.day = Some(irq_0.day());
        }

        if irq_1.dotw_ena() {
            // Convert day of week value to DayOfWeek enum
            let Ok(day_of_week) = conversions::day_of_week_from_u8(irq_1.dotw()) else {
                return None; // Invalid day of week
            };
            filter.day_of_week = Some(day_of_week);
        }

        if irq_1.hour_ena() {
            filter.hour = Some(irq_1.hour());
        }

        if irq_1.min_ena() {
            filter.minute = Some(irq_1.min());
        }

        if irq_1.sec_ena() {
            filter.second = Some(irq_1.sec());
        }

        Some(filter)
    }

    /// Wait for an RTC alarm to trigger.
    ///
    /// This function will wait until the RTC alarm is triggered. If the alarm is already triggered, it will return
    /// immediately. If no occurrence is latched, waits indefinitely for the bound handler. Cancellation leaves the
    /// alarm and latch intact; successful completion consumes one latched occurrence and disables matching.
    /// A previously latched occurrence can therefore complete a newly created wait immediately.
    pub async fn wait_for_alarm(&mut self) {
        poll_fn(|cx| {
            WAKER.register(cx.waker());

            // Atomically check and clear the alarm occurred flag to prevent race conditions
            if critical_section::with(|_| {
                let occurred = ALARM_OCCURRED.load(Ordering::SeqCst);
                if occurred {
                    ALARM_OCCURRED.store(false, Ordering::SeqCst);
                }
                occurred
            }) {
                // Clear the interrupt and disable the alarm
                self.clear_interrupt();

                compiler_fence(Ordering::SeqCst);
                return Poll::Ready(());
            } else {
                return Poll::Pending;
            }
        })
        .await;
    }
}

/// Interrupt handler.
pub struct InterruptHandler {
    /// Zero-sized marker; peripheral binding supplies interrupt ownership.
    _empty: (),
}

impl crate::interrupt::typelevel::Handler<crate::interrupt::typelevel::RTC_IRQ> for InterruptHandler {
    /// Disables matching, latches occurrence and wakes the waiter without waiting for hardware acknowledgment.
    /// # Safety
    /// Must be invoked only for the owned RTC IRQ, with no competing direct register owner.
    unsafe fn on_interrupt() {
        // Disable the alarm first thing, to prevent unexpected re-entry
        let rtc = crate::pac::RTC;
        rtc.irq_setup_0().modify(|w| w.set_match_ena(false));

        // Set the alarm occurred flag and wake the waker
        ALARM_OCCURRED.store(true, Ordering::SeqCst);
        WAKER.wake();
    }
}

/// Errors that can occur on methods on [Rtc]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RtcError {
    /// An invalid DateTime was given or stored on the hardware.
    InvalidDateTime(
        /// Backend-specific invalid field/date/time reason.
        DateTimeError,
    ),

    /// The RTC clock is not running
    NotRunning,
}

/// Private RTC register access for the singleton token.
trait SealedInstance {
    /// Returns the instance register block.
    fn regs(&self) -> crate::pac::rtc::Rtc;
}

/// RTC peripheral instance.
#[allow(private_bounds)]
pub trait Instance: SealedInstance + PeripheralType {}

impl SealedInstance for crate::peripherals::RTC {
    /// Returns the RP2040 RTC register block.
    fn regs(&self) -> crate::pac::rtc::Rtc { crate::pac::RTC }
}
impl Instance for crate::peripherals::RTC {}
