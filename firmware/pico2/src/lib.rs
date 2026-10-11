#![no_std]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_sign_loss)]

pub mod config;
pub mod display;
pub mod health;

pub mod render;

pub use config::sensors as thresholds;

mod profiling {
    pub mod cpu_cycles;
    pub mod memory;
}

mod state {
    pub mod extrema;
    pub mod pages;
    pub mod sensor_state;
}

mod ui {
    pub mod colors;
}

pub use profiling::{cpu_cycles, memory};
pub use state::{extrema, pages, sensor_state};
pub use ui::colors;
