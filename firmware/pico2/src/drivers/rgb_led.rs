//! PIM715 active-low RGB: slice5 A/B (GP26/27), slice6 A (GP28).
use dashboard_core::status_led::LedDuty;

/// Compare is light duty because the owned outputs are inverted. Clamp at the
/// hardware full-light endpoint TOP+1, preserving each channel independently.
/// Returns red/green/blue compares from `duty`, independently bounded to 0..=10000.
pub fn compares(duty: LedDuty) -> [u16; 3] { [duty.red.min(10000), duty.green.min(10000), duty.blue.min(10000)] }

#[cfg(target_arch = "arm")]
pub use hardware::RgbLed;

#[cfg(target_arch = "arm")]
/// ARM-only PWM ownership; host callers can exercise the shared compare calculation.
mod hardware {
    use embassy_rp::Peri;
    use embassy_rp::peripherals::{PIN_26, PIN_27, PIN_28, PWM_SLICE5, PWM_SLICE6};
    use embassy_rp::pwm::{Config, Pwm};

    use super::*;

    /// Kept in Core0's dashboard future for the application lifetime. Off never
    /// disables/drops a slice (Drop reconnects pads with pull-down).
    pub struct RgbLed<'d> {
        /// Slice5 A/B outputs for active-low GP26 red and GP27 green.
        red_green: Pwm<'d>,
        /// Slice6 A output for active-low GP28 blue.
        blue: Pwm<'d>,
        /// Last applied red/green/blue compares, suppressing redundant reconfiguration.
        applied: [u16; 3],
    }
    /// Returns enabled, inverted PWM with TOP 9999 and divider 1; `a` and `b`
    /// are already-clamped light-duty compares supplied by this module.
    fn config(
        a: u16,
        b: u16,
    ) -> Config {
        let mut config = Config::default();
        config.top = 9999;
        config.divider = 1.into();
        config.invert_a = true;
        config.invert_b = true;
        config.phase_correct = false;
        config.enable = true;
        config.compare_a = a;
        config.compare_b = b;
        config
    }
    impl<'d> RgbLed<'d> {
        /// Consumes `slice5` with `red`/`green` pins and `slice6` with `blue`.
        /// Returns an all-off owner; pin mux release follows inverted zero-duty
        /// configuration in the locked HAL. Keep it alive even while LEDs are off.
        pub fn new(
            slice5: Peri<'d, PWM_SLICE5>,
            red: Peri<'d, PIN_26>,
            green: Peri<'d, PIN_27>,
            slice6: Peri<'d, PWM_SLICE6>,
            blue: Peri<'d, PIN_28>,
        ) -> Self {
            // Locked HAL configures zero compares + inversion before function4
            // and RP235x pad isolation release. Both constructors receive zero light-duty compares.
            Self {
                red_green: Pwm::new_output_ab(slice5, red, green, config(0, 0)),
                blue: Pwm::new_output_a(slice6, blue, config(0, 0)),
                applied: [0; 3],
            }
        }

        /// Applies independently clamped `duty` channels, updating only slices
        /// whose compares changed. Zero leaves the inverted PWM outputs enabled.
        pub fn apply(
            &mut self,
            duty: LedDuty,
        ) {
            let next = compares(duty);
            if next[..2] != self.applied[..2] {
                self.red_green.set_config(&config(next[0], next[1]));
            }
            if next[2] != self.applied[2] {
                self.blue.set_config(&config(next[2], 0));
            }
            self.applied = next;
        }
    }
}
