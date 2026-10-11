use dashboard_pico2::display::{FrameBuffer, HEIGHT, WIDTH};
use embassy_rp::gpio::Output;
use embassy_rp::peripherals::SPI0;
use embassy_rp::spi::{Async, Spi};
use embassy_time::Timer;

pub static mut FRAMEBUFFER_A: FrameBuffer = FrameBuffer::new();
pub static mut FRAMEBUFFER_B: FrameBuffer = FrameBuffer::new();

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
    pub unsafe fn render_buffer(&mut self) -> &'static mut FrameBuffer {
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
    ) -> &'static FrameBuffer {
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
        buffer: &FrameBuffer,
    ) {
        self.cs.set_low();
        self.dc.set_low();
        self.spi.blocking_write(&[RAMWR]).ok();
        self.dc.set_high();
        self.spi.write(&buffer[..]).await.ok();
        self.cs.set_high();
    }
}
