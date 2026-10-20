//! Clock output signal generator using PIO

use crate::Peri;
use crate::pio::program::pio_asm;
use crate::pio::{Common, Config, Direction, Instance, LoadedProgram, Pin, PioBatch, PioPin, StateMachine};
use crate::pio_programs::clock_divider::calculate_pio_clock_divider;

/// Clock generator PIO program
pub struct PioClkProgram<'a, PIO: Instance> {
    prg: LoadedProgram<'a, PIO>,
}

const PIO_CLK_PROGRAM_CLK_MULTIPLIER: u32 = 2;

impl<'a, PIO: Instance> PioClkProgram<'a, PIO> {
    /// Load the program into PIO instruction memory
    pub fn new(common: &mut Common<'a, PIO>) -> Self {
        let prg = pio_asm!("set pins 0", "set pins 1");
        let prg = common.load_program(&prg.program);
        Self { prg }
    }
}

/// Pio backed clock generator
pub struct PioClk<'d, T: Instance, const SM: usize> {
    sm: StateMachine<'d, T, SM>,
    pin: Pin<'d, T>,
}

impl<'d, T: Instance, const SM: usize> PioClk<'d, T, SM> {
    /// Configure state machine for clock generation
    pub fn new(
        pio: &mut Common<'d, T>,
        mut sm: StateMachine<'d, T, SM>,
        pin: Peri<'d, impl PioPin>,
        program: &PioClkProgram<'d, T>,
        frequency: u32,
    ) -> Self {
        let divider = calculate_pio_clock_divider(state_machine_frequency(frequency));
        let pin = pio.make_pio_pin(pin);
        sm.set_pin_dirs(Direction::Out, &[&pin]);

        let mut cfg = Config::default();
        cfg.clock_divider = divider;
        cfg.set_set_pins(&[&pin]);
        cfg.use_program(&program.prg, &[]);

        sm.set_config(&cfg);

        Self { sm, pin }
    }

    /// Start at the the same as other drivers
    pub fn start_batched(
        &mut self,
        b: &mut PioBatch<'d, T>,
    ) {
        b.set_enable(&mut self.sm, true);
    }

    /// Stop at the the same as other drivers
    pub fn stop_batched(
        &mut self,
        b: &mut PioBatch<'d, T>,
    ) {
        b.set_enable(&mut self.sm, false);
    }

    /// Start emmiting clock
    pub fn start(&mut self) { self.sm.set_enable(true); }

    /// Stop emmiting clock
    pub fn stop(&mut self) { self.sm.set_enable(false); }

    /// Return the state machine and pin
    pub fn release(mut self) -> (StateMachine<'d, T, SM>, Pin<'d, T>) {
        self.stop();
        (self.sm, self.pin)
    }
}

fn state_machine_frequency(frequency: u32) -> u32 {
    frequency
        .checked_mul(PIO_CLK_PROGRAM_CLK_MULTIPLIER)
        .expect("PIO output clock overflow")
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;

    #[test]
    fn output_frequency_multiplier_preserves_values_and_rejects_wraparound() {
        assert_eq!(state_machine_frequency(1_000_000), 2_000_000);
        assert_eq!(state_machine_frequency(u32::MAX / 2), u32::MAX - 1);
        for frequency in [0x8000_1000, u32::MAX] {
            assert!(std::panic::catch_unwind(|| state_machine_frequency(frequency)).is_err());
        }
    }
}
