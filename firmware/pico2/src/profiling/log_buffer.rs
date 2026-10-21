//! Bounded shared log storage; formatting/truncation follows the common LogStore.
pub use dashboard_pico2::logging::Level as LogLevel;
use dashboard_pico2::logging::LogStore;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
/// Cross-core bounded history with a separately protected most-recent fault.
pub static LOGS: LogStore<CriticalSectionRawMutex> = LogStore::new();

/// Appends formatted `args` at `level` with current boot-relative milliseconds.
/// The store bounds/truncates text and counts overwritten entries; no heap allocation.
pub fn push_log(
    level: LogLevel,
    args: core::fmt::Arguments<'_>,
) {
    LOGS.push(level, embassy_time::Instant::now().as_millis(), args);
}

#[macro_export]
/// Formats arguments into the bounded Info log with a boot-relative timestamp;
/// expansion returns unit and does not send a vehicle or peripheral request.
macro_rules! log_info {
    ($($arg:tt)*) => {{
        $crate::profiling::push_log($crate::profiling::LogLevel::Info, format_args!($($arg)*));
    }};
}

#[macro_export]
/// Formats arguments into the bounded Fault log and latest-fault slot; expansion
/// returns unit. Recording evidence alone does not request reset or acknowledge it.
macro_rules! log_fault {
    ($($arg:tt)*) => {{
        $crate::profiling::push_log($crate::profiling::LogLevel::Fault, format_args!($($arg)*));
    }};
}
