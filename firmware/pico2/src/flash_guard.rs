//! Shared RP2350 flash preconditions. Callers own park/resume and region policy.
use embassy_rp::flash::{Blocking, Flash};
use embassy_rp::peripherals::FLASH;
use embassy_rp::{Peri, pac, rom_data};

#[derive(Debug)]
pub enum Error {
    DebugBuild,
    Context,
    Configuration,
    Mapping,
    Identity(u32),
    Unqualified,
    Busy,
    Range,
    RamBuffer,
    Permit,
    Hardware(embassy_rp::flash::Error),
}
impl From<embassy_rp::flash::Error> for Error {
    fn from(error: embassy_rp::flash::Error) -> Self { Self::Hardware(error) }
}

#[derive(Clone, Copy)]
pub enum Region {
    Settings,
    Captures,
}
impl Region {
    fn bounds(self) -> (u32, u32) {
        match self {
            Self::Settings => (0x1FE000, 0x200000),
            Self::Captures => (dashboard_core::discovery::capture_store::FLASH_OFFSET, 0x1FE000),
        }
    }

    fn admits(
        self,
        offset: u32,
        length: usize,
    ) -> bool {
        let (start, end) = self.bounds();
        length != 0
            && offset >= start
            && u32::try_from(length)
                .ok()
                .and_then(|len| offset.checked_add(len))
                .is_some_and(|limit| limit <= end)
    }
}

pub fn interrupt_ready() -> bool {
    use cortex_m::peripheral::SCB;
    use cortex_m::peripheral::scb::VectActive;
    use cortex_m::register::{basepri, faultmask, primask};
    SCB::vect_active() == VectActive::ThreadMode
        && primask::read().is_active()
        && faultmask::read().is_active()
        && basepri::read() == 0
}
pub fn core1_ready() -> bool {
    pac::SIO.cpuid().read() == 1
        && interrupt_ready()
        && cortex_m::peripheral::NVIC::is_enabled(pac::Interrupt::SIO_IRQ_FIFO)
}
pub fn i2c_idle() -> bool {
    let status = pac::I2C0.ic_status().read();
    !status.activity() && !status.mst_activity() && status.tfe() && !status.rfne()
}

pub struct WriteConditions {
    pub maintenance_valid: bool,
    pub display_idle: bool,
    pub radio_idle: bool,
}
pub struct CheckedFlash {
    flash: Flash<'static, FLASH, Blocking, 0x20_0000>,
    region: Region,
    qualified: bool,
}
impl CheckedFlash {
    pub fn new(
        peripheral: Peri<'static, FLASH>,
        region: Region,
    ) -> Self {
        Self {
            flash: Flash::new_blocking(peripheral),
            region,
            qualified: false,
        }
    }

    /// Before core1/peripheral startup, with the watchdog armed.
    pub fn qualify(&mut self) -> Result<u32, Error> {
        if cfg!(debug_assertions) {
            return Err(Error::DebugBuild);
        }
        Self::context()?;
        Self::configuration()?;
        self.mapping()?;
        if !pac::WATCHDOG.ctrl().read().enable() || !pac::TICKS.watchdog_ctrl().read().running() {
            return Err(Error::Context);
        }
        if (0..16).any(|n| pac::DMA.ch(n).ctrl_trig().read().busy()) {
            return Err(Error::Busy);
        }
        let id = self.flash.blocking_jedec_id()?;
        if id != 0xEF4016 {
            return Err(Error::Identity(id));
        }
        self.mapping()?;
        self.qualified = true;
        Ok(id)
    }

    fn context() -> Result<(), Error> {
        if pac::SIO.cpuid().read() != 0 || !interrupt_ready() {
            return Err(Error::Context);
        }
        Ok(())
    }

    fn configuration() -> Result<(), Error> {
        if pac::PADS_QSPI.voltage_select().read().voltage_select() != pac::pads::vals::VoltageSelect::_3V3
            || (2..8).any(|i| pac::IO_QSPI.gpio(i).ctrl().read().0 != 0)
        {
            return Err(Error::Configuration);
        }
        let mut words = [0u32; 2];
        // Configured BOOTRAM metadata, not a measurement of flash capacity.
        let count = unsafe { rom_data::get_sys_info(words.as_mut_ptr(), words.len(), 8) };
        if count != 2 || words[0] != 8 {
            return Err(Error::Configuration);
        }
        let info = pac::otp_data::regs::FlashDevinfo(words[1] as u16);
        if info.cs1_size().to_bits() != 0 || !(9..=12).contains(&info.cs0_size().to_bits()) {
            return Err(Error::Configuration);
        }
        Ok(())
    }

    fn mapping(&self) -> Result<(), Error> {
        let direct = pac::QMI.direct_csr().read();
        let map = pac::QMI.atrans(0).read();
        if direct.en()
            || direct.assert_cs0n()
            || direct.assert_cs1n()
            || direct.auto_cs0n()
            || direct.auto_cs1n()
            || !direct.txempty()
            || !direct.rxempty()
            || map.base() != 0
            || map.size() < 0x200
            || pac::XIP_CTRL.stream_ctr().read().0 != 0
        {
            return Err(Error::Mapping);
        }
        let (start, end) = self.region.bounds();
        for address in [0x10000000 + start, 0x10000000 + end - 1] {
            if unsafe { rom_data::flash_runtime_to_storage_addr(address) } != address as i32 {
                return Err(Error::Mapping);
            }
        }
        Ok(())
    }

    fn check_read(&self) -> Result<(), Error> {
        if !self.qualified {
            return Err(Error::Unqualified);
        }
        Self::context()?;
        self.mapping()
    }

    fn check_write(
        &self,
        conditions: WriteConditions,
    ) -> Result<(), Error> {
        self.check_read()?;
        if !conditions.maintenance_valid {
            return Err(Error::Permit);
        }
        Self::configuration()?;
        let spi = pac::SPI0.sr().read();
        if !conditions.display_idle
            || !conditions.radio_idle
            || (0..16).any(|n| pac::DMA.ch(n).ctrl_trig().read().busy())
            || pac::DMA.ch(0).ctrl_trig().read().chain_to() != 0
            || spi.bsy()
            || !spi.tfe()
            || !i2c_idle()
            || pac::SIO.gpio_out(0).value().read() & (1 << 17) == 0
        {
            return Err(Error::Busy);
        }
        Ok(())
    }

    pub fn read(
        &mut self,
        offset: u32,
        bytes: &mut [u8],
    ) -> Result<(), Error> {
        if !self.region.admits(offset, bytes.len()) {
            return Err(Error::Range);
        }
        self.check_read()?;
        self.flash.blocking_read(offset, bytes).map_err(Error::from)
    }

    pub fn program(
        &mut self,
        offset: u32,
        bytes: &[u8; 256],
        conditions: WriteConditions,
    ) -> Result<(), Error> {
        if offset % 256 != 0 || !self.region.admits(offset, bytes.len()) {
            return Err(Error::Range);
        }
        self.check_write(conditions)?;
        let start = bytes.as_ptr() as usize;
        if !(0x20000000..0x20080000).contains(&start)
            || start.checked_add(bytes.len()).is_none_or(|end| end > 0x20080000)
        {
            return Err(Error::RamBuffer);
        }
        self.flash.blocking_write(offset, bytes).map_err(Error::from)
    }

    pub fn erase(
        &mut self,
        offset: u32,
        conditions: WriteConditions,
    ) -> Result<(), Error> {
        if offset % 4096 != 0 || !self.region.admits(offset, 4096) {
            return Err(Error::Range);
        }
        self.check_write(conditions)?;
        self.flash.blocking_erase(offset, offset + 4096).map_err(Error::from)
    }
}
