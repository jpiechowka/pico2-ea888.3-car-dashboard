//! Bounded view geometry and one widget renderer for device and browser.
use core::fmt::Write;

use dashboard_config::{Channel, Fuel, GraphRange, ValidatedConfig, Widget};
use embedded_graphics::mono_font::{MonoFont, MonoTextStyle};
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{PrimitiveStyle, Rectangle};
use embedded_graphics::text::Text;

use crate::acquisition::DashboardSnapshot;
use crate::dashboard::DashboardScene;
use crate::samples::{DataMode, Quality, SensorId};
use crate::thresholds::BAR_TO_PSI;
use crate::ui::{BLACK, CENTERED, GRAY, LABEL_FONT, LEFT_ALIGNED, RED, RIGHT_ALIGNED, WHITE};
use crate::widgets::cell_text::CellText;
use crate::widgets::primitives::{draw_mini_graph, draw_trend_arrow};
use crate::widgets::value_text::draw_fitted_value;

/// Validated lookup table for one view. No configuration copy or repeated
/// 64-widget search is needed while drawing its eight visible slots.
#[derive(Clone, Copy)]
pub struct Layout<'a> {
    configuration: &'a ValidatedConfig,
    view_index: usize,
    widgets: [u8; 8],
    count: usize,
    pub fault: bool,
}

impl<'a> Layout<'a> {
    pub fn new(
        configuration: &'a ValidatedConfig,
        view: u8,
        fault: bool,
    ) -> Option<Self> {
        let view_index = configuration.config().views.iter().position(|v| v.id == view)?;
        let mut layout = Self {
            configuration,
            view_index,
            widgets: [0; 8],
            count: 0,
            fault,
        };
        for (index, widget) in configuration.config().widgets.iter().enumerate() {
            if widget.view == view {
                // Nonoverlapping validated placements occupy at least one of
                // eight grid cells, proving at most eight widgets per view.
                *layout.widgets.get_mut(layout.count)? = index as u8;
                layout.count += 1;
            }
        }
        Some(layout)
    }

    pub fn name(&self) -> &str { &self.configuration.config().views[self.view_index].name }

    pub fn id(&self) -> u8 { self.configuration.config().views[self.view_index].id }

    pub fn alarm_active(
        &self,
        slot: usize,
        samples: &DashboardSnapshot,
        now: u64,
    ) -> bool {
        self.rule_index(slot)
            .and_then(crate::alarms::RuleIndex::new)
            .is_some_and(|i| samples.rule_active_at(i, now))
    }

    pub const fn configuration(&self) -> &'a ValidatedConfig { self.configuration }

    pub const fn count(&self) -> usize { self.count }

    pub fn widget(
        &self,
        slot: usize,
    ) -> Option<&'a Widget> {
        (slot < self.count).then(|| &self.configuration.config().widgets[self.widgets[slot] as usize])
    }

    pub fn rule_index(
        &self,
        slot: usize,
    ) -> Option<usize> {
        (slot < self.count).then(|| self.widgets[slot] as usize)
    }
}

fn number(
    out: &mut heapless::String<16>,
    prefix: &str,
    value: f32,
    decimals: usize,
    suffix: &str,
) {
    if !value.is_finite() || write!(out, "{prefix}{value:.decimals$}{suffix}").is_err() {
        out.clear();
        let _ = out.push_str("--");
    }
}

/// Exact strings used both for pixels and retained equality. Units convert
/// presentation only; histories, configured bounds and alarms stay canonical.
pub fn widget_text(
    widget: &Widget,
    snapshot: &DashboardSnapshot,
    now: u64,
    psi: bool,
    stoich: f32,
    fuel: &Fuel,
) -> CellText {
    let mut text = core::array::from_fn(|_| heapless::String::new());
    let id = SensorId::from_channel(widget.channel);
    let Some(value) = snapshot.value_at(id, now) else {
        text[0].push_str("--").ok();
        text[5].push_str(snapshot.quality(id, now).label()).ok();
        return text;
    };
    let state = snapshot.sensor(id);
    let scale = if widget.channel == Channel::Boost && psi {
        BAR_TO_PSI
    } else {
        1.0
    };
    let suffix = match widget.channel {
        Channel::Boost | Channel::Lambda => "",
        Channel::Battery => "V",
        _ => "C",
    };
    let decimals = usize::from(widget.display.decimals.min(3));
    number(&mut text[0], "", value * scale, decimals, suffix);
    if widget.channel == Channel::Boost {
        if psi {
            number(&mut text[1], "", value, 2, " BAR");
        } else {
            number(&mut text[1], "", value * BAR_TO_PSI, 1, " PSI");
        }
    }
    if widget.channel == Channel::Lambda && widget.display.afr {
        let afr = value * stoich;
        if (0.01..=999.99).contains(&afr) {
            number(&mut text[1], "AFR ", afr, 2, "");
        } else {
            text[1].push_str("AFR --").ok();
        }
        match fuel {
            Fuel::EquivalentGasoline => number(&mut text[5], "EQ ", stoich, 2, ""),
            Fuel::Stoichiometric { .. } => number(&mut text[5], "S ", stoich, 2, ""),
            Fuel::EthanolBlend(blend) => number(&mut text[5], "E", blend.percent, 2, " EST"),
        }
    }
    if widget.shows_average()
        && let Some(value) = state.get_average()
    {
        number(&mut text[2], "AVG ", value * scale, decimals, suffix);
    }
    if widget.shows_extrema() {
        if let Some(value) = state.minimum() {
            number(&mut text[3], "MIN ", value * scale, decimals, suffix);
        }
        if let Some(value) = state.maximum() {
            number(&mut text[4], "MAX ", value * scale, decimals, suffix);
        }
    }
    text
}

pub fn fitted_font(
    text: &str,
    width: u32,
    height: u32,
) -> &'static MonoFont<'static> {
    crate::widgets::value_text::fit_value(text, Size::new(width, height), 1)
        .map_or(&embedded_graphics::mono_font::ascii::FONT_4X6, |value| value.font)
}

pub(crate) fn draw_header<D: DrawTarget<Color = Rgb565>>(
    display: &mut D,
    scene: &DashboardScene<'_>,
    layout: &Layout<'_>,
) {
    Rectangle::new(Point::zero(), Size::new(320, 26))
        .into_styled(PrimitiveStyle::with_fill(RED))
        .draw(display)
        .ok();
    let status = if scene.measurements.mode == DataMode::Demo {
        "DEMO"
    } else {
        scene.status.label()
    };
    let small = MonoTextStyle::new(LABEL_FONT, WHITE);
    let bounds = Rectangle::new(Point::new(4, 0), Size::new(58, 26));
    // The fallback fault must not hide the synthetic-data or transport status.
    // Two short lines fit the status lane without displacing the view or FPS.
    Text::with_text_style(
        status,
        Point::new(4, if layout.fault { 10 } else { 17 }),
        small,
        LEFT_ALIGNED,
    )
    .draw(&mut display.clipped(&bounds))
    .ok();
    if layout.fault {
        Text::with_text_style("CONFIG!", Point::new(4, 22), small, LEFT_ALIGNED)
            .draw(&mut display.clipped(&bounds))
            .ok();
    }
    let bounds = Rectangle::new(Point::new(64, 0), Size::new(168, 26));
    Text::with_text_style(
        layout.name(),
        Point::new(148, 19),
        crate::ui::TITLE_STYLE_WHITE,
        CENTERED,
    )
    .draw(&mut display.clipped(&bounds))
    .ok();
    let bounds = Rectangle::new(Point::new(236, 0), Size::new(80, 26));
    let fps = crate::render::format_fps(scene.fps_mode, scene.fps_instant, scene.fps_average);
    Text::with_text_style(&fps, Point::new(315, 17), small, RIGHT_ALIGNED)
        .draw(&mut display.clipped(&bounds))
        .ok();
}

pub(crate) fn draw_widget<D: DrawTarget<Color = Rgb565>>(
    display: &mut D,
    scene: &DashboardScene<'_>,
    slot: usize,
    layout: &Layout<'_>,
) {
    let Some(widget) = layout.widget(slot) else {
        return;
    };
    let Some(rect) = widget.placement.rect() else {
        return;
    };
    let outer = Rectangle::new(
        Point::new(i32::from(rect.x), i32::from(rect.y)),
        Size::new(u32::from(rect.width), u32::from(rect.height)),
    );
    let mut clipped = display.clipped(&outer);
    clipped.clear(BLACK).ok();
    outer
        .into_styled(PrimitiveStyle::with_stroke(GRAY, 1))
        .draw(&mut clipped)
        .ok();
    let interior = Rectangle::new(outer.top_left + Point::new(2, 2), outer.size - Size::new(4, 4));
    let mut display = clipped.clipped(&interior);
    let id = SensorId::from_channel(widget.channel);
    let quality = scene.quality(slot);
    let valid = quality == Quality::Valid;
    let background = if valid && scene.blink_on && layout.alarm_active(slot, scene.measurements, scene.now_ms) {
        scene.backgrounds[slot]
    } else {
        BLACK
    };
    display.clear(background).ok();
    let color = crate::widgets::label_color_for_bg(background);
    let style = MonoTextStyle::new(LABEL_FONT, color);
    let center = i32::from(rect.x + rect.width / 2);
    let mut y = i32::from(rect.y) + 3;
    Text::with_text_style(&widget.label, Point::new(center, y + 8), style, CENTERED)
        .draw(&mut display)
        .ok();
    y += 12;
    let state = scene.measurements.sensor(id);
    if widget.shows_trend() {
        if valid && let Some(rising) = state.get_trend_at(scene.now_ms) {
            draw_trend_arrow(&mut display, center, y + 4, rising, color);
        }
        y += 10;
    }
    let mut text = widget_text(
        widget,
        scene.measurements,
        scene.now_ms,
        scene.boost_psi,
        layout.configuration.stoich(),
        &layout.configuration.config().fuel,
    );
    if !valid {
        text[5].clear();
        text[5].push_str(quality.label()).ok();
    }
    let rows = text[1..].iter().filter(|s| !s.is_empty()).count() as i32;
    let bottom = i32::from(rect.y + rect.height) - 3;
    let graph = valid && widget.shows_graph();
    let available = (bottom - y - rows * 10 - if graph { 10 } else { 0 }).max(0) as u32;
    let scale = match widget.placement.span {
        dashboard_config::Span::OneByOne => 1,
        dashboard_config::Span::TwoByOne => 2,
        dashboard_config::Span::TwoByTwo => 3,
    };
    let height = if widget.gauge.kind.is_numeric() {
        draw_fitted_value(
            &mut display,
            &text[0],
            Rectangle::new(
                Point::new(interior.top_left.x, y),
                Size::new(interior.size.width, available),
            ),
            scale,
            color,
        )
    } else {
        crate::widgets::gauge::draw(
            &mut display,
            widget.gauge.kind,
            Rectangle::new(
                Point::new(interior.top_left.x, y),
                Size::new(interior.size.width, available),
            ),
            crate::widgets::gauge::Reading::new(widget, scene.measurements, scene.now_ms),
            &text[0],
            scene.boost_psi && widget.channel == Channel::Boost,
            (color, crate::ui::RED, GRAY, color),
        );
        available
    };
    y += height as i32;
    if !text[1].is_empty() {
        Text::with_text_style(
            &text[1],
            Point::new(center, y + 8),
            footer_style(&text[1], interior.size.width, color),
            CENTERED,
        )
        .draw(&mut display)
        .ok();
        y += 10;
    }
    let footer_rows = text[2..].iter().filter(|s| !s.is_empty()).count() as i32;
    let footer_top = bottom - footer_rows * 10;
    if graph {
        let (values, start, count, min, max) = state.get_graph_data();
        let (min, max) = match widget.display.graph_range {
            GraphRange::Auto => (min, max),
            GraphRange::Fixed { min, max } => (min, max),
        };
        draw_mini_graph(
            &mut display,
            i32::from(rect.x) + 5,
            y,
            u32::from(rect.width) - 10,
            (footer_top - y).max(0) as u32,
            values,
            values.len(),
            start,
            count,
            min,
            max,
            |_| color,
        );
    }
    let mut y = footer_top;
    for (index, row) in text.iter().enumerate().skip(2).filter(|(_, s)| !s.is_empty()) {
        let row_color = if color == WHITE && valid {
            if index == 2 {
                crate::ui::ORANGE
            } else if matches!(index, 3 | 4) && state.is_new_peak_at(scene.now_ms) {
                crate::ui::YELLOW
            } else {
                color
            }
        } else {
            color
        };
        Text::with_text_style(
            row,
            Point::new(center, y + 8),
            footer_style(row, interior.size.width, row_color),
            CENTERED,
        )
        .draw(&mut display)
        .ok();
        y += 10;
    }
}

fn footer_style(
    text: &str,
    width: u32,
    color: Rgb565,
) -> MonoTextStyle<'static, Rgb565> {
    let font = if text.len() as u32 * 6 <= width {
        LABEL_FONT
    } else {
        &embedded_graphics::mono_font::ascii::FONT_4X6
    };
    MonoTextStyle::new(font, color)
}
