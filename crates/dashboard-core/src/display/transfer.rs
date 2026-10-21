//! ST7789 transaction ordering shared by the device and host fault tests.
use dashboard_config::Orientation;

use super::FrameBuffer;
use super::damage::DamagePlan;

/// Exclusive ST7789 command/payload owner. Implementations retain byte borrows
/// until the underlying transfer is finished. Async cancellation is unsupported
/// by this protocol; ordinary returned errors must obey the CS rules below.
#[allow(async_fn_in_trait)]
pub trait PanelBus {
    /// Underlying bus failure, potentially after partial command/pixel effects.
    type Error;
    /// Sends `command` with DC low and `parameters` with DC high. Returns success
    /// or a transport error; leaves CS high on either normal return.
    async fn command(
        &mut self,
        command: u8,
        parameters: &[u8],
    ) -> Result<(), Self::Error>;
    /// Sends RAMWR and returns success with CS low/data mode ready for payloads,
    /// or a transport error with CS high.
    async fn begin_pixels(&mut self) -> Result<(), Self::Error>;
    /// Writes wire-order RGB565 `bytes`, preserving CS. Invokes `on_started`, if
    /// present, after first polling the actual payload future. Returns the bus
    /// result; an error may follow partial pixels. Completion does not imply scanout.
    async fn pixels<F: FnOnce()>(
        &mut self,
        bytes: &[u8],
        on_started: Option<F>,
    ) -> Result<(), Self::Error>;
    /// Releases CS after every successful begin, including a returned payload
    /// error. The caller must already have awaited the payload before invoking it.
    fn end_pixels(&mut self);
}

/// The framebuffer borrow spans every region. Never cancel an active transfer
/// and release its lease. Empty plans emit no commands and no start callback.
/// Uses exclusive `bus` to send immutable `frame` bytes selected by `plan`;
/// invokes `on_started` at the first payload's first poll. Returns success or the
/// first bus error, releasing CS after returned payload errors. Cancellation may
/// bypass that cleanup; earlier regions can remain changed on failure.
pub async fn flush_plan<B: PanelBus>(
    bus: &mut B,
    frame: &FrameBuffer,
    plan: &DamagePlan,
    on_started: impl FnOnce(),
) -> Result<(), B::Error> {
    let mut on_started = Some(on_started);
    for region in plan.regions() {
        bus.command(0x2A, region.columns().as_flattened()).await?;
        bus.command(0x2B, region.rows().as_flattened()).await?;
        bus.begin_pixels().await?;
        for range in region.payload_ranges() {
            if let Err(error) = bus.pixels(&frame[range], on_started.take()).await {
                bus.end_pixels();
                return Err(error);
            }
        }
        bus.end_pixels();
    }
    Ok(())
}

/// Returns the MADCTL byte for `orientation`; ML/MH/RGB remain zero in this
/// application's existing mapping. Bit meanings and native window dimensions:
/// [Pimoroni ST7789 driver](https://github.com/pimoroni/pimoroni-pico/blob/main/drivers/st7789/st7789.cpp).
/// The upstream driver uses a different ML policy for landscape.
pub const fn madctl(orientation: Orientation) -> u8 {
    match orientation {
        Orientation::Landscape => 0x60,
        Orientation::Portrait => 0x00,
        Orientation::LandscapeInverted => 0xA0,
        Orientation::PortraitInverted => 0xC0,
    }
}

/// Owned by the flusher. Register state and successfully completed pixels are
/// separate: a payload failure can leave MADCTL programmed but content unknown.
pub struct PanelState {
    /// Last known successful MADCTL mode; `None` after a failed mode command.
    programmed: Option<Orientation>,
    /// Last fully completed payload's mode; `None` during transfer/after failure.
    completed: Option<Orientation>,
}

impl PanelState {
    /// Successful controller initialization programs orientation but does not
    /// establish any valid pixel contents. The first update must be full.
    /// Returns tracking state for the already programmed `orientation`; does no I/O.
    pub const fn initialized(orientation: Orientation) -> Self {
        Self {
            programmed: Some(orientation),
            completed: None,
        }
    }

    /// Returns known register orientation, or `None` if a mode write left it uncertain.
    pub const fn programmed_orientation(&self) -> Option<Orientation> { self.programmed }

    /// Returns the fully completed pixel orientation, or `None` while contents are unknown.
    pub const fn completed_orientation(&self) -> Option<Orientation> { self.completed }
}

/// Admission or I/O error; only bus errors may follow partial panel mutation.
#[derive(Debug, PartialEq, Eq)]
pub enum FlushError<E> {
    /// Logical plan dimensions disagree with the submitted orientation.
    GeometryMismatch,
    /// New/unknown pixel orientation requires exactly one full-frame plan.
    FullRewriteRequired,
    /// Underlying command or payload operation failed.
    Bus(
        /// Original bus error; state already records the resulting uncertainty.
        E,
    ),
}

/// Validate immutable frame metadata before any I/O. A mode change always
/// rewrites all RAM, even when its bytes equal the previous black frame.
/// Only the flusher calls this, after the preceding HAL payload has returned.
/// Cancellation is unsupported: the caller retains the request on failure.
/// Uses `bus` and updates `state` while transferring immutable `frame` according
/// to `plan` in `orientation`; `on_started` observes the first payload poll.
/// Returns admission errors before I/O, a bus error with completed pixels marked
/// unknown, or success with the orientation committed. Empty plans succeed only
/// when the same orientation already has valid completed contents.
pub async fn flush_oriented_plan<B: PanelBus>(
    bus: &mut B,
    state: &mut PanelState,
    frame: &FrameBuffer,
    plan: &DamagePlan,
    orientation: Orientation,
    on_started: impl FnOnce(),
) -> Result<(), FlushError<B::Error>> {
    if plan.geometry() != orientation.geometry() {
        return Err(FlushError::GeometryMismatch);
    }
    let reprogram = state.programmed != Some(orientation);
    if (reprogram || state.completed != Some(orientation)) && !plan.is_full() {
        return Err(FlushError::FullRewriteRequired);
    }
    if plan.is_empty() {
        return Ok(());
    }
    state.completed = None;
    if reprogram {
        // Even a command-parameter failure leaves address mode uncertain.
        state.programmed = None;
        bus.command(0x36, &[madctl(orientation)])
            .await
            .map_err(FlushError::Bus)?;
        state.programmed = Some(orientation);
    }
    flush_plan(bus, frame, plan, on_started)
        .await
        .map_err(FlushError::Bus)?;
    state.completed = Some(orientation);
    Ok(())
}
