//! Shared RP2350 clock catalogue and allocation-free divider admission.
//!
//! [`resolve()`] separates requested frequencies from programmed rates. Its success
//! proves integer-divider representability, not board stability or electrical
//! qualification. Host builds and firmware generators share these definitions.
//! Hardware limits: [RP2350 datasheet](https://datasheets.raspberrypi.com/rp2350/rp2350-datasheet.pdf),
//! PLL and SPI chapters; SPI selection follows the vendored Embassy implementation.

#![no_std]

mod catalogue;
mod resolve;

pub use catalogue::{Profile, ProfileId, ProfileKind, UserObservation, catalogue};
pub use resolve::{
    ClockError,
    ClockSelection,
    CustomClock,
    Pll,
    ResolvedClock,
    SpiNeighbors,
    SpiRate,
    Voltage,
    resolve,
};
