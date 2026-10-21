//! RP2350-only boundary for the locked HAL's RP2040 WDSEL mask.
use dashboard_core::health::reset::{BootEvidence, RecoveryCause, ResetRecord, boot_type_from_sys_info};
use embassy_rp::peripherals::WATCHDOG;
use embassy_rp::watchdog::Watchdog;
use embassy_rp::{Peri, pac};
use embassy_time::{Duration, Instant};

/// Exclusive core0 watchdog owner with boot evidence and a retained last-stage record.
pub struct RecoveryWatchdog {
    /// HAL timer handle; only `feed` is used after the corrected RP2350 start.
    watchdog: Watchdog,
    /// Immutable raw reset/ROM/scratch evidence captured before this boot writes it.
    boot: BootEvidence,
    /// This boot's last supervised stage/time or first recorded failure.
    current: ResetRecord,
}

impl RecoveryWatchdog {
    /// Consumes `peripheral`, captures old evidence, starts a 15-second boot
    /// deadline, and returns the sole feeding owner. Immediately repairs the
    /// locked HAL's RP2040 reset mask for RP2350 and writes this boot's record.
    /// Panics if reset mask, tick/reference clock, enable or debug-pause checks fail.
    /// A TIMER reason alone does not distinguish expiry from a ROM reboot.
    pub fn start(peripheral: Peri<'static, WATCHDOG>) -> Self {
        let mut watchdog = Watchdog::new(peripheral);
        let raw_reason = pac::WATCHDOG.reason().read().0;
        let scratch = core::array::from_fn(|index| watchdog.get_scratch(index));
        watchdog.pause_on_debug(false);
        watchdog.start(Duration::from_secs(15));
        // RP2350 PSM.WDSEL has bits 0..24; ROSC/XOSC are bits 2/3.
        // SDK-equivalent mask, corrected immediately after HAL start and before
        // any await, logging or core1 launch. Do not call HAL start/trigger_reset
        // again: both overwrite this register with the RP2040 mask in 0.10.0.
        let mut select = pac::psm::regs::Wdsel(0x01FF_FFFF);
        select.set_rosc(false);
        select.set_xosc(false);
        pac::PSM.wdsel().write_value(select);
        assert_eq!(pac::PSM.wdsel().read().0, 0x01FF_FFF3, "RP2350 watchdog reset mask");
        let tick = pac::TICKS.watchdog_ctrl().read();
        assert!(tick.enable() && tick.running(), "watchdog tick is not running");
        let control = pac::WATCHDOG.ctrl().read();
        assert!(control.enable(), "watchdog is disabled");
        assert!(
            !control.pause_dbg0() && !control.pause_dbg1() && !control.pause_jtag(),
            "watchdog debug pause"
        );
        let reference_hz = embassy_rp::clocks::clk_ref_freq();
        assert!(
            reference_hz >= 1_000_000 && reference_hz.is_multiple_of(1_000_000),
            "watchdog reference clock"
        );
        assert_eq!(
            pac::TICKS.watchdog_cycles().read().0,
            reference_hz / 1_000_000,
            "watchdog tick must be 1 MHz"
        );
        let boot = BootEvidence::capture(raw_reason, rom_boot_type(), scratch);
        let mut owner = Self {
            watchdog,
            boot,
            current: ResetRecord::booting(),
        };
        owner.persist_record();
        owner
    }

    /// Records the current boot-relative millisecond supervision observation and
    /// reloads a two-second deadline; the retained time is not a future reset time.
    pub fn feed(&mut self) {
        let previous = self.current;
        self.current.supervised(Instant::now().as_millis());
        if self.current != previous {
            self.persist_record();
        }
        self.watchdog.feed(Duration::from_secs(2));
    }

    /// Returns the pre-start reset evidence without rereading mutable scratch state.
    pub const fn boot_evidence(&self) -> BootEvidence { self.boot }

    /// Returns the conservative classification label derived from captured boot evidence.
    pub const fn reset_label(&self) -> &'static str { self.boot_evidence().label() }

    /// Call before logging or pending forever, while this core0 owner can run.
    /// Records `cause` with the current boot-relative milliseconds, preserving an
    /// already latched failure; does not feed or trigger the watchdog.
    pub fn record_failure(
        &mut self,
        cause: RecoveryCause,
    ) {
        let previous = self.current;
        self.current.fail(cause, Instant::now().as_millis());
        if self.current != previous {
            self.persist_record();
        }
    }

    /// Writes invalidation, payload and commit to the owned scratch registers with
    /// peripheral ordering barriers. Must not race an RP2350 ROM reboot request.
    fn persist_record(&mut self) {
        // Core0 startup/supervisor owns these writes. RP2350 ROM reboot uses
        // scratch2/3 as parameters and may invalidate this record; never call it
        // concurrently with this owner. The ROM consumes valid scratch4..7
        // vectors before launching us. We never write those vector registers.
        for (index, value) in self.current.scratch_writes() {
            // Order invalidation/payload/commit at the peripheral before a reset.
            cortex_m::asm::dmb();
            self.watchdog.set_scratch(index, value);
        }
        cortex_m::asm::dsb();
    }
}

/// Returns the boot-type byte from a validated secure-Arm ROM GS response, or
/// `None` if the entry/response is unavailable or malformed; performs no reboot.
fn rom_boot_type() -> Option<u8> {
    // The locked HAL's function wrapper transmutes lookup results unchecked.
    // Check the secure-Arm GS entry before constructing its function pointer.
    let entry = embassy_rp::rom_data::rom_table_lookup(*b"GS", 0x0004);
    if entry == 0 || entry & 1 == 0 {
        return None;
    }
    let mut words = [0u32; 5];
    // SAFETY: the RP2350 secure ROM table supplies GS with this ABI. The
    // aligned five-word buffer stays live and exclusively borrowed for the call.
    let status = unsafe {
        let get_sys_info: unsafe extern "C" fn(*mut u32, usize, u32) -> i32 = core::mem::transmute(entry);
        get_sys_info(words.as_mut_ptr(), words.len(), 0x0040)
    };
    boot_type_from_sys_info(status, words)
}
