//! Exact pixel comparison against the last completed, unmodified framebuffer.
use core::ops::Range;

use dashboard_config::{Geometry, Orientation};
use heapless::Vec;

use super::framebuffer::FRAME_BYTES;
use super::pipeline::FrameToken;
use super::{FrameBuffer, HEIGHT, WIDTH};

const TILE: usize = 16;
const MAX_REGIONS: usize = 16;
const MAX_PAYLOAD_STARTS: usize = 128;

/// A nonempty panel rectangle. Coordinates and extents cannot leave the panel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Region {
    // Both native widths are below 512. Keep the geometry in an unused
    // coordinate bit so each retained sixteen-region plan stays compact.
    x_and_geometry: u16,
    y: u16,
    width: u16,
    height: u16,
}

const PORTRAIT_BIT: u16 = 1 << 15;
const _: () = assert!(core::mem::size_of::<Region>() == 8);
const _: () = assert!(Geometry::Landscape.width() < PORTRAIT_BIT && Geometry::Portrait.width() < PORTRAIT_BIT);

impl Region {
    const fn encode_x(
        geometry: Geometry,
        x: u16,
    ) -> u16 {
        x | match geometry {
            Geometry::Landscape => 0,
            Geometry::Portrait => PORTRAIT_BIT,
        }
    }

    const fn x(self) -> u16 { self.x_and_geometry & !PORTRAIT_BIT }

    const fn geometry(self) -> Geometry {
        if self.x_and_geometry & PORTRAIT_BIT == 0 {
            Geometry::Landscape
        } else {
            Geometry::Portrait
        }
    }

    pub fn new(
        x: u16,
        y: u16,
        width: u16,
        height: u16,
    ) -> Option<Self> {
        Self::new_in(Geometry::Landscape, x, y, width, height)
    }

    pub fn new_in(
        geometry: Geometry,
        x: u16,
        y: u16,
        width: u16,
        height: u16,
    ) -> Option<Self> {
        if width == 0
            || height == 0
            || u32::from(x) + u32::from(width) > u32::from(geometry.width())
            || u32::from(y) + u32::from(height) > u32::from(geometry.height())
        {
            return None;
        }
        Some(Self {
            x_and_geometry: Self::encode_x(geometry, x),
            y,
            width,
            height,
        })
    }

    pub const fn full() -> Self { Self::full_in(Geometry::Landscape) }

    pub const fn full_in(geometry: Geometry) -> Self {
        Self {
            x_and_geometry: Self::encode_x(geometry, 0),
            y: 0,
            width: geometry.width(),
            height: geometry.height(),
        }
    }

    pub fn columns(self) -> [[u8; 2]; 2] { [self.x().to_be_bytes(), (self.x() + self.width - 1).to_be_bytes()] }

    pub fn rows(self) -> [[u8; 2]; 2] { [self.y.to_be_bytes(), (self.y + self.height - 1).to_be_bytes()] }

    pub fn bytes(self) -> usize { usize::from(self.width) * usize::from(self.height) * 2 }

    /// Full-width bands are contiguous. Every narrower row has its own stride.
    /// This iterator is used by both the hardware writer and the panel model.
    pub fn payload_ranges(self) -> impl Iterator<Item = Range<usize>> {
        let stride = usize::from(self.geometry().width());
        let full_width = usize::from(self.width) == stride;
        let count = if full_width { 1 } else { usize::from(self.height) };
        (0..count).map(move |row| {
            let start = ((usize::from(self.y) + row) * stride + usize::from(self.x())) * 2;
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
    geometry: Geometry,
    regions: Vec<Region, MAX_REGIONS>,
}

impl DamagePlan {
    pub fn full() -> Self { Self::full_in(Geometry::Landscape) }

    pub fn full_in(geometry: Geometry) -> Self {
        let mut regions = Vec::new();
        regions.push(Region::full_in(geometry)).unwrap();
        Self { geometry, regions }
    }

    pub fn between(
        previous: &FrameBuffer,
        current: &FrameBuffer,
    ) -> Self {
        Self::between_in(Geometry::Landscape, previous, current)
    }

    pub fn between_in(
        geometry: Geometry,
        previous: &FrameBuffer,
        current: &FrameBuffer,
    ) -> Self {
        let width_pixels = usize::from(geometry.width());
        let height_pixels = usize::from(geometry.height());
        let mut plan = Self {
            geometry,
            regions: Vec::new(),
        };
        for y in (0..height_pixels).step_by(TILE) {
            let changed = |x: usize| {
                (y..y + TILE).any(|row| {
                    let start = (row * width_pixels + x) * 2;
                    previous[start..start + TILE * 2] != current[start..start + TILE * 2]
                })
            };
            let mut x = 0;
            while x < width_pixels {
                if !changed(x) {
                    x += TILE;
                    continue;
                }
                let start = x;
                x += TILE;
                while x < width_pixels && changed(x) {
                    x += TILE;
                }
                let width = (x - start) as u16;
                if let Some(region) = plan
                    .regions
                    .iter_mut()
                    .find(|r| r.x() == start as u16 && r.width == width && r.y + r.height == y as u16)
                {
                    region.height += TILE as u16;
                } else if plan
                    .regions
                    .push(Region {
                        x_and_geometry: Region::encode_x(geometry, start as u16),
                        y: y as u16,
                        width,
                        height: TILE as u16,
                    })
                    .is_err()
                {
                    return Self::full_in(geometry);
                }
            }
        }
        // Bound command/setup overhead for dense changes. This is a conservative
        // byte policy, not a measured SPI break-even point.
        if plan.bytes() >= FRAME_BYTES * 3 / 4 || plan.transfer_count() > MAX_PAYLOAD_STARTS {
            Self::full_in(geometry)
        } else {
            plan
        }
    }

    pub const fn geometry(&self) -> Geometry { self.geometry }

    pub fn regions(&self) -> &[Region] { &self.regions }

    pub fn is_empty(&self) -> bool { self.regions.is_empty() }

    pub fn is_full(&self) -> bool { self.regions.as_slice() == [Region::full_in(self.geometry)] }

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
    orientations: [Orientation; 2],
    panel_orientation: Option<Orientation>,
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
            orientations: [Orientation::Landscape; 2],
            panel_orientation: None,
            panel: None,
            pending: None,
            last_sequence: 0,
        }
    }

    pub fn begin_render(
        &mut self,
        token: FrameToken,
    ) -> Result<(), GenerationError> {
        self.begin_render_in(token, Orientation::Landscape)
    }

    pub fn begin_render_in(
        &mut self,
        token: FrameToken,
        orientation: Orientation,
    ) -> Result<(), GenerationError> {
        if self.pending.is_some_and(|p| p.buffer == token.buffer) {
            return Err(GenerationError::Busy);
        }
        if token.sequence <= self.last_sequence {
            return Err(GenerationError::Stale);
        }
        self.buffers[token.buffer.index()] = Some(token);
        self.orientations[token.buffer.index()] = orientation;
        self.last_sequence = token.sequence;
        Ok(())
    }

    pub fn reference(&self) -> Option<FrameToken> {
        self.panel
            .filter(|token| self.buffers[token.buffer.index()] == Some(*token))
    }

    pub const fn panel_orientation(&self) -> Option<Orientation> { self.panel_orientation }

    /// The mode that will precede the next submitted frame. Rendering can run
    /// ahead of completion, so the pending request takes precedence over the
    /// current panel. Merely rendering or discarding a frame never changes it.
    pub fn target_orientation(&self) -> Option<Orientation> {
        self.pending
            .map(|token| self.orientations[token.buffer.index()])
            .or(self.panel_orientation)
    }

    pub fn reference_for(
        &self,
        orientation: Orientation,
    ) -> Option<FrameToken> {
        self.reference().filter(|_| self.panel_orientation == Some(orientation))
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
        if base.is_some()
            && (base != self.reference_for(self.orientations[token.buffer.index()])
                || base.is_some_and(|b| b.buffer == token.buffer))
        {
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
        self.panel_orientation = Some(self.orientations[token.buffer.index()]);
        Ok(())
    }
}
