//! Pio Stepper Driver for 5-wire steppers

use crate::Peri;
use crate::pio::{Common, Config, Direction, Instance, Irq, LoadedProgram, PioPin, StateMachine};
use crate::pio_programs::clock_divider::calculate_pio_clock_divider;

/// This struct represents a Stepper driver program loaded into pio instruction memory.
pub struct PioStepperProgram<'a, PIO: Instance> {
    /// Relocated count/pattern loop ending in a relative completion IRQ.
    prg: LoadedProgram<'a, PIO>,
}

impl<'a, PIO: Instance> PioStepperProgram<'a, PIO> {
    /// Loads the step-count loop through `common`; returns its allocation handle.
    /// Panics if instruction memory cannot fit the program.
    pub fn new(common: &mut Common<'a, PIO>) -> Self {
        let prg = common.load_program(&stepper_program());
        Self { prg }
    }
}

/// Returns the assembled count/pattern loop; zero count emits no pin writes but still raises completion.
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
    )
    .program
}

/// Pio backed Stepper driver
pub struct PioStepper<'d, T: Instance, const SM: usize> {
    /// Relative IRQ matching the state-machine number.
    irq: Irq<'d, T, SM>,
    /// Enabled four-pin pattern sequencer.
    sm: StateMachine<'d, T, SM>,
    /// Loaded entry address for cancellation recovery.
    origin: u8,
}

impl<'d, T: Instance, const SM: usize> PioStepper<'d, T, SM> {
    /// Registers consecutive outputs `pin0`, `pin1`, `pin2`, `pin3` through `pio` and starts `sm` with `program`.
    /// Returns the driver owning completion `irq`; initial instruction rate is 13600 Hz.
    /// Panics for nonconsecutive pins or unrepresentable divider. External coil-drive circuitry is required.
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
        Self {
            irq,
            sm,
            origin: program.prg.origin,
        }
    }

    /// Sets instruction Hz to `freq` times 136 and restarts divider phase.
    /// Panics on zero, multiplication overflow or unrepresentable divider. Actual per-step timing includes
    /// the 32-cycle OUT delay plus branches; `freq` is a scaling parameter, not a measured step rate.
    pub fn set_frequency(
        &mut self,
        freq: u32,
    ) {
        let clock_divider = calculate_pio_clock_divider(freq.checked_mul(136).expect("stepper clock overflow"));
        let divider_f32 = clock_divider.to_num::<f32>();
        assert!(divider_f32 <= 65536.0, "clkdiv must be <= 65536");
        assert!(divider_f32 >= 1.0, "clkdiv must be >= 1");

        self.sm.set_clock_divider(clock_divider);
        self.sm.clkdiv_restart();
    }

    /// Emits abs(`steps`) one-phase full steps; the sign selects pattern direction.
    /// Zero emits none; i32::MIN magnitude is supported. Cancellation resets the sequencer/IRQ but retains coil level.
    pub async fn step(
        &mut self,
        steps: i32,
    ) {
        if steps > 0 {
            self.run(steps.unsigned_abs(), 0b1000_0100_0010_0001_1000_0100_0010_0001)
                .await
        } else {
            self.run(steps.unsigned_abs(), 0b0001_0010_0100_1000_0001_0010_0100_1000)
                .await
        }
    }

    /// Emits abs(`steps`) two-phase full steps; the sign selects pattern direction.
    /// Zero emits none; cancellation resets sequencing/IRQ but does not de-energize the coils.
    pub async fn step2(
        &mut self,
        steps: i32,
    ) {
        if steps > 0 {
            self.run(steps.unsigned_abs(), 0b1001_1100_0110_0011_1001_1100_0110_0011)
                .await
        } else {
            self.run(steps.unsigned_abs(), 0b0011_0110_1100_1001_0011_0110_1100_1001)
                .await
        }
    }

    /// Emits abs(`steps`) half steps using alternating one/two-phase drive; sign selects direction.
    /// Zero emits none; cancellation resets sequencing/IRQ but does not undo physical movement.
    pub async fn step_half(
        &mut self,
        steps: i32,
    ) {
        if steps > 0 {
            self.run(steps.unsigned_abs(), 0b1001_1000_1100_0100_0110_0010_0011_0001)
                .await
        } else {
            self.run(steps.unsigned_abs(), 0b0001_0011_0010_0110_0100_1100_1000_1001)
                .await
        }
    }

    /// Queues `steps` and packed eight-nibble `pattern`, then waits indefinitely for completion IRQ.
    /// Cancellation resets FIFO/program/IRQ; previously emitted levels and movement cannot be rolled back.
    async fn run(
        &mut self,
        steps: u32,
        pattern: u32,
    ) {
        let mut guard = RunGuard {
            sm: &mut self.sm,
            irq: &mut self.irq,
            origin: self.origin,
            active: true,
        };
        guard.sm.tx().wait_push(steps).await;
        guard.sm.tx().wait_push(pattern).await;
        guard.irq.wait().await;
        guard.active = false;
    }
}

/// Cancellation cleanup for a partially queued or executing movement command.
struct RunGuard<'a, 'd, T: Instance, const SM: usize> {
    /// Borrowed sequencer to reset after cancellation.
    sm: &'a mut StateMachine<'d, T, SM>,
    /// Borrowed completion flag to clear after cancellation.
    irq: &'a mut Irq<'d, T, SM>,
    /// Loaded entry address for cancellation recovery.
    origin: u8,
    /// True until the command completion IRQ has been consumed.
    active: bool,
}

impl<T: Instance, const SM: usize> Drop for RunGuard<'_, '_, T, SM> {
    /// Resets incomplete sequencing and stale completion, then resumes waiting for a new command.
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
/// Assembled instruction-flow count regression; no physical motor behavior is simulated.
mod tests {
    use pio::{InstructionOperands as Op, JmpCondition as Jump, MovDestination as Dest};

    use super::*;

    /// Returns pin writes emitted by the assembled program for `count`, modeling test-before-decrement JMP.
    /// Panics for unexpected branches/decoding or if completion exceeds the 1000-instruction test budget.
    fn emitted_steps(count: u32) -> u32 {
        let code = stepper_program().code;
        let mut pc = 0;
        let mut x = 0u32;
        let mut writes = 0;
        for _ in 0..1000 {
            match Op::decode(code[pc]).unwrap() {
                Op::MOV {
                    destination: Dest::X, ..
                } => x = count,
                Op::OUT {
                    destination: pio::OutDestination::PINS,
                    ..
                } => writes += 1,
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

    /// Confirms zero and positive requested counts emit exactly that many pin writes.
    #[test]
    fn actual_program_emits_exact_requested_steps_including_zero() {
        for count in [0, 1, 2, 8, 31, 64] {
            assert_eq!(emitted_steps(count), count);
        }
    }
}
