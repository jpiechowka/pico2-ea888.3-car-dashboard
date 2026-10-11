use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use dashboard_pico2::health::{Progress, ProgressMonitor};
use embassy_time::{Instant, Timer};

use super::COMPLETED_FRAMES;
use crate::drivers::watchdog::RecoveryWatchdog;

// These are independent liveness counters, not a coherent sensor snapshot or
// buffer-ownership protocol. Wrap is allowed; the monitor tests for change.
pub static ACQUISITION_PROGRESS: AtomicU32 = AtomicU32::new(0);
pub static RENDER_PROGRESS: AtomicU32 = AtomicU32::new(0);
pub static DISPLAY_PENDING: AtomicBool = AtomicBool::new(true);

fn progress() -> Progress {
    Progress {
        acquisition: ACQUISITION_PROGRESS.load(Ordering::Relaxed),
        rendering: RENDER_PROGRESS.load(Ordering::Relaxed),
        completed: COMPLETED_FRAMES.load(Ordering::Relaxed),
        display_pending: DISPLAY_PENDING.load(Ordering::Relaxed),
    }
}

#[embassy_executor::task]
pub async fn recovery_task(mut watchdog: RecoveryWatchdog) {
    let mut monitor = ProgressMonitor::new(Instant::now().as_millis(), progress());
    loop {
        if monitor.observe(Instant::now().as_millis(), progress()).is_err() {
            // Retain all owners and stop feeding. The watchdog is configured to
            // reset both cores and peripherals without executor/IRQ progress.
            // This still depends on clk_ref and needs physical fault injection.
            core::future::pending::<()>().await;
        }
        watchdog.feed();
        Timer::after_millis(50).await;
    }
}
