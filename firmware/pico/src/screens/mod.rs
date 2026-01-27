//! Screens for the Pico dashboard.
//!
//! Provides boot screens and diagnostic/profiling screens.

mod loading;
mod profiling;
mod welcome;

pub use loading::show_loading_screen;
pub use profiling::{ProfilingData, draw_profiling_page};
pub use welcome::show_welcome_screen;
