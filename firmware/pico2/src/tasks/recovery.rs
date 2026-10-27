//! Independent progress supervision and finite maintenance watchdog tickets.
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

#[cfg(feature = "ble-obd")]
use dashboard_pico2::health::reset::RecoveryCause;
use dashboard_pico2::health::{MaintenanceTicket, Progress, ProgressMonitor};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_time::{Instant, Timer};

use super::COMPLETED_FRAMES;
use crate::drivers::watchdog::RecoveryWatchdog;

// These are independent liveness counters, not a coherent sensor snapshot or
// buffer-ownership protocol. Wrap is allowed; the monitor tests for change.
/// Wrapping acquisition liveness sequence; only change, not magnitude, is observed.
pub static ACQUISITION_PROGRESS: AtomicU32 = AtomicU32::new(0);
/// Wrapping completed renderer-iteration sequence, including unchanged scenes.
pub static RENDER_PROGRESS: AtomicU32 = AtomicU32::new(0);
/// Wrapping input-poll progress sequence; unrelated to button event counts.
pub static INPUT_PROGRESS: AtomicU32 = AtomicU32::new(0);
/// Renderer indication of outstanding display work requiring eventual completion.
pub static DISPLAY_PENDING: AtomicBool = AtomicBool::new(false);

/// Sole main-loop client's request to the watchdog owner.
enum Request {
    /// Request one bounded maintenance ticket after ordinary health validation.
    Begin,
    /// Return the unique ticket after all participating owners resumed.
    End(
        /// Move-only ticket previously granted by this monitor.
        MaintenanceTicket,
    ),
}
/// Acknowledgement paired with the sole client's outstanding request.
enum Reply {
    /// Admission completed and exactly one entry feed was issued.
    Begun(
        /// Ticket retained across the complete maintenance window.
        MaintenanceTicket,
    ),
    /// Ticket ended successfully; ordinary progress monitoring is restored.
    Ended,
}
/// Capacity-one client mailbox; another concurrent requester is a protocol fault.
static REQUEST: Channel<CriticalSectionRawMutex, Request, 1> = Channel::new();
/// Capacity-one supervisor mailbox; replies must be consumed in request order.
static REPLY: Channel<CriticalSectionRawMutex, Reply, 1> = Channel::new();

/// Returns the sole maintenance ticket after the supervisor admits the request.
/// Panics on overlapping requests or a mismatched reply. Do not cancel the wait:
/// an unconsumed grant leaves feeding suspended until hardware recovery.
pub async fn begin_maintenance() -> MaintenanceTicket {
    assert!(REQUEST.try_send(Request::Begin).is_ok(), "overlapping watchdog request");
    let Reply::Begun(ticket) = REPLY.receive().await else {
        panic!("wrong watchdog reply")
    };
    ticket
}
/// Moves `ticket` back to the supervisor and waits until it validates completion.
/// Panics on concurrent requests or a mismatched reply; do not cancel this exchange.
pub async fn end_maintenance(ticket: MaintenanceTicket) {
    assert!(
        REQUEST.try_send(Request::End(ticket)).is_ok(),
        "overlapping watchdog request"
    );
    assert!(matches!(REPLY.receive().await, Reply::Ended), "wrong watchdog reply");
}

/// Returns independent relaxed liveness snapshots; these do not establish an
/// atomic frame lifecycle or a coherent sensor-data observation.
fn progress() -> Progress {
    Progress {
        acquisition: ACQUISITION_PROGRESS.load(Ordering::Relaxed),
        rendering: RENDER_PROGRESS.load(Ordering::Relaxed),
        input: INPUT_PROGRESS.load(Ordering::Relaxed),
        completed: COMPLETED_FRAMES.load(Ordering::Relaxed),
        display_pending: DISPLAY_PENDING.load(Ordering::Relaxed),
        submitted: crate::tasks::BUFFER_SWAPS.load(Ordering::Relaxed),
    }
}

#[embassy_executor::task]
/// Owns `watchdog` indefinitely and checks progress every 50 ms of executor time.
/// A latched fault records retained evidence and stops feeding without releasing
/// other owners. Maintenance gets one entry feed and no feed until validated end.
/// Hardware reset still depends on the reference clock; this task never returns.
pub async fn recovery_task(mut watchdog: RecoveryWatchdog) {
    let mut monitor = ProgressMonitor::new(Instant::now().as_millis(), progress());
    #[cfg(feature = "ble-obd")]
    let mut radio = dashboard_pico2::ble::runtime::Liveness::new(Instant::now().as_millis());
    loop {
        let now = Instant::now().as_millis();
        let current = progress();
        #[cfg(feature = "ble-obd")]
        if super::ble::FATAL.load(Ordering::Relaxed)
            || !radio.observe(
                super::ble::PROGRESS.load(Ordering::Relaxed),
                now,
                !monitor.feeding_allowed(),
            )
        {
            watchdog.record_failure(if super::ble::FATAL.load(Ordering::Relaxed) {
                RecoveryCause::RadioFatal
            } else {
                RecoveryCause::RadioStalled
            });
            crate::log_fault!("BLE owner failed or stalled; watchdog recovery");
            core::future::pending::<()>().await;
        }
        let result = monitor.observe(now, current).and_then(|()| {
            match REQUEST.try_receive() {
                Ok(Request::Begin) => {
                    let ticket = monitor.begin_maintenance(now, current)?;
                    // Exactly one explicit entry feed. There is no feed while
                    // the permit remains outstanding, even if tasks wake up.
                    watchdog.feed();
                    assert!(
                        REPLY.try_send(Reply::Begun(ticket)).is_ok(),
                        "unconsumed watchdog grant"
                    );
                }
                Ok(Request::End(ticket)) => {
                    monitor.end_maintenance(ticket, now, current)?;
                    assert!(REPLY.try_send(Reply::Ended).is_ok(), "unconsumed watchdog completion");
                }
                Err(_) => {}
            }
            Ok(())
        });
        if let Err(fault) = result {
            watchdog.record_failure(fault.into());
            crate::log_fault!("Watchdog: {}", fault.label());
            // Retain all owners and stop feeding. The watchdog is configured to
            // reset both cores and peripherals without executor/IRQ progress.
            // This still depends on clk_ref and needs physical fault injection.
            core::future::pending::<()>().await;
        }
        if monitor.feeding_allowed() {
            watchdog.feed();
        }
        Timer::after_millis(50).await;
    }
}
