#![no_std]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_sign_loss)]

pub mod acquisition;
pub mod config;
pub mod dashboard;
pub mod display;
pub mod health;
pub mod input_model;

pub mod render;
pub mod samples;
pub mod widgets;

pub use config::sensors as thresholds;

mod profiling {
    pub mod cpu_cycles;
    pub mod cycle_segments;
    pub mod memory;
    pub mod stack_samples;
}

mod state {
    pub mod extrema;
    pub mod pages;
    pub mod sensor_state;
}

mod ui {
    pub mod colors;
    pub mod styles;
    pub use colors::*;
    pub use styles::*;
}

pub use profiling::{cpu_cycles, cycle_segments, memory, stack_samples};
pub use state::{extrema, pages, sensor_state};
pub use ui::{colors, styles};
