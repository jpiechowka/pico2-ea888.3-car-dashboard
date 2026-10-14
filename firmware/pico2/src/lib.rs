#![no_std]

pub use dashboard_core::*;
pub mod clock_profile;

mod profiling {
    pub mod cpu_cycles;
    pub mod memory;
}
pub use profiling::{cpu_cycles, memory};
