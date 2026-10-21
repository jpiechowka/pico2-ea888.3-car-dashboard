//! Hardware diagnostics, bounded logs and opt-in target measurements.
use dashboard_pico2::cpu_cycles;
/// Bounded cross-core application history and latest-fault retention.
mod log_buffer;
use dashboard_pico2::memory;
/// Instrumented UI work excluding cooperative yields.
mod render_work;
#[cfg(feature = "target-bench")]
/// Optional finite device benchmarks with counter/timebase guards.
pub mod target_bench;
#[cfg(feature = "frame-trace")]
/// Optional bounded defmt protocol encoding for host collection.
pub mod telemetry_export;
pub use cpu_cycles::{elapsed, init, read};
pub use log_buffer::{LOGS, LogLevel, push_log};
pub use memory::{FRAMEBUFFER_SIZE, MemoryStats};
pub use render_work::RenderWork;
