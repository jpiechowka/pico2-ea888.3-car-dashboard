//! Bounded view geometry and one widget renderer for device and browser.
use dashboard_config::{Channel, Fuel, GraphRange, ValidatedConfig, Widget, WidgetColors};
use embedded_graphics::mono_font::{MonoFont, MonoTextStyle};
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{PrimitiveStyle, Rectangle};
use embedded_graphics::text::Text;

use crate::acquisition::DashboardSnapshot;
use crate::dashboard::DashboardScene;
use crate::presentation::{Unit, format_value, number};
use crate::samples::{DataMode, Quality, SensorId};
use crate::ui::{CENTERED, LABEL_FONT, LEFT_ALIGNED, RED, RIGHT_ALIGNED, WHITE};
use crate::widgets::cell_text::CellText;
use crate::widgets::primitives::{draw_history_graph, draw_trend_arrow};
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
    widget_text_with_value(
        widget,
        snapshot,
        now,
        psi,
        stoich,
        fuel,
        snapshot.value_at(SensorId::from_channel(widget.channel), now),
    )
}

/// Format canonical presentation current while statistics remain in the snapshot.
pub fn widget_text_with_value(
    widget: &Widget,
    snapshot: &DashboardSnapshot,
    now: u64,
    psi: bool,
    stoich: f32,
    fuel: &Fuel,
    current: Option<f32>,
) -> CellText {
    let mut text = core::array::from_fn(|_| heapless::String::new());
    let id = SensorId::from_channel(widget.channel);
    let Some(value) = current else {
        text[0].push_str("--").ok();
        text[5].push_str(snapshot.quality(id, now).label()).ok();
        return text;
    };
    let state = snapshot.sensor(id);
    let unit = Unit::resolve(widget.advanced.readout, psi);
    let suffix = match widget.channel {
        Channel::Boost => "",
        _ => unit.suffix(),
    };
    let decimals = usize::from(widget.display.decimals.min(3));
    let prefix = if unit == Unit::Afr { "AFR " } else { "" };
    format_value(&mut text[0], value, unit, stoich, decimals, prefix, suffix);
    if unit == Unit::Afr && !(0.01..=999.99).contains(&unit.value(value, stoich)) {
        text[0].clear();
        text[0].push_str("AFR --").ok();
    }
    if widget.capabilities().secondary {
        if let Some(secondary) = Unit::secondary(widget.advanced.secondary, unit) {
            format_value(&mut text[1], value, secondary, stoich, decimals, "", secondary.suffix());
        }
        if widget.advanced.readout == dashboard_config::Readout::LambdaAndAfr {
            if (0.01..=999.99).contains(&Unit::Afr.value(value, stoich)) {
                format_value(&mut text[1], value, Unit::Afr, stoich, 2, "AFR ", "");
            } else {
                text[1].push_str("AFR --").ok();
            }
        }
    }
    if unit == Unit::Afr || (widget.capabilities().secondary && widget.advanced.readout.includes_afr()) {
        match fuel {
            Fuel::EquivalentGasoline => number(&mut text[5], "EQ ", stoich, 2, ""),
            Fuel::Stoichiometric { .. } => number(&mut text[5], "S ", stoich, 2, ""),
            Fuel::EthanolBlend(blend) => number(&mut text[5], "E", blend.percent, 2, " EST"),
        }
    }
    if widget.shows_average()
        && let Some(value) = state.get_average()
    {
        format_value(&mut text[2], value, unit, stoich, decimals, "AVG ", suffix);
    }
    if widget.shows_extrema() {
        if let Some(value) = state.minimum() {
            format_value(&mut text[3], value, unit, stoich, decimals, "MIN ", suffix);
        }
        if let Some(value) = state.maximum() {
            format_value(&mut text[4], value, unit, stoich, decimals, "MAX ", suffix);
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
    draw_widget_palette(display, scene, slot, layout, widget.colors);
}

/// Browser-only diagnostic projection: geometry still comes from a validated
/// isolated widget. Arbitrary RGB565 colours cannot affect bounds or work, and
/// remain visible under the caller's explicit INVALID watermark.
pub fn draw_diagnostic_widget<D: DrawTarget<Color = Rgb565>>(
    display: &mut D,
    scene: &DashboardScene<'_>,
    slot: usize,
    colors: WidgetColors,
) {
    if let Some(layout) = &scene.layout {
        draw_widget_palette(display, scene, slot, layout, colors);
    }
}

pub(crate) fn panel_color(color: dashboard_config::Color) -> Rgb565 {
    embedded_graphics::pixelcolor::raw::RawU16::new(color.0).into()
}

fn draw_widget_palette<D: DrawTarget<Color = Rgb565>>(
    display: &mut D,
    scene: &DashboardScene<'_>,
    slot: usize,
    layout: &Layout<'_>,
    palette: WidgetColors,
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
    clipped.clear(panel_color(palette.background)).ok();
    outer
        .into_styled(PrimitiveStyle::with_stroke(panel_color(palette.border), 1))
        .draw(&mut clipped)
        .ok();
    let interior = Rectangle::new(outer.top_left + Point::new(2, 2), outer.size - Size::new(4, 4));
    let mut display = clipped.clipped(&interior);
    let id = SensorId::from_channel(widget.channel);
    let quality = scene.quality(slot);
    let valid = quality == Quality::Valid;
    let warning = valid && scene.blink_on && layout.alarm_active(slot, scene.measurements, scene.now_ms);
    let background = panel_color(if warning {
        palette.warning.background
    } else {
        palette.background
    });
    let foreground = |color| panel_color(if warning { palette.warning.foreground } else { color });
    // The outer clear already covers the inset. Only a different warning
    // background requires another fill; the one-pixel border cannot reach it.
    if background != panel_color(palette.background) {
        display.clear(background).ok();
    }
    let style = MonoTextStyle::new(LABEL_FONT, foreground(palette.label));
    let center = i32::from(rect.x + rect.width / 2);
    let mut y = i32::from(rect.y) + 3;
    Text::with_text_style(&widget.label, Point::new(center, y + 8), style, CENTERED)
        .draw(&mut display)
        .ok();
    y += 12;
    let state = scene.measurements.sensor(id);
    if widget.shows_trend() {
        if valid && let Some(rising) = state.get_trend_at(scene.now_ms) {
            draw_trend_arrow(&mut display, center, y + 4, rising, foreground(palette.trend));
        }
        y += 10;
    }
    let mut text = scene.configured_text(slot).expect("validated visible widget");
    if !valid {
        text[5].clear();
        text[5].push_str(quality.label()).ok();
    }
    let rows = text[1..].iter().filter(|s| !s.is_empty()).count() as i32;
    let bottom = i32::from(rect.y + rect.height) - 3;
    let graph = valid && widget.shows_graph();
    let available = (bottom - y - rows * 10 - if graph { 17 } else { 0 }).max(0) as u32;
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
            foreground(palette.value),
        )
    } else {
        crate::widgets::gauge::draw(
            &mut display,
            widget.gauge.kind,
            Rectangle::new(
                Point::new(interior.top_left.x, y),
                Size::new(interior.size.width, available),
            ),
            scene.gauge_reading(slot).expect("validated gauge"),
            &text[0],
            false, // Prepared gauge endpoints are already in the resolved unit.
            (
                foreground(palette.value),
                foreground(palette.gauge),
                if warning {
                    background
                } else {
                    panel_color(palette.track)
                },
                foreground(palette.ticks),
            ),
        );
        available
    };
    y += height as i32;
    if !text[1].is_empty() {
        Text::with_text_style(
            &text[1],
            Point::new(center, y + 8),
            footer_style(&text[1], interior.size.width, foreground(palette.secondary)),
            CENTERED,
        )
        .draw(&mut display)
        .ok();
        y += 10;
    }
    let footer_rows = text[2..].iter().filter(|s| !s.is_empty()).count() as i32;
    let footer_top = bottom - footer_rows * 10;
    if graph {
        let history = state.history();
        let bounds = match widget.display.graph_range {
            GraphRange::Auto => history.bounds(),
            GraphRange::Fixed { min, max } => Some((min, max)),
        };
        draw_history_graph(
            &mut display,
            i32::from(rect.x) + 5,
            y,
            u32::from(rect.width) - 10,
            (footer_top - y).max(0) as u32,
            history,
            bounds,
            |_| foreground(palette.graph),
        );
    }
    let mut y = footer_top;
    for (index, row) in text.iter().enumerate().skip(2).filter(|(_, s)| !s.is_empty()) {
        let row_color = foreground(match index {
            2 => palette.average,
            3 | 4 if valid && state.is_new_peak_at(scene.now_ms) => palette.peak,
            3 => palette.minimum,
            4 => palette.maximum,
            _ => palette.secondary,
        });
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
