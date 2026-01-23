//! Display driver for Pimoroni PIM715 Display Pack 2.8" (ST7789).
//!
//! Pin mapping for PIM715:
//! - CS: GPIO17
//! - DC: GPIO16
//! - CLK: GPIO18 (SPI0 CLK)
//! - MOSI: GPIO19 (SPI0 TX)
//! - Backlight: GPIO20
//! - Reset: Tied to RUN pin (resets with Pico)

use display_interface_spi::SPIInterface;
use embassy_rp::gpio::Output;
use embassy_rp::peripherals::SPI0;
use embassy_rp::spi::{Blocking, Config as SpiConfig, Spi};
use embedded_hal_bus::spi::ExclusiveDevice;
use mipidsi::models::ST7789;
use mipidsi::options::{ColorInversion, Orientation, Rotation};
use mipidsi::{Builder, NoResetPin};

/// Display type alias for the ST7789 on PIM715 (no reset pin).
pub type Pim715Display<'d> = mipidsi::Display<
    SPIInterface<ExclusiveDevice<Spi<'d, SPI0, Blocking>, Output<'d>, embedded_hal_bus::spi::NoDelay>, Output<'d>>,
    ST7789,
    NoResetPin,
>;

/// Initialize the PIM715 display.
///
/// Returns the initialized display ready for drawing.
pub fn init_display<'d>(
    spi: Spi<'d, SPI0, Blocking>,
    cs: Output<'d>,
    dc: Output<'d>,
) -> Pim715Display<'d> {
    // Create SPI device with chip select
    let spi_device = ExclusiveDevice::new_no_delay(spi, cs).unwrap();

    // Create display interface
    let di = SPIInterface::new(spi_device, dc);

    // Build the display driver
    // PIM715 2.8" display: ST7789V controller
    // Native panel is 240x320 (portrait), we rotate 90° for 320x240 (landscape)
    // No reset pin on PIM715 - it's tied to RUN pin
    Builder::new(ST7789, di)
        .display_size(240, 320) // Native panel size (before rotation)
        .orientation(Orientation::new().rotate(Rotation::Deg90))
        .invert_colors(ColorInversion::Inverted)
        .init(&mut embassy_time::Delay)
        .unwrap()
}

/// SPI configuration for the ST7789 display.
///
/// The ST7789 supports up to 62.5MHz SPI clock.
/// We use 40MHz for reliable operation.
pub fn display_spi_config() -> SpiConfig {
    let mut config = SpiConfig::default();
    config.frequency = 40_000_000; // 40MHz
    config
}
