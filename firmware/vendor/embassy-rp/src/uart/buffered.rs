//! Buffered UART driver.
use core::future::Future;
use core::slice;
use core::sync::atomic::{AtomicU8, Ordering};

use embassy_hal_internal::atomic_ring_buffer::RingBuffer;

use super::*;

#[path = "buffered_io.rs"]
mod buffered_io;

pub struct State {
    tx_waker: AtomicWaker,
    tx_buf: RingBuffer,
    rx_waker: AtomicWaker,
    rx_buf: RingBuffer,
    rx_error: AtomicU8,
    irq_owner: AtomicU8,
}

// these must match bits 8..11 in UARTDR
const RXE_OVERRUN: u8 = 8;
const RXE_BREAK: u8 = 4;
const RXE_PARITY: u8 = 2;
const RXE_FRAMING: u8 = 1;

impl State {
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
    pub(super) rx: BufferedUartRx,
    pub(super) tx: BufferedUartTx,
}

/// Buffered UART RX handle.
pub struct BufferedUartRx {
    pub(super) info: &'static Info,
    pub(super) state: &'static State,
    // IRQ, reset and Drop must stay on the constructing core.
    pub(super) _not_send_sync: PhantomData<*mut ()>,
}

/// Buffered UART TX handle.
pub struct BufferedUartTx {
    pub(super) info: &'static Info,
    pub(super) state: &'static State,
    // IRQ, reset and Drop must stay on the constructing core.
    pub(super) _not_send_sync: PhantomData<*mut ()>,
}

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

fn rx_error_bits(ris: Uartris) -> u8 {
    (if ris.feris() { RXE_FRAMING } else { 0 })
        | (if ris.peris() { RXE_PARITY } else { 0 })
        | (if ris.beris() { RXE_BREAK } else { 0 })
        | (if ris.oeris() { RXE_OVERRUN } else { 0 })
}

impl BufferedUart {
    /// Observe a sticky RX error without consuming it.
    pub fn rx_error(&self) -> Option<Error> {
        self.rx.rx_error()
    }

    /// Discard RX storage before a new protocol handshake; not a wire boundary.
    pub fn discard_rx(&mut self) -> Result<(), Error> {
        self.rx.discard_rx()
    }

    /// Create a buffered UART instance.
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

    /// Create a buffered UART instance with flow control.
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

    /// Write to UART TX buffer blocking execution until done.
    pub fn blocking_write(&mut self, buffer: &[u8]) -> Result<usize, Error> {
        self.tx.blocking_write(buffer)
    }

    /// Flush UART TX blocking execution until done.
    pub fn blocking_flush(&mut self) -> Result<(), Error> {
        self.tx.blocking_flush()
    }

    /// Read from UART RX buffer blocking execution until done.
    pub fn blocking_read(&mut self, buffer: &mut [u8]) -> Result<usize, Error> {
        self.rx.blocking_read(buffer)
    }

    /// Check if UART is busy transmitting.
    pub fn busy(&self) -> bool {
        self.tx.busy()
    }

    /// Wait until TX is empty and send break condition.
    pub async fn send_break(&mut self, bits: u32) {
        self.tx.send_break(bits).await
    }

    /// sets baudrate on runtime
    pub fn set_baudrate(&mut self, baudrate: u32) {
        super::Uart::<'static, Async>::set_baudrate_inner(self.rx.info, baudrate);
    }

    /// Split into separate RX and TX handles.
    pub fn split(self) -> (BufferedUartTx, BufferedUartRx) {
        (self.tx, self.rx)
    }

    /// Split the Uart into a transmitter and receiver by mutable reference,
    /// which is particularly useful when having two tasks correlating to
    /// transmitting and receiving.
    pub fn split_ref(&mut self) -> (&mut BufferedUartTx, &mut BufferedUartRx) {
        (&mut self.tx, &mut self.rx)
    }
}

impl BufferedUartRx {
    /// Create a new buffered UART RX.
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

    /// Create a new buffered UART RX with flow control.
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

    fn get_rx_error(state: &State) -> Option<Error> {
        let errors = state.rx_error.load(Ordering::Acquire);
        (errors != 0).then(|| Self::decode_error(errors))
    }

    fn rearm(info: &Info, state: &State) {
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

    fn try_read(info: &Info, state: &State, buf: &mut [u8]) -> Poll<Result<usize, Error>> {
        let mut reader = unsafe { state.rx_buf.reader() };
        buffered_io::try_read(&mut reader, &state.rx_error, buf, || Self::rearm(info, state))
            .map(|result| result.map_err(Self::decode_error))
    }

    /// Observe the sticky RX error without consuming it or reading FIFO data.
    pub fn rx_error(&self) -> Option<Error> {
        Self::get_rx_error(self.state)
    }

    /// Discard queued RX data and clear a sticky fault, with bounded FIFO work.
    /// This is destructive and does NOT establish protocol alignment. The owner
    /// must invalidate its parser/transactions before calling, then perform a
    /// qualified protocol handshake. RX/TX handles and IRQ stay on one core.
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

    /// Read from UART RX buffer blocking execution until done.
    pub fn blocking_read(&mut self, buf: &mut [u8]) -> Result<usize, Error> {
        loop {
            match Self::try_read(self.info, self.state, buf) {
                Poll::Ready(res) => return res,
                Poll::Pending => continue,
            }
        }
    }

    fn fill_buf<'a>(info: &'static Info, state: &'static State) -> impl Future<Output = Result<&'a [u8], Error>> {
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

    fn consume(info: &Info, state: &State, amt: usize) {
        let mut reader = unsafe { state.rx_buf.reader() };
        buffered_io::consume_rx(&mut reader, amt);
        Self::rearm(info, state);
    }

    fn read_ready(state: &State) -> Result<bool, Error> {
        match Self::get_rx_error(state) {
            Some(error) => Err(error),
            None => Ok(!state.rx_buf.is_empty()),
        }
    }
}

impl BufferedUartTx {
    /// Create a new buffered UART TX.
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

    /// Create a new buffered UART TX with flow control.
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

    fn flush(info: &'static Info, state: &'static State) -> impl Future<Output = Result<(), Error>> {
        poll_fn(move |cx| buffered_io::poll_flush(&state.tx_buf, &state.tx_waker, cx, || info.interrupt.pend()).map(Ok))
    }

    /// Write to UART TX buffer blocking execution until done.
    pub fn blocking_write(&mut self, buf: &[u8]) -> Result<usize, Error> {
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

    /// Flush UART TX blocking execution until done.
    pub fn blocking_flush(&mut self) -> Result<(), Error> {
        loop {
            if self.state.tx_buf.is_empty() {
                return Ok(());
            }
        }
    }

    /// Check if UART is busy.
    pub fn busy(&self) -> bool {
        self.info.regs.uartfr().read().busy()
    }

    /// Assert a break condition after waiting for the transmit buffers to empty,
    /// for the specified number of bit times. This condition must be asserted
    /// for at least two frame times to be effective, `bits` will adjusted
    /// according to frame size, parity, and stop bit settings to ensure this.
    ///
    /// This method may block for a long amount of time since it has to wait
    /// for the transmit fifo to empty, which may take a while on slow links.
    pub async fn send_break(&mut self, bits: u32) {
        let regs = self.info.regs;
        let bits = bits.max({
            let lcr = regs.uartlcr_h().read();
            let width = lcr.wlen() as u32 + 5;
            let parity = lcr.pen() as u32;
            let stops = 1 + lcr.stp2() as u32;
            2 * (1 + width + parity + stops)
        });
        let divx64 = (((regs.uartibrd().read().baud_divint() as u32) << 6)
            + regs.uartfbrd().read().baud_divfrac() as u32) as u64;
        let div_clk = clk_peri_freq() as u64 * 64;
        let wait_usecs = (1_000_000 * bits as u64 * divx64 * 16 + div_clk - 1) / div_clk;

        Self::flush(self.info, self.state).await.unwrap();
        while self.busy() {}
        regs.uartlcr_h().write_set(|w| w.set_brk(true));
        Timer::after_micros(wait_usecs).await;
        regs.uartlcr_h().write_clear(|w| w.set_brk(true));
    }
}

impl Drop for BufferedUartRx {
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
    _uart: PhantomData<T>,
}

impl<T: Instance> interrupt::typelevel::Handler<T::Interrupt> for BufferedInterruptHandler<T> {
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
    fn kind(&self) -> embedded_io::ErrorKind {
        embedded_io::ErrorKind::Other
    }
}

impl embedded_io_async::ErrorType for BufferedUart {
    type Error = Error;
}

impl embedded_io_async::ErrorType for BufferedUartRx {
    type Error = Error;
}

impl embedded_io_async::ErrorType for BufferedUartTx {
    type Error = Error;
}

impl embedded_io_async::Read for BufferedUart {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        BufferedUartRx::read(self.rx.info, self.rx.state, buf).await
    }
}

impl embedded_io_async::Read for BufferedUartRx {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        Self::read(self.info, self.state, buf).await
    }
}

impl embedded_io_async::ReadReady for BufferedUart {
    fn read_ready(&mut self) -> Result<bool, Self::Error> {
        BufferedUartRx::read_ready(self.rx.state)
    }
}

impl embedded_io_async::ReadReady for BufferedUartRx {
    fn read_ready(&mut self) -> Result<bool, Self::Error> {
        Self::read_ready(self.state)
    }
}

impl embedded_io_async::BufRead for BufferedUart {
    async fn fill_buf(&mut self) -> Result<&[u8], Self::Error> {
        BufferedUartRx::fill_buf(self.rx.info, self.rx.state).await
    }

    fn consume(&mut self, amt: usize) {
        BufferedUartRx::consume(self.rx.info, self.rx.state, amt)
    }
}

impl embedded_io_async::BufRead for BufferedUartRx {
    async fn fill_buf(&mut self) -> Result<&[u8], Self::Error> {
        Self::fill_buf(self.info, self.state).await
    }

    fn consume(&mut self, amt: usize) {
        Self::consume(self.info, self.state, amt)
    }
}

impl embedded_io_async::Write for BufferedUart {
    async fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        BufferedUartTx::write(self.tx.info, self.tx.state, buf).await
    }

    async fn flush(&mut self) -> Result<(), Self::Error> {
        BufferedUartTx::flush(self.tx.info, self.tx.state).await
    }
}

impl embedded_io_async::Write for BufferedUartTx {
    async fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        Self::write(self.info, self.state, buf).await
    }

    async fn flush(&mut self) -> Result<(), Self::Error> {
        Self::flush(self.info, self.state).await
    }
}

impl embedded_io::Read for BufferedUart {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        self.rx.blocking_read(buf)
    }
}

impl embedded_io::Read for BufferedUartRx {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        self.blocking_read(buf)
    }
}

impl embedded_io::Write for BufferedUart {
    fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        self.tx.blocking_write(buf)
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        self.tx.blocking_flush()
    }
}

impl embedded_io::Write for BufferedUartTx {
    fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        self.blocking_write(buf)
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        self.blocking_flush()
    }
}

impl embedded_hal_02::serial::Read<u8> for BufferedUartRx {
    type Error = Error;

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
    type Error = Error;

    fn bwrite_all(&mut self, mut buffer: &[u8]) -> Result<(), Self::Error> {
        while !buffer.is_empty() {
            match self.blocking_write(buffer) {
                Ok(0) => panic!("zero-length write."),
                Ok(n) => buffer = &buffer[n..],
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }

    fn bflush(&mut self) -> Result<(), Self::Error> {
        self.blocking_flush()
    }
}

impl embedded_hal_02::serial::Read<u8> for BufferedUart {
    type Error = Error;

    fn read(&mut self) -> Result<u8, nb::Error<Self::Error>> {
        embedded_hal_02::serial::Read::read(&mut self.rx)
    }
}

impl embedded_hal_02::blocking::serial::Write<u8> for BufferedUart {
    type Error = Error;

    fn bwrite_all(&mut self, mut buffer: &[u8]) -> Result<(), Self::Error> {
        while !buffer.is_empty() {
            match self.blocking_write(buffer) {
                Ok(0) => panic!("zero-length write."),
                Ok(n) => buffer = &buffer[n..],
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }

    fn bflush(&mut self) -> Result<(), Self::Error> {
        self.blocking_flush()
    }
}

impl embedded_hal_nb::serial::ErrorType for BufferedUartRx {
    type Error = Error;
}

impl embedded_hal_nb::serial::ErrorType for BufferedUartTx {
    type Error = Error;
}

impl embedded_hal_nb::serial::ErrorType for BufferedUart {
    type Error = Error;
}

impl embedded_hal_nb::serial::Read for BufferedUartRx {
    fn read(&mut self) -> nb::Result<u8, Self::Error> {
        embedded_hal_02::serial::Read::read(self)
    }
}

impl embedded_hal_nb::serial::Write for BufferedUartTx {
    fn write(&mut self, char: u8) -> nb::Result<(), Self::Error> {
        self.blocking_write(&[char]).map(drop).map_err(nb::Error::Other)
    }

    fn flush(&mut self) -> nb::Result<(), Self::Error> {
        self.blocking_flush().map_err(nb::Error::Other)
    }
}

impl embedded_hal_nb::serial::Read for BufferedUart {
    fn read(&mut self) -> Result<u8, nb::Error<Self::Error>> {
        embedded_hal_02::serial::Read::read(&mut self.rx)
    }
}

impl embedded_hal_nb::serial::Write for BufferedUart {
    fn write(&mut self, char: u8) -> nb::Result<(), Self::Error> {
        self.blocking_write(&[char]).map(drop).map_err(nb::Error::Other)
    }

    fn flush(&mut self) -> nb::Result<(), Self::Error> {
        self.blocking_flush().map_err(nb::Error::Other)
    }
}
