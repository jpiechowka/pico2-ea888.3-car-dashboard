//! RP2350-only boundary for the locked HAL's RP2040 WDSEL mask.
use embassy_rp::peripherals::WATCHDOG;
use embassy_rp::watchdog::Watchdog;
use embassy_rp::{Peri, pac};
use embassy_time::Duration;

pub struct RecoveryWatchdog {
    watchdog: Watchdog,
}

impl RecoveryWatchdog {
    pub fn start(peripheral: Peri<'static, WATCHDOG>) -> Self {
        let mut watchdog = Watchdog::new(peripheral);
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
        Self { watchdog }
    }

    pub fn feed(&mut self) { self.watchdog.feed(Duration::from_secs(2)); }
}
