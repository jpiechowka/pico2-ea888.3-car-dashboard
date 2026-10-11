use dashboard_pico2::display::FrameBuffer;
use dashboard_pico2::display::damage::DamagePlan;
use dashboard_pico2::display::pipeline::after_first_poll;
use dashboard_pico2::display::transfer::{PanelBus, flush_plan};
use embassy_rp::gpio::Output;
use embassy_rp::peripherals::SPI0;
use embassy_rp::spi::{Async, Error, Spi};
use embassy_time::Timer;

const SWRESET: u8 = 0x01;
const SLPOUT: u8 = 0x11;
const NORON: u8 = 0x13;
const INVON: u8 = 0x21;
const DISPON: u8 = 0x29;
const RAMWR: u8 = 0x2C;
const MADCTL: u8 = 0x36;
const COLMOD: u8 = 0x3A;

const MADCTL_MX: u8 = 0x40;
const MADCTL_MV: u8 = 0x20;

pub struct St7789Flusher<'d> {
    spi: Spi<'d, SPI0, Async>,
    dc: Output<'d>,
    cs: Output<'d>,
}

impl<'d> St7789Flusher<'d> {
    pub fn new(
        spi: Spi<'d, SPI0, Async>,
        dc: Output<'d>,
        cs: Output<'d>,
    ) -> Self {
        Self { spi, dc, cs }
    }

    pub async fn init(&mut self) -> Result<(), Error> {
        self.command(SWRESET, &[]).await?;
        Timer::after_millis(150).await;

        self.command(SLPOUT, &[]).await?;
        Timer::after_millis(10).await;

        self.command(COLMOD, &[0x55]).await?;

        self.command(MADCTL, &[MADCTL_MV | MADCTL_MX]).await?;

        self.command(INVON, &[]).await?;
        Timer::after_millis(10).await;

        self.command(NORON, &[]).await?;
        Timer::after_millis(10).await;

        self.command(DISPON, &[]).await?;
        Timer::after_millis(10).await;

        Ok(())
    }

    /// The callback follows the first poll of the payload write, which arms DMA
    /// in the locked RP HAL. Completion includes its SPI BSY wait before CS rises.
    /// Do not wrap this in a cancelling timeout and return the buffer to rendering.
    pub async fn flush_buffer(
        &mut self,
        buffer: &FrameBuffer,
        on_started: impl FnOnce(),
    ) -> Result<(), Error> {
        flush_plan(self, buffer, &DamagePlan::full(), on_started).await
    }
}

impl PanelBus for St7789Flusher<'_> {
    type Error = Error;

    async fn command(
        &mut self,
        command: u8,
        parameters: &[u8],
    ) -> Result<(), Error> {
        self.cs.set_low();
        self.dc.set_low();
        let result = async {
            self.spi.write(&[command]).await?;
            self.dc.set_high();
            if !parameters.is_empty() {
                self.spi.write(parameters).await?;
            }
            Ok(())
        }
        .await;
        self.cs.set_high();
        result
    }

    async fn begin_pixels(&mut self) -> Result<(), Error> {
        self.cs.set_low();
        self.dc.set_low();
        if let Err(error) = self.spi.write(&[RAMWR]).await {
            self.cs.set_high();
            return Err(error);
        }
        self.dc.set_high();
        Ok(())
    }

    async fn pixels<F: FnOnce()>(
        &mut self,
        bytes: &[u8],
        on_started: Option<F>,
    ) -> Result<(), Error> {
        after_first_poll(self.spi.write(bytes), || {
            if let Some(callback) = on_started {
                callback();
            }
        })
        .await
    }

    fn end_pixels(&mut self) { self.cs.set_high(); }
}
