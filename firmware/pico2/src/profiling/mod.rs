//! Profiling and debugging utilities.
//!
//! - `cpu_cycles`: CPU cycle counter via DWT for utilization measurement
//! - `memory`: Memory profiling (stack/RAM usage)
//! - `log_buffer`: Log buffer with levels and dual-output macros

mod cpu_cycles;
mod log_buffer;
mod memory;

pub use cpu_cycles::{calc_util_percent, elapsed, init, read};
pub use log_buffer::{LOG_BUFFER, LOG_MSG_LEN, LogEntry, LogLevel, push_log};
pub use memory::{FRAMEBUFFER_SIZE, MemoryStats};
