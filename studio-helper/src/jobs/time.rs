use std::time::{SystemTime, UNIX_EPOCH};
pub fn checked_millis(value: u128) -> Option<u64> { u64::try_from(value).ok() }
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
pub fn utc_millis(time: SystemTime) -> Option<u64> {
    time.duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|d| checked_millis(d.as_millis()))
}
