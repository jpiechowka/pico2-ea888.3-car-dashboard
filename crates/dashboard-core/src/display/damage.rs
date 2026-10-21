//! Exact pixel comparison against the last completed, unmodified framebuffer.
use core::ops::Range;

use dashboard_config::{Geometry, Orientation};
use heapless::Vec;

use super::framebuffer::FRAME_BYTES;
use super::pipeline::FrameToken;
use super::{FrameBuffer, HEIGHT, WIDTH};

/// Square comparison granularity in pixels; both native axes divide exactly.
const TILE: usize = 16;
/// Maximum disjoint/merged rectangles before falling back to a full transfer.
const MAX_REGIONS: usize = 16;
/// Maximum row payload starts before conservative full-frame fallback.
const MAX_PAYLOAD_STARTS: usize = 128;

/// A nonempty panel rectangle. Coordinates and extents cannot leave the panel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Region {
    // Native panels have one 320-pixel axis and one 240-pixel axis. Store
    // the short axis in bytes and tag the unused high bit of the long origin.
    // This preserves all rectangles in six bytes, including full-width bands.
    /// Origin on the 320-pixel axis; high bit tags portrait geometry.
    long_origin_and_geometry: u16,
    /// Positive extent on the 320-pixel axis, at most 320.
    long_extent: u16,
    /// Origin on the 240-pixel axis, at most 239.
    short_origin: u8,
    /// Positive extent on the 240-pixel axis, at most 240.
    short_extent: u8,
}

/// Geometry tag outside the representable native coordinate range.
const PORTRAIT_BIT: u16 = 1 << 15;
/// Compile-time channel/future storage-budget constraint.
const _: () = assert!(core::mem::size_of::<Region>() == 6);
/// Encoding assumes the landscape short axis fits in a byte.
const _: () = assert!(Geometry::Landscape.width() == 320 && Geometry::Landscape.height() == 240);
/// Encoding assumes portrait is the exact transposition of the native axes.
const _: () = assert!(Geometry::Portrait.width() == 240 && Geometry::Portrait.height() == 320);

impl Region {
    /// Returns packed `geometry` and pixel rectangle `x`, `y`, `width`, `height`.
    /// Internal callers must supply positive, fully panel-bounded extents before
    /// narrowing the short axis; this encoder does not validate those invariants.
    const fn encode(
        geometry: Geometry,
        x: u16,
        y: u16,
        width: u16,
        height: u16,
    ) -> Self {
        match geometry {
            Geometry::Landscape => {
                Self {
                    long_origin_and_geometry: x,
                    long_extent: width,
                    short_origin: y as u8,
                    short_extent: height as u8,
                }
            }
            Geometry::Portrait => {
                Self {
                    long_origin_and_geometry: y | PORTRAIT_BIT,
                    long_extent: height,
                    short_origin: x as u8,
                    short_extent: width as u8,
                }
            }
        }
    }

    /// Returns the native geometry encoded in the high origin bit.
    const fn geometry(self) -> Geometry {
        if self.long_origin_and_geometry & PORTRAIT_BIT == 0 {
            Geometry::Landscape
        } else {
            Geometry::Portrait
        }
    }

    /// Returns the 320-axis origin with its geometry tag removed.
    const fn long_origin(self) -> u16 { self.long_origin_and_geometry & !PORTRAIT_BIT }

    /// Returns logical leftmost pixel column for the encoded geometry.
    fn x(self) -> u16 {
        match self.geometry() {
            Geometry::Landscape => self.long_origin(),
            Geometry::Portrait => u16::from(self.short_origin),
        }
    }

    /// Returns logical topmost pixel row for the encoded geometry.
    fn y(self) -> u16 {
        match self.geometry() {
            Geometry::Landscape => u16::from(self.short_origin),
            Geometry::Portrait => self.long_origin(),
        }
    }

    /// Returns positive horizontal extent in pixels.
    fn width(self) -> u16 {
        match self.geometry() {
            Geometry::Landscape => self.long_extent,
            Geometry::Portrait => u16::from(self.short_extent),
        }
    }

    /// Returns positive vertical extent in pixels.
    fn height(self) -> u16 {
        match self.geometry() {
            Geometry::Landscape => u16::from(self.short_extent),
            Geometry::Portrait => self.long_extent,
        }
    }

    /// Extends downward by one 16-pixel tile band. Internal callers must ensure
    /// the adjacent band stays inside the native panel before this unchecked add.
    fn extend_height(&mut self) {
        match self.geometry() {
            Geometry::Landscape => self.short_extent += TILE as u8,
            Geometry::Portrait => self.long_extent += TILE as u16,
        }
    }

    /// Returns a landscape rectangle at `x`, `y` with pixel extents `width`,
    /// `height`, or `None` for empty/out-of-panel bounds; additions do not wrap.
    pub fn new(
        x: u16,
        y: u16,
        width: u16,
        height: u16,
    ) -> Option<Self> {
        Self::new_in(Geometry::Landscape, x, y, width, height)
    }

    /// Returns a rectangle in `geometry` at `x`, `y` with positive `width`,
    /// `height` in pixels, or `None` if empty or extending outside the panel.
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
        Some(Self::encode(geometry, x, y, width, height))
    }

    /// Returns the complete 320x240 landscape rectangle.
    pub const fn full() -> Self { Self::full_in(Geometry::Landscape) }

    /// Returns the complete panel rectangle for `geometry`, starting at (0, 0).
    pub const fn full_in(geometry: Geometry) -> Self {
        Self::encode(geometry, 0, 0, geometry.width(), geometry.height())
    }

    /// Returns inclusive first/last columns as big-endian ST7789 CASET parameters.
    pub fn columns(self) -> [[u8; 2]; 2] { [self.x().to_be_bytes(), (self.x() + self.width() - 1).to_be_bytes()] }

    /// Returns inclusive first/last rows as big-endian ST7789 RASET parameters.
    pub fn rows(self) -> [[u8; 2]; 2] { [self.y().to_be_bytes(), (self.y() + self.height() - 1).to_be_bytes()] }

    /// Returns RGB565 payload size, excluding address/command bytes.
    pub fn bytes(self) -> usize { usize::from(self.width()) * usize::from(self.height()) * 2 }

    /// Full-width bands are contiguous. Every narrower row has its own stride.
    /// This iterator is used by both the hardware writer and the panel model.
    /// Returns ordered, nonempty byte ranges inside the 153,600-byte framebuffer.
    pub fn payload_ranges(self) -> impl Iterator<Item = Range<usize>> {
        let stride = usize::from(self.geometry().width());
        let full_width = usize::from(self.width()) == stride;
        let count = if full_width { 1 } else { usize::from(self.height()) };
        (0..count).map(move |row| {
            let start = ((usize::from(self.y()) + row) * stride + usize::from(self.x())) * 2;
            let bytes = if full_width {
                self.bytes()
            } else {
                usize::from(self.width()) * 2
            };
            start..start + bytes
        })
    }
}

/// At most sixteen nonoverlapping tile rectangles, or one full-frame fallback.
#[derive(Clone, Debug)]
pub struct DamagePlan {
    /// Pixel stride and coordinate bounds shared by every region.
    geometry: Geometry,
    /// Transfer order; empty means identical pixels and no bus transactions.
    regions: Vec<Region, MAX_REGIONS>,
}

impl DamagePlan {
    /// Returns a single full landscape region with one contiguous payload.
    pub fn full() -> Self { Self::full_in(Geometry::Landscape) }

    /// Returns a single full region for `geometry`, independent of prior contents.
    pub fn full_in(geometry: Geometry) -> Self {
        let mut regions = Vec::new();
        regions.push(Region::full_in(geometry)).unwrap();
        Self { geometry, regions }
    }

    /// Returns a landscape difference plan from completed `previous` to `current`;
    /// caller must establish that previous pixels still exactly represent the panel.
    pub fn between(
        previous: &FrameBuffer,
        current: &FrameBuffer,
    ) -> Self {
        Self::between_in(Geometry::Landscape, previous, current)
    }

    /// Compares `previous` and `current` using `geometry` and returns changed
    /// 16x16 tile runs, vertically merging equal adjacent runs. Identical buffers
    /// yield an empty plan. Falls back to full at >16 regions, >=75% payload, or
    /// more than 128 payload starts. This is a bounded policy, not a measured break-even;
    /// valid completed-base/orientation admission remains the caller's responsibility.
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
                    .find(|r| r.x() == start as u16 && r.width() == width && r.y() + r.height() == y as u16)
                {
                    region.extend_height();
                } else if plan
                    .regions
                    .push(Region::encode(geometry, start as u16, y as u16, width, TILE as u16))
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

    /// Returns logical coordinates/stride used by all regions.
    pub const fn geometry(&self) -> Geometry { self.geometry }

    /// Returns the ordered region slice, empty for an unchanged frame.
    pub fn regions(&self) -> &[Region] { &self.regions }

    /// Returns whether no pixel payload or panel command is required.
    pub fn is_empty(&self) -> bool { self.regions.is_empty() }

    /// Returns whether this is exactly one native full-frame region.
    pub fn is_full(&self) -> bool { self.regions.as_slice() == [Region::full_in(self.geometry)] }

    /// Returns total RGB565 payload bytes, zero through 153,600, excluding commands.
    pub fn bytes(&self) -> usize { self.regions.iter().map(|r| r.bytes()).sum() }

    /// Returns the number of contiguous payload writes, at most 128 for generated plans.
    pub fn transfer_count(&self) -> usize { self.regions.iter().map(|r| r.payload_ranges().count()).sum() }
}

/// Comparison loops require complete tiles without partial edge reads.
const _: () = assert!(WIDTH.is_multiple_of(TILE) && HEIGHT.is_multiple_of(TILE));

/// History admission failure; rejected operations preserve the previous history.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GenerationError {
    /// A pending transfer owns this render buffer or occupies the queue slot.
    Busy,
    /// Generation is obsolete, mismatched, duplicated or out of sequence.
    Stale,
    /// Partial reference is modified, not completed, same-buffer or differently oriented.
    WrongBase,
}

/// Pixel generations are independent of buffer availability. A free lease is
/// only a panel reference while its exact completed generation is unchanged.
pub struct PanelHistory {
    /// Latest render identity assigned to each buffer, even if later discarded.
    buffers: [Option<FrameToken>; 2],
    /// Immutable orientation assigned when each current buffer generation began.
    orientations: [Orientation; 2],
    /// Address mode of the last successfully completed transfer, if any.
    panel_orientation: Option<Orientation>,
    /// Last completed panel generation; its former buffer may since be overwritten.
    panel: Option<FrameToken>,
    /// At most one submitted generation awaiting completion.
    pending: Option<FrameToken>,
    /// Greatest admitted render ID; render order is global across both buffers.
    last_sequence: u64,
}

impl Default for PanelHistory {
    /// Returns empty history with no trusted panel contents.
    fn default() -> Self { Self::new() }
}
impl PanelHistory {
    /// Returns initial history; the first transfer requires no partial base.
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

    /// Admits landscape `token` before drawing, invalidating its old pixel identity.
    /// Returns success or the busy/stale errors of [`Self::begin_render_in`].
    pub fn begin_render(
        &mut self,
        token: FrameToken,
    ) -> Result<(), GenerationError> {
        self.begin_render_in(token, Orientation::Landscape)
    }

    /// Records new `token` with `orientation` before any pixel mutation. Returns
    /// `Busy` if its buffer is pending, `Stale` if the ID is not globally newer,
    /// or success. Discarded renders still consume IDs and invalidate old references.
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

    /// Returns the completed generation only while its buffer remains unmodified;
    /// `None` means no trustworthy in-memory copy of current panel pixels.
    pub fn reference(&self) -> Option<FrameToken> {
        self.panel
            .filter(|token| self.buffers[token.buffer.index()] == Some(*token))
    }

    /// Returns the last completed orientation, or `None` before any completion.
    pub const fn panel_orientation(&self) -> Option<Orientation> { self.panel_orientation }

    /// The mode that will precede the next submitted frame. Rendering can run
    /// ahead of completion, so the pending request takes precedence over the
    /// current panel. Merely rendering or discarding a frame never changes it.
    /// Returns `None` only before any submission or completed transfer.
    pub fn target_orientation(&self) -> Option<Orientation> {
        self.pending
            .map(|token| self.orientations[token.buffer.index()])
            .or(self.panel_orientation)
    }

    /// Returns an unmodified completed base with exactly `orientation`, including
    /// inversion, or `None` if either its pixels or address mode cannot be reused.
    pub fn reference_for(
        &self,
        orientation: Orientation,
    ) -> Option<FrameToken> {
        self.reference().filter(|_| self.panel_orientation == Some(orientation))
    }

    /// Queues current `token` with optional completed `base`. `None` declares a
    /// full transfer; a partial base must be a distinct, unmodified buffer with
    /// matching orientation. Returns success or busy/stale/wrong-base errors.
    /// Does not inspect the actual damage plan; callers enforce full-plan intent.
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

    /// Commits exactly pending `token` as panel contents and clears the pending
    /// slot. Returns success or `Stale`; software completion must be established
    /// by the transfer owner before calling this, not inferred from elapsed time.
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
