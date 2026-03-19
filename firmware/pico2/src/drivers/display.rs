use embassy_rp::spi::Config as SpiConfig;

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

pub fn get_actual_spi_freq(sys_clk_hz: u32) -> u32 {
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

        sys_clk_hz / (cpsdvsr * (scr + 1))
    }
}
