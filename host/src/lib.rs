//! Host checks against the production firmware's hardware-independent modules.
//! This crate is deliberately outside the embedded Cargo workspace.

#[path = "../../firmware/pico2/src/ui/colors.rs"]
pub mod colors;
#[path = "../../firmware/pico2/src/config/mod.rs"]
pub mod config;
#[path = "../../firmware/pico2/src/state/extrema.rs"]
pub mod extrema;
#[path = "../../firmware/pico2/src/render.rs"]
pub mod render;
#[path = "../../firmware/pico2/src/state/sensor_state.rs"]
pub mod sensor_state;
#[path = "../../firmware/pico2/src/ui/styles.rs"]
pub mod styles;
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

#[path = "../../firmware/pico2/src/display/framebuffer.rs"]
pub mod renderer;
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
