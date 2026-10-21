//! PIO/DMA RGB and RGBW streams at nominal 800 kbit/s with a 300 microsecond reset wait.
//! Instruction timing follows the [Raspberry Pi PIO example](https://github.com/raspberrypi/pico-examples/blob/master/pio/ws2812/ws2812.pio); verify timing against the actual LED revision.

use embassy_time::Timer;
use smart_leds::{RGB8, RGBW};

use crate::pio::{
    Common,
    Config,
    FifoJoin,
    Instance,
    LoadedProgram,
    PioPin,
    ShiftConfig,
    ShiftDirection,
    StateMachine,
};
use crate::pio_programs::clock_divider::calculate_pio_clock_divider;
use crate::pio_programs::wait_tx_idle;
use crate::{Peri, dma, interrupt};

/// High prefix in PIO cycles for either bit value.
const T1: u8 = 2; // start bit
/// Data-dependent high/low extension in PIO cycles.
const T2: u8 = 5; // data bit
/// Low prefix before the next bit, in PIO cycles.
const T3: u8 = 3; // stop bit
/// Total instruction cycles per transmitted bit.
const CYCLES_PER_BIT: u32 = (T1 + T2 + T3) as u32;

/// Color orders for WS2812B, type RGB8
pub trait RgbColorOrder {
    /// Returns `color` packed into the upper 24 bits in the implementation-specific byte order.
    /// PIO shifts most-significant bits first; the low byte is unused.
    fn pack(color: RGB8) -> u32;
}

/// Green, Red, Blue order is the common default for WS2812B
pub struct Grb;
impl RgbColorOrder for Grb {
    /// Returns `color` as G/R/B bytes in bits 31..8; low byte is zero.
    fn pack(color: RGB8) -> u32 { (u32::from(color.g) << 24) | (u32::from(color.r) << 16) | (u32::from(color.b) << 8) }
}

/// Red, Green, Blue is used by some WS2812B implementations
pub struct Rgb;
impl RgbColorOrder for Rgb {
    /// Returns `color` as R/G/B bytes in bits 31..8; low byte is zero.
    fn pack(color: RGB8) -> u32 { (u32::from(color.r) << 24) | (u32::from(color.g) << 16) | (u32::from(color.b) << 8) }
}

/// Color orders RGBW strips
pub trait RgbwColorOrder {
    /// Returns all four channels of `color` packed in implementation-specific order, MSB first.
    fn pack(color: RGBW<u8>) -> u32;
}

/// Green, Red, Blue, White order is the common default for RGBW strips
pub struct Grbw;
impl RgbwColorOrder for Grbw {
    /// Returns `color` as G/R/B/W bytes from most to least significant.
    fn pack(color: RGBW<u8>) -> u32 {
        (u32::from(color.g) << 24) | (u32::from(color.r) << 16) | (u32::from(color.b) << 8) | u32::from(color.a.0)
    }
}

/// Red, Green, Blue, White order
pub struct Rgbw;
impl RgbwColorOrder for Rgbw {
    /// Returns `color` as R/G/B/W bytes from most to least significant.
    fn pack(color: RGBW<u8>) -> u32 {
        (u32::from(color.r) << 24) | (u32::from(color.g) << 16) | (u32::from(color.b) << 8) | u32::from(color.a.0)
    }
}

/// This struct represents a ws2812 program loaded into pio instruction memory.
pub struct PioWs2812Program<'a, PIO: Instance> {
    /// Relocated side-set bitstream program.
    prg: LoadedProgram<'a, PIO>,
}

impl<'a, PIO: Instance> PioWs2812Program<'a, PIO> {
    /// Loads the LED bitstream instructions through `common`; returns their allocation handle.
    /// Panics if instruction memory cannot fit the program.
    pub fn new(common: &mut Common<'a, PIO>) -> Self {
        let side_set = pio::SideSet::new(false, 1, false);
        let mut a: pio::Assembler<32> = pio::Assembler::new_with_side_set(side_set);

        let mut wrap_target = a.label();
        let mut wrap_source = a.label();
        let mut do_zero = a.label();
        a.set_with_side_set(pio::SetDestination::PINDIRS, 1, 0);
        a.bind(&mut wrap_target);
        // Do stop bit
        a.out_with_delay_and_side_set(pio::OutDestination::X, 1, T3 - 1, 0);
        // Do start bit
        a.jmp_with_delay_and_side_set(pio::JmpCondition::XIsZero, &mut do_zero, T1 - 1, 1);
        // Do data bit = 1
        a.jmp_with_delay_and_side_set(pio::JmpCondition::Always, &mut wrap_target, T2 - 1, 1);
        a.bind(&mut do_zero);
        // Do data bit = 0
        a.nop_with_delay_and_side_set(T2 - 1, 0);
        a.bind(&mut wrap_source);

        let prg = a.assemble_with_wrap(wrap_source, wrap_target);
        let prg = common.load_program(&prg);

        Self { prg }
    }
}

/// Pio backed RGB ws2812 driver
/// Const N is the number of ws2812 leds attached to this pin
pub struct PioWs2812<'d, P: Instance, const S: usize, const N: usize, ORDER>
where
    ORDER: RgbColorOrder,
{
    /// Exclusively owned DMA channel feeding the transmit FIFO.
    dma: dma::Channel<'d>,
    /// Enabled transmitter with an eight-word joined FIFO.
    sm: StateMachine<'d, P, S>,
    /// Compile-time channel packing policy; no runtime storage.
    _order: core::marker::PhantomData<ORDER>,
}

impl<'d, P: Instance, const S: usize, const N: usize> PioWs2812<'d, P, S, N, Grb> {
    /// Returns a GRB transmitter using `program` on `sm`, with output `pin` registered through `pio`.
    /// Owns `dma`; `irq` proves its matching DMA interrupt handler. Starts the machine immediately.
    /// Panics if the system clock cannot represent the required 8 MHz instruction rate.
    pub fn new<D: dma::ChannelInstance>(
        pio: &mut Common<'d, P>,
        sm: StateMachine<'d, P, S>,
        dma: Peri<'d, D>,
        irq: impl interrupt::typelevel::Binding<D::Interrupt, dma::InterruptHandler<D>> + 'd,
        pin: Peri<'d, impl PioPin>,
        program: &PioWs2812Program<'d, P>,
    ) -> Self {
        Self::with_color_order(pio, sm, dma, irq, pin, program)
    }
}

impl<'d, P: Instance, const S: usize, const N: usize, ORDER> PioWs2812<'d, P, S, N, ORDER>
where
    ORDER: RgbColorOrder,
{
    /// Returns an ORDER-packed RGB transmitter using `program` on `sm` and output `pin` through `pio`.
    /// Owns `dma`; `irq` proves its matching interrupt handler. Starts at nominal 800 kbit/s.
    /// Panics for an unrepresentable 8 MHz instruction rate; system clock must stay unchanged.
    pub fn with_color_order<D: dma::ChannelInstance>(
        pio: &mut Common<'d, P>,
        mut sm: StateMachine<'d, P, S>,
        dma: Peri<'d, D>,
        irq: impl interrupt::typelevel::Binding<D::Interrupt, dma::InterruptHandler<D>> + 'd,
        pin: Peri<'d, impl PioPin>,
        program: &PioWs2812Program<'d, P>,
    ) -> Self {
        // Setup sm0
        let mut cfg = Config::default();

        // Pin config
        let out_pin = pio.make_pio_pin(pin);
        cfg.set_out_pins(&[&out_pin]);
        cfg.set_set_pins(&[&out_pin]);

        cfg.use_program(&program.prg, &[&out_pin]);

        cfg.clock_divider = calculate_pio_clock_divider(800_000 * CYCLES_PER_BIT);

        // FIFO config
        cfg.fifo_join = FifoJoin::TxOnly;
        cfg.shift_out = ShiftConfig {
            auto_fill: true,
            threshold: 24,
            direction: ShiftDirection::Left,
        };

        sm.set_config(&cfg);
        sm.set_enable(true);

        Self {
            dma: dma::Channel::new(dma, irq),
            sm,
            _order: core::marker::PhantomData,
        }
    }

    /// Sends exactly N `colors`, using a 4*N-byte local word buffer; N=0 is an immediate no-op.
    /// Waits for prior output/reset, DMA transfer, then shifter drain/reset. Cancellation during DMA
    /// aborts the channel but can leave a prefix on the wire; the next write performs a fresh reset wait.
    pub async fn write(
        &mut self,
        colors: &[RGB8; N],
    ) {
        if N == 0 {
            return;
        }
        // Also covers a previous write canceled during DMA or its reset delay.
        reset_interval(&mut self.sm).await;
        // Precompute the word bytes from the colors
        let mut words = [0u32; N];
        for i in 0..N {
            words[i] = ORDER::pack(colors[i]);
        }

        // DMA transfer
        self.sm.tx().dma_push(&mut self.dma, &words, false).await;

        reset_interval(&mut self.sm).await;
    }
}

/// Pio backed RGBW ws2812 driver
/// This version is intended for ws2812 leds with 4 addressable lights
/// Const N is the number of ws2812 leds attached to this pin
pub struct RgbwPioWs2812<'d, P: Instance, const S: usize, const N: usize, ORDER>
where
    ORDER: RgbwColorOrder,
{
    /// Exclusively owned DMA channel feeding the transmit FIFO.
    dma: dma::Channel<'d>,
    /// Enabled transmitter with an eight-word joined FIFO.
    sm: StateMachine<'d, P, S>,
    /// Compile-time channel packing policy; no runtime storage.
    _order: core::marker::PhantomData<ORDER>,
}

impl<'d, P: Instance, const S: usize, const N: usize> RgbwPioWs2812<'d, P, S, N, Grbw> {
    /// Returns a GRBW transmitter using `program` on `sm`, with output `pin` registered through `pio`.
    /// Owns `dma`; `irq` proves its matching DMA interrupt handler. Starts the machine immediately.
    /// Panics if the system clock cannot represent the required 8 MHz instruction rate.
    pub fn new<D: dma::ChannelInstance>(
        pio: &mut Common<'d, P>,
        sm: StateMachine<'d, P, S>,
        dma: Peri<'d, D>,
        irq: impl interrupt::typelevel::Binding<D::Interrupt, dma::InterruptHandler<D>> + 'd,
        pin: Peri<'d, impl PioPin>,
        program: &PioWs2812Program<'d, P>,
    ) -> Self {
        Self::with_color_order(pio, sm, dma, irq, pin, program)
    }
}

impl<'d, P: Instance, const S: usize, const N: usize, ORDER> RgbwPioWs2812<'d, P, S, N, ORDER>
where
    ORDER: RgbwColorOrder,
{
    /// Returns an ORDER-packed RGBW transmitter using `program` on `sm` and output `pin` through `pio`.
    /// Owns `dma`; `irq` proves its matching interrupt handler. Starts at nominal 800 kbit/s.
    /// Panics for an unrepresentable 8 MHz instruction rate; system clock must stay unchanged.
    pub fn with_color_order<D: dma::ChannelInstance>(
        pio: &mut Common<'d, P>,
        mut sm: StateMachine<'d, P, S>,
        dma: Peri<'d, D>,
        irq: impl interrupt::typelevel::Binding<D::Interrupt, dma::InterruptHandler<D>> + 'd,
        pin: Peri<'d, impl PioPin>,
        program: &PioWs2812Program<'d, P>,
    ) -> Self {
        // Setup sm0
        let mut cfg = Config::default();

        // Pin config
        let out_pin = pio.make_pio_pin(pin);
        cfg.set_out_pins(&[&out_pin]);
        cfg.set_set_pins(&[&out_pin]);

        cfg.use_program(&program.prg, &[&out_pin]);

        cfg.clock_divider = calculate_pio_clock_divider(800_000 * CYCLES_PER_BIT);

        // FIFO config
        cfg.fifo_join = FifoJoin::TxOnly;
        cfg.shift_out = ShiftConfig {
            auto_fill: true,
            threshold: 32,
            direction: ShiftDirection::Left,
        };

        sm.set_config(&cfg);
        sm.set_enable(true);

        Self {
            dma: dma::Channel::new(dma, irq),
            sm,
            _order: core::marker::PhantomData,
        }
    }

    /// Sends exactly N `colors`, including white, using a 4*N-byte local word buffer. N=0 is a no-op.
    /// Waits for prior output/reset, DMA transfer, then shifter drain/reset. Cancellation during DMA
    /// aborts the channel but can leave a prefix on the wire; the next write performs a fresh reset wait.
    pub async fn write(
        &mut self,
        colors: &[RGBW<u8>; N],
    ) {
        if N == 0 {
            return;
        }
        reset_interval(&mut self.sm).await;
        // Precompute the word bytes from the colors
        let mut words = [0u32; N];
        for i in 0..N {
            words[i] = ORDER::pack(colors[i]);
        }

        // DMA transfer
        self.sm.tx().dma_push(&mut self.dma, &words, false).await;

        reset_interval(&mut self.sm).await;
    }
}

/// Waits for `sm` to drain its FIFO/shifter then remain idle for 300 microseconds.
/// Requires a functioning time driver and running program; cancellation leaves output running.
async fn reset_interval<P: Instance, const S: usize>(sm: &mut StateMachine<'_, P, S>) {
    // DMA completion leaves up to eight FIFO words plus OSR on the wire.
    wait_tx_idle(sm).await;
    // Cover the longer reset interval of recent WS2812B revisions as well.
    Timer::after_micros(300).await;
}
