//! Boot Select button
//!
//! The RP2040 rom supports a BOOTSEL button that is used to enter the USB bootloader
//! if held during reset. To avoid wasting GPIO pins, the button is multiplexed onto
//! the CS pin of the QSPI flash, but that makes it somewhat expensive and complicated
//! to utilize outside of the rom's bootloader.
//!
//! This module provides functionality to poll BOOTSEL from an embassy application.

use crate::Peri;
use crate::flash::in_ram;

/// Borrows BOOTSEL ownership through `_p` and returns true when QSPI CS is sampled low.
/// Panics off core0; follows flash-maintenance exclusion requirements.
///
/// Reading parks core1 through its FIFO IRQ and waits for observed flash DMA; waits have no deadline.
pub fn is_bootsel_pressed(_p: Peri<'_, crate::peripherals::BOOTSEL>) -> bool {
    let mut cs_status = Default::default();

    unsafe { in_ram(|| cs_status = ram_helpers::read_cs_status()) }.expect("Must be called from Core 0");

    // bootsel is active low, so invert
    !cs_status.infrompad()
}

/// RAM-only CS tri-state/sample/restore sequence.
mod ram_helpers {
    use rp_pac::io::regs::GpioStatus;

    /// Temporarily reconfigures the CS gpio and returns the GpioStatus.

    /// This function runs from RAM so it can disable flash XIP.
    ///
    /// # Safety
    ///
    /// The caller must ensure flash is idle and will remain idle.
    /// This function must live in ram. It uses inline asm to avoid any
    /// potential calls to ABI functions that might be in flash.
    #[inline(never)]
    #[unsafe(link_section = ".data.ram_func")]
    #[cfg(target_arch = "arm")]
    pub unsafe fn read_cs_status() -> GpioStatus {
        let result: u32;

        // Magic value, used as both OEOVER::DISABLE and delay loop counter
        let magic = 0x2000;

        core::arch::asm!(
            ".equiv GPIO_STATUS, 0x0",
            ".equiv GPIO_CTRL,   0x4",

            "ldr {orig_ctrl}, [{cs_gpio}, $GPIO_CTRL]",

            // The BOOTSEL pulls the flash's CS line low though a 1K resistor.
            // this is weak enough to avoid disrupting normal operation.
            // But, if we disable CS's output drive and allow it to float...
            "str {val}, [{cs_gpio}, $GPIO_CTRL]",

            // ...then wait for the state to settle...
            "2:", // ~4000 cycle delay loop
            "subs {val}, #8",
            "bne 2b",

            // ...we can read the current state of bootsel
            "ldr {val}, [{cs_gpio}, $GPIO_STATUS]",

            // Finally, restore CS to normal operation so XIP can continue
            "str {orig_ctrl}, [{cs_gpio}, $GPIO_CTRL]",

            cs_gpio = in(reg) rp_pac::IO_QSPI.gpio(1).as_ptr(),
            orig_ctrl = out(reg) _,
            val = inout(reg) magic => result,
            options(nostack),
        );

        core::mem::transmute(result)
    }

    /// Unsupported-architecture stub; always panics and never returns a status.
    /// # Safety
    /// Same exclusive flash/CS ownership as the Arm implementation is required.
    #[cfg(not(target_arch = "arm"))]
    pub unsafe fn read_cs_status() -> GpioStatus { unimplemented!() }
}
