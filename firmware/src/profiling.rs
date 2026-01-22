//! Profiling metrics and debug logging for performance monitoring.
//!
//! Provides frame timing statistics, render counters, and a ring buffer
//! for debug messages displayed on the debug page.
//!
//! # Usage
//!
//! ```ignore
//! let mut metrics = ProfilingMetrics::new();
//! let mut log = DebugLog::new();
//!
//! // In main loop:
//! let frame_start = Instant::now();
//! // ... render work ...
//! let render_time = frame_start.elapsed();
//! // ... sleep ...
//! let sleep_time = /* calculated sleep duration */;
//! metrics.record_frame(frame_start.elapsed(), render_time, sleep_time);
//!
//! // Log events:
//! log.push("Reset triggered");
//! ```
//!
//! # Embassy Preparation
//!
//! On real Pico hardware with Embassy:
//! - Replace `std::time::Instant` with `embassy_time::Instant`
//! - Add DWT cycle counter for more precise measurements
//! - Consider defmt + RTT for efficient binary logging

use std::time::{Duration, Instant};

use heapless::{Deque, String};

// =============================================================================
// Debug Log Configuration
// =============================================================================

/// Maximum number of log lines to keep in the ring buffer.
pub const LOG_BUFFER_SIZE: usize = 6;

/// Maximum characters per log line.
pub const LOG_LINE_LENGTH: usize = 48;

// =============================================================================
// Profiling Metrics
// =============================================================================

/// Frame timing and render statistics for profiling.
///
/// Tracks per-frame timing, min/max/average statistics, and render counters.
/// Updated every frame in the main loop.
pub struct ProfilingMetrics {
    // Frame timing (microseconds for precision)
    /// Total frame time (render + sleep + overhead)
    pub frame_time_us: u32,
    /// Time spent rendering (drawing to display buffer)
    pub render_time_us: u32,
    /// Time spent sleeping (rate limiting)
    pub sleep_time_us: u32,

    // Statistics (computed over time)
    /// Minimum frame time observed
    pub frame_time_min_us: u32,
    /// Maximum frame time observed
    pub frame_time_max_us: u32,
    /// Rolling average frame time (simple exponential moving average)
    frame_time_avg_us: f32,

    // Counters
    /// Total frames rendered since startup
    pub total_frames: u64,
    /// Header redraw count (should be low if dirty tracking works)
    pub header_redraws: u32,
    /// Divider redraw count
    pub divider_redraws: u32,
    /// Cell draw count (8 per frame on dashboard)
    pub cell_draws: u32,
    /// Color transition updates
    pub color_transitions: u32,
    /// Peak values detected
    pub peaks_detected: u32,

    // Uptime tracking
    start_time: Instant,
}

impl ProfilingMetrics {
    /// Create new profiling metrics, starting the uptime timer.
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

    /// Exponential moving average alpha (0.1 for smooth updates).
    const EMA_ALPHA: f32 = 0.1;

    /// Record frame timing for this frame.
    ///
    /// Updates current frame stats, min/max, and rolling average.
    pub fn record_frame(&mut self, total_time: Duration, render_time: Duration, sleep_time: Duration) {
        let total_us = total_time.as_micros() as u32;
        let render_us = render_time.as_micros() as u32;
        let sleep_us = sleep_time.as_micros() as u32;

        self.frame_time_us = total_us;
        self.render_time_us = render_us;
        self.sleep_time_us = sleep_us;

        // Update min/max
        if total_us < self.frame_time_min_us {
            self.frame_time_min_us = total_us;
        }
        if total_us > self.frame_time_max_us {
            self.frame_time_max_us = total_us;
        }

        // Exponential moving average
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
    pub const fn frame_time_avg_us(&self) -> u32 {
        self.frame_time_avg_us as u32
    }

    /// Get uptime since metrics were created.
    #[inline]
    pub fn uptime(&self) -> Duration {
        self.start_time.elapsed()
    }

    /// Format uptime as HH:MM:SS string.
    pub fn uptime_string(&self) -> String<12> {
        let secs = self.uptime().as_secs();
        let hours = secs / 3600;
        let mins = (secs % 3600) / 60;
        let secs = secs % 60;

        let mut s = String::new();
        // Manual formatting to avoid format! macro
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

    /// Increment header redraw counter.
    #[inline]
    pub const fn inc_header_redraws(&mut self) {
        self.header_redraws += 1;
    }

    /// Increment divider redraw counter.
    #[inline]
    pub const fn inc_divider_redraws(&mut self) {
        self.divider_redraws += 1;
    }

    /// Increment cell draw counter by n.
    #[inline]
    pub const fn inc_cell_draws(&mut self, n: u32) {
        self.cell_draws += n;
    }
}

impl Default for ProfilingMetrics {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Debug Log Ring Buffer
// =============================================================================

/// Ring buffer for debug log messages.
///
/// Stores the last `LOG_BUFFER_SIZE` messages (6 lines by default).
/// Old messages are automatically dropped when the buffer is full.
pub struct DebugLog {
    buffer: Deque<String<LOG_LINE_LENGTH>, LOG_BUFFER_SIZE>,
}

impl DebugLog {
    /// Create a new empty debug log.
    pub const fn new() -> Self {
        Self { buffer: Deque::new() }
    }

    /// Push a log message. If buffer is full, oldest message is dropped.
    pub fn push(&mut self, msg: &str) {
        // If full, remove oldest
        if self.buffer.is_full() {
            self.buffer.pop_front();
        }

        // Truncate message if too long
        let mut line: String<LOG_LINE_LENGTH> = String::new();
        for (i, c) in msg.chars().enumerate() {
            if i >= LOG_LINE_LENGTH - 1 {
                break;
            }
            line.push(c).ok();
        }

        self.buffer.push_back(line).ok();
    }

    /// Iterate over log messages (oldest first).
    pub fn iter(&self) -> impl Iterator<Item = &str> {
        self.buffer.iter().map(heapless::string::StringInner::as_str)
    }

    /// Get number of log entries.
    #[inline]
    #[allow(dead_code)]
    pub const fn len(&self) -> usize {
        self.buffer.len()
    }

    /// Check if log is empty.
    #[inline]
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }
}

impl Default for DebugLog {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Helper Functions
// =============================================================================

/// Push a u32 value to a heapless string (no format! macro).
fn push_u32<const N: usize>(s: &mut String<N>, mut val: u32) {
    if val == 0 {
        s.push('0').ok();
        return;
    }

    // Build digits in reverse
    let mut digits = [0u8; 10];
    let mut i = 0;
    while val > 0 {
        digits[i] = (val % 10) as u8;
        val /= 10;
        i += 1;
    }

    // Push in correct order
    while i > 0 {
        i -= 1;
        s.push((b'0' + digits[i]) as char).ok();
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_profiling_metrics_new() {
        let metrics = ProfilingMetrics::new();
        assert_eq!(metrics.total_frames, 0);
        assert_eq!(metrics.frame_time_us, 0);
        assert_eq!(metrics.frame_time_min_us, u32::MAX);
        assert_eq!(metrics.frame_time_max_us, 0);
    }

    #[test]
    fn test_record_frame() {
        let mut metrics = ProfilingMetrics::new();
        metrics.record_frame(
            Duration::from_micros(20000),
            Duration::from_micros(15000),
            Duration::from_micros(5000),
        );

        assert_eq!(metrics.total_frames, 1);
        assert_eq!(metrics.frame_time_us, 20000);
        assert_eq!(metrics.render_time_us, 15000);
        assert_eq!(metrics.sleep_time_us, 5000);
        assert_eq!(metrics.frame_time_min_us, 20000);
        assert_eq!(metrics.frame_time_max_us, 20000);
    }

    #[test]
    fn test_frame_min_max() {
        let mut metrics = ProfilingMetrics::new();

        metrics.record_frame(
            Duration::from_micros(20000),
            Duration::from_micros(15000),
            Duration::from_micros(5000),
        );
        metrics.record_frame(
            Duration::from_micros(15000),
            Duration::from_micros(10000),
            Duration::from_micros(5000),
        );
        metrics.record_frame(
            Duration::from_micros(25000),
            Duration::from_micros(20000),
            Duration::from_micros(5000),
        );

        assert_eq!(metrics.frame_time_min_us, 15000);
        assert_eq!(metrics.frame_time_max_us, 25000);
    }

    #[test]
    fn test_debug_log_push() {
        let mut log = DebugLog::new();
        assert!(log.is_empty());

        log.push("Test message");
        assert_eq!(log.len(), 1);

        log.push("Another message");
        assert_eq!(log.len(), 2);
    }

    #[test]
    fn test_debug_log_ring_buffer() {
        let mut log = DebugLog::new();

        // Fill buffer
        for i in 0..LOG_BUFFER_SIZE {
            log.push(&format!("Message {i}"));
        }
        assert_eq!(log.len(), LOG_BUFFER_SIZE);

        // Push one more - should drop oldest
        log.push("New message");
        assert_eq!(log.len(), LOG_BUFFER_SIZE);

        // First message should now be "Message 1" (Message 0 was dropped)
        let first = log.iter().next().unwrap();
        assert!(first.starts_with("Message 1"));
    }

    #[test]
    fn test_debug_log_truncation() {
        let mut log = DebugLog::new();
        let long_msg = "This is a very long message that exceeds the maximum line length limit";
        log.push(long_msg);

        let stored = log.iter().next().unwrap();
        assert!(stored.len() < LOG_LINE_LENGTH);
    }

    #[test]
    fn test_uptime_string_format() {
        let metrics = ProfilingMetrics::new();
        let uptime = metrics.uptime_string();
        // Should be "00:00:00" or close to it
        assert_eq!(uptime.len(), 8);
        assert!(uptime.contains(':'));
    }

    #[test]
    fn test_push_u32() {
        let mut s: String<16> = String::new();
        push_u32(&mut s, 0);
        assert_eq!(s.as_str(), "0");

        let mut s: String<16> = String::new();
        push_u32(&mut s, 123);
        assert_eq!(s.as_str(), "123");

        let mut s: String<16> = String::new();
        push_u32(&mut s, 9999);
        assert_eq!(s.as_str(), "9999");
    }
}
