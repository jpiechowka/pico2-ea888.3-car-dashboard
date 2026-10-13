//! One portable implementation for firmware, host verification and the web preview.
#![no_std]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_sign_loss)]

#[cfg(feature = "alloc")]
extern crate alloc;

pub mod acquisition;
pub mod alarms;
pub mod backlight;
pub mod cache_diagnostics;
pub mod config;
pub use sensor_state::GraphCacheIdentity;
pub mod configured_scene;
pub mod control_session;
pub mod controls;
pub mod dashboard;
pub mod demo;
pub mod diagnostics;
pub mod display;
pub mod health;
pub mod input_model;
pub mod input_trace;
pub mod logging;
pub mod logging_view;
pub mod maintenance;
pub mod metrics;
pub mod microbench;
pub mod render;
pub mod samples;
pub mod settings;
pub mod status_led;
pub mod telemetry_protocol;
pub mod vehicle_status;
pub mod view_state;
pub mod widgets;

pub use config::sensors as thresholds;
pub use dashboard_config as view_config;
pub use widgets::primitives;

mod profiling {
    pub mod cycle_segments;
    pub mod stack_samples;
}

mod state {
    pub mod extrema;
    pub mod history;
    pub mod pages;
    pub mod popup;
    pub mod sensor_state;
}

pub mod ui {
    pub mod animations;
    pub mod colors;
    pub mod styles;
    pub use colors::*;
    pub use styles::*;
}

pub use profiling::{cycle_segments, stack_samples};
pub use state::{extrema, history, pages, popup, sensor_state};
pub use ui::{animations, colors, styles};
