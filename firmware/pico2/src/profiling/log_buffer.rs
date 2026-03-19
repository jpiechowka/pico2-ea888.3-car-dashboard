use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use embedded_graphics::pixelcolor::Rgb565;
use heapless::String;

use crate::ui::GREEN;

pub const LOG_ENTRIES: usize = 14;

pub const LOG_MSG_LEN: usize = 40;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
#[repr(u8)]
pub enum LogLevel {
    #[default]
    Info = 2,
}

impl LogLevel {
    pub const fn color(self) -> Rgb565 {
        match self {
            Self::Info => GREEN,
        }
    }

    pub const fn prefix(self) -> char {
        match self {
            Self::Info => 'I',
        }
    }
}

#[derive(Clone)]
pub struct LogEntry {
    pub level: LogLevel,
    pub message: String<LOG_MSG_LEN>,
    pub timestamp_ms: u32,
}

impl LogEntry {
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

pub struct LogBuffer {
    entries: [LogEntry; LOG_ENTRIES],
    head: usize,
    count: usize,
}

impl LogBuffer {
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

    #[inline]
    pub const fn is_empty(&self) -> bool { self.count == 0 }

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

pub static LOG_BUFFER: Mutex<CriticalSectionRawMutex, LogBuffer> = Mutex::new(LogBuffer::new());

#[inline]
pub fn current_timestamp_ms() -> u32 { embassy_time::Instant::now().as_millis() as u32 }

pub fn push_log(
    level: LogLevel,
    message: &str,
) {
    let timestamp = current_timestamp_ms();
    let entry = LogEntry::new(level, message, timestamp);

    if let Ok(mut buffer) = LOG_BUFFER.try_lock() {
        buffer.push(entry);
    }
}

#[macro_export]
macro_rules! log_info {
    ($($arg:tt)*) => {{
        use core::fmt::Write;
        let mut buf: heapless::String<{ $crate::profiling::LOG_MSG_LEN }> = heapless::String::new();
        let _ = write!(buf, $($arg)*);
        $crate::profiling::push_log($crate::profiling::LogLevel::Info, buf.as_str());
        defmt::info!($($arg)*);
    }};
}
