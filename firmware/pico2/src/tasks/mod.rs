//! Async tasks for the dashboard firmware.
//!
//! This module contains Embassy async tasks that run concurrently:
//! - `flush`: Display buffer flush task (DMA transfers)
//! - `demo`: Demo sensor value generation task

pub mod demo;
pub mod flush;

pub use demo::{DEMO_VALUES, demo_values_task};
pub use flush::{
    BUFFER_SWAPS, BUFFER_WAITS, FLUSH_BUFFER_IDX, FLUSH_DONE, FLUSH_SIGNAL, LAST_FLUSH_TIME_US,
    display_flush_task,
};
