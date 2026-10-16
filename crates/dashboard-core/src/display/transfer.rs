//! ST7789 transaction ordering shared by the device and host fault tests.
use super::FrameBuffer;
use dashboard_config::Orientation;
use super::damage::DamagePlan;

#[allow(async_fn_in_trait)]
pub trait PanelBus {
    type Error;
    /// Send command and parameters; leave CS high even on error.
    async fn command(
        &mut self,
        command: u8,
        parameters: &[u8],
    ) -> Result<(), Self::Error>;
    /// Send RAMWR and hold CS low in data mode; leave CS high on error.
    async fn begin_pixels(&mut self) -> Result<(), Self::Error>;
    /// Preserve CS; invoke callback after polling the first payload write once.
    async fn pixels<F: FnOnce()>(
        &mut self,
        bytes: &[u8],
        on_started: Option<F>,
    ) -> Result<(), Self::Error>;
    /// Called after every successful begin, including a failed payload.
    fn end_pixels(&mut self);
}

/// The framebuffer borrow spans every region. Never cancel an active transfer
/// and release its lease. Empty plans emit no commands and no start callback.
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

/// Source-derived address modes, retaining the existing ML/MH/RGB bits.
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
    programmed: Option<Orientation>,
    completed: Option<Orientation>,
}

impl PanelState {
    /// Successful controller initialization programs orientation but does not
    /// establish any valid pixel contents. The first update must be full.
    pub const fn initialized(orientation: Orientation) -> Self {
        Self { programmed: Some(orientation), completed: None }
    }

    pub const fn programmed_orientation(&self) -> Option<Orientation> { self.programmed }
    pub const fn completed_orientation(&self) -> Option<Orientation> { self.completed }
}

#[derive(Debug, PartialEq, Eq)]
pub enum FlushError<E> {
    GeometryMismatch,
    FullRewriteRequired,
    Bus(E),
}

/// Validate immutable frame metadata before any I/O. A mode change always
/// rewrites all RAM, even when its bytes equal the previous black frame.
/// Only the flusher calls this, after the preceding HAL payload has returned.
/// Cancellation is unsupported: the caller retains the request on failure.
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
        bus.command(0x36, &[madctl(orientation)]).await.map_err(FlushError::Bus)?;
        state.programmed = Some(orientation);
    }
    flush_plan(bus, frame, plan, on_started).await.map_err(FlushError::Bus)?;
    state.completed = Some(orientation);
    Ok(())
}
