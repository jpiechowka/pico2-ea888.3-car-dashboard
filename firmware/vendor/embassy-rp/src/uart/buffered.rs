//! Same-core UART IRQ rings with sticky receive faults and bounded interrupt work.
//!
//! Handles are neither Send nor Sync: initialization, consumers, IRQ and drop stay
//! on one core. Pending read/write polls copy zero bytes and retain no caller slice.
//! Cancellation never retracts queued TX. Blocking calls need IRQ progress and can
//! wait forever with masked interrupts or deasserted CTS. Writes finish at ring admission.
use core::future::Future;
use core::slice;
use core::sync::atomic::{AtomicU8, Ordering};

use embassy_hal_internal::atomic_ring_buffer::RingBuffer;

use super::*;

#[path = "buffered_io.rs"]
mod buffered_io;

/// Reusable per-UART rings, waiters, sticky RX faults and constructing-core claim.
pub struct State {
    /// Writer/flush waiter, woken by ring drain.
    tx_waker: AtomicWaker,
    /// Task producer to IRQ consumer transmit ring.
    tx_buf: RingBuffer,
    /// Reader waiter, woken for data or sticky faults.
    rx_waker: AtomicWaker,
    /// IRQ producer to task consumer receive ring.
    rx_buf: RingBuffer,
    /// Sticky UARTDR fault bits, cleared only by explicit discard.
    rx_error: AtomicU8,
    /// Zero when unclaimed, otherwise constructing core number plus one.
    irq_owner: AtomicU8,
}

// these must match bits 8..11 in UARTDR
/// Hardware receive FIFO overflow bit.
const RXE_OVERRUN: u8 = 8;
/// Received break-condition bit.
const RXE_BREAK: u8 = 4;
/// Character parity-error bit.
const RXE_PARITY: u8 = 2;
/// Character framing-error bit.
const RXE_FRAMING: u8 = 1;

impl State {
    /// Returns uninitialized empty rings with no IRQ owner or fault.
    pub const fn new() -> Self {
        Self {
            rx_buf: RingBuffer::new(),
            tx_buf: RingBuffer::new(),
            rx_waker: AtomicWaker::new(),
            tx_waker: AtomicWaker::new(),
            rx_error: AtomicU8::new(0),
            irq_owner: AtomicU8::new(0),
        }
    }
}

/// Buffered UART driver.
pub struct BufferedUart {
    /// Exclusive same-core receive-ring consumer.
    pub(super) rx: BufferedUartRx,
    /// Exclusive same-core transmit-ring producer.
    pub(super) tx: BufferedUartTx,
}

/// Buffered UART RX handle.
pub struct BufferedUartRx {
    /// Immutable UART register/IRQ identity.
    pub(super) info: &'static Info,
    /// Per-instance rings and fault state.
    pub(super) state: &'static State,
    // IRQ, reset and Drop must stay on the constructing core.
    /// Prevents moving initialized IRQ ownership or sharing ring consumers across cores.
    pub(super) _not_send_sync: PhantomData<*mut ()>,
}

/// Buffered UART TX handle.
pub struct BufferedUartTx {
    /// Immutable UART register/IRQ identity.
    pub(super) info: &'static Info,
    /// Per-instance rings and fault state.
    pub(super) state: &'static State,
    // IRQ, reset and Drop must stay on the constructing core.
    /// Prevents moving initialized IRQ ownership or sharing ring consumers across cores.
    pub(super) _not_send_sync: PhantomData<*mut ()>,
}

/// Attaches optional nonempty `tx_buffer`/`rx_buffer` to `state` and enables `info` IRQ.
/// Panics for neither buffer, a supplied live ring, or another core's IRQ ownership.
/// Existing opposite-half state is preserved; supplied RX starts with no fault.
pub(super) fn init_buffers(
    info: &Info,
    state: &State,
    tx_buffer: Option<&'static mut [u8]>,
    rx_buffer: Option<&'static mut [u8]>,
) {
    assert!(tx_buffer.as_ref().is_none_or(|buffer| !buffer.is_empty()));
    assert!(rx_buffer.as_ref().is_none_or(|buffer| !buffer.is_empty()));
    // Split blocking halves can be converted independently. Never enable their
    // shared IRQ on two cores, or reinitialize a live ring while it is in use.
    critical_section::with(|cs| {
        assert!(tx_buffer.is_some() || rx_buffer.is_some());
        assert!(tx_buffer.is_none() || !state.tx_buf.is_available());
        assert!(rx_buffer.is_none() || !state.rx_buf.is_available());
        buffered_io::claim_irq_core(&state.irq_owner, crate::pac::SIO.cpuid().read() as u8, cs);
        if let Some(tx_buffer) = tx_buffer {
            let len = tx_buffer.len();
            unsafe { state.tx_buf.init(tx_buffer.as_mut_ptr(), len) };
        }

        if let Some(rx_buffer) = rx_buffer {
            state.rx_error.store(0, Ordering::Release);
            let len = rx_buffer.len();
            unsafe { state.rx_buf.init(rx_buffer.as_mut_ptr(), len) };
        }

        // From the datasheet:
        // "The transmit interrupt is based on a transition through a level, rather
        // than on the level itself. When the interrupt and the UART is enabled
        // before any data is written to the transmit FIFO the interrupt is not set.
        // The interrupt is only set, after written data leaves the single location
        // of the transmit FIFO and it becomes empty."
        //
        // This means we can leave the interrupt enabled the whole time as long as
        // we clear it after it happens. The downside is that the we manually have
        // to pend the ISR when we want data transmission to start.
        info.regs.uartimsc().write(|w| {
            let rx = state.rx_buf.is_available() && state.rx_error.load(Ordering::Acquire) == 0;
            w.set_rxim(rx);
            w.set_rtim(rx);
            w.set_feim(rx);
            w.set_peim(rx);
            w.set_beim(rx);
            w.set_oeim(rx);
            w.set_txim(state.tx_buf.is_available());
        });

        unsafe { info.interrupt.enable() };
        // Also preserves a queued TX kick when the other half is added later.
        info.interrupt.pend();
    });
}

/// Returns compact UARTDR-compatible fault bits from interrupt status `ris`.
fn rx_error_bits(ris: Uartris) -> u8 {
    (if ris.feris() { RXE_FRAMING } else { 0 })
        | (if ris.peris() { RXE_PARITY } else { 0 })
        | (if ris.beris() { RXE_BREAK } else { 0 })
        | (if ris.oeris() { RXE_OVERRUN } else { 0 })
}

impl BufferedUart {
    /// Returns the sticky receive fault or `None`, without consuming bytes or clearing it.
    pub fn rx_error(&self) -> Option<Error> { self.rx.rx_error() }

    /// Discards RX storage and old faults; returns success or a newly observed fault.
    /// Success is not a wire boundary; invalidate parser state before a new qualified handshake.
    /// Returns success after ring discard and at most 32 FIFO reads, or a new sticky fault.
    pub fn discard_rx(&mut self) -> Result<(), Error> { self.rx.discard_rx() }

    /// Returns same-core ownership of `_uart`, `tx`, `rx` with `config` and `_irq` binding.
    /// Takes static `tx_buffer`/`rx_buffer`; empty/live rings, wrong core or zero baud/clock panic.
    pub fn new<T: Instance>(
        _uart: Peri<'static, T>,
        tx: Peri<'static, impl TxPin<T>>,
        rx: Peri<'static, impl RxPin<T>>,
        _irq: impl Binding<T::Interrupt, BufferedInterruptHandler<T>>,
        tx_buffer: &'static mut [u8],
        rx_buffer: &'static mut [u8],
        config: Config,
    ) -> Self {
        super::Uart::<'static, Async>::init(T::info(), Some(tx.into()), Some(rx.into()), None, None, config);
        init_buffers(T::info(), T::buffered_state(), Some(tx_buffer), Some(rx_buffer));

        Self {
            rx: BufferedUartRx {
                info: T::info(),
                state: T::buffered_state(),
                _not_send_sync: PhantomData,
            },
            tx: BufferedUartTx {
                info: T::info(),
                state: T::buffered_state(),
                _not_send_sync: PhantomData,
            },
        }
    }

    /// Returns same-core ownership of `_uart`, `tx`, `rx`, `rts`, `cts`, using `config` and `_irq`.
    /// Takes static `tx_buffer`/`rx_buffer`; empty/live rings, wrong core or zero baud/clock panic.
    /// CTS can stall indefinitely; RTS reflects hardware FIFO, not software-ring space.
    pub fn new_with_rtscts<T: Instance>(
        _uart: Peri<'static, T>,
        tx: Peri<'static, impl TxPin<T>>,
        rx: Peri<'static, impl RxPin<T>>,
        rts: Peri<'static, impl RtsPin<T>>,
        cts: Peri<'static, impl CtsPin<T>>,
        _irq: impl Binding<T::Interrupt, BufferedInterruptHandler<T>>,
        tx_buffer: &'static mut [u8],
        rx_buffer: &'static mut [u8],
        config: Config,
    ) -> Self {
        super::Uart::<'static, Async>::init(
            T::info(),
            Some(tx.into()),
            Some(rx.into()),
            Some(rts.into()),
            Some(cts.into()),
            config,
        );
        init_buffers(T::info(), T::buffered_state(), Some(tx_buffer), Some(rx_buffer));

        Self {
            rx: BufferedUartRx {
                info: T::info(),
                state: T::buffered_state(),
                _not_send_sync: PhantomData,
            },
            tx: BufferedUartTx {
                info: T::info(),
                state: T::buffered_state(),
                _not_send_sync: PhantomData,
            },
        }
    }

    /// Queues a nonempty input prefix and returns its count, or zero for empty input.
    /// Spins for ring space; requires IRQ progress and does not drain the wire.
    /// `buffer` supplies bytes copied into the transmit ring.
    pub fn blocking_write(
        &mut self,
        buffer: &[u8],
    ) -> Result<usize, Error> {
        self.tx.blocking_write(buffer)
    }

    /// Spins until ring/FIFO/shifter drain, then returns success; requires IRQ progress.
    pub fn blocking_flush(&mut self) -> Result<(), Error> { self.tx.blocking_flush() }

    /// Returns a contiguous copied prefix, zero for empty output, or sticky receive fault.
    /// Spins for data; a racing fault can consume a prefix before returning error.
    /// `buffer` receives the prefix; remaining bytes are unchanged.
    pub fn blocking_read(
        &mut self,
        buffer: &mut [u8],
    ) -> Result<usize, Error> {
        self.rx.blocking_read(buffer)
    }

    /// Returns hardware BUSY, excluding queued software-ring bytes.
    pub fn busy(&self) -> bool { self.tx.busy() }

    /// Drains TX then asserts break for `bits` periods, at least two frames.
    /// Cancellation releases break but leaves queued data; zero clock panics.
    /// Cancellation clears break; `bits` is rounded up to two complete configured frames.
    pub async fn send_break(
        &mut self,
        bits: u32,
    ) {
        self.tx.send_break(bits).await
    }

    /// Replaces timing for `baudrate` bits/s; zero baud/clock panics.
    /// Caller must quiesce ring/IRQ/peer traffic; this does not flush or reset protocol state.
    pub fn set_baudrate(
        &mut self,
        baudrate: u32,
    ) {
        super::Uart::<'static, Async>::set_baudrate_inner(self.rx.info, baudrate);
    }

    /// Returns owned TX/RX halves, both retaining the same-core IRQ constraint.
    pub fn split(self) -> (BufferedUartTx, BufferedUartRx) { (self.tx, self.rx) }

    /// Returns disjoint mutable TX/RX borrows, both confined to the constructing core.
    pub fn split_ref(&mut self) -> (&mut BufferedUartTx, &mut BufferedUartRx) { (&mut self.tx, &mut self.rx) }
}

impl BufferedUartRx {
    /// Returns same-core RX ownership of `_uart` and `rx`, using `config` and `_irq`.
    /// Takes static `rx_buffer`; empty/live ring, wrong core or zero baud/clock panics.
    pub fn new<T: Instance>(
        _uart: Peri<'static, T>,
        _irq: impl Binding<T::Interrupt, BufferedInterruptHandler<T>>,
        rx: Peri<'static, impl RxPin<T>>,
        rx_buffer: &'static mut [u8],
        config: Config,
    ) -> Self {
        super::Uart::<'static, Async>::init(T::info(), None, Some(rx.into()), None, None, config);
        init_buffers(T::info(), T::buffered_state(), None, Some(rx_buffer));

        Self {
            info: T::info(),
            state: T::buffered_state(),
            _not_send_sync: PhantomData,
        }
    }

    /// Returns same-core RX ownership of `_uart`, `rx`, `rts`, using `config` and `_irq`.
    /// Takes static `rx_buffer`; empty/live ring, wrong core or zero baud/clock panics.
    /// RTS reflects hardware FIFO occupancy, not software-ring free space.
    pub fn new_with_rts<T: Instance>(
        _uart: Peri<'static, T>,
        _irq: impl Binding<T::Interrupt, BufferedInterruptHandler<T>>,
        rx: Peri<'static, impl RxPin<T>>,
        rts: Peri<'static, impl RtsPin<T>>,
        rx_buffer: &'static mut [u8],
        config: Config,
    ) -> Self {
        super::Uart::<'static, Async>::init(T::info(), None, Some(rx.into()), Some(rts.into()), None, config);
        init_buffers(T::info(), T::buffered_state(), None, Some(rx_buffer));

        Self {
            info: T::info(),
            state: T::buffered_state(),
            _not_send_sync: PhantomData,
        }
    }

    /// Returns a future copying a prefix into `buf` from `state` and rearming `info`.
    /// Pending copies nothing; empty output succeeds with zero; faults can follow partial consumption.
    fn read<'a>(
        info: &'static Info,
        state: &'static State,
        buf: &'a mut [u8],
    ) -> impl Future<Output = Result<usize, Error>> + 'a {
        poll_fn(move |cx| {
            let mut reader = unsafe { state.rx_buf.reader() };
            buffered_io::poll_read(&mut reader, &state.rx_error, &state.rx_waker, cx, buf, || {
                Self::rearm(info, state)
            })
            .map(|result| result.map_err(Self::decode_error))
        })
    }

    /// Returns the highest-priority nonzero fault in `errs`: overrun, break, parity, then framing.
    fn decode_error(errs: u8) -> Error {
        if errs & RXE_OVERRUN != 0 {
            Error::Overrun
        } else if errs & RXE_BREAK != 0 {
            Error::Break
        } else if errs & RXE_PARITY != 0 {
            Error::Parity
        } else {
            Error::Framing
        }
    }

    /// Returns the sticky error in `state`, or `None` without clearing it.
    fn get_rx_error(state: &State) -> Option<Error> {
        let errors = state.rx_error.load(Ordering::Acquire);
        (errors != 0).then(|| Self::decode_error(errors))
    }

    /// Enables `info` receive masks only if `state` has no sticky fault, atomically on its owning core.
    fn rearm(
        info: &Info,
        state: &State,
    ) {
        // IRQ and consumer run on the owning core. Check+enable is atomic
        // against error latching; a sticky fault must never re-enable RX.
        critical_section::with(|_| {
            if state.rx_error.load(Ordering::Acquire) == 0 {
                info.regs.uartimsc().write_set(|w| {
                    w.set_rxim(true);
                    w.set_rtim(true);
                    w.set_feim(true);
                    w.set_peim(true);
                    w.set_beim(true);
                    w.set_oeim(true);
                });
            }
        });
    }

    /// Copies from `state` into `buf` and rearms `info`; returns count/error or Pending without copying.
    /// Empty buffers succeed with zero even on a fault; racing faults may consume a prefix.
    fn try_read(
        info: &Info,
        state: &State,
        buf: &mut [u8],
    ) -> Poll<Result<usize, Error>> {
        let mut reader = unsafe { state.rx_buf.reader() };
        buffered_io::try_read(&mut reader, &state.rx_error, buf, || Self::rearm(info, state))
            .map(|result| result.map_err(Self::decode_error))
    }

    /// Returns the sticky receive fault or `None`, without consuming bytes or clearing it.
    pub fn rx_error(&self) -> Option<Error> { Self::get_rx_error(self.state) }

    /// Discard queued RX data and clear a sticky fault, with bounded FIFO work.
    /// This is destructive and does NOT establish protocol alignment. The owner
    /// must invalidate its parser/transactions before calling, then perform a
    /// qualified protocol handshake. RX/TX handles and IRQ stay on one core.
    /// Returns success after ring discard and at most 32 FIFO reads, or a new sticky fault.
    pub fn discard_rx(&mut self) -> Result<(), Error> {
        critical_section::with(|_| {
            let r = self.info.regs;
            r.uartimsc().write_clear(|w| {
                w.set_rxim(true);
                w.set_rtim(true);
                w.set_feim(true);
                w.set_peim(true);
                w.set_beim(true);
                w.set_oeim(true);
            });
            let mut reader = unsafe { self.state.rx_buf.reader() };
            // No producer can run here. A ring has at most two spans.
            for _ in 0..2 {
                let n = reader.pop_slice().len();
                reader.pop_done(n);
            }
            // Clear OLD errors before draining, so any new error remains visible.
            r.uarticr().write(|w| {
                w.set_feic(true);
                w.set_peic(true);
                w.set_beic(true);
                w.set_oeic(true);
                w.set_rtic(true);
                w.set_rxic(true);
            });
            r.uartrsr().write(|w| w.0 = 0);
            for _ in 0..32 {
                if r.uartfr().read().rxfe() {
                    break;
                }
                let _ = r.uartdr().read();
            }
            let ris = r.uartris().read();
            let mut errors = rx_error_bits(ris);
            if !r.uartfr().read().rxfe() {
                errors |= RXE_OVERRUN;
            }
            self.state.rx_error.store(errors, Ordering::Release);
            if errors != 0 {
                return Err(Self::decode_error(errors));
            }
            Self::rearm(self.info, self.state);
            Ok(())
        })
    }

    /// Returns a contiguous copied prefix, zero for empty output, or sticky receive fault.
    /// Spins for data; a racing fault can consume a prefix before returning error.
    /// `buf` receives the prefix; remaining bytes are unchanged.
    pub fn blocking_read(
        &mut self,
        buf: &mut [u8],
    ) -> Result<usize, Error> {
        loop {
            match Self::try_read(self.info, self.state, buf) {
                Poll::Ready(res) => return res,
                Poll::Pending => continue,
            }
        }
    }

    /// Returns a future borrowing the next RX span in `state`, rearming `info`, or reporting a fault.
    /// Caller must keep sole RX ownership and consume the span before reuse/drop.
    fn fill_buf<'a>(
        info: &'static Info,
        state: &'static State,
    ) -> impl Future<Output = Result<&'a [u8], Error>> {
        poll_fn(move |cx| {
            state.rx_waker.register(cx.waker());
            if let Some(error) = Self::get_rx_error(state) {
                return Poll::Ready(Err(error));
            }
            let mut reader = unsafe { state.rx_buf.reader() };
            let (p, n) = reader.pop_buf();
            Self::rearm(info, state);
            if let Some(error) = Self::get_rx_error(state) {
                return Poll::Ready(Err(error));
            }
            if n == 0 {
                Poll::Pending
            } else {
                Poll::Ready(Ok(unsafe { slice::from_raw_parts(p, n) }))
            }
        })
    }

    /// Advances `state` by `amt` and rearms `info`; panics beyond the current contiguous span.
    fn consume(
        info: &Info,
        state: &State,
        amt: usize,
    ) {
        let mut reader = unsafe { state.rx_buf.reader() };
        buffered_io::consume_rx(&mut reader, amt);
        Self::rearm(info, state);
    }

    /// Returns data availability in `state`, or its sticky fault, without consuming anything.
    fn read_ready(state: &State) -> Result<bool, Error> {
        match Self::get_rx_error(state) {
            Some(error) => Err(error),
            None => Ok(!state.rx_buf.is_empty()),
        }
    }
}

impl BufferedUartTx {
    /// Returns same-core TX ownership of `_uart` and `tx`, using `config` and `_irq`.
    /// Takes static `tx_buffer`; empty/live ring, wrong core or zero baud/clock panics.
    pub fn new<T: Instance>(
        _uart: Peri<'static, T>,
        _irq: impl Binding<T::Interrupt, BufferedInterruptHandler<T>>,
        tx: Peri<'static, impl TxPin<T>>,
        tx_buffer: &'static mut [u8],
        config: Config,
    ) -> Self {
        super::Uart::<'static, Async>::init(T::info(), Some(tx.into()), None, None, None, config);
        init_buffers(T::info(), T::buffered_state(), Some(tx_buffer), None);

        Self {
            info: T::info(),
            state: T::buffered_state(),
            _not_send_sync: PhantomData,
        }
    }

    /// Returns same-core TX ownership of `_uart`, `tx`, `cts`, using `config` and `_irq`.
    /// Takes static `tx_buffer`; empty/live ring, wrong core or zero baud/clock panics.
    /// CTS can stall writes/flush indefinitely.
    pub fn new_with_cts<T: Instance>(
        _uart: Peri<'static, T>,
        _irq: impl Binding<T::Interrupt, BufferedInterruptHandler<T>>,
        tx: Peri<'static, impl TxPin<T>>,
        cts: Peri<'static, impl CtsPin<T>>,
        tx_buffer: &'static mut [u8],
        config: Config,
    ) -> Self {
        super::Uart::<'static, Async>::init(T::info(), Some(tx.into()), None, None, Some(cts.into()), config);
        init_buffers(T::info(), T::buffered_state(), Some(tx_buffer), None);

        Self {
            info: T::info(),
            state: T::buffered_state(),
            _not_send_sync: PhantomData,
        }
    }

    /// Returns a future queuing a prefix of `buf` in `state` and pending `info` IRQ.
    /// Pending copies nothing; empty input returns zero; accepted data remains queued.
    fn write<'d>(
        info: &'static Info,
        state: &'static State,
        buf: &'d [u8],
    ) -> impl Future<Output = Result<usize, Error>> + 'd {
        poll_fn(move |cx| {
            let mut writer = unsafe { state.tx_buf.writer() };
            buffered_io::poll_write(&mut writer, &state.tx_waker, cx, buf, || info.interrupt.pend()).map(Ok)
        })
    }

    /// Returns success after `state` ring and `info` FIFO/shifter drain, polling hardware every 10 us.
    /// CTS/IRQ stalls have no timeout; cancellation leaves queued bytes.
    async fn flush(
        info: &'static Info,
        state: &'static State,
    ) -> Result<(), Error> {
        poll_fn(move |cx| buffered_io::poll_flush(&state.tx_buf, &state.tx_waker, cx, || info.interrupt.pend())).await;
        // The last ring drain wake can precede FIFO/shift-register completion.
        // There is no UART interrupt for the final stop bit.
        while !tx_drained(info.regs) {
            Timer::after_micros(10).await;
        }
        Ok(())
    }

    /// Queues `byte` and returns success, or WouldBlock with no copy when full.
    fn try_write(
        &mut self,
        byte: u8,
    ) -> nb::Result<(), Error> {
        let mut writer = unsafe { self.state.tx_buf.writer() };
        match buffered_io::try_write(&mut writer, &[byte], || self.info.interrupt.pend()) {
            Poll::Ready(1) => Ok(()),
            _ => Err(nb::Error::WouldBlock),
        }
    }

    /// Returns success only when ring/FIFO/shifter are empty; otherwise WouldBlock.
    fn try_flush(&mut self) -> nb::Result<(), Error> {
        if self.state.tx_buf.is_empty() && tx_drained(self.info.regs) {
            Ok(())
        } else {
            Err(nb::Error::WouldBlock)
        }
    }

    /// Queues a nonempty input prefix and returns its count, or zero for empty input.
    /// Spins for ring space; requires IRQ progress and does not drain the wire.
    /// `buf` supplies bytes copied into the transmit ring.
    pub fn blocking_write(
        &mut self,
        buf: &[u8],
    ) -> Result<usize, Error> {
        if buf.is_empty() {
            return Ok(0);
        }

        loop {
            let mut tx_writer = unsafe { self.state.tx_buf.writer() };
            let n = tx_writer.push(|data| {
                let n = data.len().min(buf.len());
                data[..n].copy_from_slice(&buf[..n]);
                n
            });

            if n != 0 {
                // The TX interrupt only triggers when the there was data in the
                // FIFO and the number of bytes drops below a threshold. When the
                // FIFO was empty we have to manually pend the interrupt to shovel
                // TX data from the buffer into the FIFO.
                self.info.interrupt.pend();
                return Ok(n);
            }
        }
    }

    /// Spins until ring/FIFO/shifter drain, then returns success; requires IRQ progress.
    pub fn blocking_flush(&mut self) -> Result<(), Error> {
        loop {
            if self.state.tx_buf.is_empty() && tx_drained(self.info.regs) {
                return Ok(());
            }
        }
    }

    /// Returns hardware BUSY, excluding queued software-ring bytes.
    pub fn busy(&self) -> bool { self.info.regs.uartfr().read().busy() }

    /// Assert a break condition after waiting for the transmit buffers to empty,
    /// for the specified number of bit times. This condition must be asserted
    /// for at least two frame times to be effective, `bits` will adjusted
    /// according to frame size, parity, and stop bit settings to ensure this.
    ///
    /// This method may block for a long amount of time since it has to wait
    /// for the transmit fifo to empty, which may take a while on slow links.
    /// Cancellation clears break; `bits` is rounded up to two complete configured frames.
    pub async fn send_break(
        &mut self,
        bits: u32,
    ) {
        let regs = self.info.regs;
        let wait_usecs = break_duration_us(regs, bits);

        Self::flush(self.info, self.state).await.unwrap();
        let _guard = BreakGuard::new(regs);
        Timer::after_micros(wait_usecs).await;
    }
}

impl Drop for BufferedUartRx {
    /// Masks RX IRQs and releases its ring; the last half releases this core's IRQ claim.
    fn drop(&mut self) {
        critical_section::with(|_| {
            self.info.regs.uartimsc().write_clear(|w| {
                w.set_rxim(true);
                w.set_rtim(true);
                w.set_feim(true);
                w.set_peim(true);
                w.set_beim(true);
                w.set_oeim(true);
            });
            unsafe { self.state.rx_buf.deinit() };
            if !self.state.tx_buf.is_available() {
                self.info.interrupt.disable();
                self.state.irq_owner.store(0, Ordering::Release);
            }
        });
    }
}

impl Drop for BufferedUartTx {
    /// Discards queued ring data; FIFO may keep sending. The last half releases the IRQ claim.
    fn drop(&mut self) {
        critical_section::with(|_| {
            self.info.regs.uartimsc().write_clear(|w| w.set_txim(true));
            unsafe { self.state.tx_buf.deinit() };
            if !self.state.rx_buf.is_available() {
                self.info.interrupt.disable();
                self.state.irq_owner.store(0, Ordering::Release);
            }
        });
    }
}

/// Interrupt handler.
pub struct BufferedInterruptHandler<T: Instance> {
    /// Compile-time peripheral identity.
    _uart: PhantomData<T>,
}

impl<T: Instance> interrupt::typelevel::Handler<T::Interrupt> for BufferedInterruptHandler<T> {
    /// Moves at most 32 RX and 32 TX bytes, latches faults and wakes waiters; re-pends remaining work.
    ///
    /// # Safety
    /// Run only for T's UART IRQ on the core owning its initialized rings.
    unsafe fn on_interrupt() {
        let r = T::info().regs;
        if r.uartdmacr().read().rxdmae() {
            return;
        }

        let s = T::buffered_state();

        // Clear TX and error interrupt flags
        // RX interrupt flags are cleared by reading from the FIFO.
        let ris = r.uartris().read();
        r.uarticr().write(|w| {
            w.set_txic(ris.txris());
            w.set_feic(ris.feris());
            w.set_peic(ris.peris());
            w.set_beic(ris.beris());
            w.set_oeic(ris.oeris());
        });

        // No formatted logging in the IRQ path. Error flags are sticky until
        // the owner explicitly discards RX and establishes a new wire boundary.
        if s.rx_buf.is_available() {
            let bits = rx_error_bits(ris);
            if bits != 0 {
                buffered_io::latch_error(&s.rx_error, bits);
            }
            let mut writer = unsafe { s.rx_buf.writer() };
            let count = buffered_io::fill_rx(&mut writer, &s.rx_error, 32, || {
                if r.uartfr().read().rxfe() {
                    return None;
                }
                let dr = r.uartdr().read();
                let bits = (dr.0 >> 8) as u8 & 0x0F;
                Some(if bits == 0 { Ok(dr.data()) } else { Err(bits) })
            });
            let failed = s.rx_error.load(Ordering::Acquire) != 0;
            if count > 0 || failed {
                s.rx_waker.wake();
            }
            if s.rx_buf.is_full() || failed {
                r.uartimsc().write_clear(|w| {
                    w.set_rxim(true);
                    w.set_rtim(true);
                    // Keep error IRQs enabled for a full ring: hardware may
                    // overrun before a reader makes space. Sticky faults mask all.
                    w.set_feim(failed);
                    w.set_peim(failed);
                    w.set_beim(failed);
                    w.set_oeim(failed);
                });
            } else if !r.uartfr().read().rxfe() {
                T::info().interrupt.pend();
            }
        }

        if s.tx_buf.is_available() {
            let mut reader = unsafe { s.tx_buf.reader() };
            let count = buffered_io::drain_tx(&mut reader, 32, |byte| {
                if r.uartfr().read().txff() {
                    return false;
                }
                r.uartdr().write(|w| w.set_data(byte));
                true
            });
            if count > 0 {
                s.tx_waker.wake();
            }
            // Preserve liveness if the bounded drain stops with FIFO space.
            if !s.tx_buf.is_empty() && !r.uartfr().read().txff() {
                T::info().interrupt.pend();
            }
        }
    }
}

impl embedded_io::Error for Error {
    /// Returns Other; concrete Error retains the UART fault detail.
    fn kind(&self) -> embedded_io::ErrorKind { embedded_io::ErrorKind::Other }
}

impl embedded_io_async::ErrorType for BufferedUart {
    /// Sticky receive fault; TX adapters currently report no UART errors.
    type Error = Error;
}

impl embedded_io_async::ErrorType for BufferedUartRx {
    /// Sticky receive fault; TX adapters currently report no UART errors.
    type Error = Error;
}

impl embedded_io_async::ErrorType for BufferedUartTx {
    /// Sticky receive fault; TX adapters currently report no UART errors.
    type Error = Error;
}

impl embedded_io_async::Read for BufferedUart {
    /// Returns a prefix copied into `buf`, zero for empty output, or sticky fault.
    /// Pending copies nothing; racing faults can consume a prefix before error.
    async fn read(
        &mut self,
        buf: &mut [u8],
    ) -> Result<usize, Self::Error> {
        BufferedUartRx::read(self.rx.info, self.rx.state, buf).await
    }
}

impl embedded_io_async::Read for BufferedUartRx {
    /// Returns a prefix copied into `buf`, zero for empty output, or sticky fault.
    /// Pending copies nothing; racing faults can consume a prefix before error.
    async fn read(
        &mut self,
        buf: &mut [u8],
    ) -> Result<usize, Self::Error> {
        Self::read(self.info, self.state, buf).await
    }
}

impl embedded_io_async::ReadReady for BufferedUart {
    /// Returns immediate data availability or sticky fault without consumption.
    fn read_ready(&mut self) -> Result<bool, Self::Error> { BufferedUartRx::read_ready(self.rx.state) }
}

impl embedded_io_async::ReadReady for BufferedUartRx {
    /// Returns immediate data availability or sticky fault without consumption.
    fn read_ready(&mut self) -> Result<bool, Self::Error> { Self::read_ready(self.state) }
}

impl embedded_io_async::BufRead for BufferedUart {
    /// Returns the next contiguous nonempty span or sticky fault; cancellation consumes nothing.
    async fn fill_buf(&mut self) -> Result<&[u8], Self::Error> {
        BufferedUartRx::fill_buf(self.rx.info, self.rx.state).await
    }

    /// Consumes `amt` from the current contiguous span; panics beyond it.
    fn consume(
        &mut self,
        amt: usize,
    ) {
        BufferedUartRx::consume(self.rx.info, self.rx.state, amt)
    }
}

impl embedded_io_async::BufRead for BufferedUartRx {
    /// Returns the next contiguous nonempty span or sticky fault; cancellation consumes nothing.
    async fn fill_buf(&mut self) -> Result<&[u8], Self::Error> { Self::fill_buf(self.info, self.state).await }

    /// Consumes `amt` from the current contiguous span; panics beyond it.
    fn consume(
        &mut self,
        amt: usize,
    ) {
        Self::consume(self.info, self.state, amt)
    }
}

impl embedded_io_async::Write for BufferedUart {
    /// Queues a prefix of `buf` and returns its length, zero if empty; Pending copies nothing.
    async fn write(
        &mut self,
        buf: &[u8],
    ) -> Result<usize, Self::Error> {
        BufferedUartTx::write(self.tx.info, self.tx.state, buf).await
    }

    /// Returns success after ring/FIFO/shifter drain; cancellation leaves queued bytes.
    async fn flush(&mut self) -> Result<(), Self::Error> { BufferedUartTx::flush(self.tx.info, self.tx.state).await }
}

impl embedded_io_async::Write for BufferedUartTx {
    /// Queues a prefix of `buf` and returns its length, zero if empty; Pending copies nothing.
    async fn write(
        &mut self,
        buf: &[u8],
    ) -> Result<usize, Self::Error> {
        Self::write(self.info, self.state, buf).await
    }

    /// Returns success after ring/FIFO/shifter drain; cancellation leaves queued bytes.
    async fn flush(&mut self) -> Result<(), Self::Error> { Self::flush(self.info, self.state).await }
}

impl embedded_io::Read for BufferedUart {
    /// Blocks for a prefix into `buf`; returns its count, zero for empty output, or sticky fault.
    fn read(
        &mut self,
        buf: &mut [u8],
    ) -> Result<usize, Self::Error> {
        self.rx.blocking_read(buf)
    }
}

impl embedded_io::Read for BufferedUartRx {
    /// Blocks for a prefix into `buf`; returns its count, zero for empty output, or sticky fault.
    fn read(
        &mut self,
        buf: &mut [u8],
    ) -> Result<usize, Self::Error> {
        self.blocking_read(buf)
    }
}

impl embedded_io::Write for BufferedUart {
    /// Blocks for ring space, queues a prefix of `buf` and returns its length; empty input returns zero.
    fn write(
        &mut self,
        buf: &[u8],
    ) -> Result<usize, Self::Error> {
        self.tx.blocking_write(buf)
    }

    /// Blocks until ring/FIFO/shifter drain then returns success; requires IRQ progress.
    fn flush(&mut self) -> Result<(), Self::Error> { self.tx.blocking_flush() }
}

impl embedded_io::Write for BufferedUartTx {
    /// Blocks for ring space, queues a prefix of `buf` and returns its length; empty input returns zero.
    fn write(
        &mut self,
        buf: &[u8],
    ) -> Result<usize, Self::Error> {
        self.blocking_write(buf)
    }

    /// Blocks until ring/FIFO/shifter drain then returns success; requires IRQ progress.
    fn flush(&mut self) -> Result<(), Self::Error> { self.blocking_flush() }
}

impl embedded_hal_02::serial::Read<u8> for BufferedUartRx {
    /// Sticky receive fault; TX adapters currently report no UART errors.
    type Error = Error;

    /// Returns one ring byte, WouldBlock without data, or the sticky receive fault.
    fn read(&mut self) -> Result<u8, nb::Error<Self::Error>> {
        let mut byte = [0];
        match Self::try_read(self.info, self.state, &mut byte) {
            Poll::Ready(Ok(1)) => Ok(byte[0]),
            Poll::Ready(Err(error)) => Err(nb::Error::Other(error)),
            _ => Err(nb::Error::WouldBlock),
        }
    }
}

impl embedded_hal_02::blocking::serial::Write<u8> for BufferedUartTx {
    /// Sticky receive fault; TX adapters currently report no UART errors.
    type Error = Error;

    /// Queues all `buffer` and returns success; no deadline, unexpected zero progress panics.
    fn bwrite_all(
        &mut self,
        mut buffer: &[u8],
    ) -> Result<(), Self::Error> {
        while !buffer.is_empty() {
            match self.blocking_write(buffer) {
                Ok(0) => panic!("zero-length write."),
                Ok(n) => buffer = &buffer[n..],
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }

    /// Blocks for ring/FIFO/shifter drain and returns success without peer acknowledgment.
    fn bflush(&mut self) -> Result<(), Self::Error> { self.blocking_flush() }
}

impl embedded_hal_02::serial::Read<u8> for BufferedUart {
    /// Sticky receive fault; TX adapters currently report no UART errors.
    type Error = Error;

    /// Returns one ring byte, WouldBlock without data, or the sticky receive fault.
    fn read(&mut self) -> Result<u8, nb::Error<Self::Error>> { embedded_hal_02::serial::Read::read(&mut self.rx) }
}

impl embedded_hal_02::blocking::serial::Write<u8> for BufferedUart {
    /// Sticky receive fault; TX adapters currently report no UART errors.
    type Error = Error;

    /// Queues all `buffer` and returns success; no deadline, unexpected zero progress panics.
    fn bwrite_all(
        &mut self,
        mut buffer: &[u8],
    ) -> Result<(), Self::Error> {
        while !buffer.is_empty() {
            match self.blocking_write(buffer) {
                Ok(0) => panic!("zero-length write."),
                Ok(n) => buffer = &buffer[n..],
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }

    /// Blocks for ring/FIFO/shifter drain and returns success without peer acknowledgment.
    fn bflush(&mut self) -> Result<(), Self::Error> { self.blocking_flush() }
}

impl embedded_hal_nb::serial::ErrorType for BufferedUartRx {
    /// Sticky receive fault; TX adapters currently report no UART errors.
    type Error = Error;
}

impl embedded_hal_nb::serial::ErrorType for BufferedUartTx {
    /// Sticky receive fault; TX adapters currently report no UART errors.
    type Error = Error;
}

impl embedded_hal_nb::serial::ErrorType for BufferedUart {
    /// Sticky receive fault; TX adapters currently report no UART errors.
    type Error = Error;
}

impl embedded_hal_nb::serial::Read for BufferedUartRx {
    /// Returns one ring byte, WouldBlock without data, or the sticky receive fault.
    fn read(&mut self) -> nb::Result<u8, Self::Error> { embedded_hal_02::serial::Read::read(self) }
}

impl embedded_hal_nb::serial::Write for BufferedUartTx {
    /// Queues `char` or returns WouldBlock when full; success does not flush.
    fn write(
        &mut self,
        char: u8,
    ) -> nb::Result<(), Self::Error> {
        self.try_write(char)
    }

    /// Returns success with ring/FIFO/shifter empty, otherwise WouldBlock.
    fn flush(&mut self) -> nb::Result<(), Self::Error> { self.try_flush() }
}

impl embedded_hal_nb::serial::Read for BufferedUart {
    /// Returns one ring byte, WouldBlock without data, or the sticky receive fault.
    fn read(&mut self) -> Result<u8, nb::Error<Self::Error>> { embedded_hal_02::serial::Read::read(&mut self.rx) }
}

impl embedded_hal_nb::serial::Write for BufferedUart {
    /// Queues `char` or returns WouldBlock when full; success does not flush.
    fn write(
        &mut self,
        char: u8,
    ) -> nb::Result<(), Self::Error> {
        self.tx.try_write(char)
    }

    /// Returns success with ring/FIFO/shifter empty, otherwise WouldBlock.
    fn flush(&mut self) -> nb::Result<(), Self::Error> { self.tx.try_flush() }
}
