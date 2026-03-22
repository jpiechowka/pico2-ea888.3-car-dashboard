use core::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;
use embassy_time::Instant;

use crate::drivers::St7789Flusher;
use crate::log_info;

pub static FLUSH_SIGNAL: Signal<CriticalSectionRawMutex, usize> = Signal::new();

pub static FLUSH_DONE: Signal<CriticalSectionRawMutex, ()> = Signal::new();

pub static BUFFER_SWAPS: AtomicU32 = AtomicU32::new(0);

pub static BUFFER_WAITS: AtomicU32 = AtomicU32::new(0);

pub static FLUSH_BUFFER_IDX: AtomicUsize = AtomicUsize::new(0);

pub static LAST_FLUSH_TIME_US: AtomicU32 = AtomicU32::new(0);

#[embassy_executor::task]
pub async fn display_flush_task(flusher: &'static mut St7789Flusher<'static>) {
    log_info!("Flush task started");

    loop {
        let buffer_idx = FLUSH_SIGNAL.wait().await;
        FLUSH_BUFFER_IDX.store(buffer_idx, Ordering::Relaxed);

        let flush_start = Instant::now();

        let buffer = unsafe {
            if buffer_idx == 0 {
                &*core::ptr::addr_of!(crate::drivers::FRAMEBUFFER_A)
            } else {
                &*core::ptr::addr_of!(crate::drivers::FRAMEBUFFER_B)
            }
        };

        flusher.flush_buffer(buffer).await;

        LAST_FLUSH_TIME_US.store(flush_start.elapsed().as_micros() as u32, Ordering::Relaxed);

        FLUSH_DONE.signal(());
    }
}
