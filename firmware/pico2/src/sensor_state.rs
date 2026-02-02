//! Sensor state tracking for trend detection, peak hold, rolling average, and graph history.
//!
//! This is a no_std compatible version that uses frame-based timing instead of wall-clock time.
//! Frame-based intervals are intentional for embedded use where FPS is relatively stable (~35 FPS).
//! This approach avoids the overhead of reading system time on every update.

use crate::config::{HISTORY_SIZE, TREND_THRESHOLD};

// =============================================================================
// Configuration Constants
// =============================================================================

/// Number of samples in the rolling average buffer.
const AVG_BUFFER_SIZE: usize = 60;

/// Interval between rolling average samples (in frames).
const AVG_SAMPLE_INTERVAL: u32 = 250;

/// Number of samples in the graph history buffer.
pub const GRAPH_HISTORY_SIZE: usize = 60;

/// Interval between graph samples (in frames).
const GRAPH_SAMPLE_INTERVAL: u32 = 100;

/// Peak hold duration in frames.
/// At ~35 FPS (typical with SPI 70 MHz), this is approximately 500-850ms.
const PEAK_HOLD_FRAMES: u32 = 30;

// =============================================================================
// Sensor State Structure
// =============================================================================

/// Tracks sensor history for trend arrows, peak detection, and rolling average.
///
/// This is a no_std compatible version using fixed arrays and frame-based timing.
pub struct SensorState {
    /// Circular buffer of recent sensor values for trend calculation.
    history: [f32; HISTORY_SIZE],
    history_index: usize,
    history_count: usize,

    /// Previous frame's value.
    prev_value: f32,

    /// Frame counter for peak hold timing.
    peak_hold_frames: u32,

    /// True for ~500ms after a new extreme value is recorded.
    pub is_new_peak: bool,

    // Rolling Average State
    avg_buffer: [f32; AVG_BUFFER_SIZE],
    avg_index: usize,
    avg_count: usize,
    avg_sum: f32,
    avg_frame_counter: u32,

    // Graph History State
    graph_buffer: [f32; GRAPH_HISTORY_SIZE],
    graph_index: usize,
    graph_count: usize,
    graph_frame_counter: u32,
    graph_min: f32,
    graph_max: f32,
}

impl SensorState {
    /// Create a new sensor state with pre-allocated history buffer.
    pub const fn new() -> Self {
        Self {
            history: [0.0; HISTORY_SIZE],
            history_index: 0,
            history_count: 0,
            prev_value: 0.0,
            peak_hold_frames: 0,
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
    pub fn update(
        &mut self,
        value: f32,
        is_max_updated: bool,
    ) {
        // Maintain fixed-size history buffer
        self.history[self.history_index] = value;
        self.history_index = (self.history_index + 1) % HISTORY_SIZE;
        if self.history_count < HISTORY_SIZE {
            self.history_count += 1;
        }
        self.prev_value = value;

        // Peak hold: highlight new extreme value for ~500ms (frame-based)
        if is_max_updated {
            self.peak_hold_frames = PEAK_HOLD_FRAMES;
            self.is_new_peak = true;
        } else if self.peak_hold_frames > 0 {
            self.peak_hold_frames -= 1;
            if self.peak_hold_frames == 0 {
                self.is_new_peak = false;
            }
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

    fn add_avg_sample(
        &mut self,
        value: f32,
    ) {
        if self.avg_count >= AVG_BUFFER_SIZE {
            self.avg_sum -= self.avg_buffer[self.avg_index];
        } else {
            self.avg_count += 1;
        }

        self.avg_buffer[self.avg_index] = value;
        self.avg_sum += value;
        self.avg_index = (self.avg_index + 1) % AVG_BUFFER_SIZE;
    }

    /// Get the rolling average.
    pub fn get_average(&self) -> Option<f32> {
        if self.avg_count == 0 {
            None
        } else {
            Some(self.avg_sum / self.avg_count as f32)
        }
    }

    /// Reset the rolling average buffer.
    pub fn reset_average(&mut self) {
        self.avg_buffer = [0.0; AVG_BUFFER_SIZE];
        self.avg_index = 0;
        self.avg_count = 0;
        self.avg_sum = 0.0;
        self.avg_frame_counter = 0;
    }

    fn add_graph_sample(
        &mut self,
        value: f32,
    ) {
        self.graph_buffer[self.graph_index] = value;
        self.graph_index = (self.graph_index + 1) % GRAPH_HISTORY_SIZE;

        if self.graph_count < GRAPH_HISTORY_SIZE {
            self.graph_count += 1;
        }

        self.recalculate_graph_minmax();
    }

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

    /// Get the graph history data.
    ///
    /// Returns (buffer, start_idx, count, data_min, data_max).
    pub const fn get_graph_data(&self) -> (&[f32; GRAPH_HISTORY_SIZE], usize, usize, f32, f32) {
        let start_idx = if self.graph_count < GRAPH_HISTORY_SIZE {
            0
        } else {
            self.graph_index
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
    pub fn reset_graph(&mut self) {
        self.graph_buffer = [0.0; GRAPH_HISTORY_SIZE];
        self.graph_index = 0;
        self.graph_count = 0;
        self.graph_frame_counter = 0;
        self.graph_min = f32::MAX;
        self.graph_max = f32::MIN;
    }

    /// Reset the peak highlight state.
    pub fn reset_peak(&mut self) {
        self.is_new_peak = false;
        self.peak_hold_frames = 0;
    }

    /// Get the current trend direction.
    pub fn get_trend(&self) -> Option<bool> {
        if self.history_count < 20 {
            return None;
        }

        // Calculate recent average (last 10 samples)
        let mut recent_sum = 0.0f32;
        for i in 0..10 {
            let idx = (self.history_index + HISTORY_SIZE - 1 - i) % HISTORY_SIZE;
            recent_sum += self.history[idx];
        }
        let recent_avg = recent_sum / 10.0;

        // Calculate older average (oldest 10 samples in buffer)
        let mut older_sum = 0.0f32;
        let start = if self.history_count < HISTORY_SIZE {
            0
        } else {
            self.history_index
        };
        for i in 0..10 {
            let idx = (start + i) % HISTORY_SIZE;
            older_sum += self.history[idx];
        }
        let older_avg = older_sum / 10.0;

        let diff = recent_avg - older_avg;
        if diff.abs() < TREND_THRESHOLD {
            None
        } else {
            Some(diff > 0.0)
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

    #[test]
    fn test_new_state() {
        let state = SensorState::new();
        assert_eq!(state.history_count, 0);
        assert_eq!(state.is_new_peak, false);
        assert!(state.get_average().is_none());
        assert!(state.get_trend().is_none());
    }

    #[test]
    fn test_default_impl() {
        let state = SensorState::default();
        assert_eq!(state.history_count, 0);
    }

    #[test]
    fn test_update_increments_history() {
        let mut state = SensorState::new();
        state.update(100.0, false);
        assert_eq!(state.history_count, 1);
        state.update(101.0, false);
        assert_eq!(state.history_count, 2);
    }

    #[test]
    fn test_peak_hold_activation() {
        let mut state = SensorState::new();
        state.update(100.0, true); // New peak
        assert!(state.is_new_peak);
        assert_eq!(state.peak_hold_frames, PEAK_HOLD_FRAMES);
    }

    #[test]
    fn test_peak_hold_decay() {
        let mut state = SensorState::new();
        state.update(100.0, true); // Activate peak
        assert!(state.is_new_peak);

        // Simulate frames passing
        for _ in 0..PEAK_HOLD_FRAMES {
            state.update(100.0, false);
        }

        // Peak should be cleared after PEAK_HOLD_FRAMES
        assert!(!state.is_new_peak);
        assert_eq!(state.peak_hold_frames, 0);
    }

    #[test]
    fn test_reset_peak() {
        let mut state = SensorState::new();
        state.update(100.0, true); // Activate peak
        assert!(state.is_new_peak);

        state.reset_peak();
        assert!(!state.is_new_peak);
        assert_eq!(state.peak_hold_frames, 0);
    }

    #[test]
    fn test_rolling_average() {
        let mut state = SensorState::new();

        // First, no average available
        assert!(state.get_average().is_none());

        // Manually trigger avg sample (normally happens every AVG_SAMPLE_INTERVAL frames)
        state.avg_frame_counter = AVG_SAMPLE_INTERVAL - 1;
        state.update(100.0, false);

        // Now average should be available
        let avg = state.get_average();
        assert!(avg.is_some());
        assert!((avg.unwrap() - 100.0).abs() < 0.001);
    }

    #[test]
    fn test_reset_average() {
        let mut state = SensorState::new();
        state.avg_frame_counter = AVG_SAMPLE_INTERVAL - 1;
        state.update(100.0, false);
        assert!(state.get_average().is_some());

        state.reset_average();
        assert!(state.get_average().is_none());
        assert_eq!(state.avg_count, 0);
        assert_eq!(state.avg_sum, 0.0);
    }

    #[test]
    fn test_graph_data_initial() {
        let state = SensorState::new();
        let (_, start_idx, count, ..) = state.get_graph_data();
        assert_eq!(start_idx, 0);
        assert_eq!(count, 0);
    }

    #[test]
    fn test_graph_sampling() {
        let mut state = SensorState::new();

        // Trigger graph sample
        state.graph_frame_counter = GRAPH_SAMPLE_INTERVAL - 1;
        state.update(50.0, false);

        let (_, _, count, min, max) = state.get_graph_data();
        assert_eq!(count, 1);
        assert!((min - 50.0).abs() < 0.001);
        assert!((max - 50.0).abs() < 0.001);
    }

    #[test]
    fn test_reset_graph() {
        let mut state = SensorState::new();
        state.graph_frame_counter = GRAPH_SAMPLE_INTERVAL - 1;
        state.update(50.0, false);

        state.reset_graph();
        let (_, _, count, ..) = state.get_graph_data();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_trend_requires_minimum_samples() {
        let mut state = SensorState::new();

        // Less than 20 samples should return None
        for _ in 0..19 {
            state.update(100.0, false);
        }
        assert!(state.get_trend().is_none());

        // 20th sample should allow trend calculation
        state.update(100.0, false);
        // Trend might still be None if values are stable, but function should work
        let _ = state.get_trend(); // Just verify it doesn't panic
    }

    #[test]
    fn test_trend_rising() {
        let mut state = SensorState::new();

        // Fill with rising values that exceed TREND_THRESHOLD
        for i in 0..HISTORY_SIZE {
            state.update(i as f32, false);
        }

        let trend = state.get_trend();
        assert!(trend.is_some());
        assert!(trend.unwrap()); // Rising = true
    }

    #[test]
    fn test_trend_falling() {
        let mut state = SensorState::new();

        // Fill with falling values
        for i in 0..HISTORY_SIZE {
            state.update((HISTORY_SIZE - i) as f32, false);
        }

        let trend = state.get_trend();
        assert!(trend.is_some());
        assert!(!trend.unwrap()); // Falling = false
    }

    #[test]
    fn test_constants() {
        assert_eq!(GRAPH_HISTORY_SIZE, 60);
        assert!(AVG_BUFFER_SIZE > 0);
        assert!(AVG_SAMPLE_INTERVAL > 0);
        assert!(GRAPH_SAMPLE_INTERVAL > 0);
        assert!(PEAK_HOLD_FRAMES > 0);
    }
}
