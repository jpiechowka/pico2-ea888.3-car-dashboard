//! Legacy landscape geometry; configured orientations resolve through shared dashboard-config geometry.
/// Landscape pixel width; portrait swaps to240.
pub const SCREEN_WIDTH: u32 = 320;

/// Landscape pixel height; portrait swaps to320.
pub const SCREEN_HEIGHT: u32 = 240;

/// Capacity of the accepted-sample trend ring, separate from sixty graph buckets.
pub const HISTORY_SIZE: usize = 50;

/// Absolute difference between recent/older trend means required for an arrow, in canonical channel units.
pub const TREND_THRESHOLD: f32 = 0.5;

/// Fixed header height in pixels, shared by both orientations.
pub const HEADER_HEIGHT: u32 = 26;

/// Legacy landscape four-column cell width in pixels.
pub const COL_WIDTH: u32 = SCREEN_WIDTH / 4;

/// Legacy landscape two-row cell height in pixels below the header.
pub const ROW_HEIGHT: u32 = (SCREEN_HEIGHT - HEADER_HEIGHT) / 2;

/// Landscape screen horizontal midpoint in pixels from the left edge.
pub const CENTER_X: i32 = (SCREEN_WIDTH / 2) as i32;

/// Landscape screen vertical midpoint in pixels from the top edge.
pub const CENTER_Y: i32 = (SCREEN_HEIGHT / 2) as i32;
