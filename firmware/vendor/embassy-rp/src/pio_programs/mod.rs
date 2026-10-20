//! Pre-built pio programs for common interfaces

pub mod clk;
pub mod clock_divider;
pub mod hd44780;
pub mod i2s;
pub mod onewire;
pub mod pwm;
pub mod rotary_encoder;
pub mod spi;
pub mod stepper;
pub mod uart;
pub mod ws2812;

// Programs using this helper must stall on an empty TX FIFO with the output idle.
// Clear the sticky flag first so an earlier idle period cannot complete a transfer.
pub(crate) async fn wait_tx_idle<PIO: crate::pio::Instance, const SM: usize>(
    sm: &mut crate::pio::StateMachine<'_, PIO, SM>,
) {
    sm.tx().stalled();
    while !sm.tx().empty() || !sm.tx().stalled() {
        embassy_time::Timer::after_micros(1).await;
    }
}
