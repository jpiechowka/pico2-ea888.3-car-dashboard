//! Hardware drivers for the dashboard.
//!
//! - `st7789`: Custom async ST7789 display driver with DMA and double buffering
//! - `display`: SPI configuration for the Pimoroni PIM715 Display Pack 2.8"

mod display;
mod st7789;

pub use display::{display_spi_config, get_actual_spi_freq};
pub use st7789::{DoubleBuffer, FRAMEBUFFER_A, FRAMEBUFFER_B, St7789Flusher, St7789Renderer};
