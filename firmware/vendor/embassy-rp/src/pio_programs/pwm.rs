//! PIO backed PWM driver

use core::time::Duration;

use pio::InstructionOperands;

use crate::gpio::Level;
use crate::pio::{Common, Config, Direction, Instance, LoadedProgram, Pin, PioPin, StateMachine};
use crate::{Peri, clocks};

/// Returns the three-instruction loop count for `duration` at the current system clock.
/// Truncates fractional loops; excludes fixed setup overhead. Panics if the count exceeds u32.
fn to_pio_cycles(duration: Duration) -> u32 { duration_cycles(duration, clocks::clk_sys_freq()) }

/// Returns floor(`duration` nanoseconds times `clock_hz` / 3e9).
/// Panics for zero clock or a count above u32; zero duration returns zero.
fn duration_cycles(
    duration: Duration,
    clock_hz: u32,
) -> u32 {
    assert!(clock_hz > 0);
    // The loop costs three instructions. Fixed setup instructions are additional.
    u32::try_from(duration.as_nanos() * u128::from(clock_hz) / 3_000_000_000)
        .expect("PWM duration exceeds the PIO counter")
}

/// This struct represents a PWM program loaded into pio instruction memory.
pub struct PioPwmProgram<'a, PIO: Instance> {
    /// Relocated comparator/countdown PWM instructions.
    prg: LoadedProgram<'a, PIO>,
}

#[cfg(test)]
/// Pure duration conversion boundaries; no waveform timing is measured.
mod tests {
    extern crate std;
    use super::*;

    /// Retains sub-microsecond and fractional-clock precision, including zero.
    #[test]
    fn pwm_duration_retains_sub_microsecond_and_fractional_clock_precision() {
        assert_eq!(duration_cycles(Duration::from_nanos(500), 150_000_000), 25);
        assert_eq!(duration_cycles(Duration::from_micros(3), 125_000_000), 125);
        assert_eq!(duration_cycles(Duration::ZERO, 150_000_000), 0);
    }

    /// Rejects duration overflow before the u32 counter conversion.
    #[test]
    fn excessive_pwm_duration_is_rejected_before_narrowing() {
        assert!(std::panic::catch_unwind(|| duration_cycles(Duration::MAX, 150_000_000)).is_err());
    }
}

impl<'a, PIO: Instance> PioPwmProgram<'a, PIO> {
    /// Loads PWM instructions through `common`; returns their allocation handle.
    /// Panics if instruction memory cannot fit the program.
    pub fn new(common: &mut Common<'a, PIO>) -> Self {
        let prg = pio::pio_asm!(
            ".side_set 1 opt"
                "pull noblock    side 0"
                "mov x, osr"
                "mov y, isr"
            "countloop:"
                "jmp x!=y noset"
                "jmp skip        side 1"
            "noset:"
                "nop"
            "skip:"
                "jmp y-- countloop"
        );

        let prg = common.load_program(&prg.program);

        Self { prg }
    }
}

/// Pio backed PWM output
pub struct PioPwm<'d, T: Instance, const SM: usize> {
    /// Exclusively owned counter/comparator state machine.
    sm: StateMachine<'d, T, SM>,
    /// Registered output retained for release.
    pin: Pin<'d, T>,
}

impl<'d, T: Instance, const SM: usize> PioPwm<'d, T, SM> {
    /// Configures `sm` with `program`, registering output `pin` through `pio`.
    /// Returns the driver with initial pin high; does not explicitly change the supplied enable state.
    /// System clock changes alter period and level timing.
    pub fn new(
        pio: &mut Common<'d, T>,
        mut sm: StateMachine<'d, T, SM>,
        pin: Peri<'d, impl PioPin>,
        program: &PioPwmProgram<'d, T>,
    ) -> Self {
        let pin = pio.make_pio_pin(pin);
        sm.set_pins(Level::High, &[&pin]);
        sm.set_pin_dirs(Direction::Out, &[&pin]);

        let mut cfg = Config::default();
        cfg.use_program(&program.prg, &[&pin]);

        sm.set_config(&cfg);

        Self { sm, pin }
    }

    /// Enables the PIO program, continuing the wave generation from the PIO program.
    pub fn start(&mut self) { self.sm.set_enable(true); }

    /// Pauses instructions; the pin retains its last level and the machine retains its phase.
    pub fn stop(&mut self) { self.sm.set_enable(false); }

    /// Sets the countdown from `duration`, truncating to three-instruction loops.
    /// Pauses the machine, drains queued levels into X, writes ISR, then restores enable state.
    /// Fixed instruction overhead is additional; zero is accepted. Panics for excessive duration.
    pub fn set_period(
        &mut self,
        duration: Duration,
    ) {
        let cycles = to_pio_cycles(duration);
        let is_enabled = self.sm.is_enabled();
        self.sm.set_enable(false);
        self.drain_levels();
        self.sm.tx().push(cycles);
        unsafe {
            self.sm.exec_instr(
                InstructionOperands::PULL {
                    if_empty: false,
                    block: false,
                }
                .encode(),
            );
            self.sm.exec_instr(
                InstructionOperands::OUT {
                    destination: ::pio::OutDestination::ISR,
                    bit_count: 32,
                }
                .encode(),
            );
        };
        if is_enabled {
            self.sm.set_enable(true) // Enable if previously enabled
        }
    }

    /// Queues comparator `level` in three-instruction loop units, without checking it against period.
    /// High is asserted when the descending period counter equals `level`; out-of-range values never match.
    /// A full queue is coalesced to its newest level without waiting for a stopped SM.
    pub fn set_level(
        &mut self,
        level: u32,
    ) {
        if self.sm.tx().full() {
            let enabled = self.sm.is_enabled();
            self.sm.set_enable(false);
            self.drain_levels();
            self.sm.tx().push(level);
            self.sm.set_enable(enabled);
            return;
        }
        self.sm.tx().push(level);
    }

    /// Drains at most four queued levels into X while the caller keeps the machine disabled.
    /// The final X becomes the fallback when the program executes PULL NOBLOCK on an empty FIFO.
    fn drain_levels(&mut self) {
        while !self.sm.tx().empty() {
            unsafe {
                self.sm.exec_instr(
                    InstructionOperands::PULL {
                        if_empty: false,
                        block: false,
                    }
                    .encode(),
                );
                self.sm.exec_instr(
                    InstructionOperands::MOV {
                        destination: pio::MovDestination::X,
                        op: pio::MovOperation::None,
                        source: pio::MovSource::OSR,
                    }
                    .encode(),
                );
            }
        }
    }

    /// Converts `duration` to loop units and queues it as the comparator level.
    /// Truncates fractional loops; panics above u32 capacity and does not clamp to period.
    pub fn write(
        &mut self,
        duration: Duration,
    ) {
        self.set_level(to_pio_cycles(duration));
    }

    /// Returns the state machine and registered pin without stopping the current waveform.
    pub fn release(self) -> (StateMachine<'d, T, SM>, Pin<'d, T>) { (self.sm, self.pin) }
}
