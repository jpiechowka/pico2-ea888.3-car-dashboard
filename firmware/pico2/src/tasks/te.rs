//! Optional TE edge observations; software timestamps are not scanout measurements.
use core::cell::RefCell;

use dashboard_pico2::display::te::TeObservations;
use embassy_rp::gpio::Input;
use embassy_sync::blocking_mutex::Mutex;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_time::Instant;

// Probe-visible bounded history, with the same monotonic microsecond time base
// as FRAME_TRACES. No copying/formatting or logging inside the edge loop.
/// Bounded TE history sharing frame-trace monotonic microsecond units.
pub static TE_OBSERVATIONS: Mutex<CriticalSectionRawMutex, RefCell<TeObservations>> =
    Mutex::new(RefCell::new(TeObservations::new()));

/// Invalidates interval continuity across flash IRQ masking without canceling the
/// pin future; the pending result is discarded and fresh edge evidence is required.
pub fn mark_maintenance_gap() { TE_OBSERVATIONS.lock(|edges| edges.borrow_mut().mark_gap()); }

#[embassy_executor::task]
/// Owns `pin` on core0 indefinitely, awaiting rising edges and recording wake-time
/// microseconds. Never cancel/switch the live waiter to manufacture an interval.
pub async fn observe_te(mut pin: Input<'static>) {
    loop {
        // Keep one core0 waiter; the reviewed HAL disarms its originating IRQ
        // bank on Drop. This observer never cancels or switches trigger type.
        pin.wait_for_rising_edge().await;
        let observed_us = Instant::now().as_micros();
        TE_OBSERVATIONS.lock(|edges| {
            edges.borrow_mut().record(observed_us);
        });
    }
}

/// Snapshot before logging so RTT cannot extend the ring's critical section.
pub fn log_status() {
    let now = Instant::now().as_micros();
    let (interval, age) = TE_OBSERVATIONS.lock(|edges| {
        let edges = edges.borrow();
        (edges.latest_interval_us(), edges.age_us(now))
    });
    match (interval, age) {
        (Some(interval), Some(age)) => crate::log_info!("TE obs={}us age={}us", interval, age),
        _ => crate::log_info!("TE: waiting for two observations"),
    }
}
