//! Render state tracking for optimized display updates.
//!
//! This module tracks display state for:
//! - Header conditional redraw (on FPS change, popup close, or page switch)
//! - Divider draw-once optimization (redraw only after popup closes or page switch)
//! - Popup cleanup (clear display when popup disappears)
//! - Page switch cleanup (clear display when switching between Dashboard and Debug)
//!
//! **Note:** Color transitions are handled separately by
//! [`ColorTransition`](crate::animations::ColorTransition), not by this module.
//!
//! # Update Strategy
//!
//! | Element | Update Frequency | Strategy |
//! |---------|-----------------|----------|
//! | Header | On FPS change / popup close / page switch | Conditional redraw |
//! | Dividers | Once / after popup / after page switch | Draw-once tracking |
//! | Cells | Every frame | Always redraw (values animate) |
//! | Popups | On show/hide | Full clear on close |
//!
//! # Why Cells Always Redraw
//!
//! Cell backgrounds are always redrawn because sensor values animate continuously.
//! Without clearing, old text would remain visible causing artifacts.
//!
//! # Popup Cleanup
//!
//! When a popup closes, the display is fully cleared to remove remnants (especially
//! the white border). Dividers are marked for redraw since the clear removes them.
//! This cleanup happens in the same frame the popup expires.
//!
//! # Page Switch Cleanup
//!
//! When switching between Dashboard and Debug pages, the display is cleared.
//! The `display_cleared` flag is set via `mark_display_cleared()` to ensure
//! header and dividers are redrawn when returning to the Dashboard page.

// =============================================================================
// Cell State Tracking
// =============================================================================

/// Number of cells in the dashboard grid (4 columns × 2 rows).
pub const CELL_COUNT: usize = 8;

/// Cell indices for clearer code.
/// Layout:
///   Row 1 (top):    BOOST | AFR  | BATT | COOL
///   Row 2 (bottom): OIL   | DSG  | IAT  | EGT
#[allow(dead_code)]
pub mod cell_idx {
    // Row 1: BOOST | AFR | BATT | COOL
    pub const BOOST: usize = 0;
    pub const AFR: usize = 1;
    pub const BATTERY: usize = 2;
    pub const COOLANT: usize = 3;
    // Row 2: OIL | DSG | IAT | EGT
    pub const OIL: usize = 4;
    pub const DSG: usize = 5;
    pub const IAT: usize = 6;
    pub const EGT: usize = 7;
}

use std::time::Instant;

use crate::config::POPUP_DURATION;

/// Active popup with its start time.
///
/// Consolidates popup state into a single enum. Each variant holds the `Instant`
/// when the popup was triggered, making expiration checks straightforward.
///
/// # Why This Design
///
/// Previously we had three separate `Option<Instant>` variables with logic to
/// ensure only one was set at a time. This enum makes the mutual exclusion
/// explicit and impossible to violate. It also simplifies expiration checks
/// and popup kind comparisons.
#[derive(Clone, Copy, Debug)]
pub enum Popup {
    /// "MIN/AVG/MAX RESET" popup (larger, 180×60).
    Reset(Instant),
    /// "FPS ON/OFF" popup (smaller, 140×50).
    Fps(Instant),
    /// "BOOST: BAR/PSI" popup (same size as FPS).
    BoostUnit(Instant),
}

impl Popup {
    /// Get the start time of this popup.
    #[inline]
    pub const fn start_time(&self) -> Instant {
        match self {
            Self::Reset(t) | Self::Fps(t) | Self::BoostUnit(t) => *t,
        }
    }

    /// Check if this popup has expired.
    #[inline]
    pub fn is_expired(&self) -> bool { self.start_time().elapsed() >= POPUP_DURATION }

    /// Get the popup kind (discriminant only, for comparison).
    #[inline]
    const fn kind(&self) -> u8 {
        match self {
            Self::Reset(_) => 0,
            Self::Fps(_) => 1,
            Self::BoostUnit(_) => 2,
        }
    }
}

/// Tracks render state for optimized display updates.
///
/// Manages conditional redraws for header/dividers and popup cleanup.
pub struct RenderState {
    /// Whether dividers have been drawn (only need to draw once).
    dividers_drawn: bool,

    /// Previous FPS display state.
    prev_show_fps: bool,

    /// Previous FPS value (rounded to avoid unnecessary redraws).
    prev_fps_rounded: u32,

    /// Previous popup kind (discriminant only, for detecting switches).
    prev_popup_kind: Option<u8>,

    /// Whether popup just closed or switched this frame (need to clear remnants).
    popup_just_closed: bool,

    /// Whether this is the first frame (need full redraw).
    first_frame: bool,

    /// Whether the display was cleared externally (e.g., page switch).
    /// When true, header and dividers need redrawing.
    display_cleared: bool,
}

impl RenderState {
    /// Create a new render state for first frame.
    pub const fn new() -> Self {
        Self {
            dividers_drawn: false,
            prev_show_fps: true,
            prev_fps_rounded: 0,
            prev_popup_kind: None,
            popup_just_closed: false,
            first_frame: true,
            display_cleared: false,
        }
    }

    /// Check if dividers need drawing.
    #[inline]
    pub const fn need_dividers(&self) -> bool { !self.dividers_drawn || self.first_frame || self.display_cleared }

    /// Mark dividers as drawn.
    #[inline]
    pub const fn mark_dividers_drawn(&mut self) { self.dividers_drawn = true; }

    /// Check if header/FPS needs redrawing.
    ///
    /// Uses `fps.round()` to match the display formatting (`{:.0}`) which also
    /// rounds. This prevents mismatches where the dirty check sees a different
    /// value than what gets displayed.
    pub const fn check_header_dirty(
        &mut self,
        show_fps: bool,
        fps: f32,
    ) -> bool {
        let fps_rounded = fps.round() as u32;
        let dirty = self.first_frame
            || self.popup_just_closed
            || self.display_cleared
            || show_fps != self.prev_show_fps
            || (show_fps && fps_rounded != self.prev_fps_rounded);

        self.prev_show_fps = show_fps;
        self.prev_fps_rounded = fps_rounded;
        dirty
    }

    /// Update popup state with the current active popup.
    ///
    /// Detects both popup close (becomes None) and popup switch (kind changes).
    /// Both cases require display clear to remove remnants.
    ///
    /// # Why Track Kind, Not Just Visible
    ///
    /// Different popups have different sizes. Switching from a large popup (Reset)
    /// to a smaller popup (FPS) without clearing leaves artifacts:
    /// - Areas previously covered by the larger popup become visible again
    /// - Dividers overwritten by the larger popup won't redraw (draw-once)
    /// - Old pixels in borders/grid lines can persist
    pub fn update_popup(
        &mut self,
        popup: Option<&Popup>,
    ) {
        let current_kind = popup.map(Popup::kind);
        let changed = current_kind != self.prev_popup_kind;
        let was_visible = self.prev_popup_kind.is_some();
        self.prev_popup_kind = current_kind;

        // Trigger cleanup when popup closes OR switches to different popup
        // Switching popups needs cleanup because popup sizes differ
        if changed && was_visible {
            self.popup_just_closed = true;
            self.dividers_drawn = false; // Force divider redraw after display clear
        }
    }

    /// Check if popup just closed this frame (need to clear remnants).
    #[inline]
    pub const fn popup_just_closed(&self) -> bool { self.popup_just_closed }

    /// Check if this is the first frame.
    #[inline]
    pub const fn is_first_frame(&self) -> bool { self.first_frame }

    /// Mark that the display was cleared externally.
    ///
    /// Call this when `display.clear()` is called due to page switching.
    /// This ensures header and dividers are redrawn on the next Dashboard frame.
    pub const fn mark_display_cleared(&mut self) {
        self.display_cleared = true;
        self.dividers_drawn = false; // Force divider redraw
    }

    /// Call at end of frame to reset per-frame state.
    pub const fn end_frame(&mut self) {
        self.first_frame = false;
        self.popup_just_closed = false;
        self.display_cleared = false;
    }
}

impl Default for RenderState {
    fn default() -> Self { Self::new() }
}

// =============================================================================
// Unit Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // -------------------------------------------------------------------------
    // Constants Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_cell_count() {
        // Dashboard grid is 4 columns × 2 rows
        assert_eq!(CELL_COUNT, 8, "Dashboard should have 8 cells (4x2 grid)");
    }

    #[test]
    fn test_cell_indices() {
        // Verify cell index layout matches documentation
        // Row 1: BOOST | AFR | BATT | COOL
        assert_eq!(cell_idx::BOOST, 0, "BOOST should be cell 0");
        assert_eq!(cell_idx::AFR, 1, "AFR should be cell 1");
        assert_eq!(cell_idx::BATTERY, 2, "BATTERY should be cell 2");
        assert_eq!(cell_idx::COOLANT, 3, "COOLANT should be cell 3");
        // Row 2: OIL | DSG | IAT | EGT
        assert_eq!(cell_idx::OIL, 4, "OIL should be cell 4");
        assert_eq!(cell_idx::DSG, 5, "DSG should be cell 5");
        assert_eq!(cell_idx::IAT, 6, "IAT should be cell 6");
        assert_eq!(cell_idx::EGT, 7, "EGT should be cell 7");
    }

    // -------------------------------------------------------------------------
    // RenderState Creation Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_render_state_new() {
        let state = RenderState::new();

        // First frame should be true
        assert!(state.is_first_frame(), "is_first_frame should be true initially");

        // Dividers should not be drawn yet
        assert!(state.need_dividers(), "Dividers should be needed on first frame");

        // No popup should be visible
        assert!(
            !state.popup_just_closed(),
            "popup_just_closed should be false initially"
        );
    }

    #[test]
    fn test_render_state_default() {
        let default_state = RenderState::default();
        let new_state = RenderState::new();

        // Default should produce same result as new
        assert_eq!(default_state.is_first_frame(), new_state.is_first_frame());
        assert_eq!(default_state.need_dividers(), new_state.need_dividers());
        assert_eq!(default_state.popup_just_closed(), new_state.popup_just_closed());
    }

    // -------------------------------------------------------------------------
    // Divider State Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_need_dividers_first_frame() {
        let state = RenderState::new();
        assert!(state.need_dividers(), "Dividers should be needed on first frame");
    }

    #[test]
    fn test_mark_dividers_drawn() {
        let mut state = RenderState::new();
        state.first_frame = false; // Simulate past first frame

        assert!(state.need_dividers(), "Dividers initially needed");

        state.mark_dividers_drawn();

        assert!(
            !state.need_dividers(),
            "Dividers should not be needed after marking drawn"
        );
    }

    #[test]
    fn test_dividers_needed_after_popup_close() {
        let mut state = RenderState::new();
        state.first_frame = false;
        state.mark_dividers_drawn();
        assert!(!state.need_dividers(), "Dividers not needed after drawing");

        // Show popup
        let reset_popup = Popup::Reset(Instant::now());
        state.update_popup(Some(&reset_popup));
        assert!(!state.need_dividers(), "Dividers still not needed while popup visible");

        // Close popup - should force divider redraw
        state.update_popup(None);
        assert!(state.need_dividers(), "Dividers should be needed after popup closes");
    }

    // -------------------------------------------------------------------------
    // Header Dirty Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_check_header_dirty_first_frame() {
        let mut state = RenderState::new();
        assert!(
            state.check_header_dirty(true, 50.0),
            "Header should be dirty on first frame"
        );
    }

    #[test]
    fn test_check_header_dirty_fps_change() {
        let mut state = RenderState::new();
        state.first_frame = false;

        // Initial check
        state.check_header_dirty(true, 50.0);

        // Same FPS (50.4 rounds to 50) - not dirty
        assert!(
            !state.check_header_dirty(true, 50.4),
            "Header should not be dirty with same rounded FPS"
        );

        // Different FPS (50.5 rounds to 51) - dirty
        assert!(
            state.check_header_dirty(true, 50.5),
            "Header should be dirty when 50.5 rounds to 51"
        );

        // Reset to 51 for next test
        state.check_header_dirty(true, 51.0);

        // Clearly different FPS - dirty
        assert!(
            state.check_header_dirty(true, 60.0),
            "Header should be dirty with different FPS"
        );
    }

    #[test]
    fn test_check_header_dirty_show_fps_toggle() {
        let mut state = RenderState::new();
        state.first_frame = false;

        // Initial check with FPS shown
        state.check_header_dirty(true, 50.0);

        // Toggle FPS display off - dirty
        assert!(
            state.check_header_dirty(false, 50.0),
            "Header should be dirty when FPS toggled off"
        );

        // Toggle FPS display on - dirty
        assert!(
            state.check_header_dirty(true, 50.0),
            "Header should be dirty when FPS toggled on"
        );
    }

    #[test]
    fn test_check_header_dirty_fps_rounding_boundary() {
        let mut state = RenderState::new();
        state.first_frame = false;

        // Initialize at 49
        state.check_header_dirty(true, 49.0);

        // 49.4 rounds to 49 - not dirty
        assert!(
            !state.check_header_dirty(true, 49.4),
            "49.4 should round to 49, not trigger redraw"
        );

        // 49.5 rounds to 50 (round-half-to-even: 50) - dirty
        assert!(
            state.check_header_dirty(true, 49.5),
            "49.5 should round to 50, triggering redraw"
        );

        // Now at 50, 50.4 rounds to 50 - not dirty
        assert!(
            !state.check_header_dirty(true, 50.4),
            "50.4 should round to 50, not trigger redraw"
        );

        // 50.6 rounds to 51 - dirty
        assert!(
            state.check_header_dirty(true, 50.6),
            "50.6 should round to 51, triggering redraw"
        );
    }

    #[test]
    fn test_check_header_dirty_fps_hidden_no_change() {
        let mut state = RenderState::new();
        state.first_frame = false;

        // Initial check with FPS hidden
        state.check_header_dirty(false, 50.0);

        // FPS changes but still hidden - not dirty (FPS not displayed)
        assert!(
            !state.check_header_dirty(false, 60.0),
            "Header should not be dirty when FPS hidden"
        );
    }

    #[test]
    fn test_check_header_dirty_after_popup_close() {
        let mut state = RenderState::new();
        state.first_frame = false;
        state.check_header_dirty(true, 50.0); // Initialize

        // Close popup
        let reset_popup = Popup::Reset(Instant::now());
        state.update_popup(Some(&reset_popup));
        state.update_popup(None);

        // Header should be dirty after popup closes
        assert!(
            state.check_header_dirty(true, 50.0),
            "Header should be dirty after popup closes"
        );
    }

    // -------------------------------------------------------------------------
    // Popup State Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_update_popup_show() {
        let mut state = RenderState::new();

        let reset_popup = Popup::Reset(Instant::now());
        state.update_popup(Some(&reset_popup));

        assert!(
            !state.popup_just_closed(),
            "popup_just_closed should be false when showing popup"
        );
    }

    #[test]
    fn test_update_popup_hide() {
        let mut state = RenderState::new();

        // Show then hide popup
        let reset_popup = Popup::Reset(Instant::now());
        state.update_popup(Some(&reset_popup));
        state.update_popup(None);

        assert!(
            state.popup_just_closed(),
            "popup_just_closed should be true after hiding popup"
        );
    }

    #[test]
    fn test_update_popup_no_change() {
        let mut state = RenderState::new();

        // Popup already hidden, call with None again
        state.update_popup(None);

        assert!(
            !state.popup_just_closed(),
            "popup_just_closed should be false when no change"
        );
    }

    #[test]
    fn test_popup_just_closed_clears_after_end_frame() {
        let mut state = RenderState::new();

        // Show then hide popup
        let reset_popup = Popup::Reset(Instant::now());
        state.update_popup(Some(&reset_popup));
        state.update_popup(None);
        assert!(
            state.popup_just_closed(),
            "popup_just_closed should be true after close"
        );

        // End frame clears the flag
        state.end_frame();
        assert!(
            !state.popup_just_closed(),
            "popup_just_closed should be false after end_frame"
        );
    }

    #[test]
    fn test_popup_switch_triggers_cleanup() {
        let mut state = RenderState::new();
        state.first_frame = false;
        state.mark_dividers_drawn();

        // Show Reset popup (larger)
        let reset_popup = Popup::Reset(Instant::now());
        state.update_popup(Some(&reset_popup));
        assert!(!state.popup_just_closed(), "No cleanup when showing first popup");

        // Switch to FPS popup (smaller) - should trigger cleanup
        let fps_popup = Popup::Fps(Instant::now());
        state.update_popup(Some(&fps_popup));
        assert!(state.popup_just_closed(), "Switching popups should trigger cleanup");
        assert!(state.need_dividers(), "Dividers should need redraw after popup switch");
    }

    #[test]
    fn test_popup_same_kind_no_cleanup() {
        let mut state = RenderState::new();

        // Show Reset popup
        let reset_popup = Popup::Reset(Instant::now());
        state.update_popup(Some(&reset_popup));
        state.end_frame();

        // Update with same popup kind - no cleanup needed
        let reset_popup2 = Popup::Reset(Instant::now());
        state.update_popup(Some(&reset_popup2));
        assert!(!state.popup_just_closed(), "Same popup kind should not trigger cleanup");
    }

    // -------------------------------------------------------------------------
    // Frame State Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_is_first_frame() {
        let state = RenderState::new();
        assert!(state.is_first_frame(), "is_first_frame should be true initially");
    }

    #[test]
    fn test_end_frame_clears_first_frame() {
        let mut state = RenderState::new();
        assert!(state.is_first_frame(), "is_first_frame should be true initially");

        state.end_frame();

        assert!(
            !state.is_first_frame(),
            "is_first_frame should be false after end_frame"
        );
    }

    #[test]
    fn test_end_frame_clears_popup_just_closed() {
        let mut state = RenderState::new();
        let reset_popup = Popup::Reset(Instant::now());
        state.update_popup(Some(&reset_popup));
        state.update_popup(None);
        assert!(state.popup_just_closed());

        state.end_frame();

        assert!(
            !state.popup_just_closed(),
            "popup_just_closed should be false after end_frame"
        );
    }

    #[test]
    fn test_end_frame_multiple_calls() {
        let mut state = RenderState::new();

        state.end_frame();
        assert!(!state.is_first_frame());

        // Multiple end_frame calls should be safe
        state.end_frame();
        state.end_frame();

        assert!(!state.is_first_frame(), "is_first_frame should remain false");
    }

    // -------------------------------------------------------------------------
    // Display Cleared Flag Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_mark_display_cleared_sets_flag() {
        let mut state = RenderState::new();
        state.first_frame = false;
        state.mark_dividers_drawn();

        // Initially dividers not needed (already drawn)
        assert!(!state.need_dividers(), "Dividers should not be needed initially");

        // Mark display cleared
        state.mark_display_cleared();

        // Now dividers should be needed
        assert!(state.need_dividers(), "Dividers should be needed after display cleared");
    }

    #[test]
    fn test_display_cleared_affects_header_dirty() {
        let mut state = RenderState::new();
        state.first_frame = false;
        state.check_header_dirty(true, 50.0); // Initialize

        // Header should not be dirty (no change)
        assert!(
            !state.check_header_dirty(true, 50.0),
            "Header should not be dirty with same state"
        );

        // Mark display cleared
        state.mark_display_cleared();

        // Header should now be dirty
        assert!(
            state.check_header_dirty(true, 50.0),
            "Header should be dirty after display cleared"
        );
    }

    #[test]
    fn test_end_frame_clears_display_cleared() {
        let mut state = RenderState::new();
        state.first_frame = false;

        state.mark_display_cleared();
        assert!(state.need_dividers(), "Dividers needed after display cleared");

        state.mark_dividers_drawn();
        state.end_frame();

        // After end_frame, display_cleared should be false
        // So dividers should not be needed (already drawn, no flags set)
        assert!(
            !state.need_dividers(),
            "Dividers should not be needed after end_frame clears display_cleared"
        );
    }
}
