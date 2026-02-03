//! Screens for the Pico dashboard.
//!
//! Provides boot screens, diagnostic/profiling screens, and log viewer.
//!
//! # Boot Screens
//!
//! The boot sequence consists of two screens:
//!
//! 1. **Loading screen** - Console-style initialization messages displayed
//!    sequentially with delays between each message. Uses [`draw_loading_frame`]
//!    with [`INIT_MESSAGES`] for timing.
//!
//! 2. **Welcome screen** - AEZAKMI logo with animated blinking stars.
//!    Uses [`draw_welcome_frame`] for animation frames.
//!
//! Both screens require the caller to flush the display after each frame
//! to ensure proper visual updates.

mod loading;
mod logs;
mod profiling;
mod welcome;

// Boot screen frame drawing functions and constants
pub use loading::{INIT_MESSAGES, MAX_VISIBLE_LINES, draw_loading_frame};
pub use welcome::draw_welcome_frame;

// High-level screen functions (for reference, but boot screens need frame-by-frame control)
pub use loading::show_loading_screen;
pub use logs::draw_logs_page;
pub use profiling::{ProfilingData, draw_profiling_page};
pub use welcome::show_welcome_screen;
