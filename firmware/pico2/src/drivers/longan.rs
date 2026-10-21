//! Core-1-only buffered UART owner. No DMA or cancelled slice references.
use core::future::Future;
use core::pin::pin;
use core::task::{Context, Poll};

use dashboard_pico2::config::vehicle::{BAUD, RX_BYTES, TX_BYTES};
use dashboard_pico2::diagnostics::service::{IoError, Port};
use embassy_rp::Peri;
use embassy_rp::peripherals::{PIN_0, PIN_1, UART0};
use embassy_rp::uart::{BufferedInterruptHandler, BufferedUart, Config, Error};
use embedded_io_async::{Read, Write};
use static_cell::ConstStaticCell;

embassy_rp::bind_interrupts!(
/// Core1 UART0 buffered interrupt binding; installed when the owner is constructed.
struct Irqs {
    UART0_IRQ => BufferedInterruptHandler<UART0>;
});

/// One-time RX ring allocation owned by the core1 UART driver.
static RX: ConstStaticCell<[u8; RX_BYTES]> = ConstStaticCell::new([0; RX_BYTES]);
/// One-time TX ring allocation; queued bytes remain owned until drained.
static TX: ConstStaticCell<[u8; TX_BYTES]> = ConstStaticCell::new([0; TX_BYTES]);

/// Only uninitialized peripheral tokens cross cores. The driver never does.
pub struct Resources {
    /// Uninitialized UART0 peripheral token moved to core1.
    pub uart: Peri<'static, UART0>,
    /// UART0 TX GPIO0 pin token.
    pub tx: Peri<'static, PIN_0>,
    /// UART0 RX GPIO1 pin token.
    pub rx: Peri<'static, PIN_1>,
}

/// Non-DMA UART boundary whose poll methods retain no caller slice after Pending.
pub struct HardwarePort(
    /// Exclusive buffered UART and its interrupt-owned rings.
    BufferedUart,
);
impl HardwarePort {
    /// Consumes `resources` on core1 and returns an 8N1 port at configured BAUD
    /// without inversion or flow control. Panics on another core or repeated ring take.
    pub fn new(resources: Resources) -> Self {
        assert_eq!(embassy_rp::pac::SIO.cpuid().read(), 1, "UART must initialize on core1");
        let mut config = Config::default();
        config.baudrate = BAUD; // 8N1, no inversion or RTS/CTS
        Self(BufferedUart::new(
            resources.uart,
            resources.tx,
            resources.rx,
            Irqs,
            TX.take(),
            RX.take(),
            config,
        ))
    }
}

/// Returns the matching receive fault for `error`; unclassified HAL errors map to Other.
fn error(error: Error) -> IoError {
    match error {
        Error::Overrun => IoError::Overrun,
        Error::Framing => IoError::Framing,
        Error::Parity => IoError::Parity,
        Error::Break => IoError::Break,
        _ => IoError::Other,
    }
}

impl Port for HardwarePort {
    /// Polls with `cx` to fill a prefix of `bytes`; returns its length or the
    /// mapped UART failure. Pending retains only a waker and copies zero bytes.
    fn poll_read(
        &mut self,
        cx: &mut Context<'_>,
        bytes: &mut [u8],
    ) -> Poll<Result<usize, IoError>> {
        // Source-reviewed HAL: Pending has copied zero bytes; cancellation
        // retains only its task waker, never this local slice. Generic IO traits
        // alone do not guarantee this property. Never use read_exact/write_all.
        pin!(self.0.read(bytes)).poll(cx).map(|r| r.map_err(error))
    }

    /// Polls with `cx` to queue a prefix of `bytes`; returns accepted length or
    /// mapped UART failure. Pending retains no slice; Ready does not imply wire idle.
    fn poll_write(
        &mut self,
        cx: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<Result<usize, IoError>> {
        pin!(self.0.write(bytes)).poll(cx).map(|r| r.map_err(error))
    }

    /// Polls with `cx` until the software TX ring, hardware FIFO and shifter drain.
    /// Returns the mapped result; this future retains no caller-owned buffer.
    fn poll_flush(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), IoError>> {
        pin!(self.0.flush()).poll(cx).map(|r| r.map_err(error))
    }

    /// Returns the live UART BUSY flag, independent of software ring occupancy.
    fn busy(&self) -> bool { self.0.busy() }

    /// Returns the sticky RX error if present; observation does not clear it.
    fn rx_error(&self) -> Option<IoError> { self.0.rx_error().map(error) }

    /// Discards queued receive bytes and clears old receive faults with bounded
    /// FIFO work. Returns new faults; success is not protocol alignment, so the
    /// caller must invalidate transactions and perform its qualified handshake.
    fn discard_rx(&mut self) -> Result<(), IoError> { self.0.discard_rx().map_err(error) }
}
