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

pub use dashboard_core::*;
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
