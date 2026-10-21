//! Host checks for the real inverted-output RGB compare calculation, without PWM hardware.
/// Firmware driver with ARM hardware ownership excluded by its target guard.
#[path = "../../firmware/pico2/src/drivers/rgb_led.rs"]
mod rgb_led;
use dashboard_core::status_led::LedDuty;
/// Each channel independently preserves duty and saturates at TOP+1; inversion belongs to owned hardware configuration.
#[test]
fn active_low_compare_endpoints_are_independent_and_saturated() {
    assert_eq!(
        rgb_led::compares(LedDuty {
            red: 0,
            green: 10000,
            blue: 5000
        }),
        [0, 10000, 5000]
    );
    assert_eq!(
        rgb_led::compares(LedDuty {
            red: 65535,
            green: 1,
            blue: 0
        }),
        [10000, 1, 0]
    );
    assert_eq!(
        rgb_led::compares(LedDuty {
            red: 17,
            green: 83,
            blue: 9210
        }),
        [17, 83, 9210]
    );
}
