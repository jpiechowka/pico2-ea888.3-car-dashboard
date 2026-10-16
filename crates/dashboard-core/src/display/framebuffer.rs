//! Fixed-size RGB565 storage and hardware-independent drawing.
use core::ops::{Deref, DerefMut};

use dashboard_config::Geometry;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::pixelcolor::raw::RawU16;
use embedded_graphics::prelude::*;

pub const WIDTH: usize = 320;
pub const HEIGHT: usize = 240;
pub const FRAME_BYTES: usize = WIDTH * HEIGHT * 2;

/// A framebuffer always has the complete panel size and DMA-safe alignment.
/// Mutation requires an exclusive borrow. It is intentionally neither Copy nor Clone.
#[repr(align(4))]
#[derive(PartialEq, Eq)]
pub struct FrameBuffer([u8; FRAME_BYTES]);

impl FrameBuffer {
    pub const fn new() -> Self { Self([0; FRAME_BYTES]) }
}

impl Default for FrameBuffer {
    fn default() -> Self { Self::new() }
}

impl Deref for FrameBuffer {
    type Target = [u8; FRAME_BYTES];

    fn deref(&self) -> &Self::Target { &self.0 }
}

impl DerefMut for FrameBuffer {
    fn deref_mut(&mut self) -> &mut Self::Target { &mut self.0 }
}

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

pub struct St7789Renderer<'a> {
    framebuffer: &'a mut FrameBuffer,
    geometry: Geometry,
}

impl<'a> St7789Renderer<'a> {
    pub fn new(framebuffer: &'a mut FrameBuffer) -> Self { Self::with_geometry(framebuffer, Geometry::Landscape) }

    pub fn with_geometry(
        framebuffer: &'a mut FrameBuffer,
        geometry: Geometry,
    ) -> Self {
        Self { framebuffer, geometry }
    }

    pub const fn geometry(&self) -> Geometry { self.geometry }

    /// Bounded RGB565 row for validated startup assets, in panel byte order.
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
    fn size(&self) -> Size { Size::new(self.geometry.width().into(), self.geometry.height().into()) }
}

impl DrawTarget for St7789Renderer<'_> {
    type Color = Rgb565;
    type Error = core::convert::Infallible;

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

    fn clear(
        &mut self,
        color: Self::Color,
    ) -> Result<(), Self::Error> {
        self.clear_buffer(color);
        Ok(())
    }
}
