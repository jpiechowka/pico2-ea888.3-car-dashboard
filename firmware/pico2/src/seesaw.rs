//! Shared Adafruit 5880 seesaw transport; callers own retry and stall recovery.
use dashboard_core::input_model::{decode_encoder_delta, valid_encoder_identity};
use embassy_rp::i2c::{Async, I2c};
use embassy_rp::peripherals::I2C0;
use embassy_time::Timer;
use embedded_hal_async::i2c::I2c as _;

/// Default I2C address for the Adafruit 5880 encoder breakout.
const SEESAW_ADDR: u8 = 0x36;

// Module base addresses
const SEESAW_STATUS_BASE: u8 = 0x00;
const SEESAW_GPIO_BASE: u8 = 0x01;
const SEESAW_ENCODER_BASE: u8 = 0x11;

// Status functions
const SEESAW_STATUS_HW_ID: u8 = 0x01;
const SEESAW_STATUS_VERSION: u8 = 0x02;

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

#[derive(Debug)]
pub enum EncoderError {
    Bus(embassy_rp::i2c::Error),
    Identity,
    Length,
}
impl From<embassy_rp::i2c::Error> for EncoderError {
    fn from(error: embassy_rp::i2c::Error) -> Self { Self::Bus(error) }
}

// ---------------------------------------------------------------------------
// Low-level seesaw I2C helpers
// ---------------------------------------------------------------------------

/// Write a register command (base + function + optional data).
async fn seesaw_write(
    i2c: &mut I2c<'_, I2C0, Async>,
    base: u8,
    func: u8,
    data: &[u8],
) -> Result<(), EncoderError> {
    if data.len() > 4 {
        return Err(EncoderError::Length);
    }
    let mut buf = [0u8; 6];
    buf[0] = base;
    buf[1] = func;
    let len = 2 + data.len();
    buf[2..len].copy_from_slice(&data[..len - 2]);
    i2c.write(SEESAW_ADDR, &buf[..len]).await?;
    Ok(())
}

/// Read from a seesaw register. The seesaw needs a short delay between
/// the write (register select) and the read (data fetch).
async fn seesaw_read(
    i2c: &mut I2c<'_, I2C0, Async>,
    base: u8,
    func: u8,
    buf: &mut [u8],
) -> Result<(), EncoderError> {
    if buf.is_empty() || buf.len() > 4 {
        return Err(EncoderError::Length);
    }
    i2c.write(SEESAW_ADDR, &[base, func]).await?;
    Timer::after_micros(500).await;
    i2c.read(SEESAW_ADDR, buf).await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Encoder initialization
// ---------------------------------------------------------------------------

/// Attempt to initialize the seesaw encoder: verify HW ID, configure the
/// button GPIO with pull-up, and reset the encoder position to zero.
pub async fn init_encoder(i2c: &mut I2c<'_, I2C0, Async>) -> Result<(), EncoderError> {
    // Read hardware ID to verify the seesaw is present
    let mut hw_id = [0u8; 1];
    seesaw_read(i2c, SEESAW_STATUS_BASE, SEESAW_STATUS_HW_ID, &mut hw_id).await?;
    let mut version = [0u8; 4];
    seesaw_read(i2c, SEESAW_STATUS_BASE, SEESAW_STATUS_VERSION, &mut version).await?;
    if !valid_encoder_identity(hw_id[0], version) {
        return Err(EncoderError::Identity);
    }

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

/// Read the encoder rotation delta (i32 big-endian). Resets on read.
pub async fn read_delta(i2c: &mut I2c<'_, I2C0, Async>) -> Result<Option<i32>, EncoderError> {
    let mut buf = [0u8; 4];
    seesaw_read(i2c, SEESAW_ENCODER_BASE, SEESAW_ENCODER_DELTA, &mut buf).await?;
    Ok(decode_encoder_delta(buf))
}

/// Read the button state. Returns `true` if the button is currently pressed.
/// The button is active-low (pulled up, reads 0 when pressed).
pub async fn read_button(i2c: &mut I2c<'_, I2C0, Async>) -> Result<bool, EncoderError> {
    let mut buf = [0u8; 4];
    seesaw_read(i2c, SEESAW_GPIO_BASE, SEESAW_GPIO_BULK, &mut buf).await?;
    let gpio_state = u32::from_be_bytes(buf);
    // Active-low: bit is 0 when pressed
    Ok(gpio_state & (1 << SS_SWITCH_PIN) == 0)
}
