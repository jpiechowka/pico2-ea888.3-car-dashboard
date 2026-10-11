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

embassy_rp::bind_interrupts!(struct Irqs {
    UART0_IRQ => BufferedInterruptHandler<UART0>;
});

static RX: ConstStaticCell<[u8; RX_BYTES]> = ConstStaticCell::new([0; RX_BYTES]);
static TX: ConstStaticCell<[u8; TX_BYTES]> = ConstStaticCell::new([0; TX_BYTES]);

/// Only uninitialized peripheral tokens cross cores. The driver never does.
pub struct Resources {
    pub uart: Peri<'static, UART0>,
    pub tx: Peri<'static, PIN_0>,
    pub rx: Peri<'static, PIN_1>,
}

pub struct HardwarePort(BufferedUart);
impl HardwarePort {
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

    fn poll_write(
        &mut self,
        cx: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<Result<usize, IoError>> {
        pin!(self.0.write(bytes)).poll(cx).map(|r| r.map_err(error))
    }

    fn poll_flush(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), IoError>> {
        pin!(self.0.flush()).poll(cx).map(|r| r.map_err(error))
    }

    fn busy(&self) -> bool { self.0.busy() }

    fn rx_error(&self) -> Option<IoError> { self.0.rx_error().map(error) }

    fn discard_rx(&mut self) -> Result<(), IoError> { self.0.discard_rx().map_err(error) }
}
