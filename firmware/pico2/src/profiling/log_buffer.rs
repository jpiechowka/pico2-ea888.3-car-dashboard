pub use dashboard_pico2::logging::Level as LogLevel;
use dashboard_pico2::logging::LogStore;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
pub static LOGS: LogStore<CriticalSectionRawMutex> = LogStore::new();

pub fn push_log(
    level: LogLevel,
    args: core::fmt::Arguments<'_>,
) {
    LOGS.push(level, embassy_time::Instant::now().as_millis(), args);
}

#[macro_export]
macro_rules! log_info {
    ($($arg:tt)*) => {{
        $crate::profiling::push_log($crate::profiling::LogLevel::Info, format_args!($($arg)*));
    }};
}

#[macro_export]
macro_rules! log_fault {
    ($($arg:tt)*) => {{
        $crate::profiling::push_log($crate::profiling::LogLevel::Fault, format_args!($($arg)*));
    }};
}
