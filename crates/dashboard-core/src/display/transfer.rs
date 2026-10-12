//! ST7789 transaction ordering shared by the device and host fault tests.
use super::FrameBuffer;
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
