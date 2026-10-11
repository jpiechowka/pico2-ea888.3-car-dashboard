//! Clock-derived PWM settings for the PIM715 backlight enable pin.

pub const PWM_TOP: u16 = 9_999;
pub const PWM_TARGET_HZ: u32 = 1_000;
const PERIOD_CLOCKS: u64 = (PWM_TOP as u64 + 1) * PWM_TARGET_HZ as u64;

/// Select an integer PWM divider from the configured system clock.
/// The supported 150/250/280/290/300 MHz profiles all yield exactly 1 kHz.
pub fn pwm_divider(clk_sys_hz: u32) -> Option<u8> {
    let rounded = (u64::from(clk_sys_hz) + PERIOD_CLOCKS / 2) / PERIOD_CLOCKS;
    u8::try_from(rounded).ok().filter(|divider| *divider != 0)
}

pub fn pwm_frequency_hz(
    clk_sys_hz: u32,
    divider: u8,
) -> u32 {
    if divider == 0 {
        return 0;
    }
    clk_sys_hz / (u32::from(divider) * (u32::from(PWM_TOP) + 1))
}

/// Preserve the old visible 45%-100% duty range until panel measurements
/// establish a useful low-duty response. Zero has an explicit off state.
pub fn brightness_compare(percent: u8) -> u16 {
    if percent == 0 {
        return 0;
    }
    (4_500 + u32::from(percent.min(100)) * 5_500 / 100) as u16
}
