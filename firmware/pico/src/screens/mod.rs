//! Boot screens for the Pico dashboard.
//!
//! Provides loading and welcome screens that display during startup.

mod loading;
mod welcome;

pub use loading::show_loading_screen;
pub use welcome::show_welcome_screen;
