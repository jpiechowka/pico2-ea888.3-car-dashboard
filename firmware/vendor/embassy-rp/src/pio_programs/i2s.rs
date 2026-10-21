//! PIO stereo I2S controller/receiver and controller/transmitter using DMA.

use crate::dma::Transfer;
use crate::gpio::Pull;
use crate::pio::{
    Common,
    Config,
    Direction,
    FifoJoin,
    Instance,
    LoadedProgram,
    PioBatch,
    PioPin,
    ShiftConfig,
    ShiftDirection,
    StateMachine,
};
use crate::pio_programs::clock_divider::calculate_pio_clock_divider;
use crate::{Peri, dma, interrupt};

/// This struct represents an I2S receiver & controller driver program
pub struct PioI2sInProgram<'d, PIO: Instance> {
    /// Relocated stereo shift/clock instructions.
    prg: LoadedProgram<'d, PIO>,
}

/// Input instruction cycles per serial bit.
const PIO_I2S_IN_PROGRAM_CLK_MULTIPLIER: u32 = 2;

/// Returns instruction Hz for `sample_rate` stereo frames/s, `bit_depth` bits and `channels`.
/// Panics unless rate is nonzero, depth is 16, channels is 2 and the product fits u32.
fn input_clock(
    sample_rate: u32,
    bit_depth: u32,
    channels: u32,
) -> u32 {
    assert!(sample_rate > 0);
    assert_eq!(bit_depth, 16, "the input PIO program samples 16 bits per channel");
    assert_eq!(channels, 2, "the input PIO program samples two channels");
    sample_rate
        .checked_mul(16 * 2 * PIO_I2S_IN_PROGRAM_CLK_MULTIPLIER)
        .expect("I2S clock overflow")
}

impl<'d, PIO: Instance> PioI2sInProgram<'d, PIO> {
    /// Loads fixed stereo 16-bit receive instructions through `common`; returns their allocation handle.
    /// Panics if instruction memory cannot fit the program.
    pub fn new(common: &mut Common<'d, PIO>) -> Self {
        let prg = pio::pio_asm! {
            ".side_set 2",
            "   set x, 14               side 0b01",
            "left_data:",
            "   in pins, 1              side 0b00", // read one left-channel bit from SD
            "   jmp x-- left_data       side 0b01",
            "   in pins, 1              side 0b10", // ws changes 1 clock before MSB
            "   set x, 14               side 0b11",
            "right_data:",
            "   in pins, 1             side 0b10",
            "   jmp x-- right_data     side 0b11",
            "   in pins, 1             side 0b00" // ws changes 1 clock before ms
        };
        let prg = common.load_program(&prg.program);
        Self { prg }
    }
}

/// Pio backed I2S input driver
pub struct PioI2sIn<'d, P: Instance, const S: usize> {
    /// Exclusively owned DMA channel; buffer borrows last through completion or abort.
    dma: dma::Channel<'d>,
    /// Configured stereo state machine with joined FIFO.
    sm: StateMachine<'d, P, S>,
}

impl<'d, P: Instance, const S: usize> PioI2sIn<'d, P, S> {
    /// Configures `sm` with `program`, registering `data_pin`, `bit_clock_pin` and `lr_clock_pin` through `common`.
    /// Returns a stereo receiver owning `dma`; `irq` proves its matching handler. `data_pulldown` enables
    /// the input pull-down. Clock pins must be consecutive in bit/LR order.
    /// `sample_rate` is frames/s, `bit_depth` must be 16 and `channels` must be 2. Invalid format, zero,
    /// overflow or unrepresentable divider panics. Does not explicitly change the supplied enable state.
    pub fn new<D: dma::ChannelInstance>(
        common: &mut Common<'d, P>,
        mut sm: StateMachine<'d, P, S>,
        dma: Peri<'d, D>,
        irq: impl interrupt::typelevel::Binding<D::Interrupt, dma::InterruptHandler<D>> + 'd,
        // Whether or not to use the MCU's internal pull-down resistor, as the
        // Pico 2 is known to have problems with the inbuilt pulldowns, many
        // opt to just use an external pull down resistor to meet requirements of common
        // I2S microphones such as the INMP441
        data_pulldown: bool,
        data_pin: Peri<'d, impl PioPin>,
        bit_clock_pin: Peri<'d, impl PioPin>,
        lr_clock_pin: Peri<'d, impl PioPin>,
        sample_rate: u32,
        bit_depth: u32,
        channels: u32,
        program: &PioI2sInProgram<'d, P>,
    ) -> Self {
        let divider = calculate_pio_clock_divider(input_clock(sample_rate, bit_depth, channels));
        let mut data_pin = common.make_pio_pin(data_pin);
        if data_pulldown {
            data_pin.set_pull(Pull::Down);
        }
        let bit_clock_pin = common.make_pio_pin(bit_clock_pin);
        let lr_clock_pin = common.make_pio_pin(lr_clock_pin);

        let cfg = {
            let mut cfg = Config::default();
            cfg.use_program(&program.prg, &[&bit_clock_pin, &lr_clock_pin]);
            cfg.set_in_pins(&[&data_pin]);
            cfg.clock_divider = divider;
            cfg.shift_in = ShiftConfig {
                threshold: 32,
                direction: ShiftDirection::Left,
                auto_fill: true,
            };
            // join fifos to have twice the time to start the next dma transfer
            cfg.fifo_join = FifoJoin::RxOnly; // both control signals are sent via side-setting
            cfg
        };
        sm.set_config(&cfg);
        sm.set_pin_dirs(Direction::In, &[&data_pin]);
        sm.set_pin_dirs(Direction::Out, &[&lr_clock_pin, &bit_clock_pin]);

        Self {
            dma: dma::Channel::new(dma, irq),
            sm,
        }
    }

    /// Enables instructions at the current phase; FIFO starvation can stall the generated clocks.
    pub fn start(&mut self) { self.sm.set_enable(true); }

    /// Pauses instructions while retaining FIFO contents, phase and current pin levels.
    pub fn stop(&mut self) { self.sm.set_enable(false); }

    /// Records enable in `b`; hardware changes when the batch executes.
    pub fn start_batched(
        &mut self,
        b: &mut PioBatch<'d, P>,
    ) {
        b.set_enable(&mut self.sm, true);
    }

    /// Records disable in `b`; hardware changes when the batch executes.
    pub fn stop_batched(
        &mut self,
        b: &mut PioBatch<'d, P>,
    ) {
        b.set_enable(&mut self.sm, false);
    }

    /// Starts DMA into `buff` and returns its borrowed transfer. Each word packs left then right 16-bit samples.
    /// Completion fills the buffer; dropping aborts DMA with a possible written prefix. FIFO sampling continues
    /// until full, so restarting reads does not promise gap-free audio. No timeout is imposed.
    pub fn read<'b>(
        &'b mut self,
        buff: &'b mut [u32],
    ) -> Transfer<'b> {
        self.sm.rx().dma_pull(&mut self.dma, buff, false)
    }
}

#[cfg(test)]
/// Fixed receive-format and input-clock arithmetic admission tests.
mod tests {
    extern crate std;
    use super::*;

    /// Accepts stereo 16-bit frames and rejects zero rate or unsupported format.
    #[test]
    fn input_program_requires_its_fixed_stereo_sixteen_bit_format() {
        assert_eq!(input_clock(48_000, 16, 2), 3_072_000);
        for (rate, depth, channels) in [(0, 16, 2), (48_000, 24, 2), (48_000, 16, 1)] {
            assert!(std::panic::catch_unwind(|| input_clock(rate, depth, channels)).is_err());
        }
    }
}

/// This struct represents an I2S output driver program
///
/// The sample bit-depth is set through scratch register `Y`.
/// `Y` has to be set to sample bit-depth - 2.
/// (14 = 16bit, 22 = 24bit, 30 = 32bit)
pub struct PioI2sOutProgram<'d, PIO: Instance> {
    /// Relocated stereo shift/clock instructions.
    prg: LoadedProgram<'d, PIO>,
}

/// Output instruction cycles per serial bit.
const PIO_I2S_OUT_PROGRAM_CLK_MULTIPLIER: u32 = 2;

impl<'d, PIO: Instance> PioI2sOutProgram<'d, PIO> {
    /// Loads variable-width stereo transmit instructions through `common`; returns their allocation handle.
    /// Panics if instruction memory cannot fit the program.
    pub fn new(common: &mut Common<'d, PIO>) -> Self {
        let prg = pio::pio_asm!(
            ".side_set 2",                      // side 0bWB - W = Word Clock, B = Bit Clock
            "    mov x, y           side 0b01", // y stores sample depth - 2 (14 = 16bit, 22 = 24bit, 30 = 32bit)
            "left_data:",
            "    out pins, 1        side 0b00",
            "    jmp x-- left_data  side 0b01",
            "    out pins, 1        side 0b10",
            "    mov x, y           side 0b11",
            "right_data:",
            "    out pins, 1         side 0b10",
            "    jmp x-- right_data side 0b11",
            "    out pins, 1         side 0b00",
        );

        let prg = common.load_program(&prg.program);

        Self { prg }
    }
}

/// Pio backed I2S output driver
pub struct PioI2sOut<'d, P: Instance, const S: usize> {
    /// Exclusively owned DMA channel; buffer borrows last through completion or abort.
    dma: dma::Channel<'d>,
    /// Configured stereo state machine with joined FIFO.
    sm: StateMachine<'d, P, S>,
}

impl<'d, P: Instance, const S: usize> PioI2sOut<'d, P, S> {
    /// Configures `sm` with `program`, registering output `data_pin` and clock pins through `common`.
    /// Returns a stereo transmitter owning `dma`; `irq` proves its handler. `bit_clock_pin`/`lr_clock_pin`
    /// must be consecutive. `sample_rate` is nonzero stereo frames/s and `bit_depth` is 2..=32.
    /// Invalid rates, multiplication overflow or unrepresentable divider panic. Does not explicitly enable.
    ///
    /// DMA words contain a continuous, MSB-first packed bitstream. Samples whose
    /// width does not divide 32 span words; they are not padded to 32 bits.
    pub fn new<D: dma::ChannelInstance>(
        common: &mut Common<'d, P>,
        mut sm: StateMachine<'d, P, S>,
        dma: Peri<'d, D>,
        irq: impl interrupt::typelevel::Binding<D::Interrupt, dma::InterruptHandler<D>> + 'd,
        data_pin: Peri<'d, impl PioPin>,
        bit_clock_pin: Peri<'d, impl PioPin>,
        lr_clock_pin: Peri<'d, impl PioPin>,
        sample_rate: u32,
        bit_depth: u32,
        program: &PioI2sOutProgram<'d, P>,
    ) -> Self {
        assert!((2..=32).contains(&bit_depth));
        assert!(sample_rate > 0);
        let pio_frequency = sample_rate
            .checked_mul(bit_depth * 2 * PIO_I2S_OUT_PROGRAM_CLK_MULTIPLIER)
            .expect("I2S clock overflow");
        let divider = calculate_pio_clock_divider(pio_frequency);
        let data_pin = common.make_pio_pin(data_pin);
        let bit_clock_pin = common.make_pio_pin(bit_clock_pin);
        let lr_clock_pin = common.make_pio_pin(lr_clock_pin);

        let cfg = {
            let mut cfg = Config::default();
            cfg.use_program(&program.prg, &[&bit_clock_pin, &lr_clock_pin]);
            cfg.set_out_pins(&[&data_pin]);
            cfg.clock_divider = divider;
            cfg.shift_out = ShiftConfig {
                threshold: 32,
                direction: ShiftDirection::Left,
                auto_fill: true,
            };
            // join fifos to have twice the time to start the next dma transfer
            cfg.fifo_join = FifoJoin::TxOnly;
            cfg
        };
        sm.set_config(&cfg);
        sm.set_pin_dirs(Direction::Out, &[&data_pin, &lr_clock_pin, &bit_clock_pin]);

        // Set the `y` register up to configure the sample depth
        // The SM counts down to 0 and uses one clock cycle to set up the counter,
        // which results in bit_depth - 2 as register value.
        unsafe { sm.set_y(bit_depth - 2) };

        Self {
            dma: dma::Channel::new(dma, irq),
            sm,
        }
    }

    /// Enables instructions at the current phase; FIFO starvation can stall the generated clocks.
    pub fn start(&mut self) { self.sm.set_enable(true); }

    /// Pauses instructions while retaining FIFO contents, phase and current pin levels.
    pub fn stop(&mut self) { self.sm.set_enable(false); }

    /// Records enable in `b`; hardware changes when the batch executes.
    pub fn start_batched(
        &mut self,
        b: &mut PioBatch<'d, P>,
    ) {
        b.set_enable(&mut self.sm, true);
    }

    /// Records disable in `b`; hardware changes when the batch executes.
    pub fn stop_batched(
        &mut self,
        b: &mut PioBatch<'d, P>,
    ) {
        b.set_enable(&mut self.sm, false);
    }

    /// Starts DMA from packed MSB-first `buff` and returns its borrowed transfer. Completion means FIFO delivery,
    /// not wire drain. Dropping aborts DMA but queued words continue; starvation can stall the clock.
    pub fn write<'b>(
        &'b mut self,
        buff: &'b [u32],
    ) -> Transfer<'b> {
        self.sm.tx().dma_push(&mut self.dma, buff, false)
    }
}
