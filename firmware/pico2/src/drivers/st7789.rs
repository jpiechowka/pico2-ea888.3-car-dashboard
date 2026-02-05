//! Async ST7789 display driver with double buffering for embassy-rp.
//!
//! This driver uses two framebuffers (307KB total for 320x240 RGB565) and async DMA
//! transfers for flicker-free rendering with parallel render/flush.
//!
//! # Architecture
//!
//! The driver is split into two components:
//! - [`St7789Renderer`]: Implements `DrawTarget`, writes to a framebuffer reference
//! - [`St7789Flusher`]: Owns SPI peripheral, handles async DMA transfers
//!
//! Double buffering allows rendering to one buffer while flushing the other,
//! achieving higher frame rates by parallelizing CPU and DMA work.
//!
//! # Performance Optimizations
//!
//! - **Double buffering:** Parallel render/flush for 45-50+ FPS
//! - **32-bit word writes:** `clear()` and `fill_solid()` use 32-bit writes (2 pixels at a time)
//! - **Async DMA:** `flush_buffer()` transfers via DMA without blocking the CPU
//! - **Max SPI speed:** Configured for 62.5 MHz SPI clock (ST7789 maximum)
//! - **Pre-configured window:** Display window is set to full screen during `init()`

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

/// Static framebuffer A (153,600 bytes).
pub static mut FRAMEBUFFER_A: [u8; BUFFER_SIZE] = [0u8; BUFFER_SIZE];
/// Static framebuffer B (153,600 bytes).
pub static mut FRAMEBUFFER_B: [u8; BUFFER_SIZE] = [0u8; BUFFER_SIZE];

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

/// Double buffer manager for parallel render/flush operations.
///
/// Manages two framebuffers and tracks which is currently being rendered to.
/// After rendering completes, call `swap()` to switch buffers and get the
/// index of the completed buffer for flushing.
pub struct DoubleBuffer {
    /// Index of the buffer currently being rendered to (0 or 1).
    render_idx: usize,
}

impl DoubleBuffer {
    /// Create a new double buffer manager.
    ///
    /// # Safety
    /// Must only be called once. The static framebuffers are owned by this instance.
    pub unsafe fn new() -> Self { Self { render_idx: 0 } }

    /// Get a mutable reference to the current render buffer.
    ///
    /// # Safety
    /// Caller must ensure exclusive access to the render buffer.
    #[inline]
    pub unsafe fn render_buffer(&mut self) -> &'static mut [u8] {
        if self.render_idx == 0 {
            unsafe { &mut *core::ptr::addr_of_mut!(FRAMEBUFFER_A) }
        } else {
            unsafe { &mut *core::ptr::addr_of_mut!(FRAMEBUFFER_B) }
        }
    }

    /// Get an immutable reference to a buffer by index for flushing.
    ///
    /// # Safety
    /// Caller must ensure the buffer is not being written to.
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

    /// Swap buffers after rendering completes.
    ///
    /// Returns the index of the buffer that was just rendered to (for flushing).
    /// The next render will use the other buffer.
    #[inline]
    pub fn swap(&mut self) -> usize {
        let completed_idx = self.render_idx;
        self.render_idx = 1 - self.render_idx;
        completed_idx
    }

    /// Get the current render buffer index (for profiling display).
    #[inline]
    pub const fn render_idx(&self) -> usize { self.render_idx }
}

/// ST7789 display flusher - owns SPI and handles async DMA transfers.
///
/// This component is responsible for sending framebuffer data to the display.
/// It should be owned by the flush task for parallel operation.
pub struct St7789Flusher<'d> {
    spi: Spi<'d, SPI0, Async>,
    dc: Output<'d>,
    cs: Output<'d>,
}

impl<'d> St7789Flusher<'d> {
    /// Create a new flusher from SPI and control pins.
    pub fn new(
        spi: Spi<'d, SPI0, Async>,
        dc: Output<'d>,
        cs: Output<'d>,
    ) -> Self {
        Self { spi, dc, cs }
    }

    /// Initialize the display hardware.
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

        // Pre-set window to full screen for flush optimization
        self.set_window(0, 0, WIDTH as u16, HEIGHT as u16).await;
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

    /// Flush a buffer to the display via async DMA transfer.
    ///
    /// Window is pre-configured to full screen during init() for performance.
    pub async fn flush_buffer(
        &mut self,
        buffer: &[u8],
    ) {
        // RAMWR command then large data transfer with CS held low
        self.cs.set_low();
        self.dc.set_low();
        // Use blocking write for single-byte command (faster than DMA setup)
        self.spi.blocking_write(&[RAMWR]).ok();
        self.dc.set_high();
        // Async DMA transfer for the large framebuffer
        self.spi.write(buffer).await.ok();
        self.cs.set_high();
    }
}

/// ST7789 renderer - implements DrawTarget, writes to a framebuffer.
///
/// This component handles all drawing operations. It writes to a framebuffer
/// reference and does not own any hardware. Create a new renderer each frame
/// after swapping buffers.
pub struct St7789Renderer<'a> {
    framebuffer: &'a mut [u8],
}

impl<'a> St7789Renderer<'a> {
    /// Create a new renderer targeting the given framebuffer.
    pub fn new(framebuffer: &'a mut [u8]) -> Self { Self { framebuffer } }

    /// Clear the framebuffer with a color.
    ///
    /// Uses 32-bit word writes for better performance on ARM Cortex-M33.
    pub fn clear_buffer(
        &mut self,
        color: Rgb565,
    ) {
        let raw: RawU16 = color.into();
        let pixel = raw.into_inner().to_be();
        // Pack two pixels into a 32-bit word for faster writes
        let word = (pixel as u32) | ((pixel as u32) << 16);

        // SAFETY: framebuffer is u8 aligned, and we write 4 bytes at a time
        // The buffer size (153600) is divisible by 4
        let ptr = self.framebuffer.as_mut_ptr() as *mut u32;
        let word_count = self.framebuffer.len() / 4;

        for i in 0..word_count {
            // SAFETY: We're writing within the buffer bounds
            unsafe { ptr.add(i).write(word) };
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
        // Pack two pixels into a 32-bit word for faster writes
        let word = (pixel as u32) | ((pixel as u32) << 16);

        let x_start = drawable_area.top_left.x as usize;
        let width = drawable_area.size.width as usize;

        for y in drawable_area.rows() {
            let row_start = y as usize * WIDTH * 2;
            let mut x = x_start;

            // Handle unaligned start (write single pixel if needed)
            if !x.is_multiple_of(2) && x < x_start + width {
                let idx = row_start + x * 2;
                self.framebuffer[idx] = pixel_bytes[0];
                self.framebuffer[idx + 1] = pixel_bytes[1];
                x += 1;
            }

            // Write 32-bit words (2 pixels at a time) for aligned middle section
            let word_end = x_start + width - ((x_start + width - x) % 2);
            while x + 1 < word_end {
                let idx = row_start + x * 2;
                // SAFETY: idx is within bounds and aligned to 2 pixels
                let ptr = unsafe { self.framebuffer.as_mut_ptr().add(idx) as *mut u32 };
                unsafe { ptr.write_unaligned(word) };
                x += 2;
            }

            // Handle remaining pixel if width is odd
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
