//! Hardware adapters retaining exclusive peripheral ownership across asynchronous work.
/// Display SPI setup and live divider diagnostics.
mod display;
#[cfg(feature = "longan-rx12")]
/// Optional core1 buffered UART adapter for the historical wired route.
pub mod longan;
/// Retained inverted-PWM RGB ownership for PIM715.
pub mod rgb_led;
/// Settings journal adapter over the shared checked flash handle.
pub mod settings_flash;
/// Sole panel command/payload transmitter retaining DMA buffers.
mod st7789;
/// RP2350 reset-mask correction and retained recovery evidence.
pub mod watchdog;

pub use dashboard_pico2::display::St7789Renderer;
pub use display::{display_spi_config, get_actual_spi_freq};
pub use st7789::St7789Flusher;
