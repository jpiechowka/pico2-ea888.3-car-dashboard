//! Two-buffer rendering, exact damage admission and observed transfer telemetry.
pub mod damage;
pub mod framebuffer;
pub mod pipeline;
pub mod te;
pub mod telemetry;
pub mod transfer;
pub mod window;
pub use framebuffer::{FrameBuffer, HEIGHT, St7789Renderer, WIDTH};
