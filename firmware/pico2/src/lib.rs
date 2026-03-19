#![cfg_attr(not(test), no_std)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_sign_loss)]

pub mod config;

pub mod render;

pub use config::sensors as thresholds;

mod profiling {
    pub mod cpu_cycles;
    pub mod memory;
}

mod state {
    pub mod pages;
    pub mod sensor_state;
}

mod ui {
    pub mod colors;
}

pub use profiling::{cpu_cycles, memory};
pub use state::{pages, sensor_state};
pub use ui::colors;
