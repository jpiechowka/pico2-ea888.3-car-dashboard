//! Pio backed uart drivers

use core::convert::Infallible;

use embassy_time::{Duration, Timer};
use embedded_io_async::{ErrorType, Read, Write};

use super::clock_divider::calculate_pio_clock_divider_value;
use super::wait_tx_idle;
use crate::Peri;
use crate::clocks::clk_sys_freq;
use crate::gpio::Level;
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

/// Returns the 1/256 divider for `baud` bits/s using eight instructions per bit at `sys_hz`.
/// Panics for zero baud, multiplication overflow or divider outside 1..=65536.
fn uart_clock_divider(
    sys_hz: u32,
    baud: u32,
) -> fixed::FixedU32<fixed::types::extra::U8> {
    let pio_hz = baud.checked_mul(8).expect("UART baud rate is too high");
    calculate_pio_clock_divider_value(sys_hz, pio_hz)
}

/// Returns a full eight-cycle stop interval for `divider` at nonzero `sys_hz`, rounded up
/// to microseconds plus one timer tick. Zero `sys_hz` panics; callers supply an admitted divider.
fn stop_bit_duration(
    sys_hz: u32,
    divider: fixed::FixedU32<fixed::types::extra::U8>,
) -> Duration {
    // Eight PIO cycles, rounding up for the fractional divider's phase.
    let sys_cycles = (u64::from(divider.to_bits()) * 8).div_ceil(256);
    let micros = (sys_cycles * 1_000_000).div_ceil(u64::from(sys_hz));
    // Instant::now() is tick-quantized: reserve one extra tick so even a
    // sample just before a tick boundary waits the whole stop-bit interval.
    Duration::from_micros(micros) + Duration::from_ticks(1)
}

/// This struct represents a uart tx program loaded into pio instruction memory.
pub struct PioUartTxProgram<'d, PIO: Instance> {
    /// Relocated 8N1 serial program allocation.
    prg: LoadedProgram<'d, PIO>,
}

impl<'d, PIO: Instance> PioUartTxProgram<'d, PIO> {
    /// Loads the 8N1 transmit instructions through `common`; returns their allocation handle.
    /// Panics if instruction memory cannot fit the program.
    pub fn new(common: &mut Common<'d, PIO>) -> Self {
        let prg = pio::pio_asm!(
            r#"
                .side_set 1 opt

                ; An 8n1 UART transmit program.
                ; OUT pin 0 and side-set pin 0 are both mapped to UART TX pin.

                    pull       side 1 [7]  ; Assert stop bit, or stall with line in idle state
                    set x, 7   side 0 [7]  ; Preload bit counter, assert start bit for 8 clocks
                bitloop:                   ; This loop will run 8 times (8n1 UART)
                    out pins, 1            ; Shift 1 bit from OSR to the first OUT pin
                    jmp x-- bitloop   [6]  ; Each loop iteration is 8 cycles.
            "#
        );

        let prg = common.load_program(&prg.program);

        Self { prg }
    }
}

/// PIO backed Uart transmitter
///
/// The system clock must remain unchanged while this driver is configured.
/// Cancelling a write can leave a prefix queued for transmission. Cancelling
/// a flush leaves transmission running; flushing again waits for completion.
/// Flushing requires a running Embassy time driver.
pub struct PioUartTx<'d, PIO: Instance, const SM: usize> {
    /// Enabled transmitter with joined eight-word transmit FIFO.
    sm_tx: StateMachine<'d, PIO, SM>,
    /// Conservative final stop-bit wait derived from the configured system clock/divider.
    stop_bit: Duration,
}

impl<'d, PIO: Instance, const SM: usize> PioUartTx<'d, PIO, SM> {
    /// Registers `tx_pin` through `common`, configures and starts `sm_tx` with `program`.
    /// Returns an 8N1 transmitter at requested `baud` bits/s; divider truncation can raise actual baud.
    ///
    /// Panics if `baud` is zero or cannot be represented by the PIO divider.
    pub fn new(
        baud: u32,
        common: &mut Common<'d, PIO>,
        mut sm_tx: StateMachine<'d, PIO, SM>,
        tx_pin: Peri<'d, impl PioPin>,
        program: &PioUartTxProgram<'d, PIO>,
    ) -> Self {
        let sys_hz = clk_sys_freq();
        let divider = uart_clock_divider(sys_hz, baud);
        let stop_bit = stop_bit_duration(sys_hz, divider);
        let tx_pin = common.make_pio_pin(tx_pin);
        sm_tx.set_pins(Level::High, &[&tx_pin]);
        sm_tx.set_pin_dirs(PioDirection::Out, &[&tx_pin]);

        let mut cfg = Config::default();

        cfg.set_out_pins(&[&tx_pin]);
        cfg.use_program(&program.prg, &[&tx_pin]);
        cfg.shift_out.auto_fill = false;
        cfg.shift_out.direction = ShiftDirection::Right;
        cfg.fifo_join = FifoJoin::TxOnly;
        cfg.clock_divider = divider;
        sm_tx.set_config(&cfg);
        sm_tx.set_enable(true);

        Self { sm_tx, stop_bit }
    }

    /// Enqueues `data` when FIFO space is available; completion does not mean wire completion.
    /// Cancellation while pending enqueues no byte; the transmitter remains running.
    pub async fn write_u8(
        &mut self,
        data: u8,
    ) {
        self.sm_tx.tx().wait_push(data as u32).await;
    }
}

impl<PIO: Instance, const SM: usize> ErrorType for PioUartTx<'_, PIO, SM> {
    /// This adapter exposes no framing/overflow or hardware error reports.
    type Error = Infallible;
}

impl<PIO: Instance, const SM: usize> Write for PioUartTx<'_, PIO, SM> {
    /// Enqueues every byte of `buf`; returns Ok(buf.len()), including zero for empty input.
    /// Cancellation preserves an already queued prefix; no wire-completion guarantee is implied.
    async fn write(
        &mut self,
        buf: &[u8],
    ) -> Result<usize, Infallible> {
        for byte in buf {
            self.write_u8(*byte).await;
        }
        Ok(buf.len())
    }

    /// Returns Ok after FIFO and shifter drain plus a complete stop-bit interval.
    /// Waits without timeout; cancellation leaves TX running and a later flush restarts the wait.
    async fn flush(&mut self) -> Result<(), Infallible> {
        wait_tx_idle(&mut self.sm_tx).await;
        // PULL side 1 stalls with TX high, but its [7] delay runs only after
        // PULL completes. Allow a full stop bit before reporting wire idle.
        Timer::after(self.stop_bit).await;
        Ok(())
    }
}

/// This struct represents a Uart Rx program loaded into pio instruction memory.
pub struct PioUartRxProgram<'d, PIO: Instance> {
    /// Relocated 8N1 serial program allocation.
    prg: LoadedProgram<'d, PIO>,
}

impl<'d, PIO: Instance> PioUartRxProgram<'d, PIO> {
    /// Loads the 8N1 receive instructions through `common`; returns their allocation handle.
    /// Panics if instruction memory cannot fit the program.
    pub fn new(common: &mut Common<'d, PIO>) -> Self {
        let prg = pio::pio_asm!(
            r#"
                ; Slightly more fleshed-out 8n1 UART receiver which handles framing errors and
                ; break conditions more gracefully.
                ; IN pin 0 and JMP pin are both mapped to the GPIO used as UART RX.

                start:
                    wait 0 pin 0        ; Stall until start bit is asserted
                    set x, 7    [10]    ; Preload bit counter, then delay until halfway through
                rx_bitloop:             ; the first data bit (12 cycles incl wait, set).
                    in pins, 1          ; Shift data bit into ISR
                    jmp x-- rx_bitloop [6] ; Loop 8 times, each loop iteration is 8 cycles
                    jmp pin good_rx_stop   ; Check stop bit (should be high)

                    irq 4 rel           ; Either a framing error or a break. Set a sticky flag,
                    wait 1 pin 0        ; and wait for line to return to idle state.
                    jmp start           ; Don't push data if we didn't see good framing.

                good_rx_stop:           ; No delay before returning to start; a little slack is
                    in null 24
                    push                ; important in case the TX clock is slightly too fast.
            "#
        );

        let prg = common.load_program(&prg.program);

        Self { prg }
    }
}

/// PIO backed Uart receiver
///
/// The system clock must remain unchanged while this driver is configured.
pub struct PioUartRx<'d, PIO: Instance, const SM: usize> {
    /// Enabled receiver; framing failures set a PIO flag but discard the affected byte.
    sm_rx: StateMachine<'d, PIO, SM>,
}

impl<'d, PIO: Instance, const SM: usize> PioUartRx<'d, PIO, SM> {
    /// Registers `rx_pin` through `common`, configures and starts `sm_rx` with `program`.
    /// Returns an 8N1 receiver at requested `baud` bits/s; divider truncation can raise actual baud.
    ///
    /// Panics if `baud` is zero or cannot be represented by the PIO divider.
    pub fn new(
        baud: u32,
        common: &mut Common<'d, PIO>,
        mut sm_rx: StateMachine<'d, PIO, SM>,
        rx_pin: Peri<'d, impl PioPin>,
        program: &PioUartRxProgram<'d, PIO>,
    ) -> Self {
        let divider = uart_clock_divider(clk_sys_freq(), baud);
        let mut cfg = Config::default();
        cfg.use_program(&program.prg, &[]);

        let rx_pin = common.make_pio_pin(rx_pin);
        sm_rx.set_pins(Level::High, &[&rx_pin]);
        cfg.set_in_pins(&[&rx_pin]);
        cfg.set_jmp_pin(&rx_pin);
        sm_rx.set_pin_dirs(PioDirection::In, &[&rx_pin]);

        cfg.clock_divider = divider;
        cfg.shift_in.auto_fill = false;
        cfg.shift_in.direction = ShiftDirection::Right;
        cfg.shift_in.threshold = 32;
        cfg.fifo_join = FifoJoin::RxOnly;
        sm_rx.set_config(&cfg);
        sm_rx.set_enable(true);

        Self { sm_rx }
    }

    /// Returns the next correctly framed byte, waiting indefinitely if none arrives.
    /// Framing failures are dropped by PIO; cancellation while pending consumes no FIFO word.
    pub async fn read_u8(&mut self) -> u8 { self.sm_rx.rx().wait_pull().await as u8 }
}

impl<PIO: Instance, const SM: usize> ErrorType for PioUartRx<'_, PIO, SM> {
    /// This adapter exposes no framing/overflow or hardware error reports.
    type Error = Infallible;
}

impl<PIO: Instance, const SM: usize> Read for PioUartRx<'_, PIO, SM> {
    /// Fills all of `buf`; returns Ok(buf.len()), or zero immediately for empty input.
    /// Cancellation can leave a consumed prefix written; framing/overflow are not returned as errors.
    async fn read(
        &mut self,
        buf: &mut [u8],
    ) -> Result<usize, Infallible> {
        let mut i = 0;
        while i < buf.len() {
            buf[i] = self.read_u8().await;
            i += 1;
        }
        Ok(i)
    }
}

#[cfg(test)]
/// Baud arithmetic and native register-model flush/cancellation regressions.
mod tests {
    use super::*;

    /// Preserves fractional baud dividers instead of integer rounding.
    #[test]
    fn uart_retains_fractional_baud_divider() {
        // Eight PIO cycles per bit: 125MHz / 1Mbaud / 8 = 15.625.
        assert_eq!(uart_clock_divider(125_000_000, 1_000_000).to_bits(), 4000);
        assert_eq!(uart_clock_divider(128_000_000, 1_000_000).to_bits(), 4096);
    }

    /// Rejects baud values that would alias valid rates through multiplication wraparound.
    #[test]
    #[should_panic]
    fn uart_rejects_baud_multiplication_overflow() {
        // With wrapping arithmetic this aliases the otherwise legal 1Mbaud.
        let _ = uart_clock_divider(125_000_000, (1 << 29) + 1_000_000);
    }

    /// Rejects a zero baud denominator.
    #[test]
    #[should_panic]
    fn uart_rejects_zero_baud() { let _ = uart_clock_divider(125_000_000, 0); }

    /// Rejects rates needing a divider below one.
    #[test]
    #[should_panic]
    fn uart_rejects_baud_above_system_clock_limit() { let _ = uart_clock_divider(8_000_000, 1_000_001); }

    /// Rejects rates needing a divider above 65536.
    #[test]
    #[should_panic]
    fn uart_rejects_baud_below_divider_range() { let _ = uart_clock_divider(125_000_000, 1); }

    /// Covers divider endpoints, fractional microseconds and the extra timer tick.
    #[test]
    fn uart_stop_interval_covers_divider_endpoints_and_timer_quantization() {
        let minimum = uart_clock_divider(8_000_000, 1_000_000);
        assert_eq!(minimum.to_bits(), 256);
        assert_eq!(
            stop_bit_duration(8_000_000, minimum),
            Duration::from_micros(1) + Duration::from_ticks(1)
        );
        let maximum = uart_clock_divider(524_288, 1);
        assert_eq!(maximum.to_bits(), 1 << 24);
        assert_eq!(
            stop_bit_duration(524_288, maximum),
            Duration::from_secs(1) + Duration::from_ticks(1)
        );
        // 9600baud takes about104.167us per bit; do not truncate to104us.
        assert_eq!(
            stop_bit_duration(125_000_000, uart_clock_divider(125_000_000, 9600)),
            Duration::from_micros(105) + Duration::from_ticks(1)
        );
    }

    /// Models FIFO/shifter/stop-bit completion and cancellation without executing PIO instructions.
    #[cfg(all(target_os = "windows", feature = "_test", not(feature = "time-driver")))]
    #[test]
    fn uart_flush_waits_for_fifo_osr_and_stop_bit_without_aborting_on_cancel() {
        use core::future::Future;
        use core::mem::ManuallyDrop;
        use core::pin::pin;
        use core::task::{Context, Poll, Waker};

        use embassy_time::{Duration, MockDriver};

        #[link(name = "kernel32")]
        unsafe extern "system" {
            /// Reserves/commits `size` bytes at `address` with allocation `kind` and page `protect`.
            /// Returns null on failure; caller must use valid Windows allocation flags and ownership.
            fn VirtualAlloc(
                address: *mut core::ffi::c_void,
                size: usize,
                kind: u32,
                protect: u32,
            ) -> *mut core::ffi::c_void;
            /// Releases `address` with `size`/`kind` matching its allocation. Returns nonzero on success.
            fn VirtualFree(
                address: *mut core::ffi::c_void,
                size: usize,
                kind: u32,
            ) -> i32;
        }
        /// Owns the native register-model mapping until test completion/unwind.
        struct Registers(
            /// Allocation base returned by VirtualAlloc, never a physical device mapping.
            *mut core::ffi::c_void,
        );
        impl Drop for Registers {
            /// Releases the owned mapping; panics if Windows rejects the release.
            fn drop(&mut self) {
                // SAFETY: This test exclusively owns the allocated register page.
                assert_ne!(unsafe { VirtualFree(self.0, 0, 0x8000) }, 0);
            }
        }
        let pio = crate::pac::PIO0;
        let address = pio.as_ptr().cast();
        // SAFETY: Reserve fresh native memory at the PAC address, never device memory.
        // Allocation fails rather than replacing any existing mapping.
        let allocated = unsafe { VirtualAlloc(address, 0x4000, 0x3000, 0x04) };
        assert_eq!(allocated, address);
        assert!(!allocated.is_null());
        let _registers = Registers(allocated);
        pio.ctrl().write(|w| w.set_sm_enable(1));
        let driver = MockDriver::get();
        driver.reset();
        // SAFETY: StateMachine consists only of PhantomData wrappers. Suppress its
        // hardware ownership destructor because this test owns only a register model.
        let mut tx = ManuallyDrop::new(PioUartTx::<crate::peripherals::PIO0, 0> {
            sm_tx: unsafe { core::mem::zeroed() },
            stop_bit: stop_bit_duration(125_000_000, uart_clock_divider(125_000_000, 1_000_000)),
        });
        let mut cx = Context::from_waker(Waker::noop());

        {
            let mut write = pin!(tx.write(&[0xA5]));
            assert_eq!(write.as_mut().poll(&mut cx), Poll::Ready(Ok(1)));
        }
        assert_eq!(pio.txf(0).read(), 0xA5);
        {
            let mut flush = pin!(tx.flush());
            assert!(
                flush.as_mut().poll(&mut cx).is_pending(),
                "queued byte must not be flushed"
            );
        }
        assert_eq!(pio.ctrl().read().sm_enable(), 1, "cancellation must leave TX running");
        assert_eq!(pio.txf(0).read(), 0xA5);

        // The model supplies hardware status explicitly; it does not emulate W1C
        // or execute PIO instructions. Empty FIFO alone does not mean empty OSR.
        pio.fstat().write(|w| w.set_txempty(1));
        pio.fdebug().write(|w| w.set_txstall(0));
        {
            let mut flush = pin!(tx.flush());
            assert!(flush.as_mut().poll(&mut cx).is_pending());
            driver.advance(Duration::from_micros(1));
            assert!(flush.as_mut().poll(&mut cx).is_pending(), "last byte is still shifting");
            pio.fdebug().write(|w| w.set_txstall(1));
            driver.advance(Duration::from_micros(1));
            assert!(
                flush.as_mut().poll(&mut cx).is_pending(),
                "stop bit has only just risen"
            );
            driver.advance(Duration::from_micros(1));
            assert!(
                flush.as_mut().poll(&mut cx).is_pending(),
                "reserve a tick for timer quantization"
            );
            // Cancel during the stop interval, then retry from a fresh stall.
        }
        assert_eq!(pio.ctrl().read().sm_enable(), 1);
        pio.fdebug().write(|w| w.set_txstall(0));
        {
            let mut flush = pin!(tx.flush());
            assert!(flush.as_mut().poll(&mut cx).is_pending());
            pio.fdebug().write(|w| w.set_txstall(1));
            driver.advance(Duration::from_micros(1));
            assert!(flush.as_mut().poll(&mut cx).is_pending());
            driver.advance(Duration::from_micros(1));
            assert!(flush.as_mut().poll(&mut cx).is_pending());
            driver.advance(Duration::from_micros(1));
            assert_eq!(flush.as_mut().poll(&mut cx), Poll::Ready(Ok(())));
        }
        assert_eq!(pio.ctrl().read().sm_enable(), 1);
        assert_eq!(
            pio.sm(0).instr().read().0,
            0,
            "flush must not force pins or restart the program"
        );
        let mut write = pin!(tx.write(&[0x5A]));
        assert_eq!(write.as_mut().poll(&mut cx), Poll::Ready(Ok(1)));
        assert_eq!(
            pio.txf(0).read(),
            0x5A,
            "driver remains usable after cancellation and flush"
        );
    }
}
