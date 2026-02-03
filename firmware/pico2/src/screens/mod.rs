//! Screens for the Pico dashboard.
//!
//! Provides boot screens, diagnostic/profiling screens, and log viewer.
//!
//! # Boot Screens
//!
//! The boot sequence consists of two screens:
//!
//! 1. **Loading screen** (~6 seconds) - Console-style initialization messages displayed sequentially with delays
//!    between each message. Uses [`draw_loading_frame`] with [`INIT_MESSAGES`] for timing.
//!
//! 2. **Welcome screen** (5 seconds) - AEZAKMI logo with time-based star animation. Uses [`draw_welcome_frame`] with
//!    elapsed milliseconds for consistent animation speed regardless of frame rate. Stars fill over 4 seconds, then
//!    blink for 1 second.
//!
//! Both screens require the caller to flush the display after each frame
//! to ensure proper visual updates.

mod loading;
mod logs;
mod profiling;
mod welcome;

// Boot screen frame drawing functions and constants
pub use loading::{INIT_MESSAGES, MAX_VISIBLE_LINES, draw_loading_frame};
// Other screens
pub use logs::draw_logs_page;
pub use profiling::{ProfilingData, draw_profiling_page};
pub use welcome::draw_welcome_frame;
