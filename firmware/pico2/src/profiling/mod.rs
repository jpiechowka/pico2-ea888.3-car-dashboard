use dashboard_pico2::cpu_cycles;
mod log_buffer;
use dashboard_pico2::memory;
mod render_work;
pub use cpu_cycles::{calc_util_percent, elapsed, init, read};
pub use log_buffer::{LOG_BUFFER, LOG_ENTRIES, LOG_MSG_LEN, LogEntry, LogLevel, push_log};
pub use memory::{FRAMEBUFFER_SIZE, MemoryStats};
pub use render_work::RenderWork;
