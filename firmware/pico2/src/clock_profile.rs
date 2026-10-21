//! Generated clock selection; register metadata does not measure physical clocks.
use embassy_rp::clocks::{ClockConfig, CoreVoltage, PllConfig};

include!(concat!(env!("OUT_DIR"), "/clock_profile.rs"));

/// Returns the generated PLL/voltage profile on the board's 12 MHz crystal
/// reference. Panics if PLL constraints fail; does not apply clocks or establish
/// electrical/thermal safety of an Expert profile.
pub fn clock_config() -> ClockConfig {
    assert!(
        PLL.is_valid(12_000_000),
        "generated PLL must meet pinned HAL constraints"
    );
    let mut clocks = ClockConfig::crystal(12_000_000);
    let xosc = clocks.xosc.as_mut().expect("crystal initializer has XOSC");
    xosc.sys_pll = Some(PLL);
    clocks.core_voltage = CORE_VOLTAGE;
    clocks
}
