//! Hardware-independent dashboard scene used by the device and replay tests.
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;

use crate::acquisition::DashboardSnapshot;
use crate::config::{COL_WIDTH, HEADER_HEIGHT, ROW_HEIGHT};
use crate::render::{FpsMode, FpsText, format_fps};
use crate::samples::{DataMode, Quality, SensorId, Source};
use crate::sensor_state::SensorSnapshot;
use crate::thresholds::BOOST_EASTER_EGG_BAR;
use crate::ui::BLACK;
use crate::vehicle_status::VehicleStatus;
use crate::widgets::cell_text::{self, CellText};
use crate::widgets::*;

const CELL_SENSORS: [usize; 8] = [0, 7, 6, 2, 1, 3, 4, 5];

struct Temperature {
    label: &'static str,
    colors: fn(f32) -> (Rgb565, Rgb565),
    critical: fn(f32) -> bool,
    low: Option<fn(f32) -> bool>,
}
const TEMPERATURES: [Temperature; 5] = [
    Temperature {
        label: "COOL",
        colors: temp_color_water,
        critical: is_critical_water,
        low: None,
    },
    Temperature {
        label: "OIL",
        colors: temp_color_oil_dsg,
        critical: is_critical_oil_dsg,
        low: Some(is_low_temp_oil),
    },
    Temperature {
        label: "DSG",
        colors: temp_color_oil_dsg,
        critical: is_critical_oil_dsg,
        low: None,
    },
    Temperature {
        label: "IAT",
        colors: temp_color_iat,
        critical: is_critical_iat,
        low: None,
    },
    Temperature {
        label: "EGT",
        colors: temp_color_egt,
        critical: is_critical_egt,
        low: None,
    },
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Overlay {
    Reset,
    Fps(FpsMode),
    BoostUnit(bool),
    Brightness(u8),
    Danger(bool),
}

pub struct DashboardScene<'a> {
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
    pub fn cell_sensor(
        &self,
        cell: usize,
    ) -> Option<usize> {
        CELL_SENSORS.get(cell).copied()
    }

    pub fn draw_header<D: DrawTarget<Color = Rgb565>>(
        &self,
        display: &mut D,
    ) {
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
            draw_unavailable(display, x, y, CELL_LABELS[cell], quality);
            return;
        }
        let alarms_enabled = self.measurements.mode == DataMode::Demo;
        let data = display_data(state, self.now_ms);
        let background = Some(if alarms_enabled { self.backgrounds[cell] } else { BLACK });
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
                    alarms_enabled && value >= BOOST_EASTER_EGG_BAR,
                    self.blink_on,
                    0,
                )
            }
            1 => {
                draw_lambda_cell(display, x, y, COL_WIDTH, ROW_HEIGHT, value, &data, 0, background);
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
                    self.blink_on,
                    0,
                    background,
                    alarms_enabled,
                );
            }
            _ => {
                let temperature = &TEMPERATURES[cell - 3];
                draw_temp_cell(
                    display,
                    x,
                    y,
                    COL_WIDTH,
                    ROW_HEIGHT,
                    temperature.label,
                    value,
                    maximum,
                    &data,
                    temperature.colors,
                    temperature.critical,
                    temperature.low,
                    self.blink_on,
                    0,
                    background,
                    alarms_enabled,
                );
            }
        }
    }

    pub fn draw_overlay<D: DrawTarget<Color = Rgb565>>(
        &self,
        display: &mut D,
    ) {
        match self.overlay {
            Some(Overlay::Reset) => draw_reset_popup(display),
            Some(Overlay::Fps(mode)) => draw_fps_toggle_popup(display, mode),
            Some(Overlay::BoostUnit(psi)) => draw_boost_unit_popup(display, psi),
            Some(Overlay::Brightness(percent)) => draw_brightness_popup(display, u32::from(percent.min(100))),
            Some(Overlay::Danger(blink)) => draw_danger_manifold_popup(display, blink),
            None => {}
        }
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
        draw_dividers(display);
        self.draw_overlay(display);
    }
}

const CELL_LABELS: [&str; 8] = ["BOOST REL", "LAMBDA", "ECU V", "COOL", "OIL", "DSG OIL", "IAT", "EGT"];

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
    let (buffer, start_idx, count, min, max) = state.get_graph_data();
    SensorDisplayData {
        trend: state.get_trend_at(now_ms),
        is_new_peak: state.is_new_peak_at(now_ms),
        graph_buffer: buffer,
        graph_buffer_size: crate::sensor_state::GRAPH_HISTORY_SIZE,
        graph_start_idx: start_idx,
        graph_count: count,
        graph_min: min,
        graph_max: max,
        average: state.get_average(),
    }
}

#[derive(Clone, PartialEq, Eq)]
struct GraphKey {
    values: [u32; crate::sensor_state::GRAPH_HISTORY_SIZE],
    count: usize,
    min: u32,
    max: u32,
}
impl GraphKey {
    fn new(state: Option<&SensorSnapshot>) -> Self {
        let mut key = Self {
            values: [0; crate::sensor_state::GRAPH_HISTORY_SIZE],
            count: 0,
            min: 0,
            max: 0,
        };
        if let Some(state) = state {
            let (values, start, count, min, max) = state.get_graph_data();
            key.count = count;
            if count > 0 {
                key.min = min.to_bits();
                key.max = max.to_bits();
                for (index, output) in key.values.iter_mut().take(count).enumerate() {
                    *output = values[(start + index) % values.len()].to_bits();
                }
            }
        }
        key
    }
}

#[derive(Clone, PartialEq, Eq)]
struct CellKey {
    quality: Quality,
    source: Source,
    text: CellText,
    background: Rgb565,
    flags: [bool; 6],
    trend: Option<bool>,
    graph: GraphKey,
}
impl DashboardScene<'_> {
    fn quality(
        &self,
        cell: usize,
    ) -> Quality {
        let id = SensorId::ALL[CELL_SENSORS[cell]];
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
    ) -> CellKey {
        let state = &self.measurements.sensors[CELL_SENSORS[cell]];
        let value = state.value().unwrap_or(0.0);
        let maximum = state.maximum().unwrap_or(0.0);
        let mut key = CellKey {
            quality: self.quality(cell),
            source: self.measurements.metadata[CELL_SENSORS[cell]].source,
            text: core::array::from_fn(|_| heapless::String::new()),
            background: self.backgrounds[cell],
            flags: [false; 6],
            trend: None,
            graph: GraphKey::new(None),
        };
        if key.quality != Quality::Valid {
            key.background = BLACK;
            return key;
        }
        key.graph = GraphKey::new(if cell == 0 { None } else { Some(state) });
        let alarms_enabled = self.measurements.mode == DataMode::Demo;
        if !alarms_enabled {
            key.background = BLACK;
        }
        let critical;
        match cell {
            0 => {
                let animated = alarms_enabled && value >= BOOST_EASTER_EGG_BAR;
                key.text = cell_text::boost(value, maximum, self.boost_psi);
                if animated {
                    key.text[2].clear();
                }
                key.background = BLACK;
                key.flags = [self.boost_psi, animated, animated && self.blink_on, false, false, false];
                critical = false;
            }
            1 => {
                key.text = cell_text::lambda(value);
                critical = false;
            }
            2 => {
                key.text = cell_text::battery(value, state.minimum().unwrap_or(0.0), maximum);
                key.flags[0] = alarms_enabled && is_critical_battery(value);
                key.flags[1] = state.is_new_peak_at(self.now_ms);
                key.trend = state.get_trend_at(self.now_ms);
                critical = key.flags[0];
            }
            _ => {
                let temperature = &TEMPERATURES[cell - 3];
                let low = alarms_enabled && temperature.low.is_some_and(|test| test(value));
                critical = alarms_enabled && (temperature.critical)(value);
                key.text = cell_text::temperature(value, maximum, state.get_average());
                key.flags = [
                    critical,
                    alarms_enabled && (temperature.critical)(maximum),
                    low,
                    low && self.blink_on,
                    temp_uses_medium_font(value),
                    state.is_new_peak_at(self.now_ms),
                ];
                key.trend = state.get_trend_at(self.now_ms);
            }
        }
        if critical && !self.blink_on {
            key.background = BLACK;
        }
        key
    }
}

struct CachedFrame {
    header: Option<(DataMode, VehicleStatus, FpsText)>,
    cells: [Option<CellKey>; 8],
    overlay: Option<Overlay>,
}
impl CachedFrame {
    const fn new() -> Self {
        Self {
            header: None,
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
        // Removing/replacing any opaque overlay restores its entire underlay,
        // including static dividers and pixels outside cell interiors.
        let clear = frame.header.is_none() || (frame.overlay.is_some() && frame.overlay != scene.overlay);
        let header = (
            scene.measurements.mode,
            scene.status,
            format_fps(scene.fps_mode, scene.fps_instant, scene.fps_average),
        );
        let header_dirty = clear || frame.header.as_ref() != Some(&header);
        frame.header = Some(header);
        let mut cells = 0;
        for (cell, cached) in frame.cells.iter_mut().enumerate() {
            let key = scene.cell_key(cell);
            if clear || cached.as_ref() != Some(&key) {
                cells |= 1 << cell;
            }
            *cached = Some(key);
        }
        let overlay = scene.overlay.is_some() && (clear || cells != 0 || frame.overlay != scene.overlay);
        frame.overlay = scene.overlay;
        Redraw {
            clear,
            header: header_dirty,
            cells,
            overlay,
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
        if self.clear {
            draw_dividers(display);
        }
        if self.overlay {
            scene.draw_overlay(display);
        }
    }
}
