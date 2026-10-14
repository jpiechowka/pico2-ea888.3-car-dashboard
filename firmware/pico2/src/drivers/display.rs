use embassy_rp::spi::Config as SpiConfig;

pub fn display_spi_config() -> SpiConfig {
    let mut config = SpiConfig::default();

    config.frequency = dashboard_pico2::clock_profile::REQUESTED_SPI_HZ;

    config
}

/// Divider-derived SPI rate, using clk_peri as specified by the HAL.
/// This is not a physical clock measurement.
pub fn get_actual_spi_freq(peripheral_clk_hz: u32) -> u32 {
    const SPI0_BASE: u32 = 0x4008_0000;
    const SSPCR0_OFFSET: u32 = 0x00;
    const SSPCPSR_OFFSET: u32 = 0x10;

    unsafe {
        let cr0 = core::ptr::read_volatile((SPI0_BASE + SSPCR0_OFFSET) as *const u32);
        let cpsr = core::ptr::read_volatile((SPI0_BASE + SSPCPSR_OFFSET) as *const u32);

        let scr = (cr0 >> 8) & 0xFF;
        let cpsdvsr = cpsr & 0xFF;

        if cpsdvsr == 0 {
            return 0;
        }

        peripheral_clk_hz / (cpsdvsr * (scr + 1))
    }
}
