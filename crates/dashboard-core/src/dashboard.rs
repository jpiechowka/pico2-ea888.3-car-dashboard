//! Hardware-independent dashboard scene used by the device and replay tests.
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;

use crate::acquisition::DashboardSnapshot;
use crate::backlight::Brightness;
use crate::config::{COL_WIDTH, HEADER_HEIGHT, ROW_HEIGHT};
use crate::configured_scene::{self, Layout};
use crate::pages::Page;
use crate::render::{FpsMode, FpsText, format_fps};
use crate::samples::{DataMode, Quality, SensorId, Source};
use crate::sensor_state::SensorSnapshot;
use crate::thresholds::BOOST_EASTER_EGG_BAR;
use crate::ui::BLACK;
use crate::vehicle_status::VehicleStatus;
use crate::widgets::cell_text::{self, CellText};
use crate::widgets::*;

/// Legacy cell-to-SensorId index map: boost, lambda, battery, coolant, oil, DSG, IAT, EGT.
pub const CELL_SENSORS: [usize; 8] = [0, 7, 6, 2, 1, 3, 4, 5];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Immutable opaque popup content, captured together with the frame's presentation identity.
pub enum Overlay {
    /// Producer-confirmed statistics reset feedback.
    Reset,
    /// Selected FPS-label mode feedback.
    Fps(
        /// Mode whose static label is shown.
        FpsMode,
    ),
    /// Physical boost-unit preference feedback.
    BoostUnit(
        /// True selects PSI, false selects bar.
        bool,
    ),
    /// Requested brightness feedback; completion timing remains external.
    Brightness(
        /// Selected Full/Dim/Off or Debug percentage state.
        Brightness,
    ),
    /// Legacy one-sensor warning page.
    Critical(
        /// Canonical sensor receiving the warning.
        SensorId,
        /// Captured blink phase, not evaluated by the renderer.
        bool,
    ),
    /// Configured warning page with rule mask and status rows.
    Warnings(
        /// Immutable page content selected by the warning pager.
        crate::view_state::WarningPage,
    ),
    /// Named-view selection feedback.
    View(
        /// Bounded validated view name.
        crate::view_state::PopupText,
    ),
}

impl Overlay {
    /// Returns the permitted overlay for screen `page`: Dashboard prefers `user`
    /// over configured `warning`, Debug permits only percentage brightness feedback,
    /// and Logs permits none. Does not select another warning or mutate timing.
    pub fn configured_for_page(
        page: Page,
        user: Option<Self>,
        warning: Option<crate::view_state::WarningPage>,
    ) -> Option<Self> {
        match page {
            Page::Dashboard => user.or_else(|| warning.map(Self::Warnings)),
            Page::Debug => user.filter(|overlay| matches!(overlay, Self::Brightness(Brightness::Percent(_)))),
            Page::Logs => None,
        }
    }

    /// Returns the captured blink phase for warning overlays, otherwise `None`.
    pub const fn warning_phase(self) -> Option<bool> {
        match self {
            Self::Critical(_, phase) => Some(phase),
            Self::Warnings(page) => Some(page.blink),
            _ => None,
        }
    }

    /// Returns `user` feedback if present, otherwise the highest-priority legacy
    /// sensor in `alerts` with captured phase `blink`, or `None` without alerts.
    pub fn prioritize(
        user: Option<Self>,
        alerts: crate::alarms::Alerts,
        blink: bool,
    ) -> Option<Self> {
        user.or_else(|| alerts.priority().map(|id| Self::Critical(id, blink)))
    }

    /// Warnings belong to the dashboard; diagnostic pages stay unobstructed.
    /// Returns policy for screen `page`, prioritizing `user` over `alerts` on
    /// Dashboard with warning phase `blink`; Debug allows only percentage brightness,
    /// and Logs suppresses every overlay.
    pub fn for_page(
        page: Page,
        user: Option<Self>,
        alerts: crate::alarms::Alerts,
        blink: bool,
    ) -> Option<Self> {
        match page {
            Page::Dashboard => Self::prioritize(user, alerts, blink),
            Page::Debug => user.filter(|overlay| matches!(overlay, Self::Brightness(Brightness::Percent(_)))),
            Page::Logs => None,
        }
    }

    /// Draws this captured popup into native-panel `display`, discarding target
    /// errors. Does not acknowledge warning visibility, start timers or alter hardware state.
    pub fn draw<D: DrawTarget<Color = Rgb565>>(
        self,
        display: &mut D,
    ) {
        match self {
            Self::Reset => draw_reset_popup(display),
            Self::Fps(mode) => draw_fps_toggle_popup(display, mode),
            Self::BoostUnit(psi) => draw_boost_unit_popup(display, psi),
            Self::Brightness(value) => draw_brightness_popup(display, value),
            Self::Critical(sensor, blink) => draw_critical_popup(display, sensor, blink),
            Self::Warnings(page) => draw_warning_page(display, page),
            Self::View(name) => draw_view_popup(display, name.as_str()),
        }
    }
}

/// Borrowed immutable inputs for one frame; caller binds layout/readings/snapshot,
/// clock and geometry coherently before rendering or retaining cache keys.
pub struct DashboardScene<'a> {
    /// Prepared once per configured frame; None uses raw snapshot fallbacks, including legacy fixtures.
    pub readings: Option<&'a crate::presentation::DisplayReadings<'a>>,
    /// None retains the original fixed scene for historical replay contracts.
    /// Device and browser use the same validated configurable path.
    pub layout: Option<Layout<'a>>,
    /// Raw coherent acquisition snapshot, retained for quality, warnings, history and extrema.
    pub measurements: &'a DashboardSnapshot,
    /// Already derived source/adapter status for header display.
    pub status: VehicleStatus,
    /// Monotonic presentation/freshness clock in milliseconds.
    pub now_ms: u64,
    /// Captured warning/decorative blink phase, shared across all cells in this frame.
    pub blink_on: bool,
    /// Physical boost-unit preference; fixed configured units remain independent.
    pub boost_psi: bool,
    /// Header FPS-label selection.
    pub fps_mode: FpsMode,
    /// Instantaneous completed-transfer rate per second; invalid values display unavailable.
    pub fps_instant: f32,
    /// Window-average completed-transfer rate per second; not scanout FPS.
    pub fps_average: f32,
    /// Colors already evaluated by the time-based transition owner, in cell order.
    pub backgrounds: [Rgb565; 8],
    /// Already selected configured overlay; legacy scenes additionally derive raw warning priority.
    pub overlay: Option<Overlay>,
}

impl DashboardScene<'_> {
    /// Returns visible warning marker/stale/blink state for configured slot `cell`.
    /// Uses prepared eligibility when supplied, otherwise raw snapshot rules at the
    /// scene clock; missing layout/rule/prepared slot returns `None`.
    pub(crate) fn widget_warning(
        &self,
        cell: usize,
    ) -> Option<configured_scene::WidgetWarning> {
        let eligibility = match self.readings {
            Some(readings) => readings.widget(cell)?.warning,
            None => {
                self.measurements.warning_eligibility(
                    crate::alarms::RuleIndex::new(self.layout.as_ref()?.rule_index(cell)?)?,
                    self.now_ms,
                )
            }
        };
        Some(configured_scene::WidgetWarning::new(
            eligibility,
            self.quality(cell) == Quality::Valid,
            self.blink_on,
        ))
    }

    /// Returns exact visible text lanes for configured slot `cell`, using prepared
    /// current value when present and raw snapshot statistics. Missing layout/widget/
    /// prepared slot returns `None`; units follow configuration and the physical toggle.
    pub fn configured_text(
        &self,
        cell: usize,
    ) -> Option<CellText> {
        let layout = self.layout.as_ref()?;
        let widget = layout.widget(cell)?;
        let Some(channel) = widget.channel.scalar() else {
            return Some(crate::widgets::ignition::text(widget, self.measurements, self.now_ms));
        };
        let current = match self.readings {
            Some(readings) => readings.widget(cell)?.value,
            None => self.measurements.value_at(SensorId::from_channel(channel), self.now_ms),
        };
        Some(configured_scene::widget_text_with_value(
            widget,
            self.measurements,
            self.now_ms,
            self.boost_psi,
            layout.configuration().stoich(),
            &layout.configuration().config().fuel,
            current,
        ))
    }

    /// Returns converted current/scale/raw-held-peak data for configured slot `cell`.
    /// Prepared current/peak values are preferred; fallback uses the widget's global
    /// rule index. Missing layout/widget/prepared slot returns `None`; unavailable
    /// measurements remain absent inside an otherwise valid reading. Cylinder groups
    /// return None because they have four observations and no scalar gauge.
    pub fn gauge_reading(
        &self,
        cell: usize,
    ) -> Option<crate::widgets::gauge::Reading> {
        let layout = self.layout.as_ref()?;
        let widget = layout.widget(cell)?;
        let channel = widget.channel.scalar()?;
        let (value, peak) = match self.readings {
            Some(readings) => {
                let reading = readings.widget(cell)?;
                (reading.value, reading.peak)
            }
            None => {
                (
                    self.measurements.value_at(SensorId::from_channel(channel), self.now_ms),
                    self.measurements
                        .peak_at(crate::alarms::RuleIndex::new(layout.rule_index(cell)?)?, self.now_ms),
                )
            }
        };
        Some(crate::widgets::gauge::Reading::presented(
            widget,
            self.measurements,
            value,
            peak,
            crate::presentation::Unit::resolve(widget.advanced.readout, self.boost_psi),
            layout.configuration().stoich(),
        ))
    }

    /// Returns the canonical sensor index for visible slot `cell`, or `None` beyond
    /// the configured view/legacy eight cells or for a four-cylinder group.
    /// Configured order follows the validated widget table; groups never borrow a scalar identity.
    pub fn cell_sensor(
        &self,
        cell: usize,
    ) -> Option<usize> {
        if let Some(layout) = &self.layout {
            return layout
                .widget(cell)
                .and_then(|w| w.channel.scalar())
                .map(|channel| SensorId::from_channel(channel) as usize);
        }
        CELL_SENSORS.get(cell).copied()
    }

    /// Draws this scene's configured or legacy header into `display`, discarding
    /// target errors. Caller uses the same native geometry as the frame's layout.
    pub fn draw_header<D: DrawTarget<Color = Rgb565>>(
        &self,
        display: &mut D,
    ) {
        if let Some(layout) = &self.layout {
            configured_scene::draw_header(display, self, layout);
            return;
        }
        draw_status_header(
            display,
            self.measurements.mode,
            self.status,
            self.fps_mode,
            self.fps_instant,
            self.fps_average,
        );
    }

    /// Clears and draws visible slot `cell` into `display`, clipped to its own
    /// configured/legacy bounds; invalid slots are inert. Unqualified values show
    /// explicit unavailable text, not zero. Target errors are discarded and no
    /// warning/history/smoothing state is mutated during drawing.
    pub fn draw_cell<D: DrawTarget<Color = Rgb565>>(
        &self,
        display: &mut D,
        cell: usize,
    ) {
        if let Some(layout) = &self.layout {
            configured_scene::draw_widget(display, self, cell, layout);
            return;
        }
        let Some(sensor) = self.cell_sensor(cell) else {
            return;
        };
        if configured_scene::target_geometry(display) == dashboard_config::Geometry::Portrait {
            let row = cell as u8 / 2;
            let geometry = dashboard_config::Geometry::Portrait;
            let top = i32::from(geometry.row_edge(row).unwrap());
            let bottom = i32::from(geometry.row_edge(row + 1).unwrap());
            let bounds = embedded_graphics::primitives::Rectangle::new(
                Point::new((cell as i32 % 2) * 120 + 2, top + 2),
                Size::new(116, (bottom - top - 4) as u32),
            );
            let mut target = display.clipped(&bounds);
            let key = self.cell_key(cell).expect("visible fallback cell");
            target.clear(key.background).ok();
            let mut text = key.text;
            if key.quality != Quality::Valid {
                text[0].push_str("--").ok();
                text[5].push_str(key.quality.label()).ok();
            }
            let graph = cell != 0 && key.quality == Quality::Valid;
            let lanes = configured_scene::portrait_content_layout(
                bounds,
                core::array::from_fn(|i| text[i].as_str()),
                0,
                graph,
                true,
                0,
                key.trend.is_some(),
            );
            let style = embedded_graphics::mono_font::MonoTextStyle::new(
                &embedded_graphics::mono_font::ascii::FONT_4X6,
                crate::ui::WHITE,
            );
            embedded_graphics::text::Text::with_text_style(
                SensorId::ALL[sensor].label(),
                Point::new(bounds.center().x, top + 7),
                style,
                crate::ui::CENTERED,
            )
            .draw(&mut target)
            .ok();
            crate::widgets::value_text::draw_fitted_value(&mut target, &text[0], lanes.text[0], 2, crate::ui::WHITE);
            for i in 1..6 {
                configured_scene::compact_text(&mut target, &text[i], lanes.text[i], crate::ui::WHITE);
            }
            if let Some(rising) = key.trend {
                crate::widgets::primitives::draw_trend_arrow(
                    &mut target,
                    bounds.top_left.x + 110,
                    lanes.text[0].top_left.y + 4,
                    rising,
                    crate::ui::WHITE,
                );
            }
            if graph {
                let history = self.measurements.sensors[sensor].history();
                crate::widgets::primitives::draw_history_graph(
                    &mut target,
                    lanes.graph.top_left.x,
                    lanes.graph.top_left.y,
                    lanes.graph.size.width,
                    lanes.graph.size.height,
                    history,
                    history.bounds(),
                    |_| crate::ui::WHITE,
                );
            }
            return;
        }
        let state = &self.measurements.sensors[sensor];
        let value = state.value().unwrap_or(0.0);
        let maximum = state.maximum().unwrap_or(0.0);
        let x = (cell as u32 % 4) * COL_WIDTH;
        let y = HEADER_HEIGHT + (cell as u32 / 4) * ROW_HEIGHT;
        // A retained cell must own all of its old glyph pixels and must never
        // overwrite a neighbor, even with a long signed or malformed value.
        let bounds = embedded_graphics::primitives::Rectangle::new(
            Point::new(x as i32 + 1, y as i32 + 1),
            Size::new(COL_WIDTH - 2, ROW_HEIGHT - 2),
        );
        let mut clipped = display.clipped(&bounds);
        clipped.clear(BLACK).ok();
        let display = &mut clipped;
        let quality = self.quality(cell);
        if quality != Quality::Valid {
            draw_unavailable(display, x, y, SensorId::ALL[sensor].label(), quality);
            return;
        }
        let demo = self.measurements.mode == DataMode::Demo;
        let critical = self.measurements.alarm_active_at(SensorId::ALL[sensor], self.now_ms);
        let data = display_data(state, self.now_ms);
        let background = if critical && self.blink_on {
            self.backgrounds[cell]
        } else {
            BLACK
        };
        match cell {
            0 => {
                draw_boost_cell(
                    display,
                    x,
                    y,
                    COL_WIDTH,
                    ROW_HEIGHT,
                    value,
                    maximum,
                    self.boost_psi,
                    demo && value >= BOOST_EASTER_EGG_BAR,
                    self.blink_on,
                    0,
                )
            }
            1 => {
                draw_lambda_cell(display, x, y, COL_WIDTH, ROW_HEIGHT, value, &data, 0, Some(background));
            }
            2 => {
                draw_batt_cell(
                    display,
                    x,
                    y,
                    COL_WIDTH,
                    ROW_HEIGHT,
                    value,
                    state.minimum().unwrap_or(0.0),
                    maximum,
                    &data,
                    background,
                    critical,
                );
            }
            _ => {
                draw_temp_cell(
                    display,
                    x,
                    y,
                    COL_WIDTH,
                    ROW_HEIGHT,
                    SensorId::ALL[sensor].label(),
                    value,
                    maximum,
                    &data,
                    background,
                    critical,
                );
            }
        }
    }

    /// Draws the effective captured overlay into `display` when present, discarding
    /// errors; completion and acknowledgment remain the adapter's responsibility.
    pub fn draw_overlay<D: DrawTarget<Color = Rgb565>>(
        &self,
        display: &mut D,
    ) {
        if let Some(overlay) = self.effective_overlay() {
            overlay.draw(display);
        }
    }

    /// Returns the caller's configured policy result unchanged, or legacy
    /// Dashboard feedback/warning priority evaluated at the scene's clock.
    fn effective_overlay(&self) -> Option<Overlay> {
        // Configured scenes receive the single policy result used for both
        // pixels and completion-gated LEDs. Never choose a second warning here.
        if self.layout.is_some() {
            return self.overlay;
        }
        Overlay::for_page(
            Page::Dashboard,
            self.overlay,
            self.measurements.alerts_at(self.now_ms),
            self.blink_on,
        )
    }

    /// Clears all `display` pixels and draws header, eight bounded slots, legacy
    /// dividers if applicable and overlay in that order. Errors are discarded;
    /// caller retains framebuffer ownership through physical transfer completion.
    pub fn draw_full<D: DrawTarget<Color = Rgb565>>(
        &self,
        display: &mut D,
    ) {
        display.clear(BLACK).ok();
        self.draw_header(display);
        for cell in 0..8 {
            self.draw_cell(display, cell);
        }
        if self.layout.is_none() {
            draw_dividers(display);
        }
        self.draw_overlay(display);
    }
}

/// Draws sensor `label`, missing value and `quality` text into already clipped
/// `display` at bounded legacy-cell pixel `x`, `y`; discards target errors.
fn draw_unavailable<D: DrawTarget<Color = Rgb565>>(
    display: &mut D,
    x: u32,
    y: u32,
    label: &str,
    quality: Quality,
) {
    use embedded_graphics::text::Text;

    use crate::ui::{CENTERED, LABEL_STYLE_WHITE, VALUE_STYLE_WHITE};
    let center = x as i32 + COL_WIDTH as i32 / 2;
    Text::with_text_style(label, Point::new(center, y as i32 + 14), LABEL_STYLE_WHITE, CENTERED)
        .draw(display)
        .ok();
    Text::with_text_style(
        "--",
        Point::new(center, y as i32 + ROW_HEIGHT as i32 / 2),
        VALUE_STYLE_WHITE,
        CENTERED,
    )
    .draw(display)
    .ok();
    Text::with_text_style(
        quality.label(),
        Point::new(center, (y + ROW_HEIGHT) as i32 - 10),
        LABEL_STYLE_WHITE,
        CENTERED,
    )
    .draw(display)
    .ok();
}

/// Returns borrowed raw history/average and time-qualified trend/recent-peak state
/// from `state` at `now_ms` milliseconds, without changing acquisition statistics.
fn display_data(
    state: &SensorSnapshot,
    now_ms: u64,
) -> SensorDisplayData<'_> {
    SensorDisplayData {
        trend: state.get_trend_at(now_ms),
        is_new_peak: state.is_new_peak_at(now_ms),
        history: state.history(),
        average: state.get_average(),
    }
}

#[derive(Clone, PartialEq)]
/// Visible cell dependencies only; raw history remains in the acquisition snapshot.
struct CellKey {
    /// Immutable configuration identity/global widget index, or legacy/untracked identity.
    widget: Option<(dashboard_config::ConfigCacheIdentity, u8)>,
    /// Effective quality at the scene clock; missing valid payload becomes InvalidPayload.
    quality: Quality,
    /// Actual source provenance, preventing equal numeric values from hiding source replacement.
    source: Source,
    /// Exact bounded strings; ignition lanes4/5 additionally retain four qualities
    /// and provenance without increasing this fixed cache record.
    text: CellText,
    /// Visible cell fill after warning phase/palette policy.
    background: Rgb565,
    /// Configured: peak/blink/stale-latch; legacy interpretation follows each cell's visible features.
    flags: [bool; 3],
    /// Configured warning badge text identity.
    warning_marker: configured_scene::WarningMarker,
    /// Visible trend arrow, omitting hidden/flat/unavailable trends.
    trend: Option<bool>,
    /// Producer-issued raw graph identity when graph pixels are visible and tracking is available.
    graph: Option<crate::GraphCacheIdentity>,
    /// Numeric scale plus quantized nonnumeric indicator/held-peak dependencies.
    gauge: Option<crate::widgets::gauge::RasterKey>,
}
impl DashboardScene<'_> {
    /// Returns effective quality for slot `cell`; unknown slots are Unknown and
    /// nominally valid metadata lacking a qualified value becomes InvalidPayload.
    pub(crate) fn quality(
        &self,
        cell: usize,
    ) -> Quality {
        if self
            .layout
            .as_ref()
            .and_then(|layout| layout.widget(cell))
            .is_some_and(|widget| widget.channel.scalar().is_none())
        {
            // Only controls shared warning blinking. Each visible cylinder is
            // independently qualified by the group formatter and warning owner.
            return if crate::ignition::Cylinder::ALL.into_iter().any(|cylinder| {
                self.measurements
                    .ignition
                    .value_at(cylinder, self.measurements.mode, self.now_ms)
                    .is_some()
            }) {
                Quality::Valid
            } else {
                Quality::Unknown
            };
        }
        let Some(sensor) = self.cell_sensor(cell) else {
            return Quality::Unknown;
        };
        let id = SensorId::ALL[sensor];
        let quality = self.measurements.quality(id, self.now_ms);
        if quality == Quality::Valid && self.measurements.value_at(id, self.now_ms).is_none() {
            Quality::InvalidPayload
        } else {
            quality
        }
    }

    /// Returns retained raster dependencies for slot `cell`, or `None` when absent
    /// or incompatible prepared data prevents a coherent key. Hidden statistics do
    /// not contribute; unavailable gauges still retain scale/badge dependencies.
    fn cell_key(
        &self,
        cell: usize,
    ) -> Option<CellKey> {
        if let Some(layout) = &self.layout
            && let Some(widget) = layout.widget(cell)
            && widget.channel.scalar().is_none()
        {
            let warning = self.widget_warning(cell)?;
            return Some(CellKey {
                widget: layout
                    .configuration()
                    .cache_identity()
                    .zip(layout.rule_index(cell).map(|index| index as u8)),
                quality: self.quality(cell),
                source: Source::Unknown,
                text: self.configured_text(cell)?,
                background: configured_scene::panel_color(if warning.blink {
                    widget.colors.warning.background
                } else {
                    widget.colors.background
                }),
                flags: [false, warning.blink, warning.unavailable],
                warning_marker: warning.marker,
                trend: None,
                graph: None,
                gauge: None,
            });
        }
        let sensor = self.cell_sensor(cell)?;
        let state = &self.measurements.sensors[sensor];
        let value = state.value().unwrap_or(0.0);
        let maximum = state.maximum().unwrap_or(0.0);
        let mut key = CellKey {
            widget: self
                .layout
                .as_ref()
                .and_then(|l| Some((l.configuration().cache_identity()?, l.rule_index(cell)? as u8))),
            quality: self.quality(cell),
            source: self.measurements.metadata[sensor].source,
            text: core::array::from_fn(|_| heapless::String::new()),
            background: self.backgrounds[cell],
            flags: [false; 3],
            warning_marker: configured_scene::WarningMarker::None,
            trend: None,
            graph: None,
            gauge: None,
        };
        // Unavailable gauges still show their range endpoints. Statistics
        // reset and bar/PSI toggles must invalidate these retained pixels too.
        if let Some(widget) = self.layout.as_ref().and_then(|layout| layout.widget(cell))
            && !widget.gauge.kind.is_numeric()
        {
            key.gauge = self.gauge_reading(cell).map(|reading| reading.raster_key());
        }
        if let Some(layout) = &self.layout {
            let widget = layout.widget(cell)?;
            let warning = self.widget_warning(cell)?;
            key.flags[1] = warning.blink;
            key.flags[2] = warning.unavailable;
            key.warning_marker = warning.marker;
            key.background = configured_scene::panel_color(if key.flags[1] {
                widget.colors.warning.background
            } else {
                widget.colors.background
            });
            if key.quality != Quality::Valid {
                return Some(key);
            }
            key.text = self.configured_text(cell)?;
            key.graph = widget.shows_graph().then(|| state.graph_cache_identity()).flatten();
            key.flags[0] = widget.shows_extrema() && state.is_new_peak_at(self.now_ms);
            key.trend = widget.shows_trend().then(|| state.get_trend_at(self.now_ms)).flatten();
            return Some(key);
        }
        if key.quality != Quality::Valid {
            key.background = BLACK;
            return Some(key);
        }
        key.graph = if cell == 0 { None } else { state.graph_cache_identity() };
        let demo = self.measurements.mode == DataMode::Demo;
        let critical = self
            .measurements
            .alarm_active_at(SensorId::ALL[CELL_SENSORS[cell]], self.now_ms);
        if !critical || !self.blink_on {
            key.background = BLACK;
        }
        match cell {
            0 => {
                let animated = demo && value >= BOOST_EASTER_EGG_BAR;
                key.text = cell_text::boost(value, maximum, self.boost_psi);
                if animated {
                    key.text[2].clear();
                }
                key.background = BLACK;
                key.flags = [self.boost_psi, animated, animated && self.blink_on];
            }
            1 => {
                key.text = cell_text::lambda(value);
            }
            2 => {
                key.text = cell_text::battery(value, state.minimum().unwrap_or(0.0), maximum);
                key.flags[0] = critical;
                key.flags[1] = state.is_new_peak_at(self.now_ms);
                key.trend = state.get_trend_at(self.now_ms);
            }
            _ => {
                key.text = cell_text::temperature(value, maximum, state.get_average());
                key.flags = [
                    critical,
                    temp_uses_medium_font(value),
                    state.is_new_peak_at(self.now_ms),
                ];
                key.trend = state.get_trend_at(self.now_ms);
            }
        }
        Some(key)
    }
}

#[derive(Clone, PartialEq, Eq)]
/// Header dependencies compared as visible strings/status, not unrounded input rates.
struct HeaderKey {
    /// Demo versus vehicle source mode label.
    mode: DataMode,
    /// Derived source/adapter status label.
    status: VehicleStatus,
    /// Already rounded/bounded FPS text.
    fps: FpsText,
    /// Validated configured view name, or absent for legacy layout.
    name: Option<heapless::String<14>>,
    /// Visible configuration fallback/fault marker.
    fault: bool,
}

/// Up to eight visible widget IDs/placements, preserving gaps and layout changes.
type Geometry = [Option<(u16, dashboard_config::Placement)>; 8];

/// Metadata for pixels in one physical buffer; does not prove a completed panel transfer.
struct CachedFrame {
    /// Last prepared header, absent until initialized/after invalidation.
    header: Option<HeaderKey>,
    /// Last prepared visible widget identities and cell placements.
    geometry: Geometry,
    /// Logical dimensions of this buffer's last prepared frame.
    panel: dashboard_config::Geometry,
    /// Last prepared key for each visible slot.
    cells: [Option<CellKey>; 8],
    /// Last prepared opaque overlay; replacement/removal requires underlay restoration.
    overlay: Option<Overlay>,
}
impl CachedFrame {
    /// Returns invalid/uninitialized metadata; default landscape does not imply valid pixels.
    const fn new() -> Self {
        Self {
            header: None,
            geometry: [None; 8],
            panel: dashboard_config::Geometry::Landscape,
            cells: [const { None }; 8],
            overlay: None,
        }
    }
}

/// Independent retained raster metadata for exactly two buffers; no pixel storage or DMA ownership.
pub struct DashboardCache {
    /// Per-buffer state indexed by physical slot0/1.
    frames: [CachedFrame; 2],
}
impl Default for DashboardCache {
    /// Returns two invalid cache entries requiring full initial paints.
    fn default() -> Self { Self::new() }
}
impl DashboardCache {
    /// Returns empty metadata for both buffers; the first preparation requests a full clear/paint.
    pub const fn new() -> Self {
        Self {
            frames: [const { CachedFrame::new() }; 2],
        }
    }

    /// Invalidates physical `buffer`0/1 after external paint/clear or abandoned work.
    /// Invalid indices are inert; the other buffer's metadata is preserved.
    pub fn invalidate(
        &mut self,
        buffer: usize,
    ) {
        if let Some(frame) = self.frames.get_mut(buffer) {
            *frame = CachedFrame::new();
        }
    }

    /// Returns and immediately records a landscape redraw plan for `scene` in
    /// physical `buffer`. Caller must execute the entire plan or invalidate that
    /// buffer before reuse; no paint/transfer happens here. Invalid indices return full redraw.
    pub fn prepare(
        &mut self,
        buffer: usize,
        scene: &DashboardScene<'_>,
    ) -> Redraw {
        self.prepare_in(buffer, scene, dashboard_config::Geometry::Landscape)
    }

    /// Geometry belongs to the frame being produced, not a mutable selected view.
    /// Returns a redraw plan for `scene`/`panel` in physical `buffer`0/1 and updates
    /// its metadata immediately. Layout/geometry changes and opaque overlay
    /// removal/replacement force full clear; exhausted graph/config identities force
    /// conservative cell redraw. Invalid indices return full redraw without mutation.
    /// Caller must complete every planned paint or invalidate the buffer after cancellation.
    pub fn prepare_in(
        &mut self,
        buffer: usize,
        scene: &DashboardScene<'_>,
        panel: dashboard_config::Geometry,
    ) -> Redraw {
        let Some(frame) = self.frames.get_mut(buffer) else {
            return Redraw::FULL;
        };
        let overlay = scene.effective_overlay();
        // Removing/replacing any opaque overlay restores its entire underlay,
        // including static dividers and pixels outside cell interiors.
        let geometry = core::array::from_fn(|slot| {
            scene
                .layout
                .as_ref()
                .and_then(|layout| layout.widget(slot))
                .map(|w| (w.id, w.placement))
        });
        let clear = frame.panel != panel
            || frame.header.is_none()
            || frame
                .header
                .as_ref()
                .is_some_and(|key| key.name.is_some() != scene.layout.is_some())
            || frame.geometry != geometry
            || (frame.overlay.is_some() && frame.overlay != overlay);
        frame.geometry = geometry;
        frame.panel = panel;
        let header = HeaderKey {
            mode: scene.measurements.mode,
            status: scene.status,
            fps: format_fps(scene.fps_mode, scene.fps_instant, scene.fps_average),
            name: scene
                .layout
                .as_ref()
                .map(|layout| layout.name().try_into().expect("validated name")),
            fault: scene.layout.as_ref().is_some_and(|layout| layout.fault),
        };
        let header_dirty = clear || frame.header.as_ref() != Some(&header);
        frame.header = Some(header);
        let mut cells = 0;
        for (cell, cached) in frame.cells.iter_mut().enumerate() {
            let key = scene.cell_key(cell);
            let untracked = scene
                .cell_sensor(cell)
                .is_some_and(|sensor| scene.measurements.sensors[sensor].graph_cache_identity().is_none())
                || scene
                    .layout
                    .as_ref()
                    .is_some_and(|layout| layout.configuration().cache_identity().is_none());
            if (clear && key.is_some()) || (untracked && key.is_some()) || *cached != key {
                cells |= 1 << cell;
                // Retain the existing value when equal. In particular, a
                // stationary frame need not rewrite eight formatted-text keys in each physical buffer.
                *cached = key;
            }
        }
        let overlay_dirty = overlay.is_some() && (clear || cells != 0 || frame.overlay != overlay);
        frame.overlay = overlay;
        Redraw {
            clear,
            header: header_dirty,
            cells,
            overlay: overlay_dirty,
        }
    }

    /// Prepares and executes this `scene` on `display` for physical `buffer`, using
    /// target geometry. Returns executed redraw metadata; target errors are discarded,
    /// so callers with fallible targets must invalidate after any externally detected failure.
    /// This synchronous paint does not submit/complete a transfer.
    pub fn draw<D: DrawTarget<Color = Rgb565>>(
        &mut self,
        display: &mut D,
        buffer: usize,
        scene: &DashboardScene<'_>,
    ) -> Redraw {
        let redraw = self.prepare_in(buffer, scene, configured_scene::target_geometry(display));
        redraw.begin(display, scene);
        for cell in 0..8 {
            if redraw.cells & (1 << cell) != 0 {
                scene.draw_cell(display, cell);
            }
        }
        redraw.finish(display, scene);
        redraw
    }
}

#[derive(Clone, Copy, Debug)]
/// Ordered work required to make one buffer match a prepared immutable scene.
pub struct Redraw {
    /// Clear entire target before any header/cell drawing.
    pub clear: bool,
    /// Redraw header after optional clear.
    pub header: bool,
    /// Visible slot bitmask to draw between begin and finish, including removed-slot repair decisions.
    pub cells: u8,
    /// Redraw opaque overlay after cells/dividers, if the scene has one.
    pub overlay: bool,
}
impl Redraw {
    /// Conservative fallback for unknown buffers: clear/header/all slots/overlay.
    const FULL: Self = Self {
        clear: true,
        header: true,
        cells: u8::MAX,
        overlay: true,
    };

    /// Applies requested clear/header work to `display` for the same immutable
    /// `scene` used during preparation. Caller draws masked cells next; errors are discarded.
    pub fn begin<D: DrawTarget<Color = Rgb565>>(
        &self,
        display: &mut D,
        scene: &DashboardScene<'_>,
    ) {
        if self.clear {
            display.clear(BLACK).ok();
        }
        if self.header {
            scene.draw_header(display);
        }
    }

    /// Restores legacy dividers after a clear and draws requested overlay on
    /// `display` for prepared `scene`, after all masked cells. Errors are discarded;
    /// this finishes raster work only, without transfer/scanout acknowledgment.
    pub fn finish<D: DrawTarget<Color = Rgb565>>(
        &self,
        display: &mut D,
        scene: &DashboardScene<'_>,
    ) {
        if self.clear && scene.layout.is_none() {
            draw_dividers(display);
        }
        if self.overlay {
            scene.draw_overlay(display);
        }
    }
}
