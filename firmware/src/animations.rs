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
//! - Shake offset is a simple sine calculation (std `f32::sin()`)
//! - Color interpolation uses fixed-point integer math for efficiency
//! - State is tracked per-cell with fixed-size arrays (no heap allocation)

use embedded_graphics::{pixelcolor::Rgb565, prelude::IntoStorage};

use crate::render::CELL_COUNT;

// =============================================================================
// Shake Effect Constants
// =============================================================================

/// Maximum horizontal shake offset in pixels.
/// Larger values = more dramatic shake, but may cause text clipping.
const SHAKE_AMPLITUDE: f32 = 3.0;

/// Shake oscillation speed. Higher = faster wiggle.
/// At 50 FPS, 0.5 gives roughly 4 wiggles per second.
const SHAKE_FREQUENCY: f32 = 0.5;

// =============================================================================
// Color Transition Constants
// =============================================================================

/// Speed of color interpolation (0.0-1.0).
/// Higher values = faster transitions, 1.0 = instant.
/// At 0.15, a full color change takes about 15-20 frames (~300ms at 50 FPS).
const COLOR_LERP_SPEED: f32 = 0.15;

/// Pre-computed fixed-point representation of `COLOR_LERP_SPEED`.
/// Computed at compile time: (`COLOR_LERP_SPEED` * 256.0) as i32 = (0.15 * 256) = 38
/// Used in test to verify the constant matches runtime calculation.
#[cfg(test)]
const COLOR_LERP_T_FIXED: i32 = 38;

/// Threshold for considering colors "close enough" to snap to target.
/// Prevents endless tiny adjustments.
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
///
/// # Example
/// ```ignore
/// let offset = calculate_shake_offset(frame_count, is_critical);
/// let text_x = center_x + offset;
/// ```
#[inline]
pub fn calculate_shake_offset(frame: u32, is_critical: bool) -> i32 {
    if !is_critical {
        return 0;
    }

    // Use sine wave for smooth oscillation
    // frame * frequency controls speed, amplitude controls magnitude
    let phase = frame as f32 * SHAKE_FREQUENCY;
    let offset = phase.sin() * SHAKE_AMPLITUDE;
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
    /// Current interpolated colors for each cell (what's actually displayed).
    current_colors: [Rgb565; CELL_COUNT],

    /// Target colors for each cell (what we're transitioning toward).
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
    pub fn set_target(&mut self, cell_idx: usize, target: Rgb565) -> bool {
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
    pub const fn get_current(&self, cell_idx: usize) -> Rgb565 {
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

                // Interpolate each RGB component
                let new_color = lerp_rgb565(current, target, COLOR_LERP_SPEED);

                // Check if close enough to snap to target
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
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Color Interpolation Helpers
// =============================================================================

/// Linear interpolation between two Rgb565 colors.
///
/// Operates on the raw RGB components extracted from Rgb565 format.
/// Uses integer math with fixed-point for efficiency.
///
/// # Minimum Step Guarantee
///
/// When the computed step `(delta * t_fixed) >> 8` is zero due to integer
/// truncation (small deltas), we force a minimum step of ±1. This prevents
/// transitions from stalling in a "dead zone" where:
/// - The lerp step rounds to 0 (no progress)
/// - The snap threshold isn't met (not close enough)
/// - `transitioning` stays true forever
///
/// With this fix, every lerp call either moves at least 1 step closer or
/// the colors are already equal.
fn lerp_rgb565(from: Rgb565, to: Rgb565, t: f32) -> Rgb565 {
    // Extract RGB components from Rgb565
    // Rgb565: RRRRRGGGGGGBBBBB (5-6-5 bits)
    let from_raw = from.into_storage();
    let to_raw = to.into_storage();

    let from_r = i32::from((from_raw >> 11) & 0x1F);
    let from_g = i32::from((from_raw >> 5) & 0x3F);
    let from_b = i32::from(from_raw & 0x1F);

    let to_r = i32::from((to_raw >> 11) & 0x1F);
    let to_g = i32::from((to_raw >> 5) & 0x3F);
    let to_b = i32::from(to_raw & 0x1F);

    // Interpolate each component
    // new = from + (to - from) * t
    // Note: When called with COLOR_LERP_SPEED (0.15), this equals COLOR_LERP_T_FIXED (38).
    // The float->int conversion is kept for test flexibility with varying t values.
    let t_fixed = (t * 256.0) as i32; // Fixed-point: 8 bits fractional

    // Helper: compute step with minimum ±1 when delta != 0 and t > 0
    // This prevents stalling when delta * t_fixed < 256 (but t > 0)
    let compute_step = |delta: i32| -> i32 {
        if delta == 0 || t_fixed == 0 {
            0
        } else {
            let step = (delta * t_fixed) >> 8;
            if step == 0 {
                // Force minimum step toward target
                if delta > 0 { 1 } else { -1 }
            } else {
                step
            }
        }
    };

    let new_r = from_r + compute_step(to_r - from_r);
    let new_g = from_g + compute_step(to_g - from_g);
    let new_b = from_b + compute_step(to_b - from_b);

    // Clamp and reconstruct Rgb565
    let r = new_r.clamp(0, 31) as u16;
    let g = new_g.clamp(0, 63) as u16;
    let b = new_b.clamp(0, 31) as u16;

    Rgb565::new(r as u8, g as u8, b as u8)
}

/// Check if two colors are close enough to be considered equal.
///
/// Uses Manhattan distance in RGB space.
fn colors_close_enough(a: Rgb565, b: Rgb565) -> bool {
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

    // -------------------------------------------------------------------------
    // Shake Effect Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_shake_offset_not_critical() {
        // When not critical, offset should always be 0
        assert_eq!(calculate_shake_offset(0, false), 0);
        assert_eq!(calculate_shake_offset(100, false), 0);
        assert_eq!(calculate_shake_offset(1000, false), 0);
    }

    #[test]
    fn test_shake_offset_critical() {
        // When critical, offset should oscillate (sine wave)
        let offset0 = calculate_shake_offset(0, true);
        let offset_later = calculate_shake_offset(10, true);

        // Sine(0) = 0, so offset at frame 0 should be 0
        assert_eq!(offset0, 0, "Offset at frame 0 should be 0");

        // At some other frame, offset should be non-zero (unless exactly at zero crossing)
        // We just verify it's bounded by amplitude
        assert!(
            offset_later.abs() <= SHAKE_AMPLITUDE as i32 + 1,
            "Shake offset should be bounded by amplitude"
        );
    }

    #[test]
    fn test_shake_offset_bounded() {
        // Check that shake offset is always within bounds across many frames
        for frame in 0..1000 {
            let offset = calculate_shake_offset(frame, true);
            assert!(
                offset.abs() <= SHAKE_AMPLITUDE as i32 + 1,
                "Frame {frame}: offset {offset} exceeds amplitude {SHAKE_AMPLITUDE}"
            );
        }
    }

    // -------------------------------------------------------------------------
    // Color Interpolation Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_lerp_rgb565_same_color() {
        // Lerping a color to itself should return the same color
        let result = lerp_rgb565(RED, RED, 0.5);
        assert_eq!(result, RED, "Lerping RED to RED should return RED");
    }

    #[test]
    fn test_lerp_rgb565_t_zero() {
        // At t=0, should return the 'from' color
        let result = lerp_rgb565(BLACK, WHITE, 0.0);
        assert_eq!(result, BLACK, "At t=0, should return 'from' color");
    }

    #[test]
    fn test_lerp_rgb565_t_one() {
        // At t=1, should return the 'to' color
        let result = lerp_rgb565(BLACK, WHITE, 1.0);
        assert_eq!(result, WHITE, "At t=1, should return 'to' color");
    }

    #[test]
    fn test_lerp_rgb565_midpoint() {
        // At t=0.5, should return a color between the two
        let result = lerp_rgb565(BLACK, WHITE, 0.5);
        let raw = result.into_storage();
        let r = (raw >> 11) & 0x1F;
        let g = (raw >> 5) & 0x3F;
        let b = raw & 0x1F;

        // Midpoint of BLACK (0,0,0) and WHITE (31,63,31) should be around (15,31,15)
        assert!(r > 10 && r < 20, "Red component should be around midpoint");
        assert!(g > 25 && g < 40, "Green component should be around midpoint");
        assert!(b > 10 && b < 20, "Blue component should be around midpoint");
    }

    #[test]
    fn test_color_lerp_t_fixed_matches_speed() {
        // Verify the precomputed constant matches the runtime calculation
        let runtime_t_fixed = (COLOR_LERP_SPEED * 256.0) as i32;
        assert_eq!(
            runtime_t_fixed, COLOR_LERP_T_FIXED,
            "COLOR_LERP_T_FIXED should equal (COLOR_LERP_SPEED * 256) as i32"
        );
    }

    // -------------------------------------------------------------------------
    // Color Distance Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_colors_close_enough_same_color() {
        assert!(colors_close_enough(RED, RED), "Same color should be close enough");
        assert!(colors_close_enough(BLACK, BLACK), "Same color should be close enough");
    }

    #[test]
    fn test_colors_close_enough_different_colors() {
        assert!(
            !colors_close_enough(BLACK, WHITE),
            "BLACK and WHITE should not be close"
        );
        assert!(!colors_close_enough(RED, BLACK), "RED and BLACK should not be close");
    }

    #[test]
    fn test_colors_close_enough_near_colors() {
        // Create two colors that differ by 1 in each component
        let color1 = Rgb565::new(15, 32, 15);
        let color2 = Rgb565::new(16, 33, 16);
        // Manhattan distance = 1+1+1 = 3, which exceeds threshold of 2
        assert!(
            !colors_close_enough(color1, color2),
            "Colors differing by 3 total should not be close"
        );

        // Create colors that differ by 1 total
        let color3 = Rgb565::new(15, 32, 15);
        let color4 = Rgb565::new(15, 33, 15);
        assert!(
            colors_close_enough(color3, color4),
            "Colors differing by 1 total should be close"
        );
    }

    // -------------------------------------------------------------------------
    // ColorTransition Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_color_transition_new() {
        let ct = ColorTransition::new();
        // All cells should start with BLACK
        for i in 0..CELL_COUNT {
            assert_eq!(ct.get_current(i), BLACK, "Cell {i} should start with BLACK");
        }
    }

    #[test]
    fn test_color_transition_set_target() {
        let mut ct = ColorTransition::new();

        // Setting a different target should return true
        assert!(ct.set_target(0, RED), "Setting new target should return true");

        // Setting the same target should return false
        assert!(!ct.set_target(0, RED), "Setting same target should return false");
    }

    #[test]
    fn test_color_transition_update_converges() {
        let mut ct = ColorTransition::new();
        ct.set_target(0, WHITE); // BLACK to WHITE

        // With minimum step guarantee, transition should complete in bounded iterations
        // Max steps: 31 (r) + 63 (g) + 31 (b) = 125 at minimum step of 1 per channel
        let mut iterations = 0;
        while ct.get_current(0) != WHITE && iterations < 150 {
            ct.update();
            iterations += 1;
        }

        assert_eq!(ct.get_current(0), WHITE, "Transition should converge to target exactly");
        assert!(
            iterations < 150,
            "Should converge within 150 iterations, took {iterations}"
        );
        // Verify transitioning is now false
        assert!(!ct.transitioning[0], "Transitioning should be false after convergence");
    }

    #[test]
    fn test_color_transition_converges_red() {
        let mut ct = ColorTransition::new();
        ct.set_target(0, RED); // BLACK to RED (only red channel changes)

        // Should converge in ~31 iterations (one step per frame)
        let mut iterations = 0;
        while ct.get_current(0) != RED && iterations < 50 {
            ct.update();
            iterations += 1;
        }

        assert_eq!(ct.get_current(0), RED, "Should converge to RED exactly");
        assert!(iterations < 50, "Should converge within 50 iterations");
    }

    #[test]
    fn test_color_transition_no_change_when_equal() {
        let mut ct = ColorTransition::new();
        ct.current_colors[0] = WHITE;
        ct.target_colors[0] = WHITE;
        ct.transitioning[0] = true; // Manually set transitioning

        let changed = ct.update();

        // Should not be marked as changed since current == target
        assert_eq!(changed & 0b001, 0, "No change when current == target");
        // Transitioning should be cleared
        assert!(!ct.transitioning[0], "Transitioning should be false");
    }

    #[test]
    fn test_color_transition_makes_progress() {
        let mut ct = ColorTransition::new();
        ct.set_target(0, RED);

        let initial = ct.get_current(0);
        ct.update();
        let after_one = ct.get_current(0);

        // Should have made progress toward RED (red channel increased)
        assert_ne!(initial, after_one, "Color should change after update");

        // Run more updates
        for _ in 0..20 {
            ct.update();
        }

        let after_many = ct.get_current(0);
        let raw = after_many.into_storage();
        let r = (raw >> 11) & 0x1F;

        // Red channel should be significantly toward target (31)
        assert!(r > 20, "Red channel should be > 20 after 20+ iterations");
    }

    #[test]
    fn test_color_transition_returns_changed_bitmask() {
        let mut ct = ColorTransition::new();
        ct.set_target(0, RED);
        ct.set_target(2, WHITE);

        let changed = ct.update();

        // Bits 0 and 2 should be set
        assert!(changed & 0b001 != 0, "Cell 0 should be marked as changed");
        assert!(changed & 0b100 != 0, "Cell 2 should be marked as changed");
        assert!(changed & 0b010 == 0, "Cell 1 should not be marked as changed");
    }
}
