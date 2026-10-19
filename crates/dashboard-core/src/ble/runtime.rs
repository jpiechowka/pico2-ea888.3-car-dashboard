//! Small hardware-independent contracts used by the radio owner.

use crate::view_config::BleProfile;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WriteMode {
    Request,
    Command,
}

/// Decide from discovered properties, never switch write type after a failure.
/// CX and the fixture require acknowledged writes. MC+ also supports the
/// command-only characteristic used by its published embedded integration.
pub const fn write_mode(
    profile: BleProfile,
    notify: bool,
    response: bool,
    command: bool,
) -> Option<WriteMode> {
    if !notify {
        None
    } else if response {
        Some(WriteMode::Request)
    } else if matches!(profile, BleProfile::VlinkerMcPlus) && command {
        Some(WriteMode::Command)
    } else {
        None
    }
}

/// Conservative mitigation for observed CYW43 receive corruption; not a proven
/// hardware fix. Both radio owners use this ceiling, independently of LCD SPI.
/// See docs/hardware/cyw43-transport-follow-up.md.
pub const RADIO_GSPI_MAX_HZ: u32 = 2_500_000;

/// Locked cyw43-pio uses two PIO cycles per clock. Integer dividers avoid
/// fractional short cycles and select its low-speed program at supported clocks.
pub const fn pio_divider(cpu_hz: u32) -> Option<u16> {
    if cpu_hz == 0 || cpu_hz > 300_000_000 {
        return None;
    }
    Some(cpu_hz.div_ceil(2 * RADIO_GSPI_MAX_HZ) as u16)
}

/// Configured GSPI frequency, rounded down to Hz; not a physical measurement.
pub const fn radio_bus_hz(cpu_hz: u32) -> Option<u32> {
    match pio_divider(cpu_hz) {
        Some(divider) => Some(cpu_hz / (2 * divider as u32)),
        None => None,
    }
}

/// Observe completed radio bus boundaries, never a timer-only heartbeat.
pub struct Liveness {
    previous: u32,
    last_ms: u64,
    observed_ms: u64,
    started: bool,
    failed: bool,
}
impl Liveness {
    pub const fn new(now_ms: u64) -> Self {
        Self {
            previous: 0,
            last_ms: now_ms,
            observed_ms: now_ms,
            started: false,
            failed: false,
        }
    }

    pub fn observe(
        &mut self,
        count: u32,
        now_ms: u64,
        maintenance: bool,
    ) -> bool {
        if self.failed || now_ms < self.observed_ms {
            self.failed = true;
            return false;
        }
        self.observed_ms = now_ms;
        if count != self.previous {
            self.started = true;
            self.previous = count;
            self.last_ms = now_ms;
        }
        // The existing maintenance monitor independently bounds the whole pause
        // and withholds watchdog feeds; this only rebases radio liveness.
        if maintenance {
            self.last_ms = now_ms;
        }
        let budget = if self.started { 500 } else { 10_000 };
        self.failed = now_ms - self.last_ms >= budget;
        !self.failed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gatt_modes_require_notify_and_profile_compatible_writes() {
        use crate::view_config::BleProfile::{ObdLinkCx, TestPeer, VlinkerMcPlus};
        for profile in [ObdLinkCx, TestPeer, VlinkerMcPlus] {
            for response in [false, true] {
                for command in [false, true] {
                    assert_eq!(write_mode(profile, false, response, command), None);
                }
            }
            assert_eq!(write_mode(profile, true, false, false), None);
        }
        // MC+ source uses Write Command; prefer an acknowledged write if exposed.
        assert_eq!(write_mode(VlinkerMcPlus, true, false, true), Some(WriteMode::Command));
        assert_eq!(write_mode(VlinkerMcPlus, true, true, true), Some(WriteMode::Request));
        for profile in [ObdLinkCx, TestPeer] {
            assert_eq!(write_mode(profile, true, false, true), None);
            assert_eq!(write_mode(profile, true, true, false), Some(WriteMode::Request));
        }
    }

    #[test]
    fn conservative_bus_clock_uses_the_same_ceiling_at_stock_and_expert() {
        assert_eq!(pio_divider(150_000_000), Some(30));
        assert_eq!(pio_divider(280_000_000), Some(56));
        for hz in [1, 4_999_999, 5_000_000, 5_000_001, 299_999_999, 300_000_000] {
            let divider = u64::from(pio_divider(hz).unwrap());
            assert!(u64::from(hz) <= divider * 2 * 2_500_000);
            assert!(divider == 1 || u64::from(hz) > (divider - 1) * 2 * 2_500_000);
        }
    }

    #[test]
    fn integer_radio_clock_is_bounded_at_all_supported_cpu_speeds() {
        for hz in [
            132_000_000,
            140_000_000,
            150_000_000,
            250_000_000,
            280_000_000,
            290_400_000,
            300_000_000,
        ] {
            let divider = u32::from(pio_divider(hz).unwrap());
            assert!(hz <= divider * 2 * RADIO_GSPI_MAX_HZ);
            assert!(divider == 1 || hz > (divider - 1) * 2 * RADIO_GSPI_MAX_HZ);
        }
        assert_eq!(pio_divider(0), None);
        assert_eq!(pio_divider(300_000_001), None);
        assert_eq!(pio_divider(u32::MAX), None);
    }

    #[test]
    fn initialization_and_stalled_boundary_deadlines_latch() {
        let mut live = Liveness::new(20);
        assert!(live.observe(0, 10_019, false));
        assert!(!live.observe(0, 10_020, false));
        assert!(!live.observe(1, 10_021, false));
        let mut live = Liveness::new(0);
        assert!(live.observe(u32::MAX, 100, false));
        assert!(live.observe(0, 101, false));
        assert!(live.observe(0, 600, false));
        assert!(!live.observe(0, 601, false));
    }

    #[test]
    fn maintenance_rebases_without_masking_clock_reversal() {
        let mut live = Liveness::new(0);
        assert!(live.observe(1, 1, false));
        assert!(live.observe(1, 400, true));
        assert!(live.observe(1, 800, true));
        assert!(live.observe(2, 801, false));
        assert!(!live.observe(3, 800, true));
    }
}
