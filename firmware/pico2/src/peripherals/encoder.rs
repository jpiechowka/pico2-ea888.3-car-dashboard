//! Adafruit I2C Seesaw Rotary Encoder driver (product 5880).
//!
//! Minimal no_std driver that communicates with the seesaw firmware
//! over I2C to read encoder rotation (delta) and button press state.
//! Publishes events via atomics for consumption by the main render loop.

use core::sync::atomic::{AtomicBool, AtomicI32, Ordering};

use embassy_rp::i2c::{Async, I2c};
use embassy_rp::peripherals::I2C0;
use embassy_time::Timer;
use embedded_hal_async::i2c::I2c as _;

use crate::log_info;

// ---------------------------------------------------------------------------
// Seesaw protocol constants
// ---------------------------------------------------------------------------

/// Default I2C address for the Adafruit 5880 encoder breakout.
const SEESAW_ADDR: u8 = 0x36;

// Module base addresses
const SEESAW_STATUS_BASE: u8 = 0x00;
const SEESAW_GPIO_BASE: u8 = 0x01;
const SEESAW_ENCODER_BASE: u8 = 0x11;

// Status functions
const SEESAW_STATUS_HW_ID: u8 = 0x01;

// GPIO functions
const SEESAW_GPIO_DIRCLR_BULK: u8 = 0x03;
const SEESAW_GPIO_BULK: u8 = 0x04;
const SEESAW_GPIO_BULK_SET: u8 = 0x05;
const SEESAW_GPIO_PULLENSET: u8 = 0x0B;

// Encoder functions
const SEESAW_ENCODER_POSITION: u8 = 0x30;
const SEESAW_ENCODER_DELTA: u8 = 0x40;

/// GPIO pin number for the encoder's built-in push button (on the seesaw).
const SS_SWITCH_PIN: u8 = 24;

// ---------------------------------------------------------------------------
// Shared state (read by main loop on Core 0, written by encoder task on Core 0)
// ---------------------------------------------------------------------------

/// Accumulated encoder rotation delta since the main loop last consumed it.
/// Positive = clockwise, negative = counter-clockwise.
/// The main loop should `swap(0)` to read-and-reset atomically.
pub static ENCODER_DELTA: AtomicI32 = AtomicI32::new(0);

/// Set to `true` on a button press edge (high→low transition).
/// The main loop should `swap(false)` to consume.
pub static ENCODER_BUTTON: AtomicBool = AtomicBool::new(false);

/// Indicates whether the encoder was successfully initialized.
pub static ENCODER_CONNECTED: AtomicBool = AtomicBool::new(false);

// ---------------------------------------------------------------------------
// Low-level seesaw I2C helpers
// ---------------------------------------------------------------------------

/// Write a register command (base + function + optional data).
async fn seesaw_write(
    i2c: &mut I2c<'_, I2C0, Async>,
    base: u8,
    func: u8,
    data: &[u8],
) -> Result<(), embassy_rp::i2c::Error> {
    let mut buf = [0u8; 6];
    buf[0] = base;
    buf[1] = func;
    let len = 2 + data.len().min(4);
    buf[2..len].copy_from_slice(&data[..len - 2]);
    i2c.write(SEESAW_ADDR, &buf[..len]).await
}

/// Read from a seesaw register. The seesaw needs a short delay between
/// the write (register select) and the read (data fetch).
async fn seesaw_read(
    i2c: &mut I2c<'_, I2C0, Async>,
    base: u8,
    func: u8,
    buf: &mut [u8],
) -> Result<(), embassy_rp::i2c::Error> {
    i2c.write(SEESAW_ADDR, &[base, func]).await?;
    Timer::after_micros(500).await;
    i2c.read(SEESAW_ADDR, buf).await
}

// ---------------------------------------------------------------------------
// Encoder initialization
// ---------------------------------------------------------------------------

/// Attempt to initialize the seesaw encoder: verify HW ID, configure the
/// button GPIO with pull-up, and reset the encoder position to zero.
async fn init_encoder(i2c: &mut I2c<'_, I2C0, Async>) -> Result<(), embassy_rp::i2c::Error> {
    // Read hardware ID to verify the seesaw is present
    let mut hw_id = [0u8; 1];
    seesaw_read(i2c, SEESAW_STATUS_BASE, SEESAW_STATUS_HW_ID, &mut hw_id).await?;
    log_info!("Seesaw HW ID: 0x{:02x}", hw_id[0]);

    // Configure button pin (SS_SWITCH=24) as input with pull-up
    let pin_mask = (1u32 << SS_SWITCH_PIN).to_be_bytes();

    // Set as input (clear direction bit)
    seesaw_write(i2c, SEESAW_GPIO_BASE, SEESAW_GPIO_DIRCLR_BULK, &pin_mask).await?;

    // Enable pull-up
    seesaw_write(i2c, SEESAW_GPIO_BASE, SEESAW_GPIO_PULLENSET, &pin_mask).await?;

    // Drive high to select pull-up (not pull-down)
    seesaw_write(i2c, SEESAW_GPIO_BASE, SEESAW_GPIO_BULK_SET, &pin_mask).await?;

    // Reset encoder position to zero
    let zero = 0i32.to_be_bytes();
    seesaw_write(i2c, SEESAW_ENCODER_BASE, SEESAW_ENCODER_POSITION, &zero).await?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Encoder polling task
// ---------------------------------------------------------------------------

/// Read the encoder rotation delta (i32 big-endian). Resets on read.
async fn read_delta(i2c: &mut I2c<'_, I2C0, Async>) -> Result<i32, embassy_rp::i2c::Error> {
    let mut buf = [0u8; 4];
    seesaw_read(i2c, SEESAW_ENCODER_BASE, SEESAW_ENCODER_DELTA, &mut buf).await?;
    Ok(i32::from_be_bytes(buf))
}

/// Read the button state. Returns `true` if the button is currently pressed.
/// The button is active-low (pulled up, reads 0 when pressed).
async fn read_button(i2c: &mut I2c<'_, I2C0, Async>) -> Result<bool, embassy_rp::i2c::Error> {
    let mut buf = [0u8; 4];
    seesaw_read(i2c, SEESAW_GPIO_BASE, SEESAW_GPIO_BULK, &mut buf).await?;
    let gpio_state = u32::from_be_bytes(buf);
    // Active-low: bit is 0 when pressed
    Ok(gpio_state & (1 << SS_SWITCH_PIN) == 0)
}

/// Async task that polls the seesaw encoder at ~50Hz and publishes
/// rotation deltas and button press events via atomics.
///
/// This task runs on Core 0 and should be spawned after display init
/// is complete so encoder events don't fire during boot.
#[embassy_executor::task]
pub async fn encoder_task(mut i2c: I2c<'static, I2C0, Async>) {
    // Small delay to let the seesaw boot after power-up
    Timer::after_millis(50).await;

    // Try to initialize; retry on failure
    let mut init_ok = false;
    for attempt in 0..5u8 {
        match init_encoder(&mut i2c).await {
            Ok(()) => {
                log_info!("Encoder OK (attempt {})", attempt + 1);
                init_ok = true;
                break;
            }
            Err(_e) => {
                log_info!("Enc fail (attempt {})", attempt + 1);
                Timer::after_millis(200).await;
            }
        }
    }

    if !init_ok {
        log_info!("Encoder not found, giving up");
        return;
    }

    ENCODER_CONNECTED.store(true, Ordering::Relaxed);

    let mut prev_button_pressed = false;

    loop {
        // Read encoder delta
        if let Ok(delta) = read_delta(&mut i2c).await {
            if delta != 0 {
                ENCODER_DELTA.fetch_add(delta, Ordering::Relaxed);
            }
        }

        // Read button state for edge detection
        if let Ok(pressed) = read_button(&mut i2c).await {
            // Detect press edge (was not pressed, now pressed)
            if pressed && !prev_button_pressed {
                ENCODER_BUTTON.store(true, Ordering::Relaxed);
            }
            prev_button_pressed = pressed;
        }

        Timer::after_millis(20).await;
    }
}
