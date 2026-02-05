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
///
/// Frequency depends on feature flags:
/// - `cpu300-spi75-1v30`: 75 MHz (300 MHz core / 4, embassy-rp max)
/// - `cpu290-spi72-1v30`: 72.5 MHz (290 MHz core / 4)
/// - `cpu280-spi70-1v30`: 70 MHz (280 MHz core / 4, beyond ST7789 datasheet)
/// - `cpu250-spi62-1v10`: 62.5 MHz (ST7789 datasheet maximum)
/// - Default: 62.5 MHz (stock 150 MHz core)
pub fn display_spi_config() -> SpiConfig {
    let mut config = SpiConfig::default();

    #[cfg(feature = "cpu300-spi75-1v30")]
    {
        config.frequency = 75_000_000;
    }
    #[cfg(all(feature = "cpu290-spi72-1v30", not(feature = "cpu300-spi75-1v30")))]
    {
        config.frequency = 72_500_000;
    }
    #[cfg(all(
        feature = "cpu280-spi70-1v30",
        not(any(feature = "cpu290-spi72-1v30", feature = "cpu300-spi75-1v30"))
    ))]
    {
        config.frequency = 70_000_000;
    }
    #[cfg(not(any(
        feature = "cpu280-spi70-1v30",
        feature = "cpu290-spi72-1v30",
        feature = "cpu300-spi75-1v30"
    )))]
    {
        config.frequency = 62_500_000;
    }

    config
}

/// Read actual SPI0 clock frequency from hardware registers.
///
/// The actual SPI clock may differ from the requested frequency due to
/// divider constraints. This reads the CPSDVSR (prescale) and SCR (serial
/// clock rate) from the SPI0 peripheral registers to calculate the true clock.
///
/// # Arguments
/// * `sys_clk_hz` - System clock frequency in Hz (peripheral clock = sys clock on RP2350)
///
/// # Returns
/// Actual SPI clock frequency in Hz, or 0 if registers indicate invalid state.
///
/// # Formula
/// `actual_freq = peri_clk / (CPSDVSR * (1 + SCR))`
#[cfg(target_arch = "arm")]
pub fn get_actual_spi_freq(sys_clk_hz: u32) -> u32 {
    // SPI0 register addresses (RP2350)
    const SPI0_BASE: u32 = 0x4008_0000;
    const SSPCR0_OFFSET: u32 = 0x00; // Control register 0 (SCR in bits 15:8)
    const SSPCPSR_OFFSET: u32 = 0x10; // Clock prescale register (CPSDVSR in bits 7:0)

    // SAFETY: Reading hardware registers, SPI0 is initialized before this is called
    unsafe {
        let cr0 = core::ptr::read_volatile((SPI0_BASE + SSPCR0_OFFSET) as *const u32);
        let cpsr = core::ptr::read_volatile((SPI0_BASE + SSPCPSR_OFFSET) as *const u32);

        let scr = (cr0 >> 8) & 0xFF; // Serial Clock Rate (bits 15:8)
        let cpsdvsr = cpsr & 0xFF; // Clock Prescale Divisor (bits 7:0)

        // Avoid division by zero
        if cpsdvsr == 0 {
            return 0;
        }

        // actual_freq = peri_clk / (CPSDVSR * (1 + SCR))
        sys_clk_hz / (cpsdvsr * (scr + 1))
    }
}

/// Placeholder for non-ARM targets (tests).
#[cfg(not(target_arch = "arm"))]
pub fn get_actual_spi_freq(_sys_clk_hz: u32) -> u32 { 0 }
