//! Checked wall-clock conversion and nondecreasing job elapsed-time observations.
use std::time::{SystemTime, UNIX_EPOCH};
/// Returns `value` milliseconds as `u64`, or `None` when it cannot fit exactly.
pub fn checked_millis(value: u128) -> Option<u64> { u64::try_from(value).ok() }
/// Returns `current` elapsed milliseconds if available and at least `previous`.
/// Otherwise retain `previous` and latch `anomaly`; valid observations never clear it.
pub fn monotonic_elapsed(
    previous: u64,
    current: Option<u64>,
    anomaly: &mut bool,
) -> u64 {
    match current {
        Some(v) if v >= previous => v,
        _ => {
            *anomaly = true;
            previous
        }
    }
}
/// Returns whole UTC milliseconds since Unix epoch for `time`; `None` means a
/// pre-epoch observation or an elapsed value exceeding `u64`, never a fabricated zero.
pub fn utc_millis(time: SystemTime) -> Option<u64> {
    time.duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|d| checked_millis(d.as_millis()))
}
