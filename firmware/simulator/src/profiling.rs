//! Profiling metrics with time-based measurements.
//!
//! Provides frame timing statistics and render counters.
//! The `DebugLog` type is in the common crate since it doesn't need time.

use std::time::{Duration, Instant};

use dashboard_common::profiling::push_u32;
use heapless::String;

/// Frame timing and render statistics for profiling.
pub struct ProfilingMetrics {
    // Frame timing (microseconds)
    pub frame_time_us: u32,
    pub render_time_us: u32,
    pub sleep_time_us: u32,

    // Statistics
    pub frame_time_min_us: u32,
    pub frame_time_max_us: u32,
    frame_time_avg_us: f32,

    // Counters
    pub total_frames: u64,
    pub header_redraws: u32,
    pub divider_redraws: u32,
    pub cell_draws: u32,
    pub color_transitions: u32,
    pub peaks_detected: u32,

    // Uptime tracking
    start_time: Instant,
}

impl ProfilingMetrics {
    const EMA_ALPHA: f32 = 0.1;

    /// Create new profiling metrics.
    pub fn new() -> Self {
        Self {
            frame_time_us: 0,
            render_time_us: 0,
            sleep_time_us: 0,
            frame_time_min_us: u32::MAX,
            frame_time_max_us: 0,
            frame_time_avg_us: 0.0,
            total_frames: 0,
            header_redraws: 0,
            divider_redraws: 0,
            cell_draws: 0,
            color_transitions: 0,
            peaks_detected: 0,
            start_time: Instant::now(),
        }
    }

    /// Record frame timing for this frame.
    pub fn record_frame(
        &mut self,
        total_time: Duration,
        render_time: Duration,
        sleep_time: Duration,
    ) {
        let total_us = total_time.as_micros() as u32;
        let render_us = render_time.as_micros() as u32;
        let sleep_us = sleep_time.as_micros() as u32;

        self.frame_time_us = total_us;
        self.render_time_us = render_us;
        self.sleep_time_us = sleep_us;

        if total_us < self.frame_time_min_us {
            self.frame_time_min_us = total_us;
        }
        if total_us > self.frame_time_max_us {
            self.frame_time_max_us = total_us;
        }

        if self.total_frames == 0 {
            self.frame_time_avg_us = total_us as f32;
        } else {
            self.frame_time_avg_us =
                Self::EMA_ALPHA.mul_add(total_us as f32, (1.0 - Self::EMA_ALPHA) * self.frame_time_avg_us);
        }

        self.total_frames += 1;
    }

    /// Get average frame time in microseconds.
    #[inline]
    pub const fn frame_time_avg_us(&self) -> u32 { self.frame_time_avg_us as u32 }

    /// Get uptime since metrics were created.
    #[inline]
    pub fn uptime(&self) -> Duration { self.start_time.elapsed() }

    /// Format uptime as HH:MM:SS string.
    pub fn uptime_string(&self) -> String<12> {
        let secs = self.uptime().as_secs();
        let hours = secs / 3600;
        let mins = (secs % 3600) / 60;
        let secs = secs % 60;

        let mut s = String::new();
        if hours < 10 {
            s.push('0').ok();
        }
        push_u32(&mut s, hours as u32);
        s.push(':').ok();
        if mins < 10 {
            s.push('0').ok();
        }
        push_u32(&mut s, mins as u32);
        s.push(':').ok();
        if secs < 10 {
            s.push('0').ok();
        }
        push_u32(&mut s, secs as u32);
        s
    }

    #[inline]
    pub fn inc_header_redraws(&mut self) { self.header_redraws += 1; }

    #[inline]
    pub fn inc_divider_redraws(&mut self) { self.divider_redraws += 1; }

    #[inline]
    pub fn inc_cell_draws(
        &mut self,
        n: u32,
    ) {
        self.cell_draws += n;
    }
}

impl Default for ProfilingMetrics {
    fn default() -> Self { Self::new() }
}
