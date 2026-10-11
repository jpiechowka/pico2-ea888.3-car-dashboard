use dashboard_pico2::display::pipeline::after_first_poll;
use dashboard_pico2::display::{FrameBuffer, HEIGHT, WIDTH};
use embassy_rp::gpio::Output;
use embassy_rp::peripherals::SPI0;
use embassy_rp::spi::{Async, Error, Spi};
use embassy_time::Timer;

const SWRESET: u8 = 0x01;
const SLPOUT: u8 = 0x11;
const NORON: u8 = 0x13;
const INVON: u8 = 0x21;
const DISPON: u8 = 0x29;
const CASET: u8 = 0x2A;
const RASET: u8 = 0x2B;
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
        self.write_command(SWRESET).await?;
        Timer::after_millis(150).await;

        self.write_command(SLPOUT).await?;
        Timer::after_millis(10).await;

        self.write_command(COLMOD).await?;
        self.write_data(&[0x55]).await?;

        self.write_command(MADCTL).await?;
        self.write_data(&[MADCTL_MV | MADCTL_MX]).await?;

        self.write_command(INVON).await?;
        Timer::after_millis(10).await;

        self.write_command(NORON).await?;
        Timer::after_millis(10).await;

        self.write_command(DISPON).await?;
        Timer::after_millis(10).await;

        self.set_full_window().await
    }

    async fn write_command(
        &mut self,
        cmd: u8,
    ) -> Result<(), Error> {
        self.cs.set_low();
        self.dc.set_low();
        let result = self.spi.write(&[cmd]).await;
        self.cs.set_high();
        result
    }

    async fn write_data(
        &mut self,
        data: &[u8],
    ) -> Result<(), Error> {
        self.cs.set_low();
        self.dc.set_high();
        let result = self.spi.write(data).await;
        self.cs.set_high();
        result
    }

    async fn set_full_window(&mut self) -> Result<(), Error> {
        let x1 = (WIDTH - 1) as u16;
        let y1 = (HEIGHT - 1) as u16;
        self.write_command(CASET).await?;
        self.write_data(&[0, 0, (x1 >> 8) as u8, x1 as u8]).await?;
        self.write_command(RASET).await?;
        self.write_data(&[0, 0, (y1 >> 8) as u8, y1 as u8]).await
    }

    /// The callback follows the first poll of the payload write, which arms DMA
    /// in the locked RP HAL. Completion includes its SPI BSY wait before CS rises.
    /// Do not wrap this in a cancelling timeout and return the buffer to rendering.
    pub async fn flush_buffer(
        &mut self,
        buffer: &FrameBuffer,
        on_started: impl FnOnce(),
    ) -> Result<(), Error> {
        self.cs.set_low();
        self.dc.set_low();
        if let Err(error) = self.spi.write(&[RAMWR]).await {
            self.cs.set_high();
            return Err(error);
        }
        self.dc.set_high();
        let result = after_first_poll(self.spi.write(&buffer[..]), on_started).await;
        self.cs.set_high();
        result
    }
}
