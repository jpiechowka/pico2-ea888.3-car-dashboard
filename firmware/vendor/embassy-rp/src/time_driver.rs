//! One-microsecond monotonic time driver using TIMER/TIMER0 alarm0 and a shared wake queue.
//! Both cores serialize queue/register updates; the initialized core owns the alarm IRQ.
use core::cell::{Cell, RefCell};

use critical_section::CriticalSection;
use embassy_sync::blocking_mutex::Mutex;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_time_driver::Driver;
use embassy_time_queue_utils::Queue;
#[cfg(feature = "rp2040")]
use pac::TIMER;
#[cfg(feature = "_rp235x")]
use pac::TIMER0 as TIMER;

use crate::interrupt::InterruptExt;
use crate::{interrupt, pac};

/// Full 64-bit deadline behind the low 32-bit hardware comparator.
struct AlarmState {
    /// Absolute microsecond deadline; u64::MAX represents no queued expiration.
    timestamp: Cell<u64>,
}
unsafe impl Send for AlarmState {}

/// Global Embassy time service; alarm0 must not be independently repurposed.
struct TimerDriver {
    /// Deadline protected by the cross-core critical-section mutex.
    alarms: Mutex<CriticalSectionRawMutex, AlarmState>,
    /// Registered task wake deadlines; queue operations run under the same exclusion.
    queue: Mutex<CriticalSectionRawMutex, RefCell<Queue>>,
}

/// Process-wide timer driver backing the Embassy time ABI.
static DRIVER: TimerDriver = TimerDriver {
    alarms: Mutex::const_new(
        CriticalSectionRawMutex::new(),
        AlarmState {
            timestamp: Cell::new(0),
        },
    ),
    queue: Mutex::new(RefCell::new(Queue::new())),
};

/// Returns the global monotonic microsecond counter through the Embassy time ABI.
#[unsafe(no_mangle)]
#[inline]
fn _embassy_time_now() -> u64 { <TimerDriver as Driver>::now(&DRIVER) }

/// Schedules `waker` for absolute microsecond `at` through the Embassy time ABI.
#[unsafe(no_mangle)]
#[inline]
fn _embassy_time_schedule_wake(
    at: u64,
    waker: &core::task::Waker,
) {
    <TimerDriver as Driver>::schedule_wake(&DRIVER, at, waker);
}

impl Driver for TimerDriver {
    /// Returns a coherent 64-bit microsecond counter, retrying when the high word changes across reads.
    fn now(&self) -> u64 {
        loop {
            let hi = TIMER.timerawh().read();
            let lo = TIMER.timerawl().read();
            let hi2 = TIMER.timerawh().read();
            if hi == hi2 {
                return (hi as u64) << 32 | (lo as u64);
            }
        }
    }

    /// Schedules `waker` at absolute microsecond `at`, waking overdue entries and retrying elapsed alarm setup.
    /// Queue policy may coalesce a task to its earlier deadline; no individual cancellation handle is returned.
    fn schedule_wake(
        &self,
        at: u64,
        waker: &core::task::Waker,
    ) {
        critical_section::with(|cs| {
            let mut queue = self.queue.borrow(cs).borrow_mut();

            if queue.schedule_wake(at, waker) {
                let mut next = queue.next_expiration(self.now());
                while !self.set_alarm(cs, next) {
                    next = queue.next_expiration(self.now());
                }
            }
        })
    }
}

impl TimerDriver {
    /// Under held exclusion `cs`, arms low 32 bits of `timestamp`; returns false and disarms if already elapsed.
    /// Future high-word matches are checked again by IRQ, so far deadlines may cause early wrap interrupts.
    fn set_alarm(
        &self,
        cs: CriticalSection,
        timestamp: u64,
    ) -> bool {
        let n = 0;
        let alarm = &self.alarms.borrow(cs);
        alarm.timestamp.set(timestamp);

        // Arm it.
        // Note that we're not checking the high bits at all. This means the irq may fire early
        // if the alarm is more than 72 minutes (2^32 us) in the future. This is OK, since on irq fire
        // it is checked if the alarm time has passed.
        TIMER.alarm(n).write_value(timestamp as u32);

        let now = self.now();
        if timestamp <= now {
            // If alarm timestamp has passed the alarm will not fire.
            // Disarm the alarm and return `false` to indicate that.
            TIMER.armed().write(|w| w.set_armed(1 << n));

            alarm.timestamp.set(u64::MAX);

            false
        } else {
            true
        }
    }

    /// Acknowledges alarm0, wakes due queue entries, or rearms an early low-word wrap match.
    fn check_alarm(&self) {
        let n = 0;
        critical_section::with(|cs| {
            // clear the irq
            TIMER.intr().write(|w| w.set_alarm(n, true));

            let alarm = &self.alarms.borrow(cs);
            let timestamp = alarm.timestamp.get();
            if timestamp <= self.now() {
                self.trigger_alarm(cs)
            } else {
                // Not elapsed, arm it again.
                // This can happen if it was set more than 2^32 us in the future.
                TIMER.alarm(n).write_value(timestamp as u32);
            }
        });
    }

    /// With `cs` held, wakes expired entries and retries until the next deadline is armed in the future.
    fn trigger_alarm(
        &self,
        cs: CriticalSection,
    ) {
        let mut next = self.queue.borrow(cs).borrow_mut().next_expiration(self.now());
        while !self.set_alarm(cs, next) {
            next = self.queue.borrow(cs).borrow_mut().next_expiration(self.now());
        }
    }
}

/// Initializes the global no-deadline state and enables alarm0 on the current core.
/// # Safety
/// Must run exactly once at boot after the 1 MHz timer clock is configured, before timer users.
/// Alarm0 and its IRQ must remain exclusively owned by this driver.
pub unsafe fn init() {
    // init alarms
    critical_section::with(|cs| {
        let alarm = DRIVER.alarms.borrow(cs);
        alarm.timestamp.set(u64::MAX);
    });

    // enable irq
    TIMER.inte().write(|w| {
        w.set_alarm(0, true);
    });
    #[cfg(feature = "rp2040")]
    {
        interrupt::TIMER_IRQ_0.enable();
    }
    #[cfg(feature = "_rp235x")]
    {
        interrupt::TIMER0_IRQ_0.enable();
    }
}

#[cfg(all(feature = "rt", feature = "rp2040"))]
#[interrupt]
/// RP2040 alarm0 interrupt entry; dispatches deadline verification and queued wakes.
fn TIMER_IRQ_0() { DRIVER.check_alarm() }

#[cfg(all(feature = "rt", feature = "_rp235x"))]
#[interrupt]
/// RP235x TIMER0 alarm0 interrupt entry; dispatches deadline verification and queued wakes.
fn TIMER0_IRQ_0() { DRIVER.check_alarm() }
