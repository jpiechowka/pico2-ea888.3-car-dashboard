//! SPI setup and register-derived diagnostic frequency for the display owner.
use embassy_rp::spi::Config as SpiConfig;

/// Returns default HAL SPI mode/word settings with the selected build profile's
/// requested frequency in hertz; HAL divisors determine the achievable frequency.
pub fn display_spi_config() -> SpiConfig {
    let mut config = SpiConfig::default();

    config.frequency = dashboard_pico2::clock_profile::REQUESTED_SPI_HZ;

    config
}

/// Returns the divider-derived SPI0 rate in hertz from `peripheral_clk_hz`
/// (`clk_peri`), or `None` for unavailable clocks or invalid register divisors.
/// Reads RP2350 SPI0 directly; this is not a physical clock measurement.
pub fn get_actual_spi_freq(peripheral_clk_hz: u32) -> Option<u32> {
    /// RP2350 SPI0 register base, not the RP2040 SPI address.
    const SPI0_BASE: u32 = 0x4008_0000;
    /// Control register containing the serial clock rate divisor field.
    const SSPCR0_OFFSET: u32 = 0x00;
    /// Clock prescale register containing the even prescaler.
    const SSPCPSR_OFFSET: u32 = 0x10;

    unsafe {
        let cr0 = core::ptr::read_volatile((SPI0_BASE + SSPCR0_OFFSET) as *const u32);
        let cpsr = core::ptr::read_volatile((SPI0_BASE + SSPCPSR_OFFSET) as *const u32);

        dashboard_pico2::diagnostic_values::spi_clock_hz(peripheral_clk_hz, cpsr as u8, (cr0 >> 8) as u8)
    }
}
