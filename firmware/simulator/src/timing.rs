//! Timing constants for the simulator.
//!
//! These constants use `std::time::Duration` which is not available in `no_std`
//! environments, so they are defined here rather than in the common crate.

use std::time::Duration;

/// Target frame time (~50 FPS). The main loop sleeps if frame completes early.
pub const FRAME_TIME: Duration = Duration::from_millis(20);

/// Duration that popups remain visible on screen.
pub const POPUP_DURATION: Duration = Duration::from_secs(3);
