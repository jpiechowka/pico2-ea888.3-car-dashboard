//! Shared RP2350 flash preconditions. Callers own park/resume and region policy.
//!
//! Checks are snapshots, not locks: retain sole peripheral ownership and the
//! maintenance barrier until each synchronous ROM operation returns. The HAL
//! parks core1 before taking its critical section; an outer critical section can
//! deadlock that handshake. ROM stalls are bounded only by the armed watchdog.
//! See the [RP2350 flash API](https://datasheets.raspberrypi.com/rp2350/rp2350-datasheet.pdf)
//! and [Pico 2 W flash hardware](https://datasheets.raspberrypi.com/picow/pico-2-w-datasheet.pdf).
use embassy_rp::flash::{Blocking, Flash};
use embassy_rp::peripherals::FLASH;
use embassy_rp::{Peri, pac, rom_data};

#[derive(Debug, defmt::Format)]
/// Rejected precondition or underlying HAL failure; rejection does not authorize retry without rechecking.
pub enum Error {
    /// Unoptimized builds have no qualified RAM flash-operation artifact.
    DebugBuild,
    /// Wrong core, masked interrupts, exception context, or inactive watchdog.
    Context,
    /// Unsupported QSPI mux, voltage, chip-select, or boot-configured capacity.
    Configuration,
    /// Direct QMI/streaming is active or the reserved range lacks identity mapping.
    Mapping,
    /// JEDEC response differs from the qualified EF/40/16 chip identity.
    Identity(
        /// Observed 24-bit manufacturer/type/capacity response.
        u32,
    ),
    /// Boot qualification has not completed successfully.
    Unqualified,
    /// A participating peripheral, DMA channel, or owner is not quiescent.
    Busy,
    /// Empty, overflowing, out-of-region, or improperly aligned operation.
    Range,
    /// Program page is not wholly inside the admitted SRAM range.
    RamBuffer,
    /// Caller did not supply a currently valid maintenance permission.
    Permit,
    /// HAL rejected or failed the flash operation.
    Hardware(
        /// Original HAL failure, retained without reinterpretation.
        embassy_rp::flash::Error,
    ),
}
impl From<embassy_rp::flash::Error> for Error {
    /// Returns `error` wrapped as a hardware failure without changing its cause.
    fn from(error: embassy_rp::flash::Error) -> Self { Self::Hardware(error) }
}

#[derive(Clone, Copy)]
/// Immutable write partition within the conservative first 2 MiB of flash.
pub enum Region {
    /// Final two 4 KiB sectors used by the dashboard preferences journal.
    Settings,
    /// Probe-only 96 KiB capture journal immediately before settings.
    Captures,
}
impl Region {
    /// Returns this partition's physical byte offsets as a half-open `(start, end)`.
    fn bounds(self) -> (u32, u32) {
        match self {
            Self::Settings => (0x1FE000, 0x200000),
            Self::Captures => (dashboard_core::discovery::capture_store::FLASH_OFFSET, 0x1FE000),
        }
    }

    /// Returns whether nonempty `length` bytes starting at physical `offset` fit
    /// wholly in this region; conversion and end-address overflow reject the range.
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

/// Returns whether the current core is in thread mode with PRIMASK/FAULTMASK
/// clear and BASEPRI zero, so HAL lockout acknowledgements can run.
pub fn interrupt_ready() -> bool {
    use cortex_m::peripheral::SCB;
    use cortex_m::peripheral::scb::VectActive;
    use cortex_m::register::{basepri, faultmask, primask};
    SCB::vect_active() == VectActive::ThreadMode
        && primask::read().is_active()
        && faultmask::read().is_active()
        && basepri::read() == 0
}
/// Returns whether core1 can receive the secure FIFO lockout IRQ now.
/// This does not park the core or establish ownership of any peripheral.
pub fn core1_ready() -> bool {
    pac::SIO.cpuid().read() == 1
        && interrupt_ready()
        && cortex_m::peripheral::NVIC::is_enabled(pac::Interrupt::SIO_IRQ_FIFO)
}
/// Returns whether I2C0 reports no bus/master activity, empty TX, and no RX data.
/// A future completing with an error alone is insufficient evidence of this state.
pub fn i2c_idle() -> bool {
    let status = pac::I2C0.ic_status().read();
    !status.activity() && !status.mst_activity() && status.tfe() && !status.rfne()
}

/// Caller-owned barrier observations rechecked alongside live hardware state.
pub struct WriteConditions {
    /// Current request generation is admitted and still within its write deadline.
    pub maintenance_valid: bool,
    /// No display request or affine framebuffer transfer remains outstanding.
    pub display_idle: bool,
    /// Radio owner is parked between complete bus operations, or radio is absent.
    pub radio_idle: bool,
}
/// Sole core0 flash handle with immutable partition and successful-qualification latch.
pub struct CheckedFlash {
    /// Blocking HAL handle; its operations own the core1/XIP lockout sequence.
    flash: Flash<'static, FLASH, Blocking, 0x20_0000>,
    /// Physical byte range admitted for every read and mutation.
    region: Region,
    /// Set only after successful JEDEC and mapping checks; false at construction.
    qualified: bool,
}
impl CheckedFlash {
    /// Consumes exclusive `peripheral` ownership and fixes `region`; returns an
    /// unqualified handle without probing, reading, erasing, or programming flash.
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

    /// Qualifies before core1/peripheral startup, with the watchdog armed.
    /// Returns the 24-bit JEDEC ID (`0xEF4016`) and latches admission on success.
    /// Rejects debug builds, context/configuration/mapping, DMA activity or another
    /// identity. Capacity metadata is configured state, not a physical measurement.
    /// The synchronous probe can fail-stop in SRAM until watchdog reset.
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

    /// Returns success only on core0 in interruptible thread mode.
    fn context() -> Result<(), Error> {
        if pac::SIO.cpuid().read() != 0 || !interrupt_ready() {
            return Err(Error::Context);
        }
        Ok(())
    }

    /// Returns success for the admitted 3.3 V QSPI mux and boot flash metadata;
    /// malformed ROM replies, configured CS1, and unsupported capacity reject it.
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

    /// Returns success only when direct/streaming access is inactive and both
    /// endpoints of this handle's region map to identical storage addresses.
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

    /// Returns success after rechecking qualification, core0 context and mapping,
    /// or the first rejected precondition, without touching payload bytes.
    fn check_read(&self) -> Result<(), Error> {
        if !self.qualified {
            return Err(Error::Unqualified);
        }
        Self::context()?;
        self.mapping()
    }

    /// Returns success when `conditions` and all live bus/DMA observations admit
    /// mutation; it neither acquires the barrier nor cancels active operations.
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

    /// Copies flash at physical `offset` into nonempty `bytes` after region and
    /// read checks. Returns a range/precondition/HAL error; success fills the slice.
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

    /// Programs the 256-byte SRAM `bytes` page at aligned physical `offset` when
    /// `conditions` still admit mutation. Returns precondition/HAL errors; this
    /// does not erase, verify readback, or atomically commit a journal record.
    /// Retain the maintenance barrier throughout the synchronous ROM call.
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

    /// Erases one 4096-byte sector at aligned physical `offset` when `conditions`
    /// admit mutation. Returns precondition/HAL errors; no readback is performed.
    /// Once ROM work starts, async cancellation cannot bound or undo the erase.
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
