//! PIO backed SPI drivers

use core::marker::PhantomData;

use embassy_futures::join::join;
use embassy_hal_internal::Peri;
use embedded_hal_02::spi::{Phase, Polarity};
use fixed::types::extra::U8;

use crate::gpio::Level;
use crate::pio::{Common, Direction, Instance, LoadedProgram, Pin, PioPin, ShiftDirection, StateMachine};
use crate::pio_programs::clock_divider::calculate_pio_clock_divider;
use crate::pio_programs::wait_tx_idle;
use crate::spi::{Async, Blocking, Config, Mode};
use crate::{dma, interrupt};

/// This struct represents an SPI program loaded into pio instruction memory.
struct PioSpiProgram<'d, PIO: Instance> {
    /// Relocated phase-specific transfer instructions.
    prg: LoadedProgram<'d, PIO>,
    /// Capture edge encoded by the loaded instruction sequence.
    phase: Phase,
}

impl<'d, PIO: Instance> PioSpiProgram<'d, PIO> {
    /// Loads the instruction sequence for `phase` through `common`; returns its allocation handle.
    /// Panics if instruction memory cannot fit the phase-specific program.
    pub fn new(
        common: &mut Common<'d, PIO>,
        phase: Phase,
    ) -> Self {
        // These PIO programs are taken straight from the datasheet (3.6.1 in
        // RP2040 datasheet, 11.6.1 in RP2350 datasheet)

        // Pin assignments:
        // - SCK is side-set pin 0
        // - MOSI is OUT pin 0
        // - MISO is IN pin 0
        //
        // Auto-push and auto-pull must be enabled, and the serial frame size is set by
        // configuring the push/pull threshold. Shift left/right is fine, but you must
        // justify the data yourself. This is done most conveniently for frame sizes of
        // 8 or 16 bits by using the narrow store replication and narrow load byte
        // picking behavior of RP2040's IO fabric.

        let prg = match phase {
            Phase::CaptureOnFirstTransition => {
                let prg = pio::pio_asm!(
                    r#"
                        .side_set 1

                        ; Clock phase = 0: data is captured on the leading edge of each SCK pulse, and
                        ; transitions on the trailing edge, or some time before the first leading edge.

                        out pins, 1 side 0 [1] ; Stall here on empty (sideset proceeds even if
                        in pins, 1  side 1 [1] ; instruction stalls, so we stall with SCK low)
                    "#
                );

                common.load_program(&prg.program)
            }
            Phase::CaptureOnSecondTransition => {
                let prg = pio::pio_asm!(
                    r#"
                        .side_set 1

                        ; Clock phase = 1: data transitions on the leading edge of each SCK pulse, and
                        ; is captured on the trailing edge.

                        out x, 1    side 0     ; Stall here on empty (keep SCK de-asserted)
                        mov pins, x side 1 [1] ; Output data, assert SCK (mov pins uses OUT mapping)
                        in pins, 1  side 0     ; Input data, de-assert SCK
                    "#
                );

                common.load_program(&prg.program)
            }
        };

        Self { prg, phase }
    }
}

/// PIO SPI errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[non_exhaustive]
pub enum Error {
    // No errors for now
}

/// PIO based SPI driver.
/// Unlike other PIO programs, the PIO SPI driver owns and holds a reference to
/// the PIO memory it uses. This is so that it can be reconfigured at runtime if
/// desired.
pub struct Spi<'d, PIO: Instance, const SM: usize, M: Mode> {
    /// Enabled byte shifter, stopped/reset by transfer cancellation recovery.
    sm: StateMachine<'d, PIO, SM>,
    /// Cached configuration used when reloading a phase-specific program.
    cfg: crate::pio::Config<'d, PIO>,
    /// Current instruction allocation; temporarily absent while replacing phase.
    program: Option<PioSpiProgram<'d, PIO>>,
    /// Clock output including polarity inversion.
    clk_pin: Pin<'d, PIO>,
    /// Transmit channel in async mode; absent in blocking mode.
    tx_dma: Option<dma::Channel<'d>>,
    /// Receive/drain channel in async mode; absent in blocking mode.
    rx_dma: Option<dma::Channel<'d>>,
    /// Compile-time blocking/async API selection.
    phantom: PhantomData<M>,
}

impl<'d, PIO: Instance, const SM: usize, M: Mode> Spi<'d, PIO, SM, M> {
    /// Registers `clk_pin`, `mosi_pin`, `miso_pin` through `pio`, loads a phase program and starts `sm`.
    /// Returns a driver using `config` frequency/phase/polarity and optional `tx_dma`/`rx_dma` channels.
    /// Panics for instruction exhaustion or invalid frequency; frame format is fixed to 8-bit MSB-first.
    #[allow(clippy::too_many_arguments)]
    fn new_inner(
        pio: &mut Common<'d, PIO>,
        mut sm: StateMachine<'d, PIO, SM>,
        clk_pin: Peri<'d, impl PioPin>,
        mosi_pin: Peri<'d, impl PioPin>,
        miso_pin: Peri<'d, impl PioPin>,
        tx_dma: Option<dma::Channel<'d>>,
        rx_dma: Option<dma::Channel<'d>>,
        config: Config,
    ) -> Self {
        let program = PioSpiProgram::new(pio, config.phase);

        let mut clk_pin = pio.make_pio_pin(clk_pin);
        let mosi_pin = pio.make_pio_pin(mosi_pin);
        let miso_pin = pio.make_pio_pin(miso_pin);

        if let Polarity::IdleHigh = config.polarity {
            clk_pin.set_output_inversion(true);
        } else {
            clk_pin.set_output_inversion(false);
        }

        let mut cfg = crate::pio::Config::default();

        cfg.use_program(&program.prg, &[&clk_pin]);
        cfg.set_out_pins(&[&mosi_pin]);
        cfg.set_in_pins(&[&miso_pin]);

        cfg.shift_in.auto_fill = true;
        cfg.shift_in.direction = ShiftDirection::Left;
        cfg.shift_in.threshold = 8;

        cfg.shift_out.auto_fill = true;
        cfg.shift_out.direction = ShiftDirection::Left;
        cfg.shift_out.threshold = 8;

        cfg.clock_divider = calculate_clock_divider(config.frequency);

        sm.set_config(&cfg);

        sm.set_pins(Level::Low, &[&clk_pin, &mosi_pin]);
        sm.set_pin_dirs(Direction::Out, &[&clk_pin, &mosi_pin]);
        sm.set_pin_dirs(Direction::In, &[&miso_pin]);

        sm.set_enable(true);

        Self {
            sm,
            program: Some(program),
            cfg,
            clk_pin,
            tx_dma,
            rx_dma,
            phantom: PhantomData,
        }
    }

    /// Returns Ok with the next received byte, spinning indefinitely while RX is empty.
    fn blocking_read_u8(&mut self) -> Result<u8, Error> {
        while self.sm.rx().empty() {}
        let value = self.sm.rx().pull() as u8;

        Ok(value)
    }

    /// Enqueues MSB-aligned `v` and clears stale TX-stall state; returns Ok without waiting for wire drain.
    /// Spins indefinitely when FIFO is full.
    fn blocking_write_u8(
        &mut self,
        v: u8,
    ) -> Result<(), Error> {
        let value = u32::from_be_bytes([v, 0, 0, 0]);

        while !self.sm.tx().try_push(value) {}

        // need to clear here for flush to work correctly
        self.sm.tx().stalled();

        Ok(())
    }

    /// Fills `data` while sending zero bytes, then waits for wire idle; returns Ok.
    /// Empty input still flushes. Hardware stalls have no timeout; chip select is caller-owned.
    pub fn blocking_read(
        &mut self,
        data: &mut [u8],
    ) -> Result<(), Error> {
        for v in data {
            self.blocking_write_u8(0)?;
            *v = self.blocking_read_u8()?;
        }
        self.flush()?;
        Ok(())
    }

    /// Sends `data`, drains received bytes and waits for wire idle; returns Ok.
    /// Empty input still flushes. Hardware stalls have no timeout; chip select is caller-owned.
    pub fn blocking_write(
        &mut self,
        data: &[u8],
    ) -> Result<(), Error> {
        for v in data {
            self.blocking_write_u8(*v)?;
            let _ = self.blocking_read_u8()?;
        }
        self.flush()?;
        Ok(())
    }

    /// Exchanges max lengths of `read`/`write`, padding TX with zeros and discarding excess RX.
    /// Returns Ok after wire idle; empty buffers still flush. No timeout or chip-select handling.
    pub fn blocking_transfer(
        &mut self,
        read: &mut [u8],
        write: &[u8],
    ) -> Result<(), Error> {
        let len = read.len().max(write.len());
        for i in 0..len {
            let wb = write.get(i).copied().unwrap_or(0);
            self.blocking_write_u8(wb)?;

            let rb = self.blocking_read_u8()?;
            if let Some(r) = read.get_mut(i) {
                *r = rb;
            }
        }
        self.flush()?;
        Ok(())
    }

    /// Replaces each transmitted byte in `data` with its received byte; returns Ok after wire idle.
    /// Empty input still flushes; waits have no timeout.
    pub fn blocking_transfer_in_place(
        &mut self,
        data: &mut [u8],
    ) -> Result<(), Error> {
        for v in data {
            self.blocking_write_u8(*v)?;
            *v = self.blocking_read_u8()?;
        }
        self.flush()?;
        Ok(())
    }

    /// Returns Ok after transmit FIFO empties and the PIO program stalls awaiting new output.
    /// Spins without timeout; does not drain pending receive words or control chip select.
    pub fn flush(&mut self) -> Result<(), Error> {
        // Discard a sticky stall from before the current transfer.
        self.sm.tx().stalled();
        // Wait for all words in the FIFO to have been pulled by the SM
        while !self.sm.tx().empty() {}

        // Wait for last value to be written out to the wire
        while !self.sm.tx().stalled() {}

        Ok(())
    }

    /// Pauses the machine, sets requested `freq` Hz and re-enables it without draining current work.
    /// Panics for zero, four-times overflow or unrepresentable divider, leaving the machine disabled.
    /// Divider truncation can raise actual rate; system clock changes require reconfiguration.
    pub fn set_frequency(
        &mut self,
        freq: u32,
    ) {
        self.sm.set_enable(false);

        let divider = calculate_clock_divider(freq);

        // save into the config for later but dont use sm.set_config() since
        // that operation is relatively more expensive than just setting the
        // clock divider
        self.cfg.clock_divider = divider;
        self.sm.set_clock_divider(divider);

        self.sm.set_enable(true);
    }

    /// Applies `config` frequency/phase/polarity, using `pio` to replace phase instructions if needed.
    /// Does not flush beforehand. Reconfigures/restarts the machine and enables it on success.
    /// Invalid frequency or allocation failure panics after pausing and may leave partially changed state.
    ///
    /// This operation will panic if the PIO program needs to be reloaded and
    /// there is insufficient room. This is unlikely since the programs for each
    /// phase only differ in size by a single instruction.
    pub fn set_config(
        &mut self,
        pio: &mut Common<'d, PIO>,
        config: &Config,
    ) {
        self.sm.set_enable(false);

        self.cfg.clock_divider = calculate_clock_divider(config.frequency);

        if let Polarity::IdleHigh = config.polarity {
            self.clk_pin.set_output_inversion(true);
        } else {
            self.clk_pin.set_output_inversion(false);
        }

        if self.program.as_ref().unwrap().phase != config.phase {
            let old_program = self.program.take().unwrap();

            // SAFETY: the state machine is disabled while this happens
            unsafe { pio.free_instr(old_program.prg.used_memory) };

            let new_program = PioSpiProgram::new(pio, config.phase);

            self.cfg.use_program(&new_program.prg, &[&self.clk_pin]);
            self.program = Some(new_program);
        }

        self.sm.set_config(&self.cfg);
        self.sm.restart();

        self.sm.set_enable(true);
    }
}

/// Returns the PIO divider for `frequency_hz` serial cycles/s, at four instructions per cycle.
/// Panics for zero, multiplication overflow or divider outside 1..=65536.
fn calculate_clock_divider(frequency_hz: u32) -> fixed::FixedU32<U8> {
    // we multiply by 4 since each clock period is equal to 4 instructions

    calculate_pio_clock_divider(frequency_hz.checked_mul(4).expect("SPI clock overflow"))
}

impl<'d, PIO: Instance, const SM: usize> Spi<'d, PIO, SM, Blocking> {
    /// Returns a running blocking byte SPI bus on `sm`, registering `clk`, `mosi`, `miso` through `pio`.
    /// `config` selects frequency/phase/polarity; invalid rates or instruction exhaustion panic.
    /// Chip select remains caller-owned; system clock must stay unchanged.
    pub fn new_blocking(
        pio: &mut Common<'d, PIO>,
        sm: StateMachine<'d, PIO, SM>,
        clk: Peri<'d, impl PioPin>,
        mosi: Peri<'d, impl PioPin>,
        miso: Peri<'d, impl PioPin>,
        config: Config,
    ) -> Self {
        Self::new_inner(pio, sm, clk, mosi, miso, None, None, config)
    }
}

impl<'d, PIO: Instance, const SM: usize> Spi<'d, PIO, SM, Async> {
    /// Returns a running DMA byte SPI bus on `sm`, registering `clk`, `mosi`, `miso` through `pio`.
    /// Owns `tx_dma`/`rx_dma`; `irq` proves both handlers. `config` selects frequency/phase/polarity.
    /// Invalid rates or instruction exhaustion panic. Chip select remains caller-owned.
    #[allow(clippy::too_many_arguments)]
    pub fn new<TxDma: dma::ChannelInstance, RxDma: dma::ChannelInstance>(
        pio: &mut Common<'d, PIO>,
        sm: StateMachine<'d, PIO, SM>,
        clk: Peri<'d, impl PioPin>,
        mosi: Peri<'d, impl PioPin>,
        miso: Peri<'d, impl PioPin>,
        tx_dma: Peri<'d, TxDma>,
        rx_dma: Peri<'d, RxDma>,
        irq: impl interrupt::typelevel::Binding<TxDma::Interrupt, dma::InterruptHandler<TxDma>>
        + interrupt::typelevel::Binding<RxDma::Interrupt, dma::InterruptHandler<RxDma>>
        + 'd,
        config: Config,
    ) -> Self {
        let tx_dma_ch = dma::Channel::new(tx_dma, irq);
        let rx_dma_ch = dma::Channel::new(rx_dma, irq);
        Self::new_inner(pio, sm, clk, mosi, miso, Some(tx_dma_ch), Some(rx_dma_ch), config)
    }

    /// Fills `buffer` while sending zeros; returns Ok after wire idle.
    /// Cancellation aborts DMA and resets FIFO/program/clock, retaining a possible received prefix.
    pub async fn read(
        &mut self,
        buffer: &mut [u8],
    ) -> Result<(), Error> {
        self.transfer_inner(buffer, &[]).await
    }

    /// Sends `buffer` and discards RX; returns Ok after wire idle.
    /// Cancellation aborts DMA and resets FIFO/program/clock; a transmitted prefix cannot be undone.
    pub async fn write(
        &mut self,
        buffer: &[u8],
    ) -> Result<(), Error> {
        self.transfer_inner(&mut [], buffer).await
    }

    /// Exchanges max lengths of `rx_buffer`/`tx_buffer`, zero-padding TX and discarding excess RX.
    /// Returns Ok after wire idle. Cancellation aborts DMA/reset state while preserving partial external effects.
    pub async fn transfer(
        &mut self,
        rx_buffer: &mut [u8],
        tx_buffer: &[u8],
    ) -> Result<(), Error> {
        self.transfer_inner(rx_buffer, tx_buffer).await
    }

    /// Replaces transmitted `words` with received bytes; returns Ok after wire idle.
    /// Cancellation may overwrite a prefix, aborts DMA and resets FIFO/program/clock. No rollback.
    pub async fn transfer_in_place(
        &mut self,
        words: &mut [u8],
    ) -> Result<(), Error> {
        let buffer: *mut [u8] = words;
        self.transfer_inner(buffer, buffer.cast_const()).await
    }

    /// Returns Ok after DMA and wire drain for raw `rx_buffer`/`tx_buffer`; lengths may differ or be zero.
    /// Callers provide valid borrowed buffers for the full future, disjoint or exactly in-place. Raw pointers
    /// avoid simultaneous Rust references for the in-place case; RX follows the corresponding TX byte.
    /// Cancellation drops both DMA futures before resetting state through the guard.
    async fn transfer_inner(
        &mut self,
        rx_buffer: *mut [u8],
        tx_buffer: *const [u8],
    ) -> Result<(), Error> {
        let rx_ptr = self.sm.rx_fifo_ptr().cast::<u8>();
        let tx_ptr = self.sm.tx_fifo_ptr().cast::<u8>();
        let rx_treq = self.sm.rx_treq();
        let tx_treq = self.sm.tx_treq();
        let mut guard = TransferGuard {
            sm: &mut self.sm,
            clk_pin: &self.clk_pin,
            origin: self.program.as_ref().unwrap().prg.origin,
            active: true,
        };

        let mut rx_ch = self.rx_dma.as_mut().unwrap().reborrow();
        let rx_transfer = async {
            // Raw DMA pointers preserve in-place aliasing: no overlapping Rust
            // references exist. RX cannot overwrite byte N before TX clocked it.
            if !rx_buffer.is_empty() {
                unsafe { rx_ch.read(rx_ptr, rx_buffer, rx_treq, false) }.await;
            }

            if tx_buffer.len() > rx_buffer.len() {
                let read_bytes_len = tx_buffer.len() - rx_buffer.len();

                unsafe { rx_ch.read_discard(rx_ptr, read_bytes_len, rx_treq) }.await;
            }
        };

        let mut tx_ch = self.tx_dma.as_mut().unwrap().reborrow();
        let tx_transfer = async {
            if !tx_buffer.is_empty() {
                unsafe { tx_ch.write(tx_buffer, tx_ptr, tx_treq, false) }.await;
            }

            if rx_buffer.len() > tx_buffer.len() {
                let write_bytes_len = rx_buffer.len() - tx_buffer.len();

                unsafe { tx_ch.write_zeros(write_bytes_len, tx_ptr, tx_treq) }.await;
            }
        };

        join(tx_transfer, rx_transfer).await;
        wait_tx_idle(guard.sm).await;
        guard.active = false;
        Ok(())
    }
}

/// Resets canceled transfers; declared before DMA futures so their drops stop DMA before FIFO recovery.
struct TransferGuard<'a, 'd, PIO: Instance, const SM: usize> {
    /// Borrowed machine recovered after incomplete transfer.
    sm: &'a mut StateMachine<'d, PIO, SM>,
    /// Clock pin forced to logical low (physical polarity is preserved by inversion).
    clk_pin: &'a Pin<'d, PIO>,
    /// Current phase-program entry point for restart.
    origin: u8,
    /// Cleared only after both DMA transfers and final wire drain complete.
    active: bool,
}

impl<PIO: Instance, const SM: usize> Drop for TransferGuard<'_, '_, PIO, SM> {
    /// Resets incomplete FIFO/program state and returns clock to idle before re-enabling the machine.
    fn drop(&mut self) {
        if self.active {
            self.sm.set_enable(false);
            self.sm.clear_fifos();
            self.sm.restart();
            self.sm.set_pins(Level::Low, &[self.clk_pin]);
            unsafe { self.sm.exec_jmp(self.origin) };
            self.sm.set_enable(true);
        }
    }
}

// ====================

impl<'d, PIO: Instance, const SM: usize, M: Mode> embedded_hal_02::blocking::spi::Transfer<u8> for Spi<'d, PIO, SM, M> {
    /// Empty error enum: waits may stall or panic but no recoverable error is constructed.
    type Error = Error;

    /// Exchanges `words` in place and returns the same now-received slice after wire idle.
    fn transfer<'w>(
        &mut self,
        words: &'w mut [u8],
    ) -> Result<&'w [u8], Self::Error> {
        self.blocking_transfer_in_place(words)?;
        Ok(words)
    }
}

impl<'d, PIO: Instance, const SM: usize, M: Mode> embedded_hal_02::blocking::spi::Write<u8> for Spi<'d, PIO, SM, M> {
    /// Empty error enum: waits may stall or panic but no recoverable error is constructed.
    type Error = Error;

    /// Sends `words` and drains RX; returns Ok after wire idle.
    fn write(
        &mut self,
        words: &[u8],
    ) -> Result<(), Self::Error> {
        self.blocking_write(words)
    }
}

impl embedded_hal_1::spi::Error for Error {
    /// Returns the generic SPI category; unreachable because this enum has no values.
    fn kind(&self) -> embedded_hal_1::spi::ErrorKind { match *self {} }
}

impl<'d, PIO: Instance, const SM: usize, M: Mode> embedded_hal_1::spi::ErrorType for Spi<'d, PIO, SM, M> {
    /// Empty error enum: waits may stall or panic but no recoverable error is constructed.
    type Error = Error;
}

impl<'d, PIO: Instance, const SM: usize, M: Mode> embedded_hal_1::spi::SpiBus<u8> for Spi<'d, PIO, SM, M> {
    /// Returns Ok after the blocking FIFO/shifter drain; no timeout.
    fn flush(&mut self) -> Result<(), Self::Error> { self.flush() }

    /// Fills `words` with zero-clocked RX bytes; returns Ok after wire idle.
    fn read(
        &mut self,
        words: &mut [u8],
    ) -> Result<(), Self::Error> {
        self.blocking_transfer(words, &[])
    }

    /// Sends `words` and drains RX; returns Ok after wire idle.
    fn write(
        &mut self,
        words: &[u8],
    ) -> Result<(), Self::Error> {
        self.blocking_write(words)
    }

    /// Exchanges `read`/`write` with zero-padding/discard for unequal lengths; returns Ok after idle.
    fn transfer(
        &mut self,
        read: &mut [u8],
        write: &[u8],
    ) -> Result<(), Self::Error> {
        self.blocking_transfer(read, write)
    }

    /// Replaces `words` with RX bytes; returns Ok after blocking wire drain.
    fn transfer_in_place(
        &mut self,
        words: &mut [u8],
    ) -> Result<(), Self::Error> {
        self.blocking_transfer_in_place(words)
    }
}

impl<'d, PIO: Instance, const SM: usize> embedded_hal_async::spi::SpiBus<u8> for Spi<'d, PIO, SM, Async> {
    /// Returns Ok after FIFO/shifter idle; cancellation leaves transmission running and may be retried.
    async fn flush(&mut self) -> Result<(), Self::Error> {
        wait_tx_idle(&mut self.sm).await;
        Ok(())
    }

    /// Sends `words`; returns Ok after idle. Cancellation aborts DMA and resets transfer state.
    async fn write(
        &mut self,
        words: &[u8],
    ) -> Result<(), Self::Error> {
        self.write(words).await
    }

    /// Fills `words` while sending zeros; returns Ok after idle. Cancellation retains a received prefix.
    async fn read(
        &mut self,
        words: &mut [u8],
    ) -> Result<(), Self::Error> {
        self.read(words).await
    }

    /// Exchanges `read`/`write` with zero-padding/discard; returns Ok after idle. Cancellation resets transfer state.
    async fn transfer(
        &mut self,
        read: &mut [u8],
        write: &[u8],
    ) -> Result<(), Self::Error> {
        self.transfer(read, write).await
    }

    /// Exchanges `words` in place; returns Ok after idle. Cancellation may retain a received prefix.
    async fn transfer_in_place(
        &mut self,
        words: &mut [u8],
    ) -> Result<(), Self::Error> {
        self.transfer_in_place(words).await
    }
}
