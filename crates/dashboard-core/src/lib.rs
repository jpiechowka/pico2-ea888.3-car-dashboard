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
pub mod ble;
pub mod cache_diagnostics;
pub mod config;
pub mod peak_hold;
pub mod presentation;
pub mod warning_state;
pub use sensor_state::GraphCacheIdentity;
pub mod configured_scene;
pub mod control_session;
pub mod controls;
pub mod dashboard;
pub mod debug_view;
pub mod demo;
pub mod diagnostics;
pub mod discovery;
pub mod display;
pub mod health;
pub mod ignition;
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
pub mod startup;
pub mod status_led;
pub mod telemetry_protocol;
pub mod vehicle_status;
pub mod view_state;
pub mod widgets;

pub use config::sensors as thresholds;
pub use dashboard_config as view_config;
pub use widgets::primitives;

/// Instrumented CPU work/cycles and observed stack diagnostic models.
mod profiling {
    pub mod cpu_math;
    pub mod cycle_segments;
    pub mod diagnostic_values;
    pub mod stack_samples;
    pub mod task_work;
}

/// Fixed state containers shared by acquisition and presentation.
mod state {
    pub mod extrema;
    mod finite_optional;
    pub mod history;
    pub mod pages;
    pub mod popup;
    pub mod sensor_state;
}

/// Shared font/color tokens and elapsed-time decorative animation.
pub mod ui {
    pub mod animations;
    pub mod colors;
    pub mod styles;
    pub use colors::*;
    pub use styles::*;
}

pub use profiling::{cpu_math, cycle_segments, diagnostic_values, stack_samples, task_work};
pub use state::{extrema, history, pages, popup, sensor_state};
pub use ui::{animations, colors, styles};
