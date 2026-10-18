use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use dashboard_pico2::health::{MaintenanceTicket, Progress, ProgressMonitor};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_time::{Instant, Timer};

use super::COMPLETED_FRAMES;
use crate::drivers::watchdog::RecoveryWatchdog;

// These are independent liveness counters, not a coherent sensor snapshot or
// buffer-ownership protocol. Wrap is allowed; the monitor tests for change.
pub static ACQUISITION_PROGRESS: AtomicU32 = AtomicU32::new(0);
pub static RENDER_PROGRESS: AtomicU32 = AtomicU32::new(0);
pub static INPUT_PROGRESS: AtomicU32 = AtomicU32::new(0);
pub static DISPLAY_PENDING: AtomicBool = AtomicBool::new(false);

enum Request {
    Begin,
    End(MaintenanceTicket),
}
enum Reply {
    Begun(MaintenanceTicket),
    Ended,
}
static REQUEST: Channel<CriticalSectionRawMutex, Request, 1> = Channel::new();
static REPLY: Channel<CriticalSectionRawMutex, Reply, 1> = Channel::new();

pub async fn begin_maintenance() -> MaintenanceTicket {
    assert!(REQUEST.try_send(Request::Begin).is_ok(), "overlapping watchdog request");
    let Reply::Begun(ticket) = REPLY.receive().await else {
        panic!("wrong watchdog reply")
    };
    ticket
}
pub async fn end_maintenance(ticket: MaintenanceTicket) {
    assert!(
        REQUEST.try_send(Request::End(ticket)).is_ok(),
        "overlapping watchdog request"
    );
    assert!(matches!(REPLY.receive().await, Reply::Ended), "wrong watchdog reply");
}

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
            crate::log_fault!("Watchdog: {:?}", fault);
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
