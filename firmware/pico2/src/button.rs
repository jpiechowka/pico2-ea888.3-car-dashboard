//! Button debounce handling for the dashboard.
//!
//! Provides time-based edge detection with debouncing to prevent
//! multiple triggers from contact bounce on physical buttons.

use embassy_time::{Duration, Instant};

/// Debounce duration in milliseconds.
pub const DEBOUNCE_MS: u64 = 50;

/// Button debounce state with time-based edge detection.
pub struct ButtonState {
    was_pressed: bool,
    last_change: Option<Instant>,
}

impl ButtonState {
    /// Create a new button state (not pressed).
    pub const fn new() -> Self {
        Self {
            was_pressed: false,
            last_change: None,
        }
    }

    /// Returns true only on the falling edge (button just pressed).
    ///
    /// Buttons are active-low, so `is_low()` means pressed.
    /// Includes debounce logic to prevent multiple triggers from contact bounce.
    pub fn just_pressed(
        &mut self,
        is_low: bool,
    ) -> bool {
        // Check if state changed
        if is_low != self.was_pressed {
            // Apply debounce: only accept change if enough time has passed
            if let Some(last) = self.last_change
                && last.elapsed() < Duration::from_millis(DEBOUNCE_MS)
            {
                return false;
            }

            self.was_pressed = is_low;
            self.last_change = Some(Instant::now());

            // Return true only on press (falling edge, is_low == true)
            return is_low;
        }

        false
    }
}

impl Default for ButtonState {
    fn default() -> Self {
        Self::new()
    }
}
