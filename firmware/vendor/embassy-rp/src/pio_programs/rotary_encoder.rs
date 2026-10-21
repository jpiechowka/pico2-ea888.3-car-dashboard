//! PIO backed quadrature encoder

use crate::Peri;
use crate::gpio::Pull;
use crate::pio::{
    Common,
    Config,
    Direction as PioDirection,
    FifoJoin,
    Instance,
    LoadedProgram,
    PioPin,
    ShiftDirection,
    StateMachine,
};
use crate::pio_programs::clock_divider::calculate_pio_clock_divider;

/// This struct represents an Encoder program loaded into pio instruction memory.
pub struct PioEncoderProgram<'a, PIO: Instance> {
    /// Relocated wait/sample/push instructions.
    prg: LoadedProgram<'a, PIO>,
}

impl<'a, PIO: Instance> PioEncoderProgram<'a, PIO> {
    /// Loads the encoder instructions through `common`; returns their allocation handle.
    /// Panics if instruction memory cannot fit the program.
    pub fn new(common: &mut Common<'a, PIO>) -> Self {
        let prg = pio::pio_asm!("wait 1 pin 1", "wait 0 pin 1", "in pins, 2", "push",);

        let prg = common.load_program(&prg.program);

        Self { prg }
    }
}

/// Pio Backed quadrature encoder reader
pub struct PioEncoder<'d, T: Instance, const SM: usize> {
    /// Enabled sampler with an eight-word joined receive FIFO.
    sm: StateMachine<'d, T, SM>,
}

impl<'d, T: Instance, const SM: usize> PioEncoder<'d, T, SM> {
    /// Registers consecutive `pin_a`/`pin_b` through `pio` with pull-ups and starts `sm` using `program`.
    /// Returns a reader sampling after pin B falls, with a 12.5 kHz instruction clock.
    /// Panics for nonconsecutive pins or an unrepresentable divider; no mechanical debounce is provided.
    pub fn new(
        pio: &mut Common<'d, T>,
        mut sm: StateMachine<'d, T, SM>,
        pin_a: Peri<'d, impl PioPin>,
        pin_b: Peri<'d, impl PioPin>,
        program: &PioEncoderProgram<'d, T>,
    ) -> Self {
        let mut pin_a = pio.make_pio_pin(pin_a);
        let mut pin_b = pio.make_pio_pin(pin_b);
        pin_a.set_pull(Pull::Up);
        pin_b.set_pull(Pull::Up);
        sm.set_pin_dirs(PioDirection::In, &[&pin_a, &pin_b]);

        let mut cfg = Config::default();
        cfg.set_in_pins(&[&pin_a, &pin_b]);
        cfg.fifo_join = FifoJoin::RxOnly;
        cfg.shift_in.direction = ShiftDirection::Left;

        // Target 12.5 KHz PIO clock
        cfg.clock_divider = calculate_pio_clock_divider(12_500);

        cfg.use_program(&program.prg, &[]);
        sm.set_config(&cfg);
        sm.set_enable(true);
        Self { sm }
    }

    /// Returns one direction decoded from a queued A/B sample (0 counterclockwise, 1 clockwise).
    /// Discards other samples and waits indefinitely for a valid one. Cancellation removes the FIFO IRQ
    /// waiter but leaves the sampler running; previously discarded samples are not restored.
    pub async fn read(&mut self) -> Direction {
        loop {
            match self.sm.rx().wait_pull().await {
                0 => return Direction::CounterClockwise,
                1 => return Direction::Clockwise,
                _ => {}
            }
        }
    }
}

/// Encoder Count Direction
pub enum Direction {
    /// Encoder turned clockwise
    Clockwise,
    /// Encoder turned counter clockwise
    CounterClockwise,
}
