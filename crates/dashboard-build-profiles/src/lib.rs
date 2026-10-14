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
