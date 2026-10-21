//! Raw reset-mask writes used while clock/driver ownership excludes competing reset users.
pub use pac::resets::regs::Peripherals;

use crate::pac;

/// Legacy 25-bit reset mask used as the clock-init baseline; not a discovered hardware inventory.
pub const ALL_PERIPHERALS: Peripherals = Peripherals(0x01FF_FFFF);

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
