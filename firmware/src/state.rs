//! Sensor state tracking for trend detection, peak hold, rolling average, and graph history.
//!
//! Each sensor maintains its own `SensorState` instance to track:
//! - Historical values for trend analysis (50 samples ≈ 1 second)
//! - Peak detection with 500ms hold timer for UI highlighting
//! - Rolling average for display (e.g., "AVG 95C" on temperature cells)
//! - Graph history for mini sparkline visualization (60 samples)
//!
//! Used by temperature sensors (oil, coolant, DSG, IAT, EGT), battery voltage, and AFR tracking.
//!
//! # Trend Detection
//!
//! Trends are calculated by comparing the average of the most recent 10 samples
//! against the average of the oldest 10 samples in the history buffer. If the
//! difference exceeds `TREND_THRESHOLD` (from [`crate::config`]), a rising or
//! falling trend is indicated.
//!
//! This smoothed approach prevents noisy sensor data from causing arrow flicker.
//!
//! # Peak Hold
//!
//! When a new extreme value is detected, `is_new_peak` becomes `true` and stays
//! true for 500ms. This allows the UI to briefly highlight the value when a new
//! peak is reached. For most sensors this means maximum value; for battery voltage
//! it can be either minimum or maximum (both are noteworthy).
//!
//! The highlight color adapts to background luminance for readability:
//! - YELLOW on dark backgrounds (high visibility)
//! - BLACK on light backgrounds (readable on yellow/orange/green)
//!
//! # Rolling Average
//!
//! Maintains a 5-minute rolling average using a circular buffer of 60 samples
//! (one sample every 5 seconds). This provides a stable average without excessive
//! memory usage (~240 bytes per sensor for the average buffer).
//!
//! The average is computed incrementally: when a new sample is added, the oldest
//! is subtracted and the newest added to a running sum. This avoids iterating
//! the entire buffer each frame.
//!
//! # Graph History
//!
//! Maintains a rolling history of 60 samples for mini-graph visualization.
//! Samples are taken every 100 frames (~2 seconds at 50 FPS), giving ~2 minutes
//! of history displayed as a sparkline in the cell.
//!
//! # Memory Usage
//!
//! - Trend history: `VecDeque` with capacity `HISTORY_SIZE` (50 samples = 1 sec)
//! - Rolling average: Fixed array of 60 samples (5 min at 5-sec intervals)
//! - Graph history: Fixed array of 60 samples (~2 min at 2-sec intervals)

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use crate::config::{HISTORY_SIZE, TREND_THRESHOLD};

// =============================================================================
// Rolling Average Configuration
// =============================================================================

/// Number of samples in the rolling average buffer.
/// 60 samples at 5-second intervals = 5 minutes of history.
const AVG_BUFFER_SIZE: usize = 60;

/// Interval between rolling average samples (in frames).
/// At 50 FPS, 250 frames = 5 seconds between samples.
const AVG_SAMPLE_INTERVAL: u32 = 250;

// =============================================================================
// Graph History Configuration
// =============================================================================

/// Number of samples in the graph history buffer.
/// 60 samples displayed as a mini sparkline in the cell.
pub const GRAPH_HISTORY_SIZE: usize = 60;

/// Interval between graph samples (in frames).
/// At 50 FPS, 100 frames = 2 seconds between samples.
const GRAPH_SAMPLE_INTERVAL: u32 = 100;

// =============================================================================
// Sensor State Structure
// =============================================================================

/// Tracks sensor history for trend arrows, peak detection, and rolling average.
///
/// Create one instance per sensor and call `update()` each frame.
pub struct SensorState {
    /// Circular buffer of recent sensor values for trend calculation.
    /// Size is `HISTORY_SIZE` (50 samples ≈ 1 second at 50 FPS).
    history: VecDeque<f32>,

    /// Previous frame's value (currently unused but available for delta).
    prev_value: f32,

    /// Timestamp when the last peak was detected.
    /// Used to implement 500ms "new peak" highlight.
    peak_hold_time: Option<Instant>,

    /// True for 500ms after a new extreme value is recorded.
    /// For most sensors this is max; for battery it can be min OR max.
    /// UI can use this to highlight the value display.
    pub is_new_peak: bool,

    // -------------------------------------------------------------------------
    // Rolling Average State
    // -------------------------------------------------------------------------
    /// Circular buffer for 5-minute rolling average (60 samples @ 5s intervals).
    avg_buffer: [f32; AVG_BUFFER_SIZE],

    /// Current write position in the circular buffer.
    avg_index: usize,

    /// Number of valid samples in the buffer (grows until `AVG_BUFFER_SIZE`).
    avg_count: usize,

    /// Running sum of all values in `avg_buffer` (for O(1) average calculation).
    avg_sum: f32,

    /// Frame counter for sampling at `AVG_SAMPLE_INTERVAL`.
    avg_frame_counter: u32,

    // -------------------------------------------------------------------------
    // Graph History State
    // -------------------------------------------------------------------------
    /// Circular buffer for mini-graph visualization (60 samples @ 2s intervals).
    graph_buffer: [f32; GRAPH_HISTORY_SIZE],

    /// Current write position in the graph buffer.
    graph_index: usize,

    /// Number of valid samples in the graph buffer (grows until `GRAPH_HISTORY_SIZE`).
    graph_count: usize,

    /// Frame counter for sampling at `GRAPH_SAMPLE_INTERVAL`.
    graph_frame_counter: u32,

    /// Local minimum value within the graph buffer (for Y-axis scaling).
    graph_min: f32,

    /// Local maximum value within the graph buffer (for Y-axis scaling).
    graph_max: f32,
}

impl SensorState {
    /// Create a new sensor state with pre-allocated history buffer.
    pub fn new() -> Self {
        Self {
            history: VecDeque::with_capacity(HISTORY_SIZE),
            prev_value: 0.0,
            peak_hold_time: None,
            is_new_peak: false,
            avg_buffer: [0.0; AVG_BUFFER_SIZE],
            avg_index: 0,
            avg_count: 0,
            avg_sum: 0.0,
            avg_frame_counter: 0,
            graph_buffer: [0.0; GRAPH_HISTORY_SIZE],
            graph_index: 0,
            graph_count: 0,
            graph_frame_counter: 0,
            graph_min: f32::MAX,
            graph_max: f32::MIN,
        }
    }

    /// Update state with a new sensor reading.
    ///
    /// Call this once per frame with the current sensor value.
    ///
    /// # Parameters
    /// - `value`: Current sensor reading
    /// - `is_max_updated`: True if this value is a new maximum
    ///
    /// # Peak Hold Behavior
    /// When `is_max_updated` is true, `is_new_peak` becomes true and stays
    /// true for 500ms, allowing the UI to highlight the new peak.
    ///
    /// # Rolling Average
    /// Samples are added to the rolling average buffer every `AVG_SAMPLE_INTERVAL`
    /// frames (every 5 seconds at 50 FPS).
    pub fn update(
        &mut self,
        value: f32,
        is_max_updated: bool,
    ) {
        // Maintain fixed-size history buffer (FIFO)
        if self.history.len() >= HISTORY_SIZE {
            self.history.pop_front();
        }
        self.history.push_back(value);
        self.prev_value = value;

        // Peak hold: highlight new extreme value for 500ms
        if is_max_updated {
            self.peak_hold_time = Some(Instant::now());
            self.is_new_peak = true;
        } else if let Some(peak_time) = self.peak_hold_time
            && peak_time.elapsed() > Duration::from_millis(500)
        {
            self.is_new_peak = false;
            self.peak_hold_time = None; // Clear to avoid repeated checks
        }

        // Rolling average: sample every AVG_SAMPLE_INTERVAL frames
        self.avg_frame_counter += 1;
        if self.avg_frame_counter >= AVG_SAMPLE_INTERVAL {
            self.avg_frame_counter = 0;
            self.add_avg_sample(value);
        }

        // Graph history: sample every GRAPH_SAMPLE_INTERVAL frames
        self.graph_frame_counter += 1;
        if self.graph_frame_counter >= GRAPH_SAMPLE_INTERVAL {
            self.graph_frame_counter = 0;
            self.add_graph_sample(value);
        }
    }

    /// Add a sample to the rolling average buffer.
    ///
    /// Uses O(1) incremental update: subtract old value, add new value.
    fn add_avg_sample(
        &mut self,
        value: f32,
    ) {
        // If buffer is full, subtract the value we're about to overwrite
        if self.avg_count >= AVG_BUFFER_SIZE {
            self.avg_sum -= self.avg_buffer[self.avg_index];
        } else {
            self.avg_count += 1;
        }

        // Add new value
        self.avg_buffer[self.avg_index] = value;
        self.avg_sum += value;

        // Advance circular index
        self.avg_index = (self.avg_index + 1) % AVG_BUFFER_SIZE;
    }

    /// Get the 5-minute rolling average.
    ///
    /// Returns `None` if no samples have been collected yet.
    /// Displayed on temperature cells as "AVG {value}C".
    pub fn get_average(&self) -> Option<f32> {
        if self.avg_count == 0 {
            None
        } else {
            Some(self.avg_sum / self.avg_count as f32)
        }
    }

    /// Reset the rolling average buffer.
    ///
    /// Call this when min/max values are reset to start fresh averaging.
    pub const fn reset_average(&mut self) {
        self.avg_buffer = [0.0; AVG_BUFFER_SIZE];
        self.avg_index = 0;
        self.avg_count = 0;
        self.avg_sum = 0.0;
        self.avg_frame_counter = 0;
    }

    // -------------------------------------------------------------------------
    // Graph History Methods
    // -------------------------------------------------------------------------

    /// Add a sample to the graph history buffer.
    ///
    /// Updates local min/max for Y-axis scaling.
    fn add_graph_sample(
        &mut self,
        value: f32,
    ) {
        // Add new value to circular buffer
        self.graph_buffer[self.graph_index] = value;
        self.graph_index = (self.graph_index + 1) % GRAPH_HISTORY_SIZE;

        if self.graph_count < GRAPH_HISTORY_SIZE {
            self.graph_count += 1;
        }

        // Recalculate min/max from all valid samples
        self.recalculate_graph_minmax();
    }

    /// Recalculate graph min/max from all valid samples.
    ///
    /// Called after adding a sample or resetting the graph.
    fn recalculate_graph_minmax(&mut self) {
        if self.graph_count == 0 {
            self.graph_min = f32::MAX;
            self.graph_max = f32::MIN;
            return;
        }

        let mut min = f32::MAX;
        let mut max = f32::MIN;

        for i in 0..self.graph_count {
            let val = self.graph_buffer[i];
            if val < min {
                min = val;
            }
            if val > max {
                max = val;
            }
        }

        self.graph_min = min;
        self.graph_max = max;
    }

    /// Get the graph history as a slice of samples in chronological order.
    ///
    /// Returns a tuple of (samples, min, max) where samples is in oldest-to-newest order.
    /// The caller can use min/max for Y-axis scaling.
    pub const fn get_graph_data(&self) -> (&[f32], usize, usize, f32, f32) {
        // Return the buffer, start index, count, and min/max
        // The start index is where the oldest sample is located
        let start_idx = if self.graph_count < GRAPH_HISTORY_SIZE {
            0
        } else {
            self.graph_index // oldest sample is at current write position
        };
        (
            &self.graph_buffer,
            start_idx,
            self.graph_count,
            self.graph_min,
            self.graph_max,
        )
    }

    /// Reset the graph history buffer.
    ///
    /// Call this when min/max values are reset to start fresh graphing.
    pub const fn reset_graph(&mut self) {
        self.graph_buffer = [0.0; GRAPH_HISTORY_SIZE];
        self.graph_index = 0;
        self.graph_count = 0;
        self.graph_frame_counter = 0;
        self.graph_min = f32::MAX;
        self.graph_max = f32::MIN;
    }

    /// Reset the peak highlight state.
    ///
    /// Call this when min/max values are reset so the peak highlight
    /// doesn't linger from before the reset. Without this, a reset
    /// could leave `is_new_peak` true for up to 500ms.
    #[allow(dead_code)] // Available for callers to use during reset operations
    pub const fn reset_peak(&mut self) {
        self.is_new_peak = false;
        self.peak_hold_time = None;
    }

    /// Get the current trend direction, if significant.
    ///
    /// Compares average of recent 10 samples vs oldest 10 samples.
    /// Requires at least 20 samples in history to calculate.
    ///
    /// # Returns
    /// - `Some(true)`: Value is rising (recent > older by `TREND_THRESHOLD`)
    /// - `Some(false)`: Value is falling (recent < older by `TREND_THRESHOLD`)
    /// - `None`: Not enough data, or change is below threshold (stable)
    pub fn get_trend(&self) -> Option<bool> {
        if self.history.len() < 20 {
            return None; // Need at least 20 samples for comparison
        }

        // Average of most recent 10 samples
        let recent_avg: f32 = self.history.iter().rev().take(10).sum::<f32>() / 10.0;
        // Average of oldest 10 samples
        let older_avg: f32 = self.history.iter().take(10).sum::<f32>() / 10.0;

        let diff = recent_avg - older_avg;
        if diff.abs() < TREND_THRESHOLD {
            None // Change is below threshold, considered stable
        } else {
            Some(diff > 0.0) // true = rising, false = falling
        }
    }
}

impl Default for SensorState {
    fn default() -> Self { Self::new() }
}

// =============================================================================
// Unit Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // -------------------------------------------------------------------------
    // Configuration Constants Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_avg_buffer_size() {
        // 60 samples at 5-second intervals = 5 minutes of history
        assert_eq!(AVG_BUFFER_SIZE, 60, "AVG_BUFFER_SIZE should be 60 for 5-minute average");
    }

    #[test]
    fn test_avg_sample_interval() {
        // At 50 FPS, 250 frames = 5 seconds between samples
        assert_eq!(AVG_SAMPLE_INTERVAL, 250, "AVG_SAMPLE_INTERVAL should be 250 frames");
    }

    #[test]
    fn test_graph_history_size() {
        // 60 samples for mini sparkline visualization
        assert_eq!(GRAPH_HISTORY_SIZE, 60, "GRAPH_HISTORY_SIZE should be 60");
    }

    #[test]
    fn test_graph_sample_interval() {
        // At 50 FPS, 100 frames = 2 seconds between samples
        assert_eq!(GRAPH_SAMPLE_INTERVAL, 100, "GRAPH_SAMPLE_INTERVAL should be 100 frames");
    }

    // -------------------------------------------------------------------------
    // SensorState Creation Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_sensor_state_new() {
        let state = SensorState::new();

        // History should be empty but with capacity
        assert!(state.history.is_empty(), "History should start empty");
        assert!(state.history.capacity() >= HISTORY_SIZE, "History should have capacity");

        // Peak hold should be inactive
        assert!(!state.is_new_peak, "is_new_peak should be false initially");
        assert!(state.peak_hold_time.is_none(), "peak_hold_time should be None");

        // Rolling average should be empty
        assert_eq!(state.avg_count, 0, "avg_count should be 0");
        assert_eq!(state.avg_sum, 0.0, "avg_sum should be 0.0");

        // Graph history should be empty
        assert_eq!(state.graph_count, 0, "graph_count should be 0");
        assert_eq!(state.graph_min, f32::MAX, "graph_min should be f32::MAX");
        assert_eq!(state.graph_max, f32::MIN, "graph_max should be f32::MIN");
    }

    #[test]
    fn test_sensor_state_default() {
        let default_state = SensorState::default();
        let new_state = SensorState::new();

        // Default should produce same result as new
        assert_eq!(default_state.history.len(), new_state.history.len());
        assert_eq!(default_state.avg_count, new_state.avg_count);
        assert_eq!(default_state.graph_count, new_state.graph_count);
    }

    // -------------------------------------------------------------------------
    // Trend Detection Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_get_trend_insufficient_data() {
        let mut state = SensorState::new();

        // With less than 20 samples, trend should be None
        for i in 0..19 {
            state.history.push_back(i as f32);
        }
        assert!(state.get_trend().is_none(), "Should return None with < 20 samples");
    }

    #[test]
    fn test_get_trend_rising() {
        let mut state = SensorState::new();

        // Add samples with a clear rising trend
        // Oldest 10 samples average around 10, newest 10 around 30
        for i in 0..30 {
            state.history.push_back(i as f32);
        }

        let trend = state.get_trend();
        assert!(trend.is_some(), "Should have a trend with 30 samples");
        assert!(trend.unwrap(), "Trend should be rising (true)");
    }

    #[test]
    fn test_get_trend_falling() {
        let mut state = SensorState::new();

        // Add samples with a clear falling trend
        // Oldest 10 samples average around 30, newest 10 around 10
        for i in (0..30).rev() {
            state.history.push_back(i as f32);
        }

        let trend = state.get_trend();
        assert!(trend.is_some(), "Should have a trend with 30 samples");
        assert!(!trend.unwrap(), "Trend should be falling (false)");
    }

    #[test]
    fn test_get_trend_stable() {
        let mut state = SensorState::new();

        // Add samples that are all the same (stable)
        for _ in 0..30 {
            state.history.push_back(100.0);
        }

        let trend = state.get_trend();
        assert!(trend.is_none(), "Stable values should return None (below threshold)");
    }

    #[test]
    fn test_get_trend_below_threshold() {
        let mut state = SensorState::new();

        // Add samples with very small changes (below TREND_THRESHOLD)
        // TREND_THRESHOLD is 0.5, so tiny changes should not register
        for i in 0..30 {
            state.history.push_back((i as f32).mul_add(0.01, 100.0));
        }

        let trend = state.get_trend();
        // With 0.01 increment, diff between avg of first/last 10 is ~0.2, below 0.5
        assert!(trend.is_none(), "Small changes should be below threshold");
    }

    // -------------------------------------------------------------------------
    // Rolling Average Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_get_average_no_samples() {
        let state = SensorState::new();
        assert!(state.get_average().is_none(), "Average should be None with no samples");
    }

    #[test]
    fn test_add_avg_sample_single() {
        let mut state = SensorState::new();
        state.add_avg_sample(100.0);

        assert_eq!(state.avg_count, 1, "Should have 1 sample");
        assert_eq!(
            state.get_average(),
            Some(100.0),
            "Average of single value should be that value"
        );
    }

    #[test]
    fn test_add_avg_sample_multiple() {
        let mut state = SensorState::new();
        state.add_avg_sample(10.0);
        state.add_avg_sample(20.0);
        state.add_avg_sample(30.0);

        assert_eq!(state.avg_count, 3, "Should have 3 samples");
        assert_eq!(state.get_average(), Some(20.0), "Average of 10,20,30 should be 20");
    }

    #[test]
    fn test_add_avg_sample_circular_buffer() {
        let mut state = SensorState::new();

        // Fill the buffer completely
        for i in 0..AVG_BUFFER_SIZE {
            state.add_avg_sample(i as f32);
        }
        assert_eq!(state.avg_count, AVG_BUFFER_SIZE, "Buffer should be full");

        // Add one more sample - should overwrite oldest
        state.add_avg_sample(1000.0);
        assert_eq!(state.avg_count, AVG_BUFFER_SIZE, "Count should stay at buffer size");

        // Average should exclude the oldest (0) and include new (1000)
        // Sum was 0+1+2+...+59 = 1770, now 1+2+...+59+1000 = 1770-0+1000 = 2770
        let expected_avg = (1770.0 - 0.0 + 1000.0) / AVG_BUFFER_SIZE as f32;
        let actual_avg = state.get_average().unwrap();
        assert!(
            (actual_avg - expected_avg).abs() < 0.01,
            "Average should reflect circular overwrite"
        );
    }

    #[test]
    fn test_reset_average() {
        let mut state = SensorState::new();

        // Add some samples
        state.add_avg_sample(100.0);
        state.add_avg_sample(200.0);
        assert_eq!(state.avg_count, 2);

        // Reset
        state.reset_average();

        assert_eq!(state.avg_count, 0, "Count should be 0 after reset");
        assert_eq!(state.avg_sum, 0.0, "Sum should be 0 after reset");
        assert_eq!(state.avg_index, 0, "Index should be 0 after reset");
        assert!(state.get_average().is_none(), "Average should be None after reset");
    }

    // -------------------------------------------------------------------------
    // Graph History Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_get_graph_data_empty() {
        let state = SensorState::new();
        let (_, _, count, min, max) = state.get_graph_data();

        assert_eq!(count, 0, "Count should be 0 when empty");
        assert_eq!(min, f32::MAX, "Min should be f32::MAX when empty");
        assert_eq!(max, f32::MIN, "Max should be f32::MIN when empty");
    }

    #[test]
    fn test_add_graph_sample_single() {
        let mut state = SensorState::new();
        state.add_graph_sample(50.0);

        let (_, _, count, min, max) = state.get_graph_data();
        assert_eq!(count, 1, "Should have 1 sample");
        assert_eq!(min, 50.0, "Min should be the single value");
        assert_eq!(max, 50.0, "Max should be the single value");
    }

    #[test]
    fn test_add_graph_sample_multiple() {
        let mut state = SensorState::new();
        state.add_graph_sample(10.0);
        state.add_graph_sample(50.0);
        state.add_graph_sample(30.0);

        let (_, _, count, min, max) = state.get_graph_data();
        assert_eq!(count, 3, "Should have 3 samples");
        assert_eq!(min, 10.0, "Min should be 10.0");
        assert_eq!(max, 50.0, "Max should be 50.0");
    }

    #[test]
    fn test_add_graph_sample_circular_buffer() {
        let mut state = SensorState::new();

        // Fill the buffer completely with values 0-59
        for i in 0..GRAPH_HISTORY_SIZE {
            state.add_graph_sample(i as f32);
        }

        let (_, _, count, min, max) = state.get_graph_data();
        assert_eq!(count, GRAPH_HISTORY_SIZE, "Buffer should be full");
        assert_eq!(min, 0.0, "Min should be 0.0");
        assert_eq!(max, 59.0, "Max should be 59.0");

        // Add one more sample (100) - should overwrite oldest (0)
        state.add_graph_sample(100.0);

        let (_, _, count2, min2, max2) = state.get_graph_data();
        assert_eq!(count2, GRAPH_HISTORY_SIZE, "Count should stay at buffer size");
        assert_eq!(min2, 1.0, "Min should now be 1.0 (0 was overwritten)");
        assert_eq!(max2, 100.0, "Max should now be 100.0");
    }

    #[test]
    fn test_reset_graph() {
        let mut state = SensorState::new();

        // Add some samples
        state.add_graph_sample(10.0);
        state.add_graph_sample(50.0);
        assert_eq!(state.graph_count, 2);

        // Reset
        state.reset_graph();

        let (_, _, count, min, max) = state.get_graph_data();
        assert_eq!(count, 0, "Count should be 0 after reset");
        assert_eq!(min, f32::MAX, "Min should be f32::MAX after reset");
        assert_eq!(max, f32::MIN, "Max should be f32::MIN after reset");
    }

    // -------------------------------------------------------------------------
    // History Buffer Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_history_buffer_limit() {
        let mut state = SensorState::new();

        // Add more samples than HISTORY_SIZE
        for i in 0..(HISTORY_SIZE + 10) {
            state.history.push_back(i as f32);
            if state.history.len() > HISTORY_SIZE {
                state.history.pop_front();
            }
        }

        assert_eq!(
            state.history.len(),
            HISTORY_SIZE,
            "History should not exceed HISTORY_SIZE"
        );
    }

    // -------------------------------------------------------------------------
    // Peak Reset Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_reset_peak() {
        let mut state = SensorState::new();

        // Simulate a peak being detected
        state.is_new_peak = true;
        state.peak_hold_time = Some(Instant::now());

        // Reset peak
        state.reset_peak();

        assert!(!state.is_new_peak, "is_new_peak should be false after reset");
        assert!(
            state.peak_hold_time.is_none(),
            "peak_hold_time should be None after reset"
        );
    }

    #[test]
    fn test_reset_peak_when_already_inactive() {
        let mut state = SensorState::new();

        // Peak is not active
        assert!(!state.is_new_peak);
        assert!(state.peak_hold_time.is_none());

        // Reset should be safe even when already inactive
        state.reset_peak();

        assert!(!state.is_new_peak, "is_new_peak should remain false");
        assert!(state.peak_hold_time.is_none(), "peak_hold_time should remain None");
    }
}
