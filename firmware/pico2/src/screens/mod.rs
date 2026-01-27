//! Screens for the Pico dashboard.
//!
//! Provides boot screens, diagnostic/profiling screens, and log viewer.

mod loading;
mod logs;
mod profiling;
mod welcome;

pub use loading::show_loading_screen;
pub use logs::draw_logs_page;
pub use profiling::{ProfilingData, draw_profiling_page};
pub use welcome::show_welcome_screen;
