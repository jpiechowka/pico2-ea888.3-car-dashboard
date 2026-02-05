//! Display flush task for parallel rendering and DMA transfers.
//!
//! This task runs on a separate Embassy executor thread, receiving signals
//! from the main render loop to flush completed framebuffers to the display
//! via DMA while the main loop continues rendering to the other buffer.

use core::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

use defmt::info;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;
use embassy_time::Instant;

use crate::st7789::St7789Flusher;

// =============================================================================
// Double Buffering Synchronization
// =============================================================================

/// Signal to notify flush task which buffer to flush (buffer index).
pub static FLUSH_SIGNAL: Signal<CriticalSectionRawMutex, usize> = Signal::new();

/// Signal to notify main task that flush is complete.
pub static FLUSH_DONE: Signal<CriticalSectionRawMutex, ()> = Signal::new();

/// Atomic counter for buffer swaps (for profiling).
pub static BUFFER_SWAPS: AtomicU32 = AtomicU32::new(0);

/// Atomic counter for times main task waited for flush (for profiling).
pub static BUFFER_WAITS: AtomicU32 = AtomicU32::new(0);

/// Current buffer being flushed (for profiling display).
pub static FLUSH_BUFFER_IDX: AtomicUsize = AtomicUsize::new(0);

/// Last flush time in microseconds (for profiling).
pub static LAST_FLUSH_TIME_US: AtomicU32 = AtomicU32::new(0);

/// Display flush task - runs in parallel with rendering.
///
/// Waits for signal from main task, then flushes the completed buffer to display.
/// This allows the main task to continue rendering to the other buffer.
#[embassy_executor::task]
pub async fn display_flush_task(flusher: &'static mut St7789Flusher<'static>) {
    info!("Display flush task started");

    loop {
        // Wait for signal with buffer index to flush
        let buffer_idx = FLUSH_SIGNAL.wait().await;
        FLUSH_BUFFER_IDX.store(buffer_idx, Ordering::Relaxed);

        let flush_start = Instant::now();

        // SAFETY: Main task is rendering to the OTHER buffer, so this one is safe to read
        let buffer = unsafe {
            if buffer_idx == 0 {
                &*core::ptr::addr_of!(crate::st7789::FRAMEBUFFER_A)
            } else {
                &*core::ptr::addr_of!(crate::st7789::FRAMEBUFFER_B)
            }
        };

        // Flush buffer to display via DMA
        flusher.flush_buffer(buffer).await;

        LAST_FLUSH_TIME_US.store(flush_start.elapsed().as_micros() as u32, Ordering::Relaxed);

        // Signal completion
        FLUSH_DONE.signal(());
    }
}
