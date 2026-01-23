//! Sensor state tracking for trend detection, peak hold, rolling average, and graph history.
//!
//! Each sensor maintains its own `SensorState` instance to track:
//! - Historical values for trend analysis (50 samples ≈ 1 second)
//! - Peak detection with 500ms hold timer for UI highlighting
//! - Rolling average for display (e.g., "AVG 95C" on temperature cells)
//! - Graph history for mini sparkline visualization (60 samples)

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use dashboard_common::config::{HISTORY_SIZE, TREND_THRESHOLD};
use dashboard_common::widgets::SensorDisplayData;

// =============================================================================
// Rolling Average Configuration
// =============================================================================

/// Number of samples in the rolling average buffer.
const AVG_BUFFER_SIZE: usize = 60;

/// Interval between rolling average samples (in frames).
const AVG_SAMPLE_INTERVAL: u32 = 250;

// =============================================================================
// Graph History Configuration
// =============================================================================

/// Number of samples in the graph history buffer.
pub const GRAPH_HISTORY_SIZE: usize = 60;

/// Interval between graph samples (in frames).
const GRAPH_SAMPLE_INTERVAL: u32 = 100;

// =============================================================================
// Sensor State Structure
// =============================================================================

/// Tracks sensor history for trend arrows, peak detection, and rolling average.
pub struct SensorState {
    /// Circular buffer of recent sensor values for trend calculation.
    history: VecDeque<f32>,

    /// Previous frame's value.
    prev_value: f32,

    /// Timestamp when the last peak was detected.
    peak_hold_time: Option<Instant>,

    /// True for 500ms after a new extreme value is recorded.
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
    pub fn update(
        &mut self,
        value: f32,
        is_max_updated: bool,
    ) {
        // Maintain fixed-size history buffer
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
            self.peak_hold_time = None;
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

    /// Get the 5-minute rolling average.
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
    pub const fn get_graph_data(&self) -> (&[f32], usize, usize, f32, f32) {
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
        self.peak_hold_time = None;
    }

    /// Get the current trend direction.
    pub fn get_trend(&self) -> Option<bool> {
        if self.history.len() < 20 {
            return None;
        }

        let recent_avg: f32 = self.history.iter().rev().take(10).sum::<f32>() / 10.0;
        let older_avg: f32 = self.history.iter().take(10).sum::<f32>() / 10.0;

        let diff = recent_avg - older_avg;
        if diff.abs() < TREND_THRESHOLD {
            None
        } else {
            Some(diff > 0.0)
        }
    }

    /// Convert to display data for use with common widgets.
    pub fn to_display_data(&self) -> SensorDisplayData<'_> {
        let (buffer, start_idx, count, min, max) = self.get_graph_data();
        SensorDisplayData {
            trend: self.get_trend(),
            is_new_peak: self.is_new_peak,
            graph_buffer: buffer,
            graph_buffer_size: GRAPH_HISTORY_SIZE,
            graph_start_idx: start_idx,
            graph_count: count,
            graph_min: min,
            graph_max: max,
            average: self.get_average(),
        }
    }
}

impl Default for SensorState {
    fn default() -> Self { Self::new() }
}
