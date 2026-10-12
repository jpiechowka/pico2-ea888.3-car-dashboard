#![no_std]

pub use dashboard_core::*;

mod profiling {
    pub mod cpu_cycles;
    pub mod memory;
}
pub use profiling::{cpu_cycles, memory};
