use embassy_rp::gpio::Output;
use embassy_rp::peripherals::SPI0;
use embassy_rp::spi::{Async, Spi};
use embassy_time::Timer;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::pixelcolor::raw::RawU16;
use embedded_graphics::prelude::*;

pub const WIDTH: usize = 320;
pub const HEIGHT: usize = 240;
const BUFFER_SIZE: usize = WIDTH * HEIGHT * 2;

pub static mut FRAMEBUFFER_A: [u8; BUFFER_SIZE] = [0u8; BUFFER_SIZE];
pub static mut FRAMEBUFFER_B: [u8; BUFFER_SIZE] = [0u8; BUFFER_SIZE];

const SWRESET: u8 = 0x01;
const SLPOUT: u8 = 0x11;
const NORON: u8 = 0x13;
const INVON: u8 = 0x21;
const DISPON: u8 = 0x29;
const CASET: u8 = 0x2A;
const RASET: u8 = 0x2B;
const RAMWR: u8 = 0x2C;
const MADCTL: u8 = 0x36;
const COLMOD: u8 = 0x3A;

const MADCTL_MX: u8 = 0x40;
const MADCTL_MV: u8 = 0x20;

pub struct DoubleBuffer {
    render_idx: usize,
}

impl DoubleBuffer {
    pub unsafe fn new() -> Self { Self { render_idx: 0 } }

    #[inline]
    pub unsafe fn render_buffer(&mut self) -> &'static mut [u8] {
        if self.render_idx == 0 {
            unsafe { &mut *core::ptr::addr_of_mut!(FRAMEBUFFER_A) }
        } else {
            unsafe { &mut *core::ptr::addr_of_mut!(FRAMEBUFFER_B) }
        }
    }

    #[inline]
    pub unsafe fn get_buffer(
        &self,
        idx: usize,
    ) -> &'static [u8] {
        if idx == 0 {
            unsafe { &*core::ptr::addr_of!(FRAMEBUFFER_A) }
        } else {
            unsafe { &*core::ptr::addr_of!(FRAMEBUFFER_B) }
        }
    }

    #[inline]
    pub fn swap(&mut self) -> usize {
        let completed_idx = self.render_idx;
        self.render_idx = 1 - self.render_idx;
        completed_idx
    }

    #[inline]
    pub const fn render_idx(&self) -> usize { self.render_idx }
}

pub struct St7789Flusher<'d> {
    spi: Spi<'d, SPI0, Async>,
    dc: Output<'d>,
    cs: Output<'d>,
}

impl<'d> St7789Flusher<'d> {
    pub fn new(
        spi: Spi<'d, SPI0, Async>,
        dc: Output<'d>,
        cs: Output<'d>,
    ) -> Self {
        Self { spi, dc, cs }
    }

    pub async fn init(&mut self) {
        self.write_command(SWRESET).await;
        Timer::after_millis(150).await;

        self.write_command(SLPOUT).await;
        Timer::after_millis(10).await;

        self.write_command(COLMOD).await;
        self.write_data(&[0x55]).await;

        self.write_command(MADCTL).await;
        self.write_data(&[MADCTL_MV | MADCTL_MX]).await;

        self.write_command(INVON).await;
        Timer::after_millis(10).await;

        self.write_command(NORON).await;
        Timer::after_millis(10).await;

        self.write_command(DISPON).await;
        Timer::after_millis(10).await;

        self.set_window(0, 0, WIDTH as u16, HEIGHT as u16).await;
    }

    async fn write_command(
        &mut self,
        cmd: u8,
    ) {
        self.cs.set_low();
        self.dc.set_low();
        self.spi.write(&[cmd]).await.ok();
        self.cs.set_high();
    }

    async fn write_data(
        &mut self,
        data: &[u8],
    ) {
        self.cs.set_low();
        self.dc.set_high();
        self.spi.write(data).await.ok();
        self.cs.set_high();
    }

    async fn set_window(
        &mut self,
        x: u16,
        y: u16,
        w: u16,
        h: u16,
    ) {
        let x1 = x + w - 1;
        let y1 = y + h - 1;

        self.write_command(CASET).await;
        self.write_data(&[(x >> 8) as u8, x as u8, (x1 >> 8) as u8, x1 as u8])
            .await;

        self.write_command(RASET).await;
        self.write_data(&[(y >> 8) as u8, y as u8, (y1 >> 8) as u8, y1 as u8])
            .await;
    }

    pub async fn flush_buffer(
        &mut self,
        buffer: &[u8],
    ) {
        self.cs.set_low();
        self.dc.set_low();
        self.spi.blocking_write(&[RAMWR]).ok();
        self.dc.set_high();
        self.spi.write(buffer).await.ok();
        self.cs.set_high();
    }
}

pub struct St7789Renderer<'a> {
    framebuffer: &'a mut [u8],
}

impl<'a> St7789Renderer<'a> {
    pub fn new(framebuffer: &'a mut [u8]) -> Self { Self { framebuffer } }

    pub fn clear_buffer(
        &mut self,
        color: Rgb565,
    ) {
        let raw: RawU16 = color.into();
        let pixel = raw.into_inner().to_be();
        let word = (pixel as u32) | ((pixel as u32) << 16);

        let ptr = self.framebuffer.as_mut_ptr() as *mut u32;
        let word_count = self.framebuffer.len() / 4;

        for i in 0..word_count {
            unsafe { ptr.add(i).write(word) };
        }
    }

    #[inline]
    fn set_pixel(
        &mut self,
        x: i32,
        y: i32,
        color: Rgb565,
    ) {
        if x >= 0 && x < WIDTH as i32 && y >= 0 && y < HEIGHT as i32 {
            let idx = (y as usize * WIDTH + x as usize) * 2;
            let raw: RawU16 = color.into();
            let bytes = raw.into_inner().to_be_bytes();
            self.framebuffer[idx] = bytes[0];
            self.framebuffer[idx + 1] = bytes[1];
        }
    }
}

impl OriginDimensions for St7789Renderer<'_> {
    fn size(&self) -> Size { Size::new(WIDTH as u32, HEIGHT as u32) }
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
        let drawable_area = area.intersection(&self.bounding_box());
        if drawable_area.size == Size::zero() {
            return Ok(());
        }

        let mut colors = colors.into_iter();
        for y in drawable_area.rows() {
            for x in drawable_area.columns() {
                if let Some(color) = colors.next() {
                    let idx = (y as usize * WIDTH + x as usize) * 2;
                    let raw: RawU16 = color.into();
                    let bytes = raw.into_inner().to_be_bytes();
                    self.framebuffer[idx] = bytes[0];
                    self.framebuffer[idx + 1] = bytes[1];
                }
            }
        }
        Ok(())
    }

    fn fill_solid(
        &mut self,
        area: &embedded_graphics::primitives::Rectangle,
        color: Self::Color,
    ) -> Result<(), Self::Error> {
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
            let row_start = y as usize * WIDTH * 2;
            let mut x = x_start;

            if !x.is_multiple_of(2) && x < x_start + width {
                let idx = row_start + x * 2;
                self.framebuffer[idx] = pixel_bytes[0];
                self.framebuffer[idx + 1] = pixel_bytes[1];
                x += 1;
            }

            let word_end = x_start + width - ((x_start + width - x) % 2);
            while x + 1 < word_end {
                let idx = row_start + x * 2;
                let ptr = unsafe { self.framebuffer.as_mut_ptr().add(idx) as *mut u32 };
                unsafe { ptr.write_unaligned(word) };
                x += 2;
            }

            if x < x_start + width {
                let idx = row_start + x * 2;
                self.framebuffer[idx] = pixel_bytes[0];
                self.framebuffer[idx + 1] = pixel_bytes[1];
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
