//! Async ST7789 display driver with framebuffer for embassy-rp.
//!
//! This driver uses a full framebuffer (153KB for 320x240 RGB565) and async DMA
//! transfers for flicker-free rendering.

use embassy_rp::gpio::Output;
use embassy_rp::peripherals::SPI0;
use embassy_rp::spi::{Async, Spi};
use embassy_time::Timer;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::pixelcolor::raw::RawU16;
use embedded_graphics::prelude::*;

/// Display dimensions (landscape mode after 90° rotation).
pub const WIDTH: usize = 320;
pub const HEIGHT: usize = 240;
const BUFFER_SIZE: usize = WIDTH * HEIGHT * 2;

/// Static framebuffer (153,600 bytes = ~30% of RP2350's 520KB SRAM).
static mut FRAMEBUFFER: [u8; BUFFER_SIZE] = [0u8; BUFFER_SIZE];

// ST7789 Commands
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

// MADCTL flags
const MADCTL_MX: u8 = 0x40; // Column address order
const MADCTL_MV: u8 = 0x20; // Row/column exchange

/// Async ST7789 display driver with framebuffer.
pub struct St7789<'d> {
    spi: Spi<'d, SPI0, Async>,
    dc: Output<'d>,
    cs: Output<'d>,
    framebuffer: &'static mut [u8],
}

impl<'d> St7789<'d> {
    /// Create a new ST7789 driver (call init() after).
    pub fn new(
        spi: Spi<'d, SPI0, Async>,
        dc: Output<'d>,
        cs: Output<'d>,
    ) -> Self {
        // SAFETY: Single-threaded embedded context, only one display instance
        let framebuffer = unsafe { &mut *core::ptr::addr_of_mut!(FRAMEBUFFER) };
        Self {
            spi,
            dc,
            cs,
            framebuffer,
        }
    }

    /// Initialize the display.
    pub async fn init(&mut self) {
        // Software reset
        self.write_command(SWRESET).await;
        Timer::after_millis(150).await;

        // Exit sleep mode
        self.write_command(SLPOUT).await;
        Timer::after_millis(10).await;

        // Set pixel format to RGB565 (16-bit)
        self.write_command(COLMOD).await;
        self.write_data(&[0x55]).await;

        // Set memory access control for 90° rotation (landscape)
        // MV=1 (row/col exchange), MX=1 (mirror X) = 0x60
        self.write_command(MADCTL).await;
        self.write_data(&[MADCTL_MV | MADCTL_MX]).await;

        // Inversion on (required for PIM715)
        self.write_command(INVON).await;
        Timer::after_millis(10).await;

        // Normal display mode
        self.write_command(NORON).await;
        Timer::after_millis(10).await;

        // Display on
        self.write_command(DISPON).await;
        Timer::after_millis(10).await;
    }

    /// Send a command byte (DC low, CS low during transfer).
    async fn write_command(
        &mut self,
        cmd: u8,
    ) {
        self.cs.set_low();
        self.dc.set_low();
        self.spi.write(&[cmd]).await.ok();
        self.cs.set_high();
    }

    /// Send data bytes (DC high, CS low during transfer).
    async fn write_data(
        &mut self,
        data: &[u8],
    ) {
        self.cs.set_low();
        self.dc.set_high();
        self.spi.write(data).await.ok();
        self.cs.set_high();
    }

    /// Set the drawing window.
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

    /// Flush the framebuffer to the display via async DMA transfer.
    pub async fn flush(&mut self) {
        self.set_window(0, 0, WIDTH as u16, HEIGHT as u16).await;

        // RAMWR command then large data transfer with CS held low
        self.cs.set_low();
        self.dc.set_low();
        self.spi.write(&[RAMWR]).await.ok();
        self.dc.set_high();
        self.spi.write(self.framebuffer).await.ok();
        self.cs.set_high();
    }

    /// Clear the framebuffer with a color.
    pub fn clear_buffer(
        &mut self,
        color: Rgb565,
    ) {
        let raw: RawU16 = color.into();
        let bytes = raw.into_inner().to_be_bytes();

        for chunk in self.framebuffer.chunks_exact_mut(2) {
            chunk[0] = bytes[0];
            chunk[1] = bytes[1];
        }
    }

    /// Set a pixel in the framebuffer.
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

impl OriginDimensions for St7789<'_> {
    fn size(&self) -> Size { Size::new(WIDTH as u32, HEIGHT as u32) }
}

impl DrawTarget for St7789<'_> {
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
        let bytes = raw.into_inner().to_be_bytes();

        for y in drawable_area.rows() {
            let row_start = y as usize * WIDTH * 2;
            let x_start = drawable_area.top_left.x as usize;
            let x_end = x_start + drawable_area.size.width as usize;

            for x in x_start..x_end {
                let idx = row_start + x * 2;
                self.framebuffer[idx] = bytes[0];
                self.framebuffer[idx + 1] = bytes[1];
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
