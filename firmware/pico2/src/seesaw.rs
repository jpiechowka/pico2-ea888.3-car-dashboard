//! Shared Adafruit 5880 seesaw transport; callers own retry and stall recovery.
//! Calls retain exclusive I2C0 ownership across register selection and its delay.
//! Never timeout-drop live I2C: this locked HAL has no transfer-drop cleanup.
//! [Encoder wiring](https://learn.adafruit.com/adafruit-i2c-qt-rotary-encoder/pinouts).
use dashboard_core::input_model::{decode_encoder_delta, valid_encoder_identity};
use embassy_rp::i2c::{Async, I2c};
use embassy_rp::peripherals::I2C0;
use embassy_time::Timer;
use embedded_hal_async::i2c::I2c as _;

/// Default I2C address for the Adafruit 5880 encoder breakout.
const SEESAW_ADDR: u8 = 0x36;

// Module base addresses
/// Seesaw status/identity module selector.
const SEESAW_STATUS_BASE: u8 = 0x00;
/// Seesaw bulk GPIO module selector.
const SEESAW_GPIO_BASE: u8 = 0x01;
/// Seesaw quadrature encoder module selector.
const SEESAW_ENCODER_BASE: u8 = 0x11;

// Status functions
/// One-byte hardware identity register.
const SEESAW_STATUS_HW_ID: u8 = 0x01;
/// Four-byte big-endian product/version identity register.
const SEESAW_STATUS_VERSION: u8 = 0x02;

// GPIO functions
/// Clear GPIO direction bits to select inputs.
const SEESAW_GPIO_DIRCLR_BULK: u8 = 0x03;
/// Read all 32 GPIO input bits, big-endian.
const SEESAW_GPIO_BULK: u8 = 0x04;
/// Set GPIO output/pull-selection bits.
const SEESAW_GPIO_BULK_SET: u8 = 0x05;
/// Enable pull resistors for bits selected by the big-endian mask.
const SEESAW_GPIO_PULLENSET: u8 = 0x0B;

// Encoder functions
/// Absolute signed encoder position; initialization writes zero.
const SEESAW_ENCODER_POSITION: u8 = 0x30;
/// Read-and-clear signed movement since the preceding delta read.
const SEESAW_ENCODER_DELTA: u8 = 0x40;

/// GPIO pin number for the encoder's built-in push button (on the seesaw).
const SS_SWITCH_PIN: u8 = 24;

#[derive(Debug)]
/// Transport, device-identity, or bounded local-command rejection.
pub enum EncoderError {
    /// I2C failure; an error return alone does not establish bus idle.
    Bus(
        /// Original locked-HAL I2C error.
        embassy_rp::i2c::Error,
    ),
    /// Hardware ID or product-version prefix is not the admitted seesaw device.
    Identity,
    /// Local register payload/read length is outside the fixed four-byte bound.
    Length,
}
impl From<embassy_rp::i2c::Error> for EncoderError {
    /// Returns the transport wrapper for `error`, preserving the original cause.
    fn from(error: embassy_rp::i2c::Error) -> Self { Self::Bus(error) }
}

// ---------------------------------------------------------------------------
// Low-level seesaw I2C helpers
// ---------------------------------------------------------------------------

/// Write a register command (base + function + optional data).
/// `i2c` is the exclusive bus; `base`/`func` select the register, and `data`
/// contains at most four bytes. Returns `Length` before I/O or the first bus error.
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
/// `i2c` remains borrowed across selecting `base`/`func`, 500 microseconds, and
/// reading 1..=4 bytes into `buf`. Returns length or bus failure; no retries.
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
/// Borrows `i2c` for the complete sequence. Returns success after reset or the
/// first error; preceding configuration writes remain applied after failure.
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
/// Returns sign-normalized movement from `i2c` within -128..=128, `None` for an
/// implausible raw delta, or a bus error. Rejected movement cannot be reread.
pub async fn read_delta(i2c: &mut I2c<'_, I2C0, Async>) -> Result<Option<i32>, EncoderError> {
    let mut buf = [0u8; 4];
    seesaw_read(i2c, SEESAW_ENCODER_BASE, SEESAW_ENCODER_DELTA, &mut buf).await?;
    Ok(decode_encoder_delta(buf))
}

/// Read the button state. Returns `true` if the button is currently pressed.
/// The button is active-low (pulled up, reads 0 when pressed).
/// Reads through exclusive `i2c`; returns the sampled level or bus failure.
/// Debouncing and press-edge recognition belong to the caller.
pub async fn read_button(i2c: &mut I2c<'_, I2C0, Async>) -> Result<bool, EncoderError> {
    let mut buf = [0u8; 4];
    seesaw_read(i2c, SEESAW_GPIO_BASE, SEESAW_GPIO_BULK, &mut buf).await?;
    let gpio_state = u32::from_be_bytes(buf);
    // Active-low: bit is 0 when pressed
    Ok(gpio_state & (1 << SS_SWITCH_PIN) == 0)
}
