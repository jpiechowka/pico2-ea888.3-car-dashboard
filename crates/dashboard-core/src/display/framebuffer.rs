//! Fixed-size RGB565 storage and hardware-independent drawing.
use core::ops::{Deref, DerefMut};

use dashboard_config::Geometry;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::pixelcolor::raw::RawU16;
use embedded_graphics::prelude::*;

/// Landscape width in pixels; portrait swaps the logical axes.
pub const WIDTH: usize = 320;
/// Landscape height in pixels; both geometries contain 76,800 pixels.
pub const HEIGHT: usize = 240;
/// Complete RGB565 allocation size, 153,600 bytes in either orientation.
pub const FRAME_BYTES: usize = WIDTH * HEIGHT * 2;

/// A framebuffer always has the complete panel size and four-byte alignment.
/// Mutation requires an exclusive borrow. It is intentionally neither Copy nor Clone.
#[repr(align(4))]
#[derive(PartialEq, Eq)]
pub struct FrameBuffer(
    /// Row-major RGB565 bytes, most significant byte first per pixel.
    [u8; FRAME_BYTES],
);

impl FrameBuffer {
    /// Returns a full black frame; allocation placement is the caller's responsibility.
    pub const fn new() -> Self { Self([0; FRAME_BYTES]) }
}

impl Default for FrameBuffer {
    /// Returns zeroed pixel storage, equivalent to [`Self::new`].
    fn default() -> Self { Self::new() }
}

impl Deref for FrameBuffer {
    /// Fixed-size wire-order byte array, without geometry metadata.
    type Target = [u8; FRAME_BYTES];

    /// Returns shared access to all panel bytes.
    fn deref(&self) -> &Self::Target { &self.0 }
}

impl DerefMut for FrameBuffer {
    /// Returns exclusive access to all bytes; callers preserve RGB565/geometry meaning.
    fn deref_mut(&mut self) -> &mut Self::Target { &mut self.0 }
}

/// Returns whether nonempty `area` fits embedded-graphics' signed intermediate
/// coordinate arithmetic. This checks representability, not panel intersection.
fn representable_rectangle(area: &embedded_graphics::primitives::Rectangle) -> bool {
    // embedded-graphics first adds size, then subtracts one for the inclusive
    // bottom-right corner. That intermediate must also fit in i32.
    // Reject invalid geometry before calling its intersection/row helpers.
    let (Ok(width), Ok(height)) = (i32::try_from(area.size.width), i32::try_from(area.size.height)) else {
        return false;
    };
    width > 0
        && height > 0
        && area.top_left.x.checked_add(width).is_some()
        && area.top_left.y.checked_add(height).is_some()
}

/// Clipped software drawing into one exclusive frame; no SPI or panel I/O.
pub struct St7789Renderer<'a> {
    /// Complete destination in panel byte order.
    framebuffer: &'a mut FrameBuffer,
    /// Logical dimensions/stride; inversion is applied by panel addressing.
    geometry: Geometry,
}

impl<'a> St7789Renderer<'a> {
    /// Returns a landscape drawing view borrowing `framebuffer`; preserves its pixels.
    pub fn new(framebuffer: &'a mut FrameBuffer) -> Self { Self::with_geometry(framebuffer, Geometry::Landscape) }

    /// Returns a drawing view over `framebuffer` using `geometry` for bounds and
    /// row stride; does not rotate or clear existing bytes.
    pub fn with_geometry(
        framebuffer: &'a mut FrameBuffer,
        geometry: Geometry,
    ) -> Self {
        Self { framebuffer, geometry }
    }

    /// Returns the logical geometry used by all drawing and row access.
    pub const fn geometry(&self) -> Geometry { self.geometry }

    /// Returns mutable RGB565 bytes for zero-based `row`, or `None` outside the
    /// current height. Exactly twice the logical width, in panel byte order.
    pub fn row_bytes_mut(
        &mut self,
        row: usize,
    ) -> Option<&mut [u8]> {
        if row >= usize::from(self.geometry.height()) {
            return None;
        }
        let stride = usize::from(self.geometry.width()) * 2;
        self.framebuffer.get_mut(row * stride..(row + 1) * stride)
    }

    /// Fills every pixel with `color`, writing repeated big-endian RGB565 pairs.
    pub fn clear_buffer(
        &mut self,
        color: Rgb565,
    ) {
        let raw: RawU16 = color.into();
        let pixel = raw.into_inner().to_be();
        let word = (pixel as u32) | ((pixel as u32) << 16);

        for pair in self.framebuffer.as_chunks_mut::<4>().0 {
            pair.copy_from_slice(&word.to_ne_bytes());
        }
    }

    #[inline]
    /// Writes `color` at signed pixel coordinates `x`, `y`; silently ignores
    /// coordinates outside the current geometry without narrowing them first.
    fn set_pixel(
        &mut self,
        x: i32,
        y: i32,
        color: Rgb565,
    ) {
        if x >= 0 && x < i32::from(self.geometry.width()) && y >= 0 && y < i32::from(self.geometry.height()) {
            let idx = (y as usize * usize::from(self.geometry.width()) + x as usize) * 2;
            let raw: RawU16 = color.into();
            let bytes = raw.into_inner().to_be_bytes();
            self.framebuffer[idx] = bytes[0];
            self.framebuffer[idx + 1] = bytes[1];
        }
    }
}

impl OriginDimensions for St7789Renderer<'_> {
    /// Returns logical pixel dimensions, 320x240 or 240x320.
    fn size(&self) -> Size { Size::new(self.geometry.width().into(), self.geometry.height().into()) }
}

impl DrawTarget for St7789Renderer<'_> {
    /// Native 5/6/5-bit RGB color, serialized most significant byte first.
    type Color = Rgb565;
    /// Drawing itself cannot fail; invalid/outside rectangles are ignored.
    type Error = core::convert::Infallible;

    /// Consumes all `pixels`, drawing in-bounds coordinates and clipping others.
    /// Returns success; work is proportional to the iterator, which must terminate.
    fn draw_iter<I>(
        &mut self,
        pixels: I,
    ) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        for Pixel(point, color) in pixels {
            self.set_pixel(point.x, point.y, color);
        }
        Ok(())
    }

    /// Maps row-major `colors` onto `area` before clipping to panel bounds.
    /// Returns success, retaining partial writes if colors end early. Invalid or
    /// wholly off-screen rectangles consume no colors; hidden leading pixels are
    /// skipped to preserve source alignment. Huge clipped prefixes may be costly.
    fn fill_contiguous<I>(
        &mut self,
        area: &embedded_graphics::primitives::Rectangle,
        colors: I,
    ) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Self::Color>,
    {
        if !representable_rectangle(area) {
            return Ok(());
        }
        let drawable_area = area.intersection(&self.bounding_box());
        if drawable_area.size == Size::zero() {
            return Ok(());
        }

        let mut colors = colors.into_iter();
        let source_width = area.size.width as usize;
        let hidden_top = (drawable_area.top_left.y - area.top_left.y) as usize;
        let hidden_left = (drawable_area.top_left.x - area.top_left.x) as usize;
        let visible_width = drawable_area.size.width as usize;
        let visible_height = drawable_area.size.height as usize;
        let hidden_right = source_width - hidden_left - visible_width;

        for _ in 0..hidden_top {
            if colors.nth(source_width - 1).is_none() {
                return Ok(());
            }
        }

        let first_x = drawable_area.top_left.x as usize;
        let first_y = drawable_area.top_left.y as usize;
        for row in 0..visible_height {
            if hidden_left > 0 && colors.nth(hidden_left - 1).is_none() {
                return Ok(());
            }
            let start = ((first_y + row) * usize::from(self.geometry.width()) + first_x) * 2;
            let row_bytes = &mut self.framebuffer[start..start + visible_width * 2];
            for pixel in row_bytes.as_chunks_mut::<2>().0 {
                let Some(color) = colors.next() else {
                    return Ok(());
                };
                let raw: RawU16 = color.into();
                let bytes = raw.into_inner().to_be_bytes();
                *pixel = bytes;
            }
            if row + 1 < visible_height && hidden_right > 0 && colors.nth(hidden_right - 1).is_none() {
                return Ok(());
            }
        }
        Ok(())
    }

    /// Fills the visible intersection of `area` with `color`. Returns success;
    /// empty/unrepresentable rectangles are ignored and work is panel-bounded.
    fn fill_solid(
        &mut self,
        area: &embedded_graphics::primitives::Rectangle,
        color: Self::Color,
    ) -> Result<(), Self::Error> {
        if !representable_rectangle(area) {
            return Ok(());
        }
        let drawable_area = area.intersection(&self.bounding_box());
        if drawable_area.size == Size::zero() {
            return Ok(());
        }

        let raw: RawU16 = color.into();
        let pixel = raw.into_inner().to_be();
        let pixel_bytes = pixel.to_ne_bytes();
        let word = (pixel as u32) | ((pixel as u32) << 16);

        let x_start = drawable_area.top_left.x as usize;
        let width = drawable_area.size.width as usize;

        for y in drawable_area.rows() {
            let start = (y as usize * usize::from(self.geometry.width()) + x_start) * 2;
            let mut row_bytes = &mut self.framebuffer[start..start + width * 2];

            if !x_start.is_multiple_of(2) {
                let (first, rest) = row_bytes.split_at_mut(2);
                first.copy_from_slice(&pixel_bytes);
                row_bytes = rest;
            }

            let (pairs, tail) = row_bytes.as_chunks_mut::<4>();
            for pair in pairs {
                *pair = word.to_ne_bytes();
            }
            if !tail.is_empty() {
                tail.copy_from_slice(&pixel_bytes);
            }
        }
        Ok(())
    }

    /// Fills the whole frame with `color` and returns success; no panel I/O occurs.
    fn clear(
        &mut self,
        color: Self::Color,
    ) -> Result<(), Self::Error> {
        self.clear_buffer(color);
        Ok(())
    }
}
