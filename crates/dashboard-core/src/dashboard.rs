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

pub const CELL_SENSORS: [usize; 8] = [0, 7, 6, 2, 1, 3, 4, 5];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Overlay {
    Reset,
    Fps(FpsMode),
    BoostUnit(bool),
    Brightness(Brightness),
    Critical(SensorId, bool),
    Warnings(crate::view_state::WarningPage),
    View(crate::view_state::PopupText),
}

impl Overlay {
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

    pub const fn warning_phase(self) -> Option<bool> {
        match self {
            Self::Critical(_, phase) => Some(phase),
            Self::Warnings(page) => Some(page.blink),
            _ => None,
        }
    }

    pub fn prioritize(
        user: Option<Self>,
        alerts: crate::alarms::Alerts,
        blink: bool,
    ) -> Option<Self> {
        user.or_else(|| alerts.priority().map(|id| Self::Critical(id, blink)))
    }

    /// Warnings belong to the dashboard; diagnostic pages stay unobstructed.
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

pub struct DashboardScene<'a> {
    /// Prepared once per configured frame; None is the raw legacy fixture path.
    pub readings: Option<&'a crate::presentation::DisplayReadings<'a>>,
    /// None retains the original fixed scene for historical replay contracts.
    /// Device and browser use the same validated configurable path.
    pub layout: Option<Layout<'a>>,
    pub measurements: &'a DashboardSnapshot,
    pub status: VehicleStatus,
    pub now_ms: u64,
    pub blink_on: bool,
    pub boost_psi: bool,
    pub fps_mode: FpsMode,
    pub fps_instant: f32,
    pub fps_average: f32,
    /// Colors already evaluated by the time-based transition owner, in cell order.
    pub backgrounds: [Rgb565; 8],
    pub overlay: Option<Overlay>,
}

impl DashboardScene<'_> {
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

    pub fn configured_text(
        &self,
        cell: usize,
    ) -> Option<CellText> {
        let layout = self.layout.as_ref()?;
        let widget = layout.widget(cell)?;
        let current = match self.readings {
            Some(readings) => readings.widget(cell)?.value,
            None => {
                self.measurements
                    .value_at(SensorId::from_channel(widget.channel), self.now_ms)
            }
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

    pub fn gauge_reading(
        &self,
        cell: usize,
    ) -> Option<crate::widgets::gauge::Reading> {
        let layout = self.layout.as_ref()?;
        let widget = layout.widget(cell)?;
        let (value, peak) = match self.readings {
            Some(readings) => {
                let reading = readings.widget(cell)?;
                (reading.value, reading.peak)
            }
            None => {
                (
                    self.measurements
                        .value_at(SensorId::from_channel(widget.channel), self.now_ms),
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

    pub fn cell_sensor(
        &self,
        cell: usize,
    ) -> Option<usize> {
        if let Some(layout) = &self.layout {
            return layout.widget(cell).map(|w| SensorId::from_channel(w.channel) as usize);
        }
        CELL_SENSORS.get(cell).copied()
    }

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

    pub fn draw_overlay<D: DrawTarget<Color = Rgb565>>(
        &self,
        display: &mut D,
    ) {
        if let Some(overlay) = self.effective_overlay() {
            overlay.draw(display);
        }
    }

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
struct CellKey {
    widget: Option<(dashboard_config::ConfigCacheIdentity, u8)>,
    quality: Quality,
    source: Source,
    text: CellText,
    background: Rgb565,
    flags: [bool; 3],
    warning_marker: configured_scene::WarningMarker,
    trend: Option<bool>,
    graph: Option<crate::GraphCacheIdentity>,
    gauge: Option<crate::widgets::gauge::RasterKey>,
}
impl DashboardScene<'_> {
    pub(crate) fn quality(
        &self,
        cell: usize,
    ) -> Quality {
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

    fn cell_key(
        &self,
        cell: usize,
    ) -> Option<CellKey> {
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
struct HeaderKey {
    mode: DataMode,
    status: VehicleStatus,
    fps: FpsText,
    name: Option<heapless::String<14>>,
    fault: bool,
}

type Geometry = [Option<(u16, dashboard_config::Placement)>; 8];

struct CachedFrame {
    header: Option<HeaderKey>,
    geometry: Geometry,
    cells: [Option<CellKey>; 8],
    overlay: Option<Overlay>,
}
impl CachedFrame {
    const fn new() -> Self {
        Self {
            header: None,
            geometry: [None; 8],
            cells: [const { None }; 8],
            overlay: None,
        }
    }
}

pub struct DashboardCache {
    frames: [CachedFrame; 2],
}
impl Default for DashboardCache {
    fn default() -> Self { Self::new() }
}
impl DashboardCache {
    pub const fn new() -> Self {
        Self {
            frames: [const { CachedFrame::new() }; 2],
        }
    }

    pub fn invalidate(
        &mut self,
        buffer: usize,
    ) {
        if let Some(frame) = self.frames.get_mut(buffer) {
            *frame = CachedFrame::new();
        }
    }

    pub fn prepare(
        &mut self,
        buffer: usize,
        scene: &DashboardScene<'_>,
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
        let clear = frame.header.is_none()
            || frame
                .header
                .as_ref()
                .is_some_and(|key| key.name.is_some() != scene.layout.is_some())
            || frame.geometry != geometry
            || (frame.overlay.is_some() && frame.overlay != overlay);
        frame.geometry = geometry;
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

    pub fn draw<D: DrawTarget<Color = Rgb565>>(
        &mut self,
        display: &mut D,
        buffer: usize,
        scene: &DashboardScene<'_>,
    ) -> Redraw {
        let redraw = self.prepare(buffer, scene);
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
pub struct Redraw {
    pub clear: bool,
    pub header: bool,
    pub cells: u8,
    pub overlay: bool,
}
impl Redraw {
    const FULL: Self = Self {
        clear: true,
        header: true,
        cells: u8::MAX,
        overlay: true,
    };

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
