//! Exclusive core0 flash owner. Every program/erase requires the live park
//! barrier, checked journal range, identity mapping and idle hardware owners.
use dashboard_pico2::maintenance::Request;
use dashboard_pico2::settings::region::Range;
use dashboard_pico2::settings::{self, Settings, Storage};
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
pub type JournalError = settings::Error<Error>;

pub struct SettingsFlash {
    flash: Flash<'static, FLASH, Blocking, 0x20_0000>,
    qualified: bool,
    permit: Option<Request>,
}
impl SettingsFlash {
    pub fn new(peripheral: Peri<'static, FLASH>) -> Self {
        Self {
            flash: Flash::new_blocking(peripheral),
            qualified: false,
            permit: None,
        }
    }

    /// Before core1/peripheral startup, with the hardware watchdog already armed.
    pub fn load(&mut self) -> Result<Option<Settings>, JournalError> {
        self.qualify().map_err(settings::Error::Storage)?;
        let scan = settings::load(self)?;
        if scan.invalid_records != 0 {
            crate::log_info!("Settings torn slots: {}", scan.invalid_records);
        }
        Ok(scan.latest.map(|record| record.settings))
    }

    fn qualify(&mut self) -> Result<(), Error> {
        // Production image admission covers optimized release artifacts.
        // Debug firmware remains usable with volatile defaults, without XIP-off.
        if cfg!(debug_assertions) {
            return Err(Error::DebugBuild);
        }
        Self::context()?;
        Self::configuration()?;
        Self::mapping()?;
        if !pac::WATCHDOG.ctrl().read().enable() || !pac::TICKS.watchdog_ctrl().read().running() {
            return Err(Error::Context);
        }
        if (0..16).any(|n| pac::DMA.ch(n).ctrl_trig().read().busy()) {
            return Err(Error::Busy);
        }
        let id = self.flash.blocking_jedec_id()?;
        crate::log_info!("Flash JEDEC: {:06x}", id);
        // W25Q32RV-Q tuple; not proof of the board revision or physical timing.
        if id != 0xEF4016 {
            return Err(Error::Identity(id));
        }
        Self::mapping()?;
        self.qualified = true;
        Ok(())
    }

    fn context() -> Result<(), Error> {
        if pac::SIO.cpuid().read() != 0 || !crate::tasks::maintenance::interrupt_ready() {
            return Err(Error::Context);
        }
        Ok(())
    }

    fn configuration() -> Result<(), Error> {
        if pac::PADS_QSPI.voltage_select().read().voltage_select() != pac::pads::vals::VoltageSelect::_3V3
            // IO_QSPI entries 0/1 are USB. The six QSPI mux/override registers
            // at entries 2..7 must have normal function zero and no overrides.
            || (2..8).any(|i| pac::IO_QSPI.gpio(i).ctrl().read().0 != 0)
        {
            return Err(Error::Configuration);
        }
        let mut words = [0u32; 2];
        // The ROM returns configured BOOTRAM metadata, NOT detected capacity.
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

    fn mapping() -> Result<(), Error> {
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
        // This ROM API returns absolute storage addresses, including XIP_BASE.
        for address in [0x101FE000, 0x101FFFFF] {
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
        Self::mapping()?;
        if self
            .permit
            .is_some_and(|request| !crate::tasks::maintenance::writing(request))
        {
            return Err(Error::Permit);
        }
        Ok(())
    }

    fn check_write(&self) -> Result<(), Error> {
        self.check_read()?;
        let permit = self.permit.ok_or(Error::Permit)?;
        if !crate::tasks::maintenance::writing(permit) {
            return Err(Error::Permit);
        }
        Self::configuration()?;
        #[cfg(feature = "ble-obd")]
        if !crate::tasks::ble::bus_idle() {
            return Err(Error::Busy);
        }
        let spi = pac::SPI0.sr().read();
        if crate::tasks::recovery::DISPLAY_PENDING.load(core::sync::atomic::Ordering::Relaxed)
            || (0..16).any(|n| pac::DMA.ch(n).ctrl_trig().read().busy())
            || pac::DMA.ch(0).ctrl_trig().read().chain_to() != 0
            || spi.bsy()
            || !spi.tfe()
            || !crate::tasks::maintenance::i2c_idle()
            || pac::SIO.gpio_out(0).value().read() & (1 << 17) == 0
        {
            return Err(Error::Busy);
        }
        Ok(())
    }

    /// Called synchronously only while main retains both completed frame leases.
    pub fn save(
        &mut self,
        values: Settings,
        permit: Request,
    ) -> Result<(), JournalError> {
        if self.permit.is_some() {
            return Err(settings::Error::Storage(Error::Permit));
        }
        self.permit = Some(permit);
        let result = self
            .check_write()
            .map_err(settings::Error::Storage)
            .and_then(|()| settings::save(self, values).map(|_| ()));
        self.permit = None;
        result
    }
}
impl Storage for SettingsFlash {
    type Error = Error;

    fn read(
        &mut self,
        offset: u32,
        out: &mut [u8],
    ) -> Result<(), Error> {
        let range = Range::read(offset, out.len()).ok_or(Error::Range)?;
        self.check_read()?;
        self.flash.blocking_read(range.start(), out).map_err(Error::from)
    }

    fn program(
        &mut self,
        offset: u32,
        page: &[u8; settings::PAGE_SIZE],
    ) -> Result<(), Error> {
        let range = Range::program(offset).ok_or(Error::Range)?;
        self.check_write()?;
        let start = page.as_ptr() as usize;
        if !(0x20000000..0x20080000).contains(&start)
            || start.checked_add(page.len()).is_none_or(|end| end > 0x20080000)
        {
            return Err(Error::RamBuffer);
        }
        self.flash.blocking_write(range.start(), page).map_err(Error::from)
    }

    fn erase(
        &mut self,
        offset: u32,
    ) -> Result<(), Error> {
        let range = Range::erase(offset).ok_or(Error::Range)?;
        self.check_write()?;
        self.flash
            .blocking_erase(range.start(), range.end())
            .map_err(Error::from)
    }
}
