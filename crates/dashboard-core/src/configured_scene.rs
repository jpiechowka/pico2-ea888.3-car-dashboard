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

/// Only the visible badge identity is retained, never another warning owner.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum WarningMarker {
    None,
    ActiveAck,
    ActiveSnoozed,
    Latched,
    LatchedAck,
    LatchedSnoozed,
    Cleared,
}
impl WarningMarker {
    fn label(self) -> &'static str {
        match self {
            Self::None => "",
            Self::ActiveAck => "ACTIVE / ACK",
            Self::ActiveSnoozed => "ACTIVE ACK SNOOZE",
            Self::Latched => "LATCHED",
            Self::LatchedAck => "LATCHED / ACK",
            Self::LatchedSnoozed => "LATCH ACK SNOOZE",
            Self::Cleared => "CLEARED ACK DUE",
        }
    }
}

pub(crate) struct WidgetWarning {
    pub marker: WarningMarker,
    pub unavailable: bool,
    pub blink: bool,
}
impl WidgetWarning {
    pub fn new(
        eligibility: crate::warning_state::WarningEligibility,
        valid: bool,
        blink_on: bool,
    ) -> Self {
        use crate::alarms::Status;
        let active = eligibility.status == Status::Active;
        let marker = if !eligibility.visible() {
            WarningMarker::None
        } else if active && eligibility.acknowledged {
            if eligibility.snoozed {
                WarningMarker::ActiveSnoozed
            } else {
                WarningMarker::ActiveAck
            }
        } else if eligibility.latched {
            if eligibility.status != Status::Unavailable && eligibility.cleared {
                WarningMarker::Cleared
            } else if eligibility.snoozed {
                WarningMarker::LatchedSnoozed
            } else if eligibility.acknowledged {
                WarningMarker::LatchedAck
            } else {
                WarningMarker::Latched
            }
        } else {
            WarningMarker::None
        };
        Self {
            marker,
            unavailable: eligibility.latched && eligibility.status == Status::Unavailable,
            blink: valid && active && eligibility.attention() && blink_on,
        }
    }
}

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
    if target_geometry(display) == dashboard_config::Geometry::Portrait {
        draw_portrait_header(display, scene, layout);
        return;
    }
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
    draw_widget_palette(display, scene, slot, layout, widget.colors, None);
}

/// Browser-only diagnostic projection: geometry still comes from a validated
/// isolated widget. Arbitrary RGB565 colours cannot affect bounds or work, and
/// remain visible under the caller's explicit INVALID watermark.
pub fn draw_diagnostic_widget<D: DrawTarget<Color = Rgb565>>(
    display: &mut D,
    scene: &DashboardScene<'_>,
    slot: usize,
    colors: WidgetColors,
    band_colors: &[dashboard_config::Color],
) {
    if let Some(layout) = &scene.layout {
        let Some(widget) = layout.widget(slot) else {
            return;
        };
        // Only bounded colours cross this diagnostic boundary. Every numeric,
        // label and geometry field still comes from the validated projection.
        let mut advanced = widget.advanced;
        let mut bands = [dashboard_config::Band::EMPTY; 3];
        let count = advanced.bands().len();
        bands[..count].copy_from_slice(advanced.bands());
        for (band, color) in bands[..count].iter_mut().zip(band_colors.iter().take(3)) {
            band.color = *color;
        }
        advanced.set_bands(&bands[..count]).expect("bounded diagnostic bands");
        draw_widget_palette(display, scene, slot, layout, colors, Some(&advanced));
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
    diagnostic_advanced: Option<&dashboard_config::AdvancedDisplay>,
) {
    let Some(widget) = layout.widget(slot) else {
        return;
    };
    let decorations = diagnostic_advanced.unwrap_or(&widget.advanced);
    let geometry = target_geometry(display);
    let Some(rect) = widget.placement.rect_in(geometry) else {
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
    let indication = scene.widget_warning(slot).expect("validated visible warning");
    let warning = indication.blink;
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
    if geometry == dashboard_config::Geometry::Portrait {
        draw_portrait_widget(
            &mut display,
            scene,
            slot,
            layout,
            widget,
            palette,
            decorations,
            interior,
            indication,
        );
        return;
    }
    let style = MonoTextStyle::new(LABEL_FONT, foreground(palette.label));
    let center = i32::from(rect.x + rect.width / 2);
    let mut y = i32::from(rect.y) + 3;
    Text::with_text_style(&widget.label, Point::new(center, y + 8), style, CENTERED)
        .draw(&mut display)
        .ok();
    y += 12;
    if indication.marker != WarningMarker::None {
        // Fixed compact lanes fit every span independently of style, statistics
        // and availability. The badge keeps its warning palette in both phases.
        let height = if indication.unavailable { 16 } else { 8 };
        display
            .fill_solid(
                &Rectangle::new(
                    Point::new(interior.top_left.x, y),
                    Size::new(interior.size.width, height),
                ),
                panel_color(palette.warning.background),
            )
            .ok();
        let style = MonoTextStyle::new(
            &embedded_graphics::mono_font::ascii::FONT_4X6,
            panel_color(palette.warning.foreground),
        );
        Text::with_text_style(indication.marker.label(), Point::new(center, y + 6), style, CENTERED)
            .draw(&mut display)
            .ok();
        if indication.unavailable {
            let label = if quality == Quality::Stale {
                "DATA STALE"
            } else {
                "UNAVAILABLE"
            };
            Text::with_text_style(label, Point::new(center, y + 14), style, CENTERED)
                .draw(&mut display)
                .ok();
        }
        y += height as i32;
    }
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
    let band_rows = if widget.capabilities().band_labels {
        widget.advanced.bands().len() as i32
    } else {
        0
    };
    let reference_row = i32::from(
        widget.capabilities().markers && widget.advanced.reference_mode != dashboard_config::ReferenceMode::Off,
    );
    let legend_height = (band_rows + reference_row) * 8;
    let available = (bottom - y - rows * 10 - legend_height - if graph { 17 } else { 0 }).max(0) as u32;
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
        crate::widgets::gauge::draw_decorated(
            &mut display,
            widget.gauge.kind,
            Rectangle::new(
                Point::new(interior.top_left.x, y),
                Size::new(interior.size.width, available),
            ),
            scene.gauge_reading(slot).expect("validated gauge"),
            &text[0],
            false, // Prepared gauge endpoints are already in the resolved unit.
            crate::widgets::gauge::Paint {
                colors: (
                    foreground(palette.value),
                    foreground(palette.gauge),
                    if warning {
                        background
                    } else {
                        panel_color(palette.track)
                    },
                    foreground(palette.ticks),
                ),
                decorations: Some(crate::widgets::gauge::Decorations {
                    settings: decorations,
                    unit: Unit::resolve(widget.advanced.readout, scene.boost_psi),
                    stoich: layout.configuration().stoich(),
                    peak_color: foreground(palette.peak),
                }),
            },
        );
        available
    };
    y += height as i32;
    // Fixed compact legend lanes are independent of sensor availability. Band
    // labels only exist in 2x2; references explicitly identify manual/zero input.
    let legend_style = MonoTextStyle::new(
        &embedded_graphics::mono_font::ascii::FONT_4X6,
        foreground(palette.label),
    );
    if reference_row != 0 {
        let label = if widget.advanced.reference_mode == dashboard_config::ReferenceMode::Manual {
            "MAN REF"
        } else {
            "ZERO REF"
        };
        Text::with_text_style(label, Point::new(center, y + 6), legend_style, CENTERED)
            .draw(&mut display)
            .ok();
        y += 8;
    }
    if band_rows != 0 {
        for band in decorations.bands() {
            display
                .fill_solid(
                    &Rectangle::new(Point::new(center - 18, y + 1), Size::new(5, 5)),
                    panel_color(band.color),
                )
                .ok();
            Text::with_text_style(band.label(), Point::new(center - 9, y + 6), legend_style, LEFT_ALIGNED)
                .draw(&mut display)
                .ok();
            y += 8;
        }
    }
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
/// Derive shape from the actual drawing target before creating clipped lanes.
pub fn target_geometry<D: Dimensions>(display: &D) -> dashboard_config::Geometry {
    if display.bounding_box().size == Size::new(240, 320) {
        dashboard_config::Geometry::Portrait
    } else {
        dashboard_config::Geometry::Landscape
    }
}

fn draw_portrait_header<D: DrawTarget<Color = Rgb565>>(
    display: &mut D,
    scene: &DashboardScene<'_>,
    layout: &Layout<'_>,
) {
    let area = Rectangle::new(Point::zero(), Size::new(240, 26));
    display.fill_solid(&area, RED).ok();
    let tiny = MonoTextStyle::new(&embedded_graphics::mono_font::ascii::FONT_4X6, WHITE);
    let status = if scene.measurements.mode == DataMode::Demo {
        "DEMO"
    } else {
        scene.status.label()
    };
    Text::new(status, Point::new(4, 7), tiny).draw(display).ok();
    if layout.fault {
        Text::new("CONFIG!", Point::new(68, 7), tiny).draw(display).ok();
    }
    let fps = crate::render::format_fps(scene.fps_mode, scene.fps_instant, scene.fps_average);
    Text::with_text_style(&fps, Point::new(235, 7), tiny, RIGHT_ALIGNED)
        .draw(display)
        .ok();
    Text::with_text_style(
        layout.name(),
        Point::new(120, 22),
        MonoTextStyle::new(LABEL_FONT, WHITE),
        CENTERED,
    )
    .draw(display)
    .ok();
}

/// Measured portrait content budget. Each text index has an independent lane;
/// overflow is explicit and never removes the primary lane or history area.
#[derive(Clone, Copy, Debug)]
pub struct PortraitContentLayout {
    pub text: [Rectangle; 6],
    pub graph: Rectangle,
    pub legend: Rectangle,
    pub fits: bool,
}

pub fn portrait_content_layout(
    bounds: Rectangle,
    text: [&str; 6],
    warning_rows: u32,
    graph: bool,
    numeric: bool,
    legend_rows: u32,
    trend: bool,
) -> PortraitContentLayout {
    let empty = Rectangle::new(Point::zero(), Size::zero());
    let invalid = PortraitContentLayout {
        text: [empty; 6],
        graph: empty,
        legend: empty,
        fits: false,
    };
    if bounds.size.width > 320
        || bounds.size.height > 320
        || warning_rows > 2
        || legend_rows > 4
        || !(-320..=320).contains(&bounds.top_left.x)
        || !(-320..=320).contains(&bounds.top_left.y)
        || text.iter().any(|value| value.len() > 64)
    {
        return invalid;
    }
    let mut lanes = [empty; 6];
    let mut row = 0u32;
    // Secondary/provenance and min/max share a row only after measuring the
    // complete strings. A failed split is promoted to two full-width rows.
    for pair in [[1usize, 5], [2, 2], [3, 4]] {
        let a = pair[0];
        let b = pair[1];
        let aw = text[a].len() as u32 * 4;
        let bw = if a == b { 0 } else { text[b].len() as u32 * 4 };
        if aw == 0 && bw == 0 {
            continue;
        }
        if a != b && aw > 0 && bw > 0 && aw + bw + 4 <= bounds.size.width {
            let split = aw + (bounds.size.width - aw - bw) / 2;
            lanes[a] = Rectangle::new(Point::new(bounds.top_left.x, row as i32 * 7), Size::new(split, 7));
            lanes[b] = Rectangle::new(
                Point::new(bounds.top_left.x + split as i32, row as i32 * 7),
                Size::new(bounds.size.width - split, 7),
            );
            row += 1;
        } else {
            for i in pair.into_iter().take(if a == b { 1 } else { 2 }) {
                if !text[i].is_empty() {
                    lanes[i] = Rectangle::new(
                        Point::new(bounds.top_left.x, row as i32 * 7),
                        Size::new(bounds.size.width, 7),
                    );
                    row += 1;
                }
            }
        }
    }
    let top = 7 + warning_rows * 7;
    // History reserves seven rows for its duration and four for plot padding.
    // Leave five actual curve rows so changing values remain visible.
    let graph_height = if graph { 16 } else { 0 };
    let reserved = top + row * 7 + legend_rows * 7 + graph_height;
    let minimum = if numeric { 8 } else { 28 };
    let fits_height = reserved + minimum <= bounds.size.height;
    // Width-stress values can exceed the compact budget. Keep all enabled
    // lanes represented with FIT! instead of silently dropping their content.
    if !fits_height {
        row = 0;
        for pair in [[1usize, 5], [2, 2], [3, 4]] {
            let a = pair[0];
            let b = pair[1];
            if text[a].is_empty() && text[b].is_empty() {
                continue;
            }
            let split = a != b && !text[a].is_empty() && !text[b].is_empty();
            for (n, i) in pair.into_iter().take(if a == b { 1 } else { 2 }).enumerate() {
                if !text[i].is_empty() {
                    let width = if split {
                        bounds.size.width / 2
                    } else {
                        bounds.size.width
                    };
                    lanes[i] = Rectangle::new(
                        Point::new(
                            bounds.top_left.x + if split { n as i32 * width as i32 } else { 0 },
                            row as i32 * 7,
                        ),
                        Size::new(width, 7),
                    );
                }
            }
            row += 1;
        }
    }
    let reserved = top + row * 7 + legend_rows * 7 + graph_height;
    if reserved + minimum > bounds.size.height {
        return invalid;
    }
    let height = bounds.size.height - reserved;
    lanes[0] = Rectangle::new(
        bounds.top_left + Point::new(0, top as i32),
        Size::new(bounds.size.width.saturating_sub(if trend { 12 } else { 0 }), height),
    );
    let details_top = bounds.top_left.y + (top + height + legend_rows * 7) as i32;
    for lane in lanes.iter_mut().skip(1) {
        if lane.size.height > 0 {
            lane.top_left.y += details_top;
        }
    }
    let legend = Rectangle::new(
        bounds.top_left + Point::new(0, (top + height) as i32),
        Size::new(bounds.size.width, legend_rows * 7),
    );
    let graph = Rectangle::new(
        Point::new(
            bounds.top_left.x,
            bounds.top_left.y + bounds.size.height as i32 - graph_height as i32,
        ),
        Size::new(bounds.size.width, graph_height),
    );
    let value_size = if numeric {
        lanes[0].size
    } else {
        crate::widgets::gauge::value_lane(lanes[0]).map_or(Size::zero(), |r| r.size)
    };
    let fits = fits_height
        && crate::widgets::value_text::fit_value(text[0], value_size, 3).is_some()
        && (1..6).all(|i| text[i].len() as u32 * 4 <= lanes[i].size.width);
    PortraitContentLayout {
        text: lanes,
        graph,
        legend,
        fits,
    }
}

pub(crate) fn compact_text<D: DrawTarget<Color = Rgb565>>(
    display: &mut D,
    text: &str,
    lane: Rectangle,
    color: Rgb565,
) {
    if text.is_empty() || lane.size.height == 0 {
        return;
    }
    let text = if text.len() as u32 * 4 <= lane.size.width {
        text
    } else {
        "FIT!"
    };
    Text::with_text_style(
        text,
        Point::new(lane.center().x, lane.top_left.y + 5),
        MonoTextStyle::new(&embedded_graphics::mono_font::ascii::FONT_4X6, color),
        CENTERED,
    )
    .draw(display)
    .ok();
}

#[allow(clippy::too_many_arguments)]
fn draw_portrait_widget<D: DrawTarget<Color = Rgb565>>(
    display: &mut D,
    scene: &DashboardScene<'_>,
    slot: usize,
    layout: &Layout<'_>,
    widget: &Widget,
    palette: WidgetColors,
    decorations: &dashboard_config::AdvancedDisplay,
    interior: Rectangle,
    indication: WidgetWarning,
) {
    let valid = scene.quality(slot) == Quality::Valid;
    let state = scene.measurements.sensor(SensorId::from_channel(widget.channel));
    let foreground = |color| {
        panel_color(if indication.blink {
            palette.warning.foreground
        } else {
            color
        })
    };
    let mut text = scene.configured_text(slot).expect("validated visible widget");
    if !valid {
        text[5].clear();
        text[5].push_str(scene.quality(slot).label()).ok();
    }
    let warning_rows = if indication.marker == WarningMarker::None {
        0
    } else {
        1 + u32::from(indication.unavailable)
    };
    let references =
        u32::from(widget.capabilities().markers && decorations.reference_mode != dashboard_config::ReferenceMode::Off);
    let bands = if widget.capabilities().band_labels {
        decorations.bands().len() as u32
    } else {
        0
    };
    let lanes = portrait_content_layout(
        interior,
        core::array::from_fn(|i| text[i].as_str()),
        warning_rows,
        valid && widget.shows_graph(),
        widget.gauge.kind.is_numeric(),
        references + bands,
        widget.shows_trend(),
    );
    compact_text(
        display,
        &widget.label,
        Rectangle::new(interior.top_left, Size::new(interior.size.width, 7)),
        foreground(palette.label),
    );
    if warning_rows > 0 {
        let lane = Rectangle::new(
            interior.top_left + Point::new(0, 7),
            Size::new(interior.size.width, warning_rows * 7),
        );
        display.fill_solid(&lane, panel_color(palette.warning.background)).ok();
        compact_text(
            display,
            indication.marker.label(),
            lane,
            panel_color(palette.warning.foreground),
        );
        if indication.unavailable {
            compact_text(
                display,
                if scene.quality(slot) == Quality::Stale {
                    "DATA STALE"
                } else {
                    "UNAVAILABLE"
                },
                Rectangle::new(lane.top_left + Point::new(0, 7), Size::new(lane.size.width, 7)),
                panel_color(palette.warning.foreground),
            );
        }
    }
    let primary = if crate::widgets::value_text::fit_value(&text[0], lanes.text[0].size, 3).is_some() {
        text[0].as_str()
    } else {
        "FIT!"
    };
    if widget.gauge.kind.is_numeric() {
        draw_fitted_value(display, primary, lanes.text[0], 3, foreground(palette.value));
    } else {
        crate::widgets::gauge::draw_decorated(
            display,
            widget.gauge.kind,
            lanes.text[0],
            scene.gauge_reading(slot).expect("validated gauge"),
            primary,
            false,
            crate::widgets::gauge::Paint {
                colors: (
                    foreground(palette.value),
                    foreground(palette.gauge),
                    panel_color(palette.track),
                    foreground(palette.ticks),
                ),
                decorations: Some(crate::widgets::gauge::Decorations {
                    settings: decorations,
                    unit: Unit::resolve(widget.advanced.readout, scene.boost_psi),
                    stoich: layout.configuration().stoich(),
                    peak_color: foreground(palette.peak),
                }),
            },
        );
    }
    if valid
        && widget.shows_trend()
        && let Some(rising) = state.get_trend_at(scene.now_ms)
    {
        draw_trend_arrow(
            display,
            interior.top_left.x + interior.size.width as i32 - 6,
            lanes.text[0].top_left.y + 4,
            rising,
            foreground(palette.trend),
        );
    }
    for i in 1..6 {
        compact_text(
            display,
            &text[i],
            lanes.text[i],
            foreground(match i {
                2 => palette.average,
                3 | 4 if valid && state.is_new_peak_at(scene.now_ms) => palette.peak,
                3 => palette.minimum,
                4 => palette.maximum,
                _ => palette.secondary,
            }),
        );
    }
    let mut legend = lanes.legend;
    legend.size.height = 7;
    if references > 0 {
        compact_text(
            display,
            if decorations.reference_mode == dashboard_config::ReferenceMode::Zero {
                "REF ZERO"
            } else {
                "REF MANUAL"
            },
            legend,
            foreground(palette.label),
        );
        legend.top_left.y += 7;
    }
    if bands > 0 {
        for band in decorations.bands() {
            compact_text(display, band.label(), legend, panel_color(band.color));
            legend.top_left.y += 7;
        }
    }
    if lanes.graph.size.height > 0 {
        let history = state.history();
        let bounds = match widget.display.graph_range {
            GraphRange::Auto => history.bounds(),
            GraphRange::Fixed { min, max } => Some((min, max)),
        };
        draw_history_graph(
            display,
            lanes.graph.top_left.x,
            lanes.graph.top_left.y,
            lanes.graph.size.width,
            lanes.graph.size.height,
            history,
            bounds,
            |_| foreground(palette.graph),
        );
    }
}
