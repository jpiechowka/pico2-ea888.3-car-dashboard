//! Controller termination evidence shared by both firmware paths and exports.
//! Bluetooth Core Vol 1, Part F: 0x13 remote user, 0x16 local host, 0x22 LL timeout.

/// Normal reason only. This never excuses unfinished work or a local ingress fault.
/// Missing, reserved and other reasons (including 0x00) cannot establish clean teardown.
pub const fn normal_reason(reason: Option<u8>) -> bool { matches!(reason, Some(0x13 | 0x16)) }

pub const fn reason_label(reason: u8) -> &'static str {
    match reason {
        0x08 => "Connection timeout",
        0x13 => "Remote user termination",
        0x14 => "Remote low resources",
        0x15 => "Remote power off",
        0x16 => "Local host termination",
        0x22 => "LL response timeout",
        0x3D => "MIC failure",
        _ => "Other / unrecognized reason",
    }
}
