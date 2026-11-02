//! Raw reset-mask writes used while clock/driver ownership excludes competing reset users.
pub use pac::resets::regs::Peripherals;

use crate::pac;

/// All RP2040 reset fields; clock initialization removes deliberate per-stage exclusions.
#[cfg(feature = "rp2040")]
pub const ALL_PERIPHERALS: Peripherals = Peripherals(0x01FF_FFFF);

/// All 29 RP2350 reset fields, including TRNG/UART0/UART1/USBCTRL in bits 25..=28.
/// Matches the RP2350 RESETS_RESET_BITS register definition; stage exclusions remain in clocks.
#[cfg(feature = "_rp235x")]
pub const ALL_PERIPHERALS: Peripherals = Peripherals(0x1FFF_FFFF);

/// Replaces the reset register with `peris`: set bits assert reset and cleared bits release it.
/// Callers must own affected peripherals and coordinate other register writers.
pub(crate) fn reset(peris: Peripherals) { pac::RESETS.reset().write_value(peris); }

/// Clears reset bits in `peris`, preserving others, then spins until all selected reset-done bits set.
/// No timeout; clocks must allow those peripherals to leave reset.
pub(crate) fn unreset_wait(peris: Peripherals) {
    // TODO use the "atomic clear" register version
    pac::RESETS.reset().modify(|v| *v = Peripherals(v.0 & !peris.0));
    while ((!pac::RESETS.reset_done().read().0) & peris.0) != 0 {}
}

#[cfg(test)]
/// Reset-mask contracts against the silicon register map, without MMIO.
mod tests {
    use super::*;

    #[cfg(feature = "_rp235x")]
    #[test]
    /// Clock initialization must include RP2350's upper reset fields and no reserved bits.
    fn rp235x_reset_mask_includes_upper_peripherals() {
        assert!(ALL_PERIPHERALS.trng(), "TRNG omitted from initial reset");
        assert!(ALL_PERIPHERALS.uart0(), "UART0 omitted from initial reset");
        assert!(ALL_PERIPHERALS.uart1(), "UART1 omitted from initial reset");
        assert!(ALL_PERIPHERALS.usbctrl(), "USBCTRL omitted from final release wait");
        assert_eq!(ALL_PERIPHERALS.0, 0x1fff_ffff);
    }

    #[cfg(feature = "rp2040")]
    #[test]
    /// RP2040 retains its own 25 reset fields rather than receiving reserved RP2350 bits.
    fn rp2040_reset_mask_stays_within_its_register_map() {
        assert!(ALL_PERIPHERALS.uart0());
        assert!(ALL_PERIPHERALS.uart1());
        assert!(ALL_PERIPHERALS.usbctrl());
        assert_eq!(ALL_PERIPHERALS.0, 0x01ff_ffff);
    }
}
