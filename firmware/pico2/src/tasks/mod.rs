use core::sync::atomic::AtomicU32;

use embassy_executor::Executor;
use static_cell::StaticCell;

pub mod demo;
pub mod flush;
pub mod recovery;
#[cfg(feature = "te-trace")]
pub mod te;

pub use demo::{DEMO_VALUES, demo_values_task};
pub use flush::{
    BUFFER_SWAPS,
    BUFFER_WAITS,
    COMPLETED_FRAMES,
    FLUSH_BUFFER_IDX,
    FLUSH_REQUESTS,
    display_flush_task,
    wait_completed,
    wait_started,
};

pub static EXECUTOR_CORE1: StaticCell<Executor> = StaticCell::new();
pub static CORE1_STACK: StaticCell<embassy_rp::multicore::Stack<{ CORE1_STACK_BYTES as usize }>> = StaticCell::new();
pub static CORE1_UTIL_PERCENT: AtomicU32 = AtomicU32::new(0);
pub static CORE1_STACK_USED_KB: AtomicU32 = AtomicU32::new(0);
pub const CORE1_STACK_BYTES: u32 = dashboard_pico2::memory::CORE1_STACK_BYTES;
