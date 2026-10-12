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

/// Explicit synthetic inspection modes. Firmware demo always uses the shared
/// elapsed-time generator; these deliberate stress inputs are preview-only.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SampleSet {
    #[default]
    Demo,
    Minimum,
    Maximum,
    WidthStress,
    Zero,
}

impl SampleSet {
    pub fn values(self) -> Option<[f32; 8]> {
        match self {
            Self::Demo => None,
            Self::Minimum => Some(Channel::ALL.map(|c| c.domain().0)),
            Self::Maximum => Some(Channel::ALL.map(|c| c.domain().1)),
            Self::WidthStress => {
                Some([
                    1_000_000.0,
                    -1_000_000.0,
                    1_000_000.0,
                    -1_000_000.0,
                    -1_000_000.0,
                    1_000_000.0,
                    1_000_000.0,
                    1_000_000.0,
                ])
            }
            Self::Zero => Some([0.0; 8]),
        }
    }
}

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
        let mut snapshot = model.snapshot();
        let same_run = self.options.is_some_and(|previous| {
            previous.warnings == options.warnings
                && previous.values == options.values
                && options.sample_ms >= previous.sample_ms
        });
        if same_run && let (Some(previous), Some(configuration)) = (&self.snapshot, &self.configuration) {
            for channel in Channel::ALL {
                if configuration.sources[channel.index()] == config.config().sources[channel.index()] {
                    snapshot.sensors[channel.index()].retain_extrema_from(&previous.sensors[channel.index()]);
                }
            }
        }

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

    /// Inspection-only rendering. Each attempted widget is independently
    /// validated after safe fallbacks; the rejected document itself is never
    /// passed to firmware rendering/acquisition. Warning simulation is suspended.
    pub fn render_diagnostic(
        &mut self,
        accepted: &ValidatedConfig,
        draft: &crate::diagnostic::Draft,
        view: u8,
        mut options: PreviewOptions,
    ) -> Result<&[u8], PreviewError> {
        use dashboard_core::colors::WHITE;
        use embedded_graphics::mono_font::MonoTextStyle;
        use embedded_graphics::mono_font::ascii::FONT_6X10;
        use embedded_graphics::prelude::*;
        use embedded_graphics::primitives::{PrimitiveStyle, Rectangle};
        use embedded_graphics::text::Text;

        options.warnings = false;
        self.render(accepted, view, options)?;
        let snapshot = self.snapshot.as_ref().expect("successful render has a snapshot");
        let now = self.rendered_at.expect("successful render has a timestamp");
        let mut display = St7789Renderer::new(&mut self.framebuffer);
        Rectangle::new(Point::new(0, 26), Size::new(320, 214))
            .into_styled(PrimitiveStyle::with_fill(BLACK))
            .draw(&mut display)
            .ok();
        for widget in draft.config.widgets.iter().filter(|w| w.view == view) {
            let Some(config) = crate::diagnostic::isolated_widget(accepted, widget) else {
                continue;
            };
            let scene = DashboardScene {
                layout: Layout::new(&config, view, false),
                measurements: snapshot,
                status: VehicleStatus::Demo,
                now_ms: now,
                blink_on: false,
                boost_psi: options.boost_psi,
                fps_mode: FpsMode::Off,
                fps_instant: 0.0,
                fps_average: 0.0,
                backgrounds: [BLACK; 8],
                overlay: None,
            };
            scene.draw_cell(&mut display, 0);
            let rect = widget.placement.rect().expect("isolated widget has valid geometry");
            let label = Rectangle::new(
                Point::new(i32::from(rect.x) + 2, i32::from(rect.y) + 2),
                Size::new(u32::from(rect.width) - 4, 12),
            );
            let mut clipped = display.clipped(&label);
            clipped.clear(BLACK).ok();
            // Bound work to one screen-width line. Controls/unsupported glyphs
            // are shown as '?'; the exact original text is listed outside canvas.
            let text: String = draft
                .label(widget)
                .chars()
                .take(54)
                .map(|c| if c.is_ascii() && !c.is_ascii_control() { c } else { '?' })
                .collect();
            Text::with_text_style(
                &text,
                Point::new(i32::from(rect.x + rect.width / 2), i32::from(rect.y) + 11),
                MonoTextStyle::new(&FONT_6X10, WHITE),
                dashboard_core::ui::CENTERED,
            )
            .draw(&mut clipped)
            .ok();
        }
        // This watermark is part of the pixels as well as the browser chrome,
        // so even a copied canvas cannot be mistaken for an accepted preview.
        Rectangle::new(Point::zero(), Size::new(320, 26))
            .into_styled(PrimitiveStyle::with_fill(RED))
            .draw(&mut display)
            .ok();
        Text::new(
            "INVALID PREVIEW",
            Point::new(7, 17),
            MonoTextStyle::new(&FONT_6X10, WHITE),
        )
        .draw(&mut display)
        .ok();
        let header = Rectangle::new(Point::new(107, 0), Size::new(206, 26));
        let name: String = draft
            .name(view)
            .chars()
            .take(34)
            .map(|c| if c.is_ascii() && !c.is_ascii_control() { c } else { '?' })
            .collect();
        Text::new(&name, Point::new(108, 17), MonoTextStyle::new(&FONT_6X10, WHITE))
            .draw(&mut display.clipped(&header))
            .ok();
        rgb565_to_rgba(&self.framebuffer[..], &mut self.rgba)?;
        Ok(&self.rgba)
    }
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
