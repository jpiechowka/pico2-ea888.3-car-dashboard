//! Hardware-independent dashboard scene used by the device and replay tests.
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;

use crate::acquisition::DashboardSnapshot;
use crate::config::{COL_WIDTH, HEADER_HEIGHT, ROW_HEIGHT};
use crate::render::FpsMode;
use crate::sensor_state::SensorSnapshot;
use crate::thresholds::BOOST_EASTER_EGG_BAR;
use crate::ui::BLACK;
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
        draw_header(display, self.fps_mode, self.fps_instant, self.fps_average);
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
        let data = display_data(state, self.now_ms);
        let background = Some(self.backgrounds[cell]);
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
                    value >= BOOST_EASTER_EGG_BAR,
                    self.blink_on,
                    0,
                )
            }
            1 => {
                draw_afr_cell(
                    display,
                    x,
                    y,
                    COL_WIDTH,
                    ROW_HEIGHT,
                    value,
                    &data,
                    self.blink_on,
                    0,
                    background,
                );
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
