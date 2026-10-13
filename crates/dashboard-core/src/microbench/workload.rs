//! Fixed production workloads. Setup and output checks are outside timing.
use core::hint::black_box;

use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::Rectangle;

use super::Error;
use crate::display::{FrameBuffer, St7789Renderer};
use crate::sensor_state::{PeakMode, SensorSnapshot, SensorState};
use crate::widgets::draw_boost_cell;
use crate::widgets::primitives::draw_mini_graph;

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kernel {
    Clear = 0,
    Clipped = 1,
    GraphWidget = 2,
    History = 3,
}
impl Kernel {
    pub const ALL: [Self; 4] = [Self::Clear, Self::Clipped, Self::GraphWidget, Self::History];

    pub const fn expected_checksum(self) -> u32 {
        match self {
            Self::Clear => 3402950085,
            Self::Clipped => 4004879533,
            Self::GraphWidget => 3907113197,
            Self::History => 2271814903,
        }
    }
}

pub struct Fixture {
    history: SensorState,
    next: u64,
    graph: [f32; 60],
    valid: bool,
}
impl Default for Fixture {
    fn default() -> Self { Self::new() }
}
impl Fixture {
    pub fn new() -> Self {
        let mut history = SensorState::new();
        for step in 0..60 {
            history.ingest((step % 19) as f32 - 9.0, step * 5000, PeakMode::Maximum);
        }
        let mut graph = core::array::from_fn(|i| (i % 11) as f32 * 0.25);
        graph[16] = 8.0;
        Self {
            history,
            next: 60,
            graph,
            valid: true,
        }
    }

    #[inline(never)]
    pub fn run(
        &mut self,
        kind: Kernel,
        frame: &mut FrameBuffer,
    ) {
        let mut display = St7789Renderer::new(black_box(&mut *frame));
        match black_box(kind) {
            Kernel::Clear => display.clear_buffer(black_box(Rgb565::new(3, 17, 29))),
            Kernel::Clipped => {
                let area = black_box(Rectangle::new(Point::new(-3, 221), Size::new(23, 27)));
                let colors = (0..black_box(23 * 27))
                    .map(|i| Rgb565::new((i % 32) as u8, ((i / 5) % 64) as u8, ((i / 11) % 32) as u8));
                let _ = display.fill_contiguous(&area, colors);
            }
            Kernel::GraphWidget => {
                draw_mini_graph(
                    &mut display,
                    160,
                    184,
                    155,
                    55,
                    black_box(&self.graph),
                    60,
                    17,
                    60,
                    0.0,
                    8.0,
                    |_| Rgb565::WHITE,
                );
                draw_boost_cell(
                    &mut display,
                    0,
                    0,
                    160,
                    110,
                    black_box(1.6),
                    black_box(2.7),
                    true,
                    false,
                    true,
                    0,
                );
            }
            Kernel::History => {
                for _ in 0..64 {
                    self.valid &= black_box(&mut self.history).ingest(
                        black_box((self.next % 19) as f32 - 9.0),
                        black_box(self.next * 5000),
                        PeakMode::Maximum,
                    );
                    self.next += 1;
                }
                black_box(self.history.snapshot());
            }
        }
        black_box(frame);
    }

    pub fn snapshot(&self) -> SensorSnapshot { self.history.snapshot() }

    pub fn checksum(
        &self,
        kind: Kernel,
        frame: &FrameBuffer,
    ) -> Result<u32, Error> {
        if !self.valid {
            return Err(Error::Checksum);
        }
        let mut hash = 2166136261u32;
        let mut append = |byte| {
            hash = (hash ^ u32::from(byte)).wrapping_mul(16777619);
        };
        if kind == Kernel::History {
            let s = self.history.snapshot();
            let history = s.history();
            let (min, max) = history.bounds().ok_or(Error::Checksum)?;
            let values = [
                s.value(),
                s.minimum(),
                s.maximum(),
                s.get_average(),
                Some(min),
                Some(max),
            ];
            for v in values {
                for byte in v.ok_or(Error::Checksum)?.to_bits().to_le_bytes() {
                    append(byte);
                }
            }
            for v in [
                history.newest_ordinal() as u32,
                history.count() as u32,
                s.sampled_at_ms().ok_or(Error::Checksum)? as u32,
            ] {
                for byte in v.to_le_bytes() {
                    append(byte);
                }
            }
            for (at, value) in history.iter() {
                for byte in at.to_le_bytes() {
                    append(byte);
                }
                append(u8::from(value.is_some()));
                for byte in value.unwrap_or(0.0).to_bits().to_le_bytes() {
                    append(byte);
                }
            }
        } else {
            for &byte in frame.iter() {
                append(byte);
            }
        }
        Ok(hash)
    }
}
