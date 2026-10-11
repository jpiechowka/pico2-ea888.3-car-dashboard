//! Exact pixel comparison against the last completed, unmodified framebuffer.
use core::ops::Range;

use heapless::Vec;

use super::framebuffer::FRAME_BYTES;
use super::pipeline::FrameToken;
use super::{FrameBuffer, HEIGHT, WIDTH};

const TILE: usize = 16;
const MAX_REGIONS: usize = 16;

/// A nonempty panel rectangle. Coordinates and extents cannot leave the panel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Region {
    x: u16,
    y: u16,
    width: u16,
    height: u16,
}

impl Region {
    pub fn new(
        x: u16,
        y: u16,
        width: u16,
        height: u16,
    ) -> Option<Self> {
        if width == 0
            || height == 0
            || u32::from(x) + u32::from(width) > WIDTH as u32
            || u32::from(y) + u32::from(height) > HEIGHT as u32
        {
            return None;
        }
        Some(Self { x, y, width, height })
    }

    pub const fn full() -> Self {
        Self {
            x: 0,
            y: 0,
            width: WIDTH as u16,
            height: HEIGHT as u16,
        }
    }

    pub fn columns(self) -> [[u8; 2]; 2] { [self.x.to_be_bytes(), (self.x + self.width - 1).to_be_bytes()] }

    pub fn rows(self) -> [[u8; 2]; 2] { [self.y.to_be_bytes(), (self.y + self.height - 1).to_be_bytes()] }

    pub fn bytes(self) -> usize { usize::from(self.width) * usize::from(self.height) * 2 }

    /// Full-width bands are contiguous. Every narrower row has its own stride.
    /// This iterator is used by both the hardware writer and the panel model.
    pub fn payload_ranges(self) -> impl Iterator<Item = Range<usize>> {
        let full_width = usize::from(self.width) == WIDTH;
        let count = if full_width { 1 } else { usize::from(self.height) };
        (0..count).map(move |row| {
            let start = ((usize::from(self.y) + row) * WIDTH + usize::from(self.x)) * 2;
            let bytes = if full_width {
                self.bytes()
            } else {
                usize::from(self.width) * 2
            };
            start..start + bytes
        })
    }
}

#[derive(Clone, Debug)]
pub struct DamagePlan {
    regions: Vec<Region, MAX_REGIONS>,
}

impl DamagePlan {
    pub fn full() -> Self {
        let mut regions = Vec::new();
        regions.push(Region::full()).unwrap();
        Self { regions }
    }

    pub fn between(
        previous: &FrameBuffer,
        current: &FrameBuffer,
    ) -> Self {
        let mut plan = Self { regions: Vec::new() };
        for y in (0..HEIGHT).step_by(TILE) {
            let changed = |x: usize| {
                (y..y + TILE).any(|row| {
                    let start = (row * WIDTH + x) * 2;
                    previous[start..start + TILE * 2] != current[start..start + TILE * 2]
                })
            };
            let mut x = 0;
            while x < WIDTH {
                if !changed(x) {
                    x += TILE;
                    continue;
                }
                let start = x;
                x += TILE;
                while x < WIDTH && changed(x) {
                    x += TILE;
                }
                let width = (x - start) as u16;
                if let Some(region) = plan
                    .regions
                    .iter_mut()
                    .find(|r| r.x == start as u16 && r.width == width && r.y + r.height == y as u16)
                {
                    region.height += TILE as u16;
                } else if plan
                    .regions
                    .push(Region {
                        x: start as u16,
                        y: y as u16,
                        width,
                        height: TILE as u16,
                    })
                    .is_err()
                {
                    return Self::full();
                }
            }
        }
        // Bound command/setup overhead for dense changes. This is a conservative
        // byte policy, not a measured SPI break-even point.
        if plan.bytes() >= FRAME_BYTES * 3 / 4 {
            Self::full()
        } else {
            plan
        }
    }

    pub fn regions(&self) -> &[Region] { &self.regions }

    pub fn is_empty(&self) -> bool { self.regions.is_empty() }

    pub fn is_full(&self) -> bool { self.regions.as_slice() == [Region::full()] }

    pub fn bytes(&self) -> usize { self.regions.iter().map(|r| r.bytes()).sum() }

    pub fn transfer_count(&self) -> usize { self.regions.iter().map(|r| r.payload_ranges().count()).sum() }
}

const _: () = assert!(WIDTH.is_multiple_of(TILE) && HEIGHT.is_multiple_of(TILE));

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GenerationError {
    Busy,
    Stale,
    WrongBase,
}

/// Pixel generations are independent of buffer availability. A free lease is
/// only a panel reference while its exact completed generation is unchanged.
pub struct PanelHistory {
    buffers: [Option<FrameToken>; 2],
    panel: Option<FrameToken>,
    pending: Option<FrameToken>,
    last_sequence: u64,
}

impl Default for PanelHistory {
    fn default() -> Self { Self::new() }
}
impl PanelHistory {
    pub const fn new() -> Self {
        Self {
            buffers: [None; 2],
            panel: None,
            pending: None,
            last_sequence: 0,
        }
    }

    pub fn begin_render(
        &mut self,
        token: FrameToken,
    ) -> Result<(), GenerationError> {
        if self.pending.is_some_and(|p| p.buffer == token.buffer) {
            return Err(GenerationError::Busy);
        }
        if token.sequence <= self.last_sequence {
            return Err(GenerationError::Stale);
        }
        self.buffers[token.buffer.index()] = Some(token);
        self.last_sequence = token.sequence;
        Ok(())
    }

    pub fn reference(&self) -> Option<FrameToken> {
        self.panel
            .filter(|token| self.buffers[token.buffer.index()] == Some(*token))
    }

    /// None means a full transfer with no dependency on old panel pixels.
    pub fn queue(
        &mut self,
        token: FrameToken,
        base: Option<FrameToken>,
    ) -> Result<(), GenerationError> {
        if self.pending.is_some() {
            return Err(GenerationError::Busy);
        }
        if self.buffers[token.buffer.index()] != Some(token) || self.panel.is_some_and(|p| token.sequence <= p.sequence)
        {
            return Err(GenerationError::Stale);
        }
        if base.is_some() && (base != self.reference() || base.is_some_and(|b| b.buffer == token.buffer)) {
            return Err(GenerationError::WrongBase);
        }
        self.pending = Some(token);
        Ok(())
    }

    pub fn complete(
        &mut self,
        token: FrameToken,
    ) -> Result<(), GenerationError> {
        if self.pending != Some(token) || self.buffers[token.buffer.index()] != Some(token) {
            return Err(GenerationError::Stale);
        }
        self.pending = None;
        self.panel = Some(token);
        Ok(())
    }
}
