//! Display driver for Pimoroni PIM715 Display Pack 2.8" (ST7789).
//!
//! Pin mapping for PIM715:
//! - DC: GPIO16
//! - CS: GPIO17 (directly to SPI peripheral)
//! - CLK: GPIO18 (SPI0 CLK)
//! - MOSI: GPIO19 (SPI0 TX)
//! - Backlight: GPIO20
//! - Reset: Tied to RUN pin (resets with Pico)

use embassy_rp::gpio::Output;
use embassy_rp::peripherals::SPI0;
use embassy_rp::spi::{Async, Config as SpiConfig, Spi};

use crate::st7789::St7789;

/// Display type alias for the ST7789 on PIM715.
pub type Pim715Display<'d> = St7789<'d>;

/// Initialize the PIM715 display with async SPI (DMA-enabled).
pub async fn init_display<'d>(
    spi: Spi<'d, SPI0, Async>,
    dc: Output<'d>,
    cs: Output<'d>,
) -> Pim715Display<'d> {
    let mut display = St7789::new(spi, dc, cs);
    display.init().await;
    display
}

/// SPI configuration for the ST7789 display.
/// The ST7789 supports up to 62.5MHz SPI clock.
pub fn display_spi_config() -> SpiConfig {
    let mut config = SpiConfig::default();
    config.frequency = 62_500_000;
    config
}
