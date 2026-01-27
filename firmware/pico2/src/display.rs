//! Display configuration for Pimoroni PIM715 Display Pack 2.8" (ST7789).
//!
//! Pin mapping for PIM715:
//! - DC: GPIO16
//! - CS: GPIO17 (directly to SPI peripheral)
//! - CLK: GPIO18 (SPI0 CLK)
//! - MOSI: GPIO19 (SPI0 TX)
//! - Backlight: GPIO20
//! - Reset: Tied to RUN pin (resets with Pico)

use embassy_rp::spi::Config as SpiConfig;

/// SPI configuration for the ST7789 display.
/// The ST7789 supports up to 62.5MHz SPI clock.
pub fn display_spi_config() -> SpiConfig {
    let mut config = SpiConfig::default();
    config.frequency = 62_500_000;
    config
}
