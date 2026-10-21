//! Dashboard encoder polling, publication and flash-maintenance boundary.
use core::sync::atomic::{AtomicBool, Ordering};

use dashboard_pico2::input_model::{DebouncedButton, EncoderEvents};
use dashboard_pico2::input_trace::Observation;
use dashboard_pico2::seesaw::{EncoderError, init_encoder, read_button, read_delta};
use embassy_rp::i2c::{Async, I2c};
use embassy_rp::peripherals::I2C0;
use embassy_time::{Instant, Timer};

use crate::log_info;

/// Accumulated encoder rotation delta since the main loop last consumed it.
/// Positive = clockwise, negative = counter-clockwise.
/// Rotation and press streams have explicit saturation/drop policies.
pub static ENCODER_EVENTS: EncoderEvents = EncoderEvents::new();

/// Indicates whether the encoder was successfully initialized.
pub static ENCODER_CONNECTED: AtomicBool = AtomicBool::new(false);

/// Async task that polls the seesaw encoder at ~50Hz and publishes
/// rotation deltas and button press events with coherent software timestamps.
///
/// This task runs on Core 0 and should be spawned after display init
/// is complete so encoder events don't fire during boot.
/// Owns `i2c` indefinitely. Errors clear queued input and retry after 200 ms;
/// success waits 20 ms after I/O, so 50 Hz is an upper scheduling target.
/// Maintenance resumes reinitialize the device and consume unobserved motion.
/// Never cancel live I2C: stalled hardware remains owned until watchdog recovery.
#[embassy_executor::task]
pub async fn encoder_task(mut i2c: I2c<'static, I2C0, Async>) {
    // Small delay to let the seesaw boot after power-up
    Timer::after_millis(50).await;

    let mut connected = false;
    let mut button = DebouncedButton::new();
    let mut reported_error = false;
    let mut last_resume = None;
    loop {
        if crate::tasks::maintenance::input_boundary(&mut last_resume).await {
            button = DebouncedButton::new();
            ENCODER_EVENTS.clear();
            // Reinitialization consumes motion accumulated while unobserved.
            connected = false;
            ENCODER_CONNECTED.store(false, Ordering::Relaxed);
        }
        // No cancelling timeout: HAL I2C has no transfer-drop cleanup. A stuck
        // future retains its owner; the independent watchdog observes no input
        // progress and resets. Ordinary NACKs retry without resetting the board.
        let result = async {
            if !connected {
                init_encoder(&mut i2c).await?;
                // Consume stale motion from before startup/reconnect.
                let _ = read_delta(&mut i2c).await?;
                button = DebouncedButton::new();
                connected = true;
                ENCODER_CONNECTED.store(true, Ordering::Relaxed);
                reported_error = false;
                log_info!("Encoder connected");
            }
            let delta = read_delta(&mut i2c).await?;
            let delta_at = Instant::now().as_micros();
            let pressed = read_button(&mut i2c).await?;
            if let Some(delta) = delta {
                ENCODER_EVENTS.publish_delta_at(delta, delta_at);
            } else {
                ENCODER_EVENTS.drop_event();
            }
            let now = Instant::now();
            if let Some(since) = button.update_with_origin(pressed, now.as_millis())
                && let Some(observation) = since
                    .checked_mul(1000)
                    .and_then(|at| Observation::new(at, now.as_micros()))
            {
                ENCODER_EVENTS.publish_press_at(observation);
            }
            Ok::<(), EncoderError>(())
        }
        .await;
        crate::tasks::recovery::INPUT_PROGRESS.fetch_add(1, Ordering::Relaxed);
        if let Err(error) = result {
            connected = false;
            ENCODER_CONNECTED.store(false, Ordering::Relaxed);
            ENCODER_EVENTS.clear();
            if !reported_error {
                match error {
                    EncoderError::Bus(cause) => crate::log_fault!("Encoder bus: {:?}", cause),
                    _ => crate::log_fault!("Encoder unavailable: {:?}", error),
                }
                reported_error = true;
            }
            Timer::after_millis(200).await;
        } else {
            Timer::after_millis(20).await;
        }
    }
}
