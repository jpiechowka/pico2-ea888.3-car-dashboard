//! Log buffer with levels and timestamps for on-device log viewing.
//!
//! Provides a circular buffer of log entries displayed on the Logs page.
//! Each entry has a log level, message, and timestamp.
//!
//! # Log Levels
//!
//! - `Trace`: Dark gray - verbose debugging
//! - `Debug`: Gray - debugging information
//! - `Info`: Green - normal operation
//! - `Warn`: Yellow - warnings
//! - `Error`: Red - errors
//!
//! # Usage
//!
//! ```ignore
//! use crate::log_buffer::{log_info, log_warn, log_error};
//!
//! log_info!("System started");
//! log_warn!("Low battery: {}V", voltage);
//! log_error!("Sensor timeout");
//! ```

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use embedded_graphics::pixelcolor::Rgb565;
use heapless::String;

use crate::colors::{GRAY, GREEN, RED, YELLOW};

/// Maximum number of log entries to keep.
pub const LOG_ENTRIES: usize = 14;

/// Maximum characters per log message.
pub const LOG_MSG_LEN: usize = 40;

/// Log severity level.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
#[repr(u8)]
#[allow(dead_code)] // Variants used via log_*! macros
pub enum LogLevel {
    /// Verbose debugging (dark gray)
    Trace = 0,
    /// Debug information (gray)
    Debug = 1,
    /// Normal operation (green)
    #[default]
    Info = 2,
    /// Warnings (yellow)
    Warn = 3,
    /// Errors (red)
    Error = 4,
}

impl LogLevel {
    /// Get the display color for this log level.
    pub const fn color(self) -> Rgb565 {
        match self {
            Self::Trace => GRAY,
            Self::Debug => GRAY,
            Self::Info => GREEN,
            Self::Warn => YELLOW,
            Self::Error => RED,
        }
    }

    /// Get the single-character prefix for this level.
    pub const fn prefix(self) -> char {
        match self {
            Self::Trace => 'T',
            Self::Debug => 'D',
            Self::Info => 'I',
            Self::Warn => 'W',
            Self::Error => 'E',
        }
    }
}

/// A single log entry with level, message, and timestamp.
#[derive(Clone)]
pub struct LogEntry {
    /// Log severity level.
    pub level: LogLevel,
    /// Log message (truncated to LOG_MSG_LEN).
    pub message: String<LOG_MSG_LEN>,
    /// Timestamp in milliseconds since boot (mod 100000 for display).
    pub timestamp_ms: u32,
}

impl LogEntry {
    /// Create a new log entry.
    pub fn new(
        level: LogLevel,
        message: &str,
        timestamp_ms: u32,
    ) -> Self {
        let mut msg: String<LOG_MSG_LEN> = String::new();
        for (i, c) in message.chars().enumerate() {
            if i >= LOG_MSG_LEN - 1 {
                break;
            }
            msg.push(c).ok();
        }
        Self {
            level,
            message: msg,
            timestamp_ms,
        }
    }
}

impl Default for LogEntry {
    fn default() -> Self {
        Self {
            level: LogLevel::Info,
            message: String::new(),
            timestamp_ms: 0,
        }
    }
}

/// Circular buffer of log entries.
pub struct LogBuffer {
    entries: [LogEntry; LOG_ENTRIES],
    head: usize, // Next write position
    count: usize,
}

impl LogBuffer {
    /// Create a new empty log buffer.
    pub const fn new() -> Self {
        Self {
            entries: [const {
                LogEntry {
                    level: LogLevel::Info,
                    message: String::new(),
                    timestamp_ms: 0,
                }
            }; LOG_ENTRIES],
            head: 0,
            count: 0,
        }
    }

    /// Push a new log entry. Oldest entry is dropped if buffer is full.
    pub fn push(
        &mut self,
        entry: LogEntry,
    ) {
        self.entries[self.head] = entry;
        self.head = (self.head + 1) % LOG_ENTRIES;
        if self.count < LOG_ENTRIES {
            self.count += 1;
        }
    }

    /// Get the number of entries in the buffer.
    #[inline]
    #[allow(dead_code)] // Public API, may be used by consumers
    pub const fn len(&self) -> usize { self.count }

    /// Check if buffer is empty.
    #[inline]
    pub const fn is_empty(&self) -> bool { self.count == 0 }

    /// Iterate over entries from oldest to newest.
    pub fn iter(&self) -> LogBufferIter<'_> {
        let start = if self.count < LOG_ENTRIES { 0 } else { self.head };
        LogBufferIter {
            buffer: self,
            pos: start,
            remaining: self.count,
        }
    }
}

impl Default for LogBuffer {
    fn default() -> Self { Self::new() }
}

/// Iterator over log buffer entries (oldest to newest).
pub struct LogBufferIter<'a> {
    buffer: &'a LogBuffer,
    pos: usize,
    remaining: usize,
}

impl<'a> Iterator for LogBufferIter<'a> {
    type Item = &'a LogEntry;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }
        let entry = &self.buffer.entries[self.pos];
        self.pos = (self.pos + 1) % LOG_ENTRIES;
        self.remaining -= 1;
        Some(entry)
    }
}

/// Global log buffer protected by a mutex.
pub static LOG_BUFFER: Mutex<CriticalSectionRawMutex, LogBuffer> = Mutex::new(LogBuffer::new());

/// Get the current timestamp in milliseconds for logging.
/// Uses embassy_time::Instant for accurate timing.
#[inline]
pub fn current_timestamp_ms() -> u32 { embassy_time::Instant::now().as_millis() as u32 }

/// Push a log entry to the global buffer.
///
/// This is non-blocking - if the mutex is held, the log is dropped.
pub fn push_log(
    level: LogLevel,
    message: &str,
) {
    let timestamp = current_timestamp_ms();
    let entry = LogEntry::new(level, message, timestamp);

    // Try to acquire lock without blocking
    if let Ok(mut buffer) = LOG_BUFFER.try_lock() {
        buffer.push(entry);
    }
    // If lock is held, silently drop the log (non-blocking requirement)
}

/// Log a message at Info level.
#[macro_export]
macro_rules! log_info {
    ($($arg:tt)*) => {{
        use core::fmt::Write;
        let mut buf: heapless::String<{ $crate::log_buffer::LOG_MSG_LEN }> = heapless::String::new();
        let _ = write!(buf, $($arg)*);
        $crate::log_buffer::push_log($crate::log_buffer::LogLevel::Info, buf.as_str());
        defmt::info!($($arg)*);
    }};
}

/// Log a message at Warn level.
#[macro_export]
macro_rules! log_warn {
    ($($arg:tt)*) => {{
        use core::fmt::Write;
        let mut buf: heapless::String<{ $crate::log_buffer::LOG_MSG_LEN }> = heapless::String::new();
        let _ = write!(buf, $($arg)*);
        $crate::log_buffer::push_log($crate::log_buffer::LogLevel::Warn, buf.as_str());
        defmt::warn!($($arg)*);
    }};
}

/// Log a message at Error level.
#[macro_export]
macro_rules! log_error {
    ($($arg:tt)*) => {{
        use core::fmt::Write;
        let mut buf: heapless::String<{ $crate::log_buffer::LOG_MSG_LEN }> = heapless::String::new();
        let _ = write!(buf, $($arg)*);
        $crate::log_buffer::push_log($crate::log_buffer::LogLevel::Error, buf.as_str());
        defmt::error!($($arg)*);
    }};
}

/// Log a message at Debug level.
#[macro_export]
macro_rules! log_debug {
    ($($arg:tt)*) => {{
        use core::fmt::Write;
        let mut buf: heapless::String<{ $crate::log_buffer::LOG_MSG_LEN }> = heapless::String::new();
        let _ = write!(buf, $($arg)*);
        $crate::log_buffer::push_log($crate::log_buffer::LogLevel::Debug, buf.as_str());
        defmt::debug!($($arg)*);
    }};
}

// Note: Unit tests are not available for this no_std embedded crate.
// The test framework requires std which is not available on ARM Cortex-M33.
// Test coverage is achieved through on-device validation.
