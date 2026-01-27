//! Animation effects for enhanced visual feedback.
//!
//! This module provides animation utilities for the dashboard:
//! - **Shake effect**: Cells wiggle when in critical state
//! - **Color transitions**: Smooth fades between background colors
//!
//! # Shake Effect
//!
//! When a sensor enters a critical state (high temperature, low voltage),
//! the cell content shifts horizontally by a small offset that oscillates
//! over time. This creates an attention-grabbing shake/wiggle effect.
//!
//! The shake uses a sine wave for smooth oscillation:
//! ```text
//! offset = sin(frame * frequency) * amplitude
//! ```
//!
//! # Color Transitions
//!
//! Instead of instant color changes when crossing thresholds, colors
//! smoothly interpolate over several frames. This is achieved by:
//! 1. Tracking the target color for each cell
//! 2. Interpolating current color toward target each frame
//! 3. Using linear interpolation in RGB565 color space
//!
//! # Performance Considerations
//!
//! - Shake offset is a simple sine calculation
//! - Color interpolation uses fixed-point integer math for efficiency
//! - State is tracked per-cell with fixed-size arrays (no heap allocation)

use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::IntoStorage;

use crate::render::CELL_COUNT;

// =============================================================================
// Shake Effect Constants
// =============================================================================

/// Maximum horizontal shake offset in pixels.
/// Larger values = more dramatic shake, but may cause text clipping.
#[allow(dead_code)]
const SHAKE_AMPLITUDE: f32 = 3.0;

/// Shake oscillation speed. Higher = faster wiggle.
/// At 50 FPS, 0.5 gives roughly 4 wiggles per second.
#[allow(dead_code)]
const SHAKE_FREQUENCY: f32 = 0.5;

// =============================================================================
// Color Transition Constants
// =============================================================================

/// Speed of color interpolation (0.0-1.0).
/// Higher values = faster transitions, 1.0 = instant.
/// At 0.15, a full color change takes about 15-20 frames (~300ms at 50 FPS).
const COLOR_LERP_SPEED: f32 = 0.15;

/// Pre-computed fixed-point representation of `COLOR_LERP_SPEED`.
#[cfg(test)]
const COLOR_LERP_T_FIXED: i32 = 38;

/// Threshold for considering colors "close enough" to snap to target.
const COLOR_SNAP_THRESHOLD: i32 = 2;

// =============================================================================
// Shake Effect
// =============================================================================

/// Calculate horizontal shake offset for critical state animation.
///
/// Returns a pixel offset that oscillates smoothly based on frame count.
/// Returns 0 when not in critical state.
///
/// # Parameters
/// - `frame`: Current frame counter (used for timing)
/// - `is_critical`: Whether the sensor is in critical state
#[inline]
#[allow(dead_code)]
pub fn calculate_shake_offset(
    frame: u32,
    is_critical: bool,
) -> i32 {
    if !is_critical {
        return 0;
    }

    // Use sine wave for smooth oscillation
    let phase = frame as f32 * SHAKE_FREQUENCY;
    let offset = micromath::F32(phase).sin().0 * SHAKE_AMPLITUDE;
    offset as i32
}

// =============================================================================
// Color Transition State
// =============================================================================

/// Tracks color transition state for smooth background changes.
///
/// Each cell has a current color that smoothly interpolates toward
/// a target color over multiple frames.
pub struct ColorTransition {
    /// Current interpolated colors for each cell.
    current_colors: [Rgb565; CELL_COUNT],

    /// Target colors for each cell.
    target_colors: [Rgb565; CELL_COUNT],

    /// Whether each cell is currently transitioning.
    transitioning: [bool; CELL_COUNT],
}

impl ColorTransition {
    /// Create a new color transition state.
    ///
    /// All cells start with black background and no active transitions.
    pub const fn new() -> Self {
        use crate::colors::BLACK;
        Self {
            current_colors: [BLACK; CELL_COUNT],
            target_colors: [BLACK; CELL_COUNT],
            transitioning: [false; CELL_COUNT],
        }
    }

    /// Set target color for a cell and start transition if different.
    ///
    /// Returns `true` if a new transition was started.
    pub fn set_target(
        &mut self,
        cell_idx: usize,
        target: Rgb565,
    ) -> bool {
        if self.target_colors[cell_idx] == target {
            false
        } else {
            self.target_colors[cell_idx] = target;
            self.transitioning[cell_idx] = true;
            true
        }
    }

    /// Get current (interpolated) color for a cell.
    #[inline]
    pub const fn get_current(
        &self,
        cell_idx: usize,
    ) -> Rgb565 {
        self.current_colors[cell_idx]
    }

    /// Update all color transitions for one frame.
    ///
    /// Call this once per frame to advance all active transitions.
    /// Returns a bitmask of which cells changed color this frame.
    pub fn update(&mut self) -> u8 {
        let mut changed: u8 = 0;

        for i in 0..CELL_COUNT {
            if self.transitioning[i] {
                let current = self.current_colors[i];
                let target = self.target_colors[i];

                if current == target {
                    self.transitioning[i] = false;
                    continue;
                }

                let new_color = lerp_rgb565(current, target, COLOR_LERP_SPEED);

                if colors_close_enough(new_color, target) {
                    self.current_colors[i] = target;
                    self.transitioning[i] = false;
                } else {
                    self.current_colors[i] = new_color;
                }

                changed |= 1 << i;
            }
        }

        changed
    }
}

impl Default for ColorTransition {
    fn default() -> Self { Self::new() }
}

// =============================================================================
// Color Interpolation Helpers
// =============================================================================

/// Linear interpolation between two Rgb565 colors.
///
/// Uses integer math with fixed-point for efficiency.
fn lerp_rgb565(
    from: Rgb565,
    to: Rgb565,
    t: f32,
) -> Rgb565 {
    let from_raw = from.into_storage();
    let to_raw = to.into_storage();

    let from_r = i32::from((from_raw >> 11) & 0x1F);
    let from_g = i32::from((from_raw >> 5) & 0x3F);
    let from_b = i32::from(from_raw & 0x1F);

    let to_r = i32::from((to_raw >> 11) & 0x1F);
    let to_g = i32::from((to_raw >> 5) & 0x3F);
    let to_b = i32::from(to_raw & 0x1F);

    let t_fixed = (t * 256.0) as i32;

    let compute_step = |delta: i32| -> i32 {
        if delta == 0 || t_fixed == 0 {
            0
        } else {
            let step = (delta * t_fixed) >> 8;
            if step == 0 {
                if delta > 0 { 1 } else { -1 }
            } else {
                step
            }
        }
    };

    let new_r = from_r + compute_step(to_r - from_r);
    let new_g = from_g + compute_step(to_g - from_g);
    let new_b = from_b + compute_step(to_b - from_b);

    let r = new_r.clamp(0, 31) as u16;
    let g = new_g.clamp(0, 63) as u16;
    let b = new_b.clamp(0, 31) as u16;

    Rgb565::new(r as u8, g as u8, b as u8)
}

/// Check if two colors are close enough to be considered equal.
fn colors_close_enough(
    a: Rgb565,
    b: Rgb565,
) -> bool {
    let a_raw = a.into_storage();
    let b_raw = b.into_storage();

    let a_r = i32::from((a_raw >> 11) & 0x1F);
    let a_g = i32::from((a_raw >> 5) & 0x3F);
    let a_b = i32::from(a_raw & 0x1F);

    let b_r = i32::from((b_raw >> 11) & 0x1F);
    let b_g = i32::from((b_raw >> 5) & 0x3F);
    let b_b = i32::from(b_raw & 0x1F);

    let diff = (a_r - b_r).abs() + (a_g - b_g).abs() + (a_b - b_b).abs();
    diff <= COLOR_SNAP_THRESHOLD
}

// =============================================================================
// Unit Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::colors::{BLACK, RED, WHITE};

    #[test]
    fn test_shake_offset_not_critical() {
        assert_eq!(calculate_shake_offset(0, false), 0);
        assert_eq!(calculate_shake_offset(100, false), 0);
    }

    #[test]
    fn test_shake_offset_critical() {
        let offset0 = calculate_shake_offset(0, true);
        assert_eq!(offset0, 0); // sin(0) = 0

        // Verify bounded
        for frame in 0..1000 {
            let offset = calculate_shake_offset(frame, true);
            assert!(offset.abs() <= SHAKE_AMPLITUDE as i32 + 1);
        }
    }

    #[test]
    fn test_lerp_rgb565_same_color() {
        let result = lerp_rgb565(RED, RED, 0.5);
        assert_eq!(result, RED);
    }

    #[test]
    fn test_lerp_rgb565_t_zero() {
        let result = lerp_rgb565(BLACK, WHITE, 0.0);
        assert_eq!(result, BLACK);
    }

    #[test]
    fn test_lerp_rgb565_t_one() {
        let result = lerp_rgb565(BLACK, WHITE, 1.0);
        assert_eq!(result, WHITE);
    }

    #[test]
    fn test_color_lerp_t_fixed_matches_speed() {
        let runtime_t_fixed = (COLOR_LERP_SPEED * 256.0) as i32;
        assert_eq!(runtime_t_fixed, COLOR_LERP_T_FIXED);
    }

    #[test]
    fn test_colors_close_enough_same() {
        assert!(colors_close_enough(RED, RED));
        assert!(colors_close_enough(BLACK, BLACK));
    }

    #[test]
    fn test_colors_close_enough_different() {
        assert!(!colors_close_enough(BLACK, WHITE));
        assert!(!colors_close_enough(RED, BLACK));
    }

    #[test]
    fn test_color_transition_converges() {
        let mut ct = ColorTransition::new();
        ct.set_target(0, WHITE);

        let mut iterations = 0;
        while ct.get_current(0) != WHITE && iterations < 150 {
            ct.update();
            iterations += 1;
        }

        assert_eq!(ct.get_current(0), WHITE);
        assert!(iterations < 150);
    }
}
