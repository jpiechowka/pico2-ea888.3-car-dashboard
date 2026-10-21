//! Sole SPI0/DC/CS owner for the PIM715 ST7789 panel.
//!
//! Normal completion means the locked HAL observed SPI idle, not panel scanout.
//! Do not drop a live command/payload future: there is no cancellation guard to
//! release CS, and HAL DMA drop waits synchronously for abort. The application
//! times out its completion channel while this owner retains the buffer.
//! [Manufacturer driver](https://github.com/pimoroni/pimoroni-pico/blob/main/drivers/st7789/st7789.cpp).
use dashboard_core::display::pipeline::after_first_poll;
use dashboard_core::display::transfer::{PanelBus, madctl};
use dashboard_core::view_config::Orientation;
use embassy_rp::gpio::Output;
use embassy_rp::peripherals::SPI0;
use embassy_rp::spi::{Async, Error, Spi};
use embassy_time::Timer;

/// Software reset command; initialization waits 150 ms afterward.
const SWRESET: u8 = 0x01;
/// Exit-sleep command.
const SLPOUT: u8 = 0x11;
/// Select normal display mode.
const NORON: u8 = 0x13;
/// Enable the panel's required inversion mode.
const INVON: u8 = 0x21;
/// Enable display output after format/orientation setup.
const DISPON: u8 = 0x29;
/// Begin pixel writes to the previously configured address window.
const RAMWR: u8 = 0x2C;
/// Set row/column order and mounting rotation.
const MADCTL: u8 = 0x36;
/// Select the RGB565 interface pixel format.
const COLMOD: u8 = 0x3A;
/// Configure RAM interface and RGB565 expansion behavior.
const RAMCTRL: u8 = 0xB0;

/// Exclusive panel bus owner; a pixel transaction keeps CS low until `end_pixels`.
pub struct St7789Flusher<'d> {
    /// DMA-capable SPI0 transmitter retained across each awaited transfer.
    spi: Spi<'d, SPI0, Async>,
    /// Low for command bytes, high for parameters and pixel payload.
    dc: Output<'d>,
    /// Active-low chip select, explicitly restored on ordinary command errors.
    cs: Output<'d>,
}

impl<'d> St7789Flusher<'d> {
    /// Consumes `spi`, command/data `dc`, and active-low `cs` pins. Returns an
    /// uninitialized bus owner; caller supplies the peripheral clock and pin setup.
    pub fn new(
        spi: Spi<'d, SPI0, Async>,
        dc: Output<'d>,
        cs: Output<'d>,
    ) -> Self {
        Self { spi, dc, cs }
    }

    /// Resets and configures RGB565, landscape and display-on; enables vertical
    /// TE only with `te-trace`. Returns the first SPI error with partial panel
    /// configuration possible. Run to completion before spawning the flush owner.
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
    /// SPI transport failure propagated by commands and payload transfers.
    type Error = Error;

    /// Sends `command` with DC low, then `parameters` with DC high. Returns the
    /// first SPI error or success, restoring CS high on either normal return.
    /// Dropping this future mid-transfer does not run that restoration.
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

    /// Sends RAMWR and leaves CS low/DC high on success for following pixel bytes.
    /// Returns a SPI error with CS restored high when the command fails.
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

    /// Writes wire-order RGB565 `bytes`, retaining their borrow until HAL return.
    /// Invokes `on_started`, when present, after the payload future's first poll;
    /// this observes software arming, not a measured first SCK edge. Returns the
    /// HAL result and leaves CS asserted for `end_pixels`.
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

    /// Releases CS after the caller has awaited all pixel writes; does not wait
    /// for DMA or repair a canceled transfer on its own.
    fn end_pixels(&mut self) { self.cs.set_high(); }
}
