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
