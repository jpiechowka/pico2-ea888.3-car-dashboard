//! Settings adapter over the shared RP2350 flash safety boundary.
pub use dashboard_pico2::flash_guard::Error;
use dashboard_pico2::flash_guard::{CheckedFlash, Region, WriteConditions};
use dashboard_pico2::maintenance::Request;
use dashboard_pico2::settings::region::Range;
use dashboard_pico2::settings::{self, Settings, Storage};
use embassy_rp::Peri;
use embassy_rp::peripherals::FLASH;
pub type JournalError = settings::Error<Error>;
pub struct SettingsFlash {
    flash: CheckedFlash,
    permit: Option<Request>,
}
impl SettingsFlash {
    pub fn new(peripheral: Peri<'static, FLASH>) -> Self {
        Self {
            flash: CheckedFlash::new(peripheral, Region::Settings),
            permit: None,
        }
    }

    pub fn load(&mut self) -> Result<Option<Settings>, JournalError> {
        let id = self.flash.qualify().map_err(settings::Error::Storage)?;
        crate::log_info!("Flash JEDEC: {:06x}", id);
        let scan = settings::load(self)?;
        if scan.invalid_records != 0 {
            crate::log_info!("Settings torn slots: {}", scan.invalid_records);
        }
        Ok(scan.latest.map(|record| record.settings))
    }

    fn conditions(&self) -> WriteConditions {
        WriteConditions {
            maintenance_valid: self.permit.is_some_and(crate::tasks::maintenance::writing),
            display_idle: !crate::tasks::recovery::DISPLAY_PENDING.load(core::sync::atomic::Ordering::Relaxed),
            radio_idle: {
                #[cfg(feature = "ble-obd")]
                {
                    crate::tasks::ble::bus_idle()
                }
                #[cfg(not(feature = "ble-obd"))]
                {
                    true
                }
            },
        }
    }

    pub fn save(
        &mut self,
        values: Settings,
        permit: Request,
    ) -> Result<(), JournalError> {
        if self.permit.is_some() {
            return Err(settings::Error::Storage(Error::Permit));
        }
        self.permit = Some(permit);
        let result = settings::save(self, values).map(|_| ());
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
        if self
            .permit
            .is_some_and(|request| !crate::tasks::maintenance::writing(request))
        {
            return Err(Error::Permit);
        }
        self.flash.read(range.start(), out)
    }

    fn program(
        &mut self,
        offset: u32,
        page: &[u8; settings::PAGE_SIZE],
    ) -> Result<(), Error> {
        let range = Range::program(offset).ok_or(Error::Range)?;
        self.flash.program(range.start(), page, self.conditions())
    }

    fn erase(
        &mut self,
        offset: u32,
    ) -> Result<(), Error> {
        let range = Range::erase(offset).ok_or(Error::Range)?;
        self.flash.erase(range.start(), self.conditions())
    }
}
