//! Debug logging utilities.
//!
//! Provides a ring buffer for debug messages displayed on the debug page.
//! The time-dependent `ProfilingMetrics` struct is defined in the simulator/pico
//! crates since it requires platform-specific time types.
//!
//! # Usage
//!
//! ```ignore
//! let mut log = DebugLog::new();
//! log.push("System started");
//! log.push("Reset triggered");
//!
//! for line in log.iter() {
//!     println!("{}", line);
//! }
//! ```

use heapless::{Deque, String};

// =============================================================================
// Debug Log Configuration
// =============================================================================

/// Maximum number of log lines to keep in the ring buffer.
pub const LOG_BUFFER_SIZE: usize = 6;

/// Maximum characters per log line.
pub const LOG_LINE_LENGTH: usize = 48;

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
    pub const fn new() -> Self { Self { buffer: Deque::new() } }

    /// Push a log message. If buffer is full, oldest message is dropped.
    pub fn push(
        &mut self,
        msg: &str,
    ) {
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
    pub fn iter(&self) -> impl Iterator<Item = &str> { self.buffer.iter().map(heapless::string::StringInner::as_str) }

    /// Get number of log entries.
    #[inline]
    #[allow(dead_code)]
    pub const fn len(&self) -> usize { self.buffer.len() }

    /// Check if log is empty.
    #[inline]
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool { self.buffer.is_empty() }
}

impl Default for DebugLog {
    fn default() -> Self { Self::new() }
}

// =============================================================================
// Helper Functions
// =============================================================================

/// Push a u32 value to a heapless string (no format! macro).
pub fn push_u32<const N: usize>(
    s: &mut String<N>,
    mut val: u32,
) {
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
            let mut msg: String<16> = String::new();
            push_u32(&mut msg, i as u32);
            log.push(&msg);
        }
        assert_eq!(log.len(), LOG_BUFFER_SIZE);

        // Push one more - should drop oldest
        log.push("New");
        assert_eq!(log.len(), LOG_BUFFER_SIZE);

        // First should now be "1" (0 was dropped)
        let first = log.iter().next().unwrap();
        assert_eq!(first, "1");
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
