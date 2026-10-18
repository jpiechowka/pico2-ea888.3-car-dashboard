use dashboard_core::display::pipeline::after_first_poll;
use dashboard_core::display::transfer::{PanelBus, madctl};
use dashboard_core::view_config::Orientation;
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
const RAMCTRL: u8 = 0xB0;

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
        // EPF=00: do not inject green's low bit into red/blue when expanding
        // RGB565. MCU mode and MSB-first byte order are preserved (Pimoroni #1040).
        self.command(RAMCTRL, &[0x00, 0xC0]).await?;

        self.command(MADCTL, &[madctl(Orientation::Landscape)]).await?;

        self.command(INVON, &[]).await?;
        Timer::after_millis(10).await;

        self.command(NORON, &[]).await?;
        Timer::after_millis(10).await;

        self.command(DISPON, &[]).await?;
        Timer::after_millis(10).await;

        // TEM=0 (datasheet Mode 1): vertical blank only. SWRESET above ensures
        // TE was disabled. Normal builds leave TE off; FRCTRL2 is unchanged.
        #[cfg(feature = "te-trace")]
        self.command(0x35, &[0x00]).await?;

        Ok(())
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
