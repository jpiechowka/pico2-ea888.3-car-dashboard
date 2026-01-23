//! Common types and constants for the OBD-II dashboard.
//!
//! This crate contains platform-agnostic code shared between the simulator
//! and the Pico 2 hardware implementation:
//!
//! - [`colors`]: RGB565 color constants for the display
//! - [`config`]: Layout and display configuration constants
//! - [`pages`]: Page navigation enum
//! - [`styles`]: Pre-computed text styles
//! - [`thresholds`]: Sensor threshold values
//! - [`render`]: Cell indices and render state tracking
//! - [`animations`]: Color transitions and shake effects
//! - [`profiling`]: Debug log buffer (no time dependencies)
//!
//! # no_std Compatibility
//!
//! This crate is `no_std` compatible and can be used on embedded targets.
//! It avoids any dependencies on `std::time` or platform-specific types.

#![no_std]
// Crate-level lints
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_sign_loss)]

pub mod animations;
pub mod colors;
pub mod config;
pub mod pages;
pub mod profiling;
pub mod render;
pub mod styles;
pub mod thresholds;

// Re-export commonly used items
pub use colors::*;
pub use config::*;
pub use pages::Page;
