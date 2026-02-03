//! Render state tracking for optimized display updates.
//!
//! This module provides:
//! - [`FpsMode`] - FPS display modes (Off, Instant, Average)
//! - [`RenderState`] - Tracks display state for conditional redraws
//! - [`cell_idx`] - Named cell indices for the dashboard grid
//!
//! # FPS Display Modes
//!
//! The FPS display cycles through modes via X button (Dashboard only):
//! - **Off** - No FPS displayed in header
//! - **Instant** - Shows current FPS (updated every second)
//! - **Average** - Shows average FPS since last page switch
//!
//! Average FPS is reset when switching pages.
//!
//! # Render State Tracking
//!
//! [`RenderState`] tracks display state for:
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

// =============================================================================
// FPS Display Mode
// =============================================================================

/// FPS display mode for the header.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum FpsMode {
    /// FPS display is off.
    #[default]
    Off,
    /// Show instantaneous FPS (updated every second).
    Instant,
    /// Show average FPS since last reset.
    Average,
}

impl FpsMode {
    /// Cycle to the next mode: Off -> Instant -> Average -> Off
    pub const fn next(self) -> Self {
        match self {
            Self::Off => Self::Instant,
            Self::Instant => Self::Average,
            Self::Average => Self::Off,
        }
    }

    /// Check if FPS should be displayed.
    pub const fn is_visible(self) -> bool { !matches!(self, Self::Off) }

    /// Get display label for the mode.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Off => "FPS OFF",
            Self::Instant => "FPS: INST",
            Self::Average => "FPS: AVG",
        }
    }

    /// Get short suffix for header display.
    pub const fn suffix(self) -> &'static str {
        match self {
            Self::Off => "",
            Self::Instant => " FPS",
            Self::Average => " AVG",
        }
    }
}

// =============================================================================
// Cell State Tracking
// =============================================================================

#[cfg(target_arch = "arm")]
use micromath::F32Ext;

/// Number of cells in the dashboard grid (4 columns × 2 rows).
pub const CELL_COUNT: usize = 8;

/// Cell indices for clearer code.
/// Layout:
///   Row 1 (top):    BOOST | AFR  | BATT | COOL
///   Row 2 (bottom): OIL   | DSG  | IAT  | EGT
pub mod cell_idx {
    // Row 1: BOOST | AFR | BATT | COOL
    #[allow(dead_code)] // Only used in tests (boost cell has special color handling)
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

/// Tracks render state for optimized display updates.
///
/// Manages conditional redraws for header/dividers and popup cleanup.
pub struct RenderState {
    /// Whether dividers have been drawn (only need to draw once).
    dividers_drawn: bool,

    /// Previous FPS display mode.
    prev_fps_mode: FpsMode,

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
            prev_fps_mode: FpsMode::Off,
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
    pub fn mark_dividers_drawn(&mut self) { self.dividers_drawn = true; }

    /// Force dividers to be redrawn on next frame.
    #[inline]
    #[allow(dead_code)]
    pub fn mark_dividers_dirty(&mut self) { self.dividers_drawn = false; }

    /// Check if header/FPS needs redrawing.
    ///
    /// Uses `fps.round()` to match the display formatting (`{:.0}`) which also
    /// rounds. This prevents mismatches where the dirty check sees a different
    /// value than what gets displayed.
    pub fn check_header_dirty(
        &mut self,
        fps_mode: FpsMode,
        fps: f32,
    ) -> bool {
        let fps_rounded = fps.round() as u32;
        let dirty = self.first_frame
            || self.popup_just_closed
            || self.display_cleared
            || fps_mode != self.prev_fps_mode
            || (fps_mode.is_visible() && fps_rounded != self.prev_fps_rounded);

        self.prev_fps_mode = fps_mode;
        self.prev_fps_rounded = fps_rounded;
        dirty
    }

    /// Update popup state with the current popup kind.
    ///
    /// Pass the popup kind as a u8 discriminant (or None if no popup).
    /// Detects both popup close (becomes None) and popup switch (kind changes).
    /// Both cases require display clear to remove remnants.
    pub fn update_popup(
        &mut self,
        popup_kind: Option<u8>,
    ) {
        let changed = popup_kind != self.prev_popup_kind;
        let was_visible = self.prev_popup_kind.is_some();
        self.prev_popup_kind = popup_kind;

        // Trigger cleanup when popup closes OR switches to different popup
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
    pub fn mark_display_cleared(&mut self) {
        self.display_cleared = true;
        self.dividers_drawn = false; // Force divider redraw
    }

    /// Call at end of frame to reset per-frame state.
    pub fn end_frame(&mut self) {
        self.first_frame = false;
        self.popup_just_closed = false;
        self.display_cleared = false;
    }
}

impl Default for RenderState {
    fn default() -> Self { Self::new() }
}

// =============================================================================
// Unit Tests (run on host with: cargo test --lib --target <host-triple>)
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cell_count() {
        assert_eq!(CELL_COUNT, 8, "Dashboard should have 8 cells (4x2 grid)");
    }

    #[test]
    fn test_cell_indices() {
        assert_eq!(cell_idx::BOOST, 0);
        assert_eq!(cell_idx::AFR, 1);
        assert_eq!(cell_idx::BATTERY, 2);
        assert_eq!(cell_idx::COOLANT, 3);
        assert_eq!(cell_idx::OIL, 4);
        assert_eq!(cell_idx::DSG, 5);
        assert_eq!(cell_idx::IAT, 6);
        assert_eq!(cell_idx::EGT, 7);
    }

    #[test]
    fn test_render_state_new() {
        let state = RenderState::new();
        assert!(state.is_first_frame());
        assert!(state.need_dividers());
        assert!(!state.popup_just_closed());
    }

    #[test]
    fn test_mark_dividers_drawn() {
        let mut state = RenderState::new();
        state.first_frame = false;
        assert!(state.need_dividers());
        state.mark_dividers_drawn();
        assert!(!state.need_dividers());
    }

    #[test]
    fn test_check_header_dirty_first_frame() {
        let mut state = RenderState::new();
        assert!(state.check_header_dirty(FpsMode::Instant, 50.0));
    }

    #[test]
    fn test_check_header_dirty_fps_change() {
        let mut state = RenderState::new();
        state.first_frame = false;
        state.check_header_dirty(FpsMode::Instant, 50.0);
        assert!(!state.check_header_dirty(FpsMode::Instant, 50.4)); // rounds to 50
        assert!(state.check_header_dirty(FpsMode::Instant, 51.0)); // different
    }

    #[test]
    fn test_fps_mode_cycle() {
        assert_eq!(FpsMode::Off.next(), FpsMode::Instant);
        assert_eq!(FpsMode::Instant.next(), FpsMode::Average);
        assert_eq!(FpsMode::Average.next(), FpsMode::Off);
    }

    #[test]
    fn test_fps_mode_visibility() {
        assert!(!FpsMode::Off.is_visible());
        assert!(FpsMode::Instant.is_visible());
        assert!(FpsMode::Average.is_visible());
    }

    #[test]
    fn test_popup_close_triggers_cleanup() {
        let mut state = RenderState::new();
        state.first_frame = false;
        state.mark_dividers_drawn();

        state.update_popup(Some(0)); // Show popup
        assert!(!state.popup_just_closed());

        state.update_popup(None); // Close popup
        assert!(state.popup_just_closed());
        assert!(state.need_dividers());
    }

    #[test]
    fn test_end_frame_clears_flags() {
        let mut state = RenderState::new();
        state.update_popup(Some(0));
        state.update_popup(None);
        assert!(state.popup_just_closed());

        state.end_frame();
        assert!(!state.popup_just_closed());
        assert!(!state.is_first_frame());
    }
}
