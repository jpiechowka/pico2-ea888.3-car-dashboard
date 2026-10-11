//! Host checks against the production firmware's hardware-independent modules.
//! This crate is deliberately outside the embedded Cargo workspace.
//!
//! A framebuffer cannot be reused after its lease moves into a DMA request:
//! ```compile_fail
//! use dashboard_host_validation::display::{FrameBuffer, pipeline::*};
//! let (mut a, mut b) = (FrameBuffer::new(), FrameBuffer::new());
//! let [mut lease, _] = FrameLease::pair(&mut a, &mut b);
//! let token = FrameLifecycle::new().begin(lease.id()).unwrap();
//! let request = lease.submit(token, dashboard_host_validation::display::telemetry::RenderTrace::instant(0));
//! lease.buffer()[0] = 1;
//! ```
//!
//! Nor can a request be released while its pixels are still borrowed:
//! ```compile_fail
//! use dashboard_host_validation::display::{FrameBuffer, pipeline::*};
//! let (mut a, mut b) = (FrameBuffer::new(), FrameBuffer::new());
//! let [lease, _] = FrameLease::pair(&mut a, &mut b);
//! let token = FrameLifecycle::new().begin(lease.id()).unwrap();
//! let request = lease.submit(token, dashboard_host_validation::display::telemetry::RenderTrace::instant(0));
//! let pixels = request.pixels();
//! let lease = request.into_lease();
//! std::hint::black_box(pixels);
//! ```

#[path = "../../firmware/pico2/src/acquisition.rs"]
pub mod acquisition;
#[path = "../../firmware/pico2/src/alarms.rs"]
pub mod alarms;
#[path = "../../firmware/pico2/src/ui/animations.rs"]
pub mod animations;
#[path = "../../firmware/pico2/src/backlight.rs"]
pub mod backlight;
#[path = "../../firmware/pico2/src/ui/colors.rs"]
pub mod colors;
#[path = "../../firmware/pico2/src/config/mod.rs"]
pub mod config;
#[path = "../../firmware/pico2/src/profiling/cycle_segments.rs"]
pub mod cycle_segments;
#[path = "../../firmware/pico2/src/dashboard.rs"]
pub mod dashboard;
#[path = "../../firmware/pico2/src/demo.rs"]
pub mod demo;
#[path = "../../firmware/pico2/src/diagnostics/mod.rs"]
pub mod diagnostics;
#[path = "../../firmware/pico2/src/state/extrema.rs"]
pub mod extrema;
#[path = "../../firmware/pico2/src/health.rs"]
pub mod health;
#[path = "../../firmware/pico2/src/input_model.rs"]
pub mod input_model;
#[path = "../../firmware/pico2/src/input_trace.rs"]
pub mod input_trace;
#[path = "../../firmware/pico2/src/logging.rs"]
pub mod logging;
#[path = "../../firmware/pico2/src/logging_view.rs"]
pub mod logging_view;
#[path = "../../firmware/pico2/src/maintenance.rs"]
pub mod maintenance;
#[path = "../../firmware/pico2/src/metrics.rs"]
pub mod metrics;
#[path = "../../firmware/pico2/src/microbench.rs"]
pub mod microbench;
#[path = "../../firmware/pico2/src/state/pages.rs"]
pub mod pages;
#[path = "../../firmware/pico2/src/state/popup.rs"]
pub mod popup;
#[path = "../../firmware/pico2/src/render.rs"]
pub mod render;
#[path = "../../firmware/pico2/src/samples.rs"]
pub mod samples;
#[path = "../../firmware/pico2/src/state/sensor_state.rs"]
pub mod sensor_state;
#[path = "../../firmware/pico2/src/settings/mod.rs"]
pub mod settings;
#[path = "../../firmware/pico2/src/profiling/stack_samples.rs"]
pub mod stack_samples;
#[path = "../../firmware/pico2/src/status_led.rs"]
pub mod status_led;
#[path = "../../firmware/pico2/src/ui/styles.rs"]
pub mod styles;
#[path = "../../firmware/pico2/src/telemetry_protocol.rs"]
pub mod telemetry_protocol;
#[path = "../../firmware/pico2/src/vehicle_status.rs"]
pub mod vehicle_status;
#[path = "../../firmware/pico2/src/widgets/mod.rs"]
pub mod widgets;
pub use widgets::primitives;

pub mod thresholds {
    pub use crate::config::sensors::*;
}

pub mod ui {
    pub use crate::colors::*;
    pub use crate::styles::*;
}

#[path = "../../firmware/pico2/src/display/mod.rs"]
pub mod display;
pub use display::framebuffer as renderer;
pub use renderer::FRAME_BYTES;

pub struct AlignedFrame(pub renderer::FrameBuffer);

impl AlignedFrame {
    pub fn boxed() -> Box<Self> { Box::new(Self(renderer::FrameBuffer::new())) }

    pub fn pixel(
        &self,
        x: usize,
        y: usize,
    ) -> u16 {
        let index = (y * 320 + x) * 2;
        u16::from_be_bytes([self.0[index], self.0[index + 1]])
    }
}
