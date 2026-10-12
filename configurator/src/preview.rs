//! Exact shared framebuffer rendering with explicitly synthetic inputs.

use dashboard_config::{Channel, Config, ValidatedConfig, validate};
use dashboard_core::acquisition::{AcquisitionModel, DashboardSnapshot};
use dashboard_core::colors::{BLACK, RED};
use dashboard_core::configured_scene::Layout;
use dashboard_core::dashboard::{DashboardScene, Overlay};
use dashboard_core::demo;
use dashboard_core::display::{FrameBuffer, St7789Renderer};
pub use dashboard_core::display::{HEIGHT, WIDTH};
use dashboard_core::render::FpsMode;
use dashboard_core::sensor_state::{GRAPH_HISTORY_SIZE, GRAPH_SAMPLE_PERIOD_MS};
use dashboard_core::vehicle_status::VehicleStatus;
use dashboard_core::view_state::{WarningPage, WarningPager};

/// Illustrative canonical values, never recommended vehicle operating limits.
pub const WARNING_VALUES: [f32; 8] = [3.0, 150.0, 120.0, 130.0, 80.0, 1100.0, 10.0, 1.3];
const HISTORY_WINDOW_MS: u64 = (GRAPH_HISTORY_SIZE as u64 - 1) * GRAPH_SAMPLE_PERIOD_MS;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PreviewOptions {
    /// Advances the synthetic history and warning-page clock. The generated
    /// timeline includes a fixed history window before the final rendered time.
    pub sample_ms: u64,
    pub boost_psi: bool,
    /// Demonstrate warning appearance with activation dwell compressed to 1 ms
    /// in a private simulation copy. This is not a firmware alarm timing test.
    pub warnings: bool,
    /// Optional canonical channel values in `Channel::ALL` order. Always Demo.
    pub values: Option<[f32; 8]>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PreviewError {
    UnknownView(u8),
    InvalidSample(Channel),
    TimeOverflow,
    BufferLength,
}

pub struct Preview {
    framebuffer: Box<FrameBuffer>,
    rgba: Vec<u8>,
    snapshot: Option<DashboardSnapshot>,
    warning: Option<WarningPage>,
    pager: WarningPager,
    configuration: Option<Box<Config>>,
    options: Option<PreviewOptions>,
    rendered_at: Option<u64>,
}

impl Default for Preview {
    fn default() -> Self { Self::new() }
}

impl Preview {
    pub fn new() -> Self {
        Self {
            framebuffer: Box::new(FrameBuffer::new()),
            rgba: vec![0; WIDTH * HEIGHT * 4],
            snapshot: None,
            warning: None,
            pager: WarningPager::new(),
            configuration: None,
            options: None,
            rendered_at: None,
        }
    }

    /// Render through the same acquisition statistics, layout and DashboardScene
    /// as firmware. Both complete pixel buffers are allocated once and reused.
    /// All recoverable errors leave the previous complete render and pager intact.
    pub fn render(
        &mut self,
        config: &ValidatedConfig,
        view: u8,
        options: PreviewOptions,
    ) -> Result<&[u8], PreviewError> {
        let layout = Layout::new(config, view, false).ok_or(PreviewError::UnknownView(view))?;
        let now = options
            .sample_ms
            .checked_add(HISTORY_WINDOW_MS + 1)
            .ok_or(PreviewError::TimeOverflow)?;
        if let Some(values) = options.values {
            for (channel, value) in Channel::ALL.into_iter().zip(values) {
                // Same finite magnitude bound as SensorState's sample gate.
                if !value.is_finite() || value.abs() > 1_000_000.0 {
                    return Err(PreviewError::InvalidSample(channel));
                }
            }
        }

        // Appearance demonstration only: avoid unbounded work proportional to
        // dwell/freshness. The original thresholds, context gates and order stay
        // intact, while the edited document is never changed by this simulation.
        let simulation = options.warnings.then(|| {
            let mut copy = config.config().clone();
            for widget in &mut copy.widgets {
                widget.warning.on_ms = 1;
            }
            validate(copy).expect("1 ms activation dwell is valid for every rule")
        });
        let mut model = AcquisitionModel::demo_configured(simulation.as_ref().unwrap_or(config));
        let values_at = |at| {
            options.values.unwrap_or_else(|| {
                if options.warnings {
                    WARNING_VALUES
                } else {
                    demo::values(at)
                }
            })
        };
        for index in 0..GRAPH_HISTORY_SIZE {
            let at = options.sample_ms + index as u64 * GRAPH_SAMPLE_PERIOD_MS;
            let accepted = model.ingest_batch(values_at(at), at);
            debug_assert!(accepted, "bounded finite synthetic sample");
        }
        let accepted = model.ingest_batch(values_at(now), now);
        debug_assert!(accepted, "one final sample advances the visual demo dwell");
        let snapshot = model.snapshot();

        let changed_config = self.configuration.as_deref() != Some(config.config());
        if changed_config
            || self.options.is_none_or(|previous| {
                previous.warnings != options.warnings
                    || previous.values != options.values
                    || options.sample_ms < previous.sample_ms
            })
        {
            self.pager = WarningPager::new();
        }
        let blink = options.warnings && (options.sample_ms / 500).is_multiple_of(2);
        let warning = if options.warnings {
            self.pager.select(config, &snapshot, view, now, blink)
        } else {
            self.pager.pause(now);
            None
        };
        let scene = DashboardScene {
            layout: Some(layout),
            measurements: &snapshot,
            status: VehicleStatus::Demo,
            now_ms: now,
            blink_on: blink,
            boost_psi: options.boost_psi,
            fps_mode: FpsMode::Off,
            fps_instant: 0.0,
            fps_average: 0.0,
            backgrounds: core::array::from_fn(|slot| {
                if blink && layout.alarm_active(slot, &snapshot, now) {
                    RED
                } else {
                    BLACK
                }
            }),
            overlay: warning.map(Overlay::Warnings),
        };
        scene.draw_full(&mut St7789Renderer::new(&mut self.framebuffer));
        rgb565_to_rgba(&self.framebuffer[..], &mut self.rgba).expect("fixed framebuffer and RGBA dimensions");
        self.snapshot = Some(snapshot);
        self.warning = warning;
        self.options = Some(options);
        self.rendered_at = Some(now);
        if changed_config {
            self.configuration = Some(Box::new(config.config().clone()));
        }
        Ok(&self.rgba)
    }

    pub fn rgb565(&self) -> &[u8] { &self.framebuffer[..] }

    pub fn rgba(&self) -> &[u8] { &self.rgba }

    pub fn snapshot(&self) -> Option<&DashboardSnapshot> { self.snapshot.as_ref() }

    pub fn warning_page(&self) -> Option<WarningPage> { self.warning }

    pub fn rendered_at_ms(&self) -> Option<u64> { self.rendered_at }
}

/// Convert panel-order big-endian words, with bit replication to full 8-bit
/// channels and opaque alpha. Length mismatches never partially write output.
pub fn rgb565_to_rgba(
    rgb565: &[u8],
    rgba: &mut [u8],
) -> Result<(), PreviewError> {
    if !rgb565.len().is_multiple_of(2) || rgb565.len().checked_mul(2) != Some(rgba.len()) {
        return Err(PreviewError::BufferLength);
    }
    for (word, pixel) in rgb565.as_chunks::<2>().0.iter().zip(rgba.as_chunks_mut::<4>().0) {
        let word = u16::from_be_bytes([word[0], word[1]]);
        let red = (word >> 11) as u8;
        let green = ((word >> 5) & 63) as u8;
        let blue = (word & 31) as u8;
        pixel.copy_from_slice(&[red << 3 | red >> 2, green << 2 | green >> 4, blue << 3 | blue >> 2, 255]);
    }
    Ok(())
}
