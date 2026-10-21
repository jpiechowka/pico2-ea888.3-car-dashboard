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

/// Waits for `sm` to drain its TX FIFO and newly stall, clearing a preexisting sticky stall first.
/// The loaded program must stall on empty TX with idle output; FIFO empty alone is not wire completion.
/// Polls at1 us intervals without a deadline. Cancellation leaves the state machine running.
pub(crate) async fn wait_tx_idle<PIO: crate::pio::Instance, const SM: usize>(
    sm: &mut crate::pio::StateMachine<'_, PIO, SM>
) {
    sm.tx().stalled();
    while !sm.tx().empty() || !sm.tx().stalled() {
        embassy_time::Timer::after_micros(1).await;
    }
}
