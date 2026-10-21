//! Shared-renderer adapters for hardware Debug and Logs pages.
/// Incremental shared log-page drawing with short synchronous borrows.
mod logs;
/// Immutable diagnostic snapshots rendered by the shared Debug page.
mod profiling;

pub use logs::draw_logs_page;
pub use profiling::{ProfilingData, draw_profiling_page};
