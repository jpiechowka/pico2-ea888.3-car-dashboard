use core::sync::atomic::AtomicU32;

use embassy_executor::Executor;
use static_cell::StaticCell;

pub mod demo;
pub mod flush;

pub use demo::{DEMO_VALUES, demo_values_task};
pub use flush::{
    BUFFER_SWAPS,
    BUFFER_WAITS,
    FLUSH_BUFFER_IDX,
    FLUSH_DONE,
    FLUSH_SIGNAL,
    LAST_FLUSH_TIME_US,
    display_flush_task,
};

pub static EXECUTOR_CORE1: StaticCell<Executor> = StaticCell::new();
pub static CORE1_STACK: StaticCell<embassy_rp::multicore::Stack<8192>> = StaticCell::new();

pub static CORE1_UTIL_PERCENT: AtomicU32 = AtomicU32::new(0);
pub static CORE1_STACK_USED_KB: AtomicU32 = AtomicU32::new(0);

/// Sentinel value used to fill the Core 1 stack for high-water-mark detection.
/// Chosen to be unlikely to appear naturally in stack data.
pub const STACK_SENTINEL: u32 = 0xDEAD_C0DE;

/// Core 1 stack size in u32 words (must match Stack<8192>).
pub const CORE1_STACK_WORDS: usize = 8192;

/// Core 1 stack size in bytes (re-exported from profiling::memory for convenience).
pub const CORE1_STACK_BYTES: u32 = (CORE1_STACK_WORDS as u32) * 4;

/// Fill the Core 1 stack region with a sentinel pattern before spawning Core 1.
/// This enables accurate high-water-mark detection by scanning for untouched sentinels.
///
/// # Safety
/// Must be called AFTER `CORE1_STACK.init()` and BEFORE `spawn_core1()`.
/// The pointer must point to the base of the initialized Stack<8192>.
pub unsafe fn fill_core1_stack_sentinel(stack_base: *mut u32) {
    unsafe {
        for i in 0..CORE1_STACK_WORDS {
            core::ptr::write_volatile(stack_base.add(i), STACK_SENTINEL);
        }
    }
}

/// Scan the Core 1 stack from the base (low address) upward and count how many
/// sentinel words have been overwritten. Returns the high-water-mark usage in bytes.
///
/// # Safety
/// The stack_base pointer must be valid and point to a CORE1_STACK_WORDS-sized region.
pub unsafe fn core1_stack_hwm_bytes(stack_base: *const u32) -> u32 {
    unsafe {
        // Stack grows downward from top. Sentinels at the base (low addresses)
        // are the last to be overwritten. Count intact sentinels from the base up.
        let mut intact = 0u32;
        for i in 0..CORE1_STACK_WORDS {
            if core::ptr::read_volatile(stack_base.add(i)) == STACK_SENTINEL {
                intact += 1;
            } else {
                break;
            }
        }
        let used_words = CORE1_STACK_WORDS as u32 - intact;
        used_words * 4 // convert to bytes
    }
}
