//! PIM715 active-low RGB: slice5 A/B (GP26/27), slice6 A (GP28).
use dashboard_core::status_led::LedDuty;

/// Compare is light duty because the owned outputs are inverted. Clamp at the
/// hardware full-light endpoint TOP+1, preserving each channel independently.
pub fn compares(duty: LedDuty) -> [u16; 3] { [duty.red.min(10000), duty.green.min(10000), duty.blue.min(10000)] }

#[cfg(target_arch = "arm")]
pub use hardware::RgbLed;

#[cfg(target_arch = "arm")]
mod hardware {
    use embassy_rp::Peri;
    use embassy_rp::peripherals::{PIN_26, PIN_27, PIN_28, PWM_SLICE5, PWM_SLICE6};
    use embassy_rp::pwm::{Config, Pwm};

    use super::*;

    /// Kept in Core0's dashboard future for the application lifetime. Off never
    /// disables/drops a slice (Drop reconnects pads with pull-down).
    pub struct RgbLed<'d> {
        red_green: Pwm<'d>,
        blue: Pwm<'d>,
        applied: [u16; 3],
    }
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
