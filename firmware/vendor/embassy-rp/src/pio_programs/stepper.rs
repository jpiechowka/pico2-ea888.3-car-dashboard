//! Pio Stepper Driver for 5-wire steppers

use crate::Peri;
use crate::pio::{Common, Config, Direction, Instance, Irq, LoadedProgram, PioPin, StateMachine};
use crate::pio_programs::clock_divider::calculate_pio_clock_divider;

/// This struct represents a Stepper driver program loaded into pio instruction memory.
pub struct PioStepperProgram<'a, PIO: Instance> {
    prg: LoadedProgram<'a, PIO>,
}

impl<'a, PIO: Instance> PioStepperProgram<'a, PIO> {
    /// Load the program into the given pio
    pub fn new(common: &mut Common<'a, PIO>) -> Self {
        let prg = common.load_program(&stepper_program());
        Self { prg }
    }
}

fn stepper_program() -> pio::Program<32> {
    pio::pio_asm!(
            "pull block",
            "mov x, osr",
            "pull block",
            "mov y, osr",
            "jmp !x end",
            "jmp x-- loop", // Convert the nonzero count to the loop's inclusive counter.
            "loop:",
            "jmp !osre step",
            "mov osr, y",
            "step:",
            "out pins, 4 [31]"
            "jmp x-- loop",
            "end:",
            "irq 0 rel"
    ).program
}

/// Pio backed Stepper driver
pub struct PioStepper<'d, T: Instance, const SM: usize> {
    irq: Irq<'d, T, SM>,
    sm: StateMachine<'d, T, SM>,
    origin: u8,
}

impl<'d, T: Instance, const SM: usize> PioStepper<'d, T, SM> {
    /// Configure a state machine to drive a stepper
    pub fn new(
        pio: &mut Common<'d, T>,
        mut sm: StateMachine<'d, T, SM>,
        irq: Irq<'d, T, SM>,
        pin0: Peri<'d, impl PioPin>,
        pin1: Peri<'d, impl PioPin>,
        pin2: Peri<'d, impl PioPin>,
        pin3: Peri<'d, impl PioPin>,
        program: &PioStepperProgram<'d, T>,
    ) -> Self {
        let pin0 = pio.make_pio_pin(pin0);
        let pin1 = pio.make_pio_pin(pin1);
        let pin2 = pio.make_pio_pin(pin2);
        let pin3 = pio.make_pio_pin(pin3);
        sm.set_pin_dirs(Direction::Out, &[&pin0, &pin1, &pin2, &pin3]);
        let mut cfg = Config::default();
        cfg.set_out_pins(&[&pin0, &pin1, &pin2, &pin3]);

        cfg.clock_divider = calculate_pio_clock_divider(100 * 136);

        cfg.use_program(&program.prg, &[]);
        sm.set_config(&cfg);
        sm.set_enable(true);
        Self { irq, sm, origin: program.prg.origin }
    }

    /// Set pulse frequency
    pub fn set_frequency(&mut self, freq: u32) {
        let clock_divider = calculate_pio_clock_divider(freq.checked_mul(136).expect("stepper clock overflow"));
        let divider_f32 = clock_divider.to_num::<f32>();
        assert!(divider_f32 <= 65536.0, "clkdiv must be <= 65536");
        assert!(divider_f32 >= 1.0, "clkdiv must be >= 1");

        self.sm.set_clock_divider(clock_divider);
        self.sm.clkdiv_restart();
    }

    /// Full step, one phase
    pub async fn step(&mut self, steps: i32) {
        if steps > 0 {
            self.run(steps.unsigned_abs(), 0b1000_0100_0010_0001_1000_0100_0010_0001).await
        } else {
            self.run(steps.unsigned_abs(), 0b0001_0010_0100_1000_0001_0010_0100_1000).await
        }
    }

    /// Full step, two phase
    pub async fn step2(&mut self, steps: i32) {
        if steps > 0 {
            self.run(steps.unsigned_abs(), 0b1001_1100_0110_0011_1001_1100_0110_0011).await
        } else {
            self.run(steps.unsigned_abs(), 0b0011_0110_1100_1001_0011_0110_1100_1001).await
        }
    }

    /// Half step
    pub async fn step_half(&mut self, steps: i32) {
        if steps > 0 {
            self.run(steps.unsigned_abs(), 0b1001_1000_1100_0100_0110_0010_0011_0001).await
        } else {
            self.run(steps.unsigned_abs(), 0b0001_0011_0010_0110_0100_1100_1000_1001).await
        }
    }

    async fn run(&mut self, steps: u32, pattern: u32) {
        let mut guard = RunGuard { sm: &mut self.sm, irq: &mut self.irq, origin: self.origin, active: true };
        guard.sm.tx().wait_push(steps).await;
        guard.sm.tx().wait_push(pattern).await;
        guard.irq.wait().await;
        guard.active = false;
    }
}

struct RunGuard<'a, 'd, T: Instance, const SM: usize> {
    sm: &'a mut StateMachine<'d, T, SM>,
    irq: &'a mut Irq<'d, T, SM>,
    origin: u8,
    active: bool,
}

impl<T: Instance, const SM: usize> Drop for RunGuard<'_, '_, T, SM> {
    fn drop(&mut self) {
        if self.active {
            self.sm.set_enable(false);
            self.sm.clear_fifos();
            self.sm.restart();
            unsafe { self.sm.exec_jmp(self.origin) };
            // A canceled previous command must not complete the next one.
            self.irq.clear();
            self.sm.set_enable(true);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pio::{InstructionOperands as Op, JmpCondition as Jump, MovDestination as Dest};

    // Execute the actual assembled control flow through IRQ, counting pin writes.
    // This intentionally models JMP X-- as a test-before-decrement operation.
    fn emitted_steps(count: u32) -> u32 {
        let code = stepper_program().code;
        let mut pc = 0;
        let mut x = 0u32;
        let mut writes = 0;
        for _ in 0..1000 {
            match Op::decode(code[pc]).unwrap() {
                Op::MOV { destination: Dest::X, .. } => x = count,
                Op::OUT { destination: pio::OutDestination::PINS, .. } => writes += 1,
                Op::JMP { condition, address } => {
                    let jump = match condition {
                        Jump::XIsZero => x == 0,
                        Jump::XDecNonZero => {
                            let previous = x;
                            x = x.wrapping_sub(1);
                            previous != 0
                        }
                        Jump::OutputShiftRegisterNotEmpty => true,
                        Jump::Always => true,
                        _ => panic!("unexpected stepper branch"),
                    };
                    if jump {
                        pc = address as usize;
                        continue;
                    }
                }
                Op::IRQ { .. } => return writes,
                _ => {}
            }
            pc += 1;
        }
        panic!("stepper program did not terminate");
    }

    #[test]
    fn actual_program_emits_exact_requested_steps_including_zero() {
        for count in [0, 1, 2, 8, 31, 64] {
            assert_eq!(emitted_steps(count), count);
        }
    }
}
