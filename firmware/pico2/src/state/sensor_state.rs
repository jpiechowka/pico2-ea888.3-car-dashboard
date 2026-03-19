use crate::config::{HISTORY_SIZE, TREND_THRESHOLD};

const AVG_BUFFER_SIZE: usize = 60;

const AVG_SAMPLE_INTERVAL: u32 = 250;

pub const GRAPH_HISTORY_SIZE: usize = 60;

const GRAPH_SAMPLE_INTERVAL: u32 = 100;

const PEAK_HOLD_FRAMES: u32 = 30;

pub struct SensorState {
    history: [f32; HISTORY_SIZE],
    history_index: usize,
    history_count: usize,

    prev_value: f32,

    peak_hold_frames: u32,

    pub is_new_peak: bool,

    avg_buffer: [f32; AVG_BUFFER_SIZE],
    avg_index: usize,
    avg_count: usize,
    avg_sum: f32,
    avg_frame_counter: u32,

    graph_buffer: [f32; GRAPH_HISTORY_SIZE],
    graph_index: usize,
    graph_count: usize,
    graph_frame_counter: u32,
    graph_min: f32,
    graph_max: f32,
}

impl SensorState {
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

    pub fn update(
        &mut self,
        value: f32,
        is_max_updated: bool,
    ) {
        self.history[self.history_index] = value;
        self.history_index = (self.history_index + 1) % HISTORY_SIZE;
        if self.history_count < HISTORY_SIZE {
            self.history_count += 1;
        }
        self.prev_value = value;

        if is_max_updated {
            self.peak_hold_frames = PEAK_HOLD_FRAMES;
            self.is_new_peak = true;
        } else if self.peak_hold_frames > 0 {
            self.peak_hold_frames -= 1;
            if self.peak_hold_frames == 0 {
                self.is_new_peak = false;
            }
        }

        self.avg_frame_counter += 1;
        if self.avg_frame_counter >= AVG_SAMPLE_INTERVAL {
            self.avg_frame_counter = 0;
            self.add_avg_sample(value);
        }

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

    pub fn get_average(&self) -> Option<f32> {
        if self.avg_count == 0 {
            None
        } else {
            Some(self.avg_sum / self.avg_count as f32)
        }
    }

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
        let needs_recalculation = if self.graph_count == GRAPH_HISTORY_SIZE {
            let old_value = self.graph_buffer[self.graph_index];
            old_value == self.graph_min || old_value == self.graph_max
        } else {
            false
        };

        self.graph_buffer[self.graph_index] = value;
        self.graph_index = (self.graph_index + 1) % GRAPH_HISTORY_SIZE;

        if self.graph_count < GRAPH_HISTORY_SIZE {
            self.graph_count += 1;
        }

        if needs_recalculation {
            self.recalculate_graph_minmax();
        } else {
            if value < self.graph_min {
                self.graph_min = value;
            }
            if value > self.graph_max {
                self.graph_max = value;
            }
        }
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

    pub fn reset_graph(&mut self) {
        self.graph_buffer = [0.0; GRAPH_HISTORY_SIZE];
        self.graph_index = 0;
        self.graph_count = 0;
        self.graph_frame_counter = 0;
        self.graph_min = f32::MAX;
        self.graph_max = f32::MIN;
    }

    pub fn reset_peak(&mut self) {
        self.is_new_peak = false;
        self.peak_hold_frames = 0;
    }

    pub fn get_trend(&self) -> Option<bool> {
        if self.history_count < 20 {
            return None;
        }

        let mut recent_sum = 0.0f32;
        for i in 0..10 {
            let idx = (self.history_index + HISTORY_SIZE - 1 - i) % HISTORY_SIZE;
            recent_sum += self.history[idx];
        }
        let recent_avg = recent_sum / 10.0;

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
        state.update(100.0, true);
        assert!(state.is_new_peak);
        assert_eq!(state.peak_hold_frames, PEAK_HOLD_FRAMES);
    }

    #[test]
    fn test_peak_hold_decay() {
        let mut state = SensorState::new();
        state.update(100.0, true);
        assert!(state.is_new_peak);

        for _ in 0..PEAK_HOLD_FRAMES {
            state.update(100.0, false);
        }

        assert!(!state.is_new_peak);
        assert_eq!(state.peak_hold_frames, 0);
    }

    #[test]
    fn test_reset_peak() {
        let mut state = SensorState::new();
        state.update(100.0, true);
        assert!(state.is_new_peak);

        state.reset_peak();
        assert!(!state.is_new_peak);
        assert_eq!(state.peak_hold_frames, 0);
    }

    #[test]
    fn test_rolling_average() {
        let mut state = SensorState::new();

        assert!(state.get_average().is_none());

        state.avg_frame_counter = AVG_SAMPLE_INTERVAL - 1;
        state.update(100.0, false);

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

        for _ in 0..19 {
            state.update(100.0, false);
        }
        assert!(state.get_trend().is_none());

        state.update(100.0, false);
        let _ = state.get_trend();
    }

    #[test]
    fn test_trend_rising() {
        let mut state = SensorState::new();

        for i in 0..HISTORY_SIZE {
            state.update(i as f32, false);
        }

        let trend = state.get_trend();
        assert!(trend.is_some());
        assert!(trend.unwrap());
    }

    #[test]
    fn test_trend_falling() {
        let mut state = SensorState::new();

        for i in 0..HISTORY_SIZE {
            state.update((HISTORY_SIZE - i) as f32, false);
        }

        let trend = state.get_trend();
        assert!(trend.is_some());
        assert!(!trend.unwrap());
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
