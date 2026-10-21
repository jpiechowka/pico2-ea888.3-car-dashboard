//! Settings adapter over the shared RP2350 flash safety boundary.
pub use dashboard_pico2::flash_guard::Error;
use dashboard_pico2::flash_guard::{CheckedFlash, Region, WriteConditions};
use dashboard_pico2::maintenance::Request;
use dashboard_pico2::settings::region::Range;
use dashboard_pico2::settings::{self, Settings, Storage};
use embassy_rp::Peri;
use embassy_rp::peripherals::FLASH;
/// Journal validation/generation errors or shared flash-guard failures.
pub type JournalError = settings::Error<Error>;
/// Core0 settings journal adapter; one request-scoped permission admits mutations.
pub struct SettingsFlash {
    /// Exclusive handle fixed to the two reserved settings sectors.
    flash: CheckedFlash,
    /// Active save request; absent outside synchronous journal execution.
    permit: Option<Request>,
}
impl SettingsFlash {
    /// Consumes `peripheral` and returns an unqualified settings-only owner with
    /// no active write permission; does not access flash.
    pub fn new(peripheral: Peri<'static, FLASH>) -> Self {
        Self {
            flash: CheckedFlash::new(peripheral, Region::Settings),
            permit: None,
        }
    }

    /// Qualifies hardware at boot and scans the journal. Returns latest validated
    /// settings, `None` if no valid record exists, or a hardware/schema/journal
    /// error. Logs JEDEC identity and torn-slot count; never repairs storage.
    pub fn load(&mut self) -> Result<Option<Settings>, JournalError> {
        let id = self.flash.qualify().map_err(settings::Error::Storage)?;
        crate::log_info!("Flash JEDEC: {:06x}", id);
        let scan = settings::load(self)?;
        if scan.invalid_records != 0 {
            crate::log_info!("Settings torn slots: {}", scan.invalid_records);
        }
        Ok(scan.latest.map(|record| record.settings))
    }

    /// Returns current permission/display/radio observations for the flash guard;
    /// checking the request can latch a shared maintenance deadline failure.
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

    /// Saves immutable `values` under `permit`, returning the journal result after
    /// clearing this adapter's permission. Rejects a nested save before mutation.
    /// The caller must retain the owner barrier and watchdog ticket throughout.
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
    /// Physical admission, context, and HAL failures preserved by the journal.
    type Error = Error;

    /// Reads into `out` at settings-relative byte `offset`. Returns range or
    /// expired-permit errors before flash access; boot reads need no save permit.
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

    /// Programs `page` at aligned settings-relative `offset`. Returns range or
    /// live flash/maintenance admission errors; commit/readback belongs to the journal.
    fn program(
        &mut self,
        offset: u32,
        page: &[u8; settings::PAGE_SIZE],
    ) -> Result<(), Error> {
        let range = Range::program(offset).ok_or(Error::Range)?;
        self.flash.program(range.start(), page, self.conditions())
    }

    /// Erases the sector at settings-relative aligned `offset`; returns range,
    /// permission or hardware failure after applying the shared mutation checks.
    fn erase(
        &mut self,
        offset: u32,
    ) -> Result<(), Error> {
        let range = Range::erase(offset).ok_or(Error::Range)?;
        self.flash.erase(range.start(), self.conditions())
    }
}
