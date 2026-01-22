//! Screen modules for boot sequence and debug view.
//!
//! # Boot Sequence
//!
//! 1. **Loading Screen** ([`loading`]): Console-style init messages with spinner
//! 2. **Welcome Screen** ([`welcome`]): Per-character rainbow-animated Sanic meme
//! 3. Main dashboard (handled in `main.rs`)
//!
//! # Runtime Screens
//!
//! - **Debug Page** ([`debug`]): Profiling metrics, frame timing, debug log terminal (accessible via `Y` button during
//!   runtime)
//!
//! # Event Handling
//!
//! Boot screens return `false` if the window is closed during boot,
//! allowing the application to exit cleanly without entering the main loop.
//!
//! # Optimizations Applied
//!
//! - Pre-computed position constants (compile-time)
//! - `heapless::String` for dynamic text formatting
//! - Static text alignment styles from [`crate::styles`]
//! - Const rainbow color array for per-character animation (no runtime allocation)

mod debug;
mod loading;
mod welcome;

pub use debug::draw_debug_page;
pub use loading::run_loading_screen;
pub use welcome::run_welcome_screen;
