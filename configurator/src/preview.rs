//! Exact shared framebuffer rendering with explicitly synthetic inputs.

use dashboard_config::{Channel, Config, ValidatedConfig, validate};
use dashboard_core::acquisition::{AcquisitionModel, DashboardSnapshot};
use dashboard_core::cache_diagnostics::CacheExhaustionStatus;
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
    StartupAsset,
    UnknownView(u8),
    InvalidSample(Channel),
    TimeOverflow,
    BufferLength,
    PendingFrame,
    UnsupportedPage(dashboard_config::ControlPage),
    InspectionOnly,
}

/// Appearance reconstructs synthetic history; Simulation borrows the live owner.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PreviewMode {
    #[default]
    Appearance,
    Simulation,
}

pub struct Preview {
    mode: PreviewMode,
    presentation: dashboard_core::presentation::PresentationState,
    simulation_epoch: Option<u64>,
    framebuffer: Box<FrameBuffer>,
    rgba: Vec<u8>,
    snapshot: Option<DashboardSnapshot>,
    warning: Option<WarningPage>,
    pager: WarningPager,
    configuration: Option<Box<Config>>,
    options: Option<PreviewOptions>,
    rendered_at: Option<u64>,
    prepared_status: CacheExhaustionStatus,
    pending: Option<(
        dashboard_core::display::pipeline::FrameToken,
        crate::simulator::SourceSessionKey,
    )>,
}

impl Default for Preview {
    fn default() -> Self { Self::new() }
}

impl Preview {
    pub fn new() -> Self {
        Self {
            mode: PreviewMode::Appearance,
            presentation: dashboard_core::presentation::PresentationState::new(),
            simulation_epoch: None,
            framebuffer: Box::new(FrameBuffer::new()),
            rgba: vec![0; WIDTH * HEIGHT * 4],
            snapshot: None,
            warning: None,
            pager: WarningPager::new(),
            configuration: None,
            options: None,
            rendered_at: None,
            prepared_status: CacheExhaustionStatus::EMPTY,
            pending: None,
        }
    }

    pub const fn mode(&self) -> PreviewMode { self.mode }

    /// Same startup pixels as Pico; browser seeking makes no scanout timing claim.
    pub fn render_startup(
        &mut self,
        config: &ValidatedConfig,
        view: u8,
        profile: dashboard_core::startup::StartupProfile,
        elapsed: u64,
        reduced_motion: bool,
    ) -> Result<&[u8], PreviewError> {
        self.render_startup_asset(config, view, profile, elapsed, reduced_motion, None)
    }

    pub fn render_startup_asset(
        &mut self,
        config: &ValidatedConfig,
        view: u8,
        profile: dashboard_core::startup::StartupProfile,
        elapsed: u64,
        reduced_motion: bool,
        asset: Option<&[u8]>,
    ) -> Result<&[u8], PreviewError> {
        if self.pending.is_some() {
            return Err(PreviewError::PendingFrame);
        }
        let (phase, time) = dashboard_core::startup::Startup::preview(profile, elapsed);
        if phase == dashboard_core::startup::Phase::Dashboard {
            return self.render(
                config,
                view,
                PreviewOptions {
                    sample_ms: elapsed,
                    ..PreviewOptions::default()
                },
            );
        }
        self.draw_startup_phase(config, profile, (phase, time), reduced_motion, asset)
    }

    fn draw_startup_phase(
        &mut self,
        config: &ValidatedConfig,
        profile: dashboard_core::startup::StartupProfile,
        frame: (dashboard_core::startup::Phase, u64),
        reduced_motion: bool,
        asset: Option<&[u8]>,
    ) -> Result<&[u8], PreviewError> {
        let (phase, time) = frame;
        if phase == dashboard_core::startup::Phase::Splash
            && profile.theme == dashboard_core::startup::StartupTheme::Custom
        {
            let asset = asset
                .and_then(|bytes| dashboard_config::startup_asset::Asset::parse(bytes).ok())
                .ok_or(PreviewError::StartupAsset)?;
            dashboard_core::startup::draw_custom(
                asset,
                time,
                profile.loop_animation,
                reduced_motion,
                &mut self.framebuffer,
            )
            .map_err(|_| PreviewError::StartupAsset)?;
            rgb565_to_rgba(&self.framebuffer[..], &mut self.rgba).expect("fixed buffer dimensions");
            self.configuration = None;
            self.presentation.reset();
            return Ok(&self.rgba);
        }
        dashboard_core::startup::draw(
            &mut St7789Renderer::new(&mut self.framebuffer),
            phase,
            profile,
            time,
            reduced_motion,
            dashboard_core::startup::StartupFacts {
                vehicle: VehicleStatus::Demo,
                config_ok: true,
                settings_loaded: false,
                configured_sources: config
                    .config()
                    .sources
                    .iter()
                    .filter(|s| s.definition != dashboard_config::DefinitionId::Disabled)
                    .count() as u8,
            },
        );
        rgb565_to_rgba(&self.framebuffer[..], &mut self.rgba).expect("fixed buffer dimensions");
        // Appearance caches must never retain assumptions about startup pixels.
        self.configuration = None;
        self.presentation.reset();
        Ok(&self.rgba)
    }

    /// Draw-only adapter for borrowed synthetic/unknown diagnostics and the
    /// actual bindings. This never submits a simulator frame or advances it.
    pub fn render_debug(
        &mut self,
        data: &dashboard_core::debug_view::DebugData<'_>,
        map: &dashboard_config::ControlMap,
    ) -> &[u8] {
        if self.pending.is_some() {
            return &self.rgba;
        }
        dashboard_core::debug_view::DebugPage::new(data, map).draw(&mut St7789Renderer::new(&mut self.framebuffer));
        rgb565_to_rgba(&self.framebuffer[..], &mut self.rgba).expect("fixed buffer dimensions");
        &self.rgba
    }

    /// Compatibility entry point for synthetic appearance inspection.
    pub fn render(
        &mut self,
        config: &ValidatedConfig,
        view: u8,
        options: PreviewOptions,
    ) -> Result<&[u8], PreviewError> {
        self.render_appearance(config, view, options)
    }

    /// Explicit synthetic transport accepts the rendered bytes before submission.
    /// Call complete_frame only after the consumer accepts those bytes; tests may delay it.
    /// LED duty continues using the previous completed scene until that completion.
    pub fn render_simulation_submitted(
        &mut self,
        simulation: &mut crate::simulator::ControlSimulator,
        token: dashboard_core::display::pipeline::FrameToken,
    ) -> Result<&[u8], PreviewError> {
        self.synchronize_source(simulation);
        let source_key = simulation.source_key().ok_or(PreviewError::TimeOverflow)?;
        if simulation.invalid() {
            return Err(PreviewError::InspectionOnly);
        }
        if self.pending.is_some() {
            return Err(PreviewError::PendingFrame);
        }
        if !simulation.can_accept_rendered(token) {
            return Err(PreviewError::PendingFrame);
        }
        self.render_simulation(simulation)?;
        if !simulation.accept_rendered(token) {
            return Err(PreviewError::PendingFrame);
        }
        self.pending = Some((token, source_key));
        Ok(&self.rgba)
    }

    /// Read-only shared Dashboard/Debug/Logs projection, without advancing acquisition.
    /// A submitted candidate protects its exact bytes until completion/source replacement.
    pub fn render_simulation(
        &mut self,
        simulation: &crate::simulator::ControlSimulator,
    ) -> Result<&[u8], PreviewError> {
        self.synchronize_source(simulation);
        if self.pending.is_some() {
            return Err(PreviewError::PendingFrame);
        }
        if let Some((profile, phase, time, asset)) = simulation.startup_frame() {
            self.draw_startup_phase(
                simulation.config(),
                profile,
                (phase, time),
                simulation.config().config().reduced_motion,
                asset,
            )?;
            self.mode = PreviewMode::Simulation;
            self.simulation_epoch = simulation.presentation_epoch();
            return Ok(&self.rgba);
        }
        self.render_projection(simulation, false, None)
    }

    fn synchronize_source(
        &mut self,
        simulation: &crate::simulator::ControlSimulator,
    ) {
        if self.pending.is_some_and(|(token, epoch)| {
            simulation.source_key() != Some(epoch) || simulation.completed_frame() == Some(token)
        }) {
            self.pending = None;
            if simulation.completed_frame().is_none() {
                self.presentation.reset();
            }
        }
    }

    pub fn pending_frame(&self) -> Option<dashboard_core::display::pipeline::FrameToken> {
        self.pending.map(|(t, _)| t)
    }

    /// Valid but unapplied candidate inspection; never submit or complete pixels.
    pub fn render_candidate_simulation(
        &mut self,
        simulation: &crate::simulator::ControlSimulator,
        candidate: &crate::json_draft::JsonPreviewProof,
    ) -> Result<&[u8], PreviewError> {
        if self.pending.is_some() {
            return Err(PreviewError::PendingFrame);
        }
        let installed = simulation.inspection_candidate().ok_or(PreviewError::InspectionOnly)?;
        if installed.projection() != candidate.projection() {
            return Err(PreviewError::InspectionOnly);
        }
        self.render_projection(simulation, true, Some(installed.config()))
    }

    /// Retained pixels are a capability of this token and source, never current scratch.
    pub fn pending_pixels(
        &self,
        simulation: &crate::simulator::ControlSimulator,
        token: dashboard_core::display::pipeline::FrameToken,
    ) -> Option<&[u8]> {
        (self.pending == simulation.source_key().map(|epoch| (token, epoch))).then_some(self.rgba.as_slice())
    }

    pub fn finish_pending(
        &mut self,
        token: dashboard_core::display::pipeline::FrameToken,
    ) -> bool {
        if self.pending_frame() != Some(token) {
            return false;
        }
        self.pending = None;
        true
    }

    fn render_projection(
        &mut self,
        simulation: &crate::simulator::ControlSimulator,
        inspection: bool,
        candidate: Option<&ValidatedConfig>,
    ) -> Result<&[u8], PreviewError> {
        let config = candidate.unwrap_or_else(|| simulation.config());
        let session = if inspection {
            simulation.display_session()
        } else {
            simulation.session()
        };
        let view = if candidate.is_some() {
            crate::json_draft::inspection_view(config.config(), session.view_id())
                .ok_or(PreviewError::UnknownView(session.view_id()))?
        } else {
            session.view_id()
        };
        let layout = Layout::new(config, view, false).ok_or(PreviewError::UnknownView(view))?;
        let snapshot = simulation.snapshot();
        let now = if inspection {
            simulation.display_now_ms()
        } else {
            simulation.now_ms()
        };
        let warning = if inspection { None } else { simulation.warning_page() };
        let epoch = simulation.presentation_epoch();
        if self.mode != PreviewMode::Simulation
            || epoch.is_none()
            || self.simulation_epoch != epoch
            || session.page() != dashboard_core::pages::Page::Dashboard
        {
            self.presentation.reset();
        }
        self.simulation_epoch = epoch;
        let readings = self
            .presentation
            .prepare(config, &snapshot, view, now, session.boost_psi());
        let readings = if candidate.is_some() {
            readings.without_rule_state()
        } else {
            readings
        };
        let user_overlay = session.overlay(config);
        let scene = DashboardScene {
            readings: Some(&readings),
            layout: Some(layout),
            measurements: &snapshot,
            status: VehicleStatus::Demo,
            now_ms: now,
            blink_on: if inspection {
                dashboard_core::animations::blink_on(now)
            } else {
                simulation.blink_on()
            },
            boost_psi: session.boost_psi(),
            fps_mode: session.fps(),
            fps_instant: 0.0,
            fps_average: 0.0,
            backgrounds: [BLACK; 8],
            overlay: if inspection {
                Overlay::configured_for_page(session.page(), user_overlay, None)
            } else {
                simulation.presented_scene().overlay
            },
        };
        let mut display = St7789Renderer::new(&mut self.framebuffer);
        match session.page() {
            dashboard_config::ControlPage::Dashboard => scene.draw_full(&mut display),
            dashboard_config::ControlPage::Debug => {
                let data = dashboard_core::debug_view::DebugData {
                    source_status: Some("SYNTHETIC DEMO"),
                    config_fault: Some(inspection && candidate.is_none()),
                    brightness: Some(session.backlight().brightness()),
                    settings_status: Some(if candidate.is_some() {
                        "UNAPPLIED JSON"
                    } else if inspection {
                        "INSPECTION ONLY"
                    } else {
                        "ACCEPTED CONFIG"
                    }),
                    ..Default::default()
                };
                dashboard_core::debug_view::DebugPage::new(&data, simulation.display_map()).draw(&mut display);
            }
            dashboard_config::ControlPage::Logs => {
                let page = simulation.log_page_at(session.log_anchor());
                page.draw_full(&mut display);
                page.draw_hint(&mut display, simulation.display_map());
            }
        }
        if session.page() != dashboard_config::ControlPage::Dashboard
            && let Some(overlay) = scene.overlay
        {
            overlay.draw(&mut display);
        }
        if session.backlight().compare() == 0 {
            use embedded_graphics::prelude::DrawTarget;
            display.clear(BLACK).ok();
        }
        rgb565_to_rgba(&self.framebuffer[..], &mut self.rgba).expect("fixed buffer dimensions");
        self.mode = PreviewMode::Simulation;
        self.prepared_status = simulation
            .cache_exhaustion_status()
            .union(CacheExhaustionStatus::from_config(config.cache_identity_status()));
        self.snapshot = Some(snapshot);
        self.warning = warning;
        self.rendered_at = Some(now);
        self.options = None;
        self.configuration = None;
        self.pager = WarningPager::new();
        Ok(&self.rgba)
    }

    /// Render through the same acquisition statistics, layout and DashboardScene
    /// as firmware. Both complete pixel buffers are allocated once and reused.
    /// All recoverable errors leave the previous complete render and pager intact.
    pub fn render_appearance(
        &mut self,
        config: &ValidatedConfig,
        view: u8,
        options: PreviewOptions,
    ) -> Result<&[u8], PreviewError> {
        if self.pending.is_some() {
            return Err(PreviewError::PendingFrame);
        }
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

        self.prepared_status = CacheExhaustionStatus::from_config(config.cache_identity_status());

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
        self.prepared_status = self.prepared_status.union(snapshot.cache_exhaustion_status());
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
        let blink = options.warnings && dashboard_core::animations::blink_on(options.sample_ms);
        let warning = if options.warnings {
            self.pager.select_appearance(config, &snapshot, view, now, blink)
        } else {
            self.pager.pause(now);
            None
        };
        if self.mode != PreviewMode::Appearance
            || changed_config
            || self
                .options
                .is_none_or(|old| old.warnings != options.warnings || options.sample_ms < old.sample_ms)
        {
            self.presentation.reset();
        }
        let readings = self
            .presentation
            .prepare_appearance(config, &snapshot, view, now, options.boost_psi);
        let scene = DashboardScene {
            readings: Some(&readings),
            layout: Some(layout),
            measurements: &snapshot,
            status: VehicleStatus::Demo,
            now_ms: now,
            blink_on: blink,
            boost_psi: options.boost_psi,
            fps_mode: FpsMode::Off,
            fps_instant: 0.0,
            fps_average: 0.0,
            backgrounds: [BLACK; 8], // Configured renderer resolves palettes itself.
            overlay: warning.map(Overlay::Warnings),
        };
        scene.draw_full(&mut St7789Renderer::new(&mut self.framebuffer));
        rgb565_to_rgba(&self.framebuffer[..], &mut self.rgba).expect("fixed framebuffer and RGBA dimensions");
        self.mode = PreviewMode::Appearance;
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

    pub const fn cache_exhaustion_status(&self) -> CacheExhaustionStatus { self.prepared_status }

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
        options: PreviewOptions,
    ) -> Result<&[u8], PreviewError> {
        self.render_diagnostic_with_status(accepted, draft, view, options, ValidatedConfig::cache_identity_status)
    }

    /// Separate scratch presentation of a candidate, reading accepted acquisition only.
    pub fn render_inspection(
        &mut self,
        simulation: &crate::simulator::ControlSimulator,
        draft: Option<&crate::diagnostic::Draft>,
    ) -> Result<&[u8], PreviewError> {
        if self.pending.is_some() {
            return Err(PreviewError::PendingFrame);
        }
        if simulation.display_session().page() != dashboard_config::ControlPage::Dashboard || draft.is_none() {
            return self.render_projection(simulation, true, None);
        }
        let view = simulation.display_session().view_id();
        let options = PreviewOptions {
            sample_ms: simulation.display_now_ms(),
            boost_psi: simulation.display_session().boost_psi(),
            ..Default::default()
        };
        self.render_diagnostic_source(
            simulation.config(),
            draft.unwrap(),
            view,
            options,
            ValidatedConfig::cache_identity_status,
            Some(simulation),
        )
    }

    // Keep status projection at the producer boundary; tests can exercise an
    // exhausted publication without constructing or modifying identity proofs.
    fn render_diagnostic_with_status(
        &mut self,
        accepted: &ValidatedConfig,
        draft: &crate::diagnostic::Draft,
        view: u8,
        options: PreviewOptions,
        status: impl FnMut(&ValidatedConfig) -> dashboard_config::CacheIdentityStatus,
    ) -> Result<&[u8], PreviewError> {
        self.render_diagnostic_source(accepted, draft, view, options, status, None)
    }

    fn render_diagnostic_source(
        &mut self,
        accepted: &ValidatedConfig,
        draft: &crate::diagnostic::Draft,
        view: u8,
        mut options: PreviewOptions,
        mut status: impl FnMut(&ValidatedConfig) -> dashboard_config::CacheIdentityStatus,
        live: Option<&crate::simulator::ControlSimulator>,
    ) -> Result<&[u8], PreviewError> {
        use dashboard_core::colors::WHITE;
        use embedded_graphics::mono_font::MonoTextStyle;
        use embedded_graphics::mono_font::ascii::FONT_6X10;
        use embedded_graphics::prelude::*;
        use embedded_graphics::primitives::{PrimitiveStyle, Rectangle};
        use embedded_graphics::text::Text;

        options.warnings = false;
        if let Some(simulation) = live {
            self.render_projection(simulation, true, None)?;
        } else {
            self.render(accepted, view, options)?;
        }
        let snapshot = self.snapshot.as_ref().expect("successful render has a snapshot");
        let now = self.rendered_at.expect("successful render has a timestamp");
        let mut display = St7789Renderer::new(&mut self.framebuffer);
        Rectangle::new(Point::new(0, 26), Size::new(320, 214))
            .into_styled(PrimitiveStyle::with_fill(BLACK))
            .draw(&mut display)
            .ok();
        for widget in draft.config.widgets.iter().take(64).filter(|w| w.view == view) {
            let Some(config) = crate::diagnostic::isolated_widget(accepted, widget) else {
                continue;
            };
            self.prepared_status = self
                .prepared_status
                .union(CacheExhaustionStatus::from_config(status(&config)));
            let mut presentation = dashboard_core::presentation::PresentationState::new();
            let readings = presentation
                .prepare(&config, snapshot, view, now, options.boost_psi)
                .without_rule_state();
            let scene = DashboardScene {
                readings: Some(&readings),
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
            let band_colors: Vec<_> = widget.advanced.bands().iter().map(|band| band.color).collect();
            dashboard_core::configured_scene::draw_diagnostic_widget(
                &mut display,
                &scene,
                0,
                widget.colors,
                &band_colors,
            );
            let rect = config.config().widgets[0]
                .placement
                .rect()
                .expect("isolated widget has valid geometry");
            let label = Rectangle::new(
                Point::new(i32::from(rect.x) + 2, i32::from(rect.y) + 2),
                Size::new(u32::from(rect.width) - 4, 12),
            );
            let mut clipped = display.clipped(&label);
            clipped
                .clear(embedded_graphics::pixelcolor::raw::RawU16::new(widget.colors.background.0).into())
                .ok();
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
                MonoTextStyle::new(
                    &FONT_6X10,
                    embedded_graphics::pixelcolor::raw::RawU16::new(widget.colors.label.0).into(),
                ),
                dashboard_core::ui::CENTERED,
            )
            .draw(&mut clipped)
            .ok();
        }
        // Inspect even an unreadable rejected warning pair, under the explicit
        // invalid banner. Geometry/message are fixed; raw hex is listed in UI.
        let palette = if draft.config.warning_colors != accepted.config().warning_colors {
            Some(draft.config.warning_colors)
        } else {
            draft.config.widgets.iter().filter(|w| w.view == view).find_map(|w| {
                accepted
                    .config()
                    .widgets
                    .iter()
                    .find(|old| old.id == w.id)
                    .filter(|old| old.colors.warning != w.colors.warning)
                    .map(|_| w.colors.warning)
            })
        };
        if let Some(colors) = palette {
            dashboard_core::widgets::draw_warning_page(
                &mut display,
                WarningPage::palette_sample(colors, dashboard_core::animations::blink_on(options.sample_ms)),
            );
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
        // Candidate Off masks every diagnostic layer, including the watermark.
        // Invalid status and protected recovery remain in the browser controls.
        if live.is_some_and(|simulation| simulation.display_session().backlight().compare() == 0) {
            display.clear(BLACK).ok();
        }
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

#[cfg(test)]
mod cache_tests {
    use dashboard_config::CacheIdentityStatus;
    use dashboard_core::cache_diagnostics::CacheExhaustionCause;

    use super::*;

    #[test]
    fn diagnostic_preparation_observes_each_isolated_publication() {
        let accepted = validate(dashboard_config::default_config()).unwrap();
        let view = accepted.config().boot_view;
        let mut draft = crate::diagnostic::Draft {
            config: accepted.config().clone(),
            raw: Vec::new(),
            error: crate::editor::EditorError::InvalidDraft,
        };
        let expected = draft.config.widgets.iter().filter(|widget| widget.view == view).count();
        assert!(expected > 1);
        let mut preview = Preview::new();
        let mut calls = 0;
        preview
            .render_diagnostic_with_status(&accepted, &draft, view, PreviewOptions::default(), |_| {
                calls += 1;
                if calls == 1 {
                    CacheIdentityStatus::Exhausted
                } else {
                    CacheIdentityStatus::Tracked
                }
            })
            .unwrap();
        assert_eq!(calls, expected);
        assert_eq!(
            preview.cache_exhaustion_status(),
            CacheExhaustionStatus::from_cause(CacheExhaustionCause::ConfigIssuer)
        );

        preview
            .render_diagnostic_with_status(&accepted, &draft, view, PreviewOptions::default(), |_| {
                CacheIdentityStatus::Untracked
            })
            .unwrap();
        assert_eq!(preview.cache_exhaustion_status(), CacheExhaustionStatus::EMPTY);

        // Outside-grid geometry now publishes an explicitly bounded validated
        // diagnostic projection, never the rejected configuration itself.
        for widget in &mut draft.config.widgets {
            widget.placement.column = u8::MAX;
        }
        preview
            .render_diagnostic_with_status(&accepted, &draft, view, PreviewOptions::default(), |safe| {
                assert!(safe.config().widgets[0].placement.rect().is_some());
                CacheIdentityStatus::Tracked
            })
            .unwrap();
        assert_eq!(preview.cache_exhaustion_status(), CacheExhaustionStatus::EMPTY);
    }
}
