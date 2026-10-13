//! Browser-only inspection of rejected drafts, never a configuration proof.
use dashboard_config::*;

use crate::editor::{EditorError, RawField, RawInput, WidgetField};

pub struct Draft {
    pub config: Config,
    pub raw: Vec<RawInput>,
    pub error: EditorError,
}

impl Draft {
    pub fn label<'a>(
        &'a self,
        widget: &'a Widget,
    ) -> &'a str {
        self.raw
            .iter()
            .find_map(|input| {
                (input.field == RawField::Widget(widget.id, WidgetField::Label)).then_some(input.text.as_str())
            })
            .unwrap_or(&widget.label)
    }

    pub fn name(
        &self,
        view: u8,
    ) -> &str {
        self.raw
            .iter()
            .find_map(|input| (input.field == RawField::ViewName(view)).then_some(input.text.as_str()))
            .or_else(|| self.config.views.iter().find(|v| v.id == view).map(|v| v.name.as_str()))
            .unwrap_or("UNKNOWN VIEW")
    }

    pub fn affected(
        &self,
        accepted: &ValidatedConfig,
        view: u8,
    ) -> Vec<u16> {
        let mut ids: Vec<_> = self
            .config
            .widgets
            .iter()
            .filter(|w| w.view == view)
            .filter(|w| {
                accepted.config().widgets.iter().find(|a| a.id == w.id) != Some(*w)
                    || (w.channel == Channel::Lambda && self.config.fuel != accepted.config().fuel)
                    || self.config.sources[w.channel.index()] != accepted.config().sources[w.channel.index()]
                    || self.config.warnings != accepted.config().warnings
                    || self.config.views.iter().find(|v| v.id == w.view)
                        != accepted.config().views.iter().find(|v| v.id == w.view)
                    || self.raw.iter().any(|input| {
                        match input.field {
                            RawField::Widget(id, _) => id == w.id,
                            RawField::Fuel(_) => w.channel == Channel::Lambda,
                            RawField::Source(channel, _) => channel == w.channel,
                            RawField::ViewName(id) => id == w.view,
                            RawField::WarningPageMilliseconds | RawField::WarningColor { .. } => true,
                            RawField::HistorySeconds(channel) => channel == w.channel,
                            RawField::ControlHoldMilliseconds | RawField::Led { .. } => false,
                        }
                    })
            })
            .map(|w| w.id)
            .collect();
        // Mark both participants of a collision, even if only one was moved.
        for (i, a) in self.config.widgets.iter().enumerate().filter(|(_, w)| w.view == view) {
            for b in self.config.widgets.iter().skip(i + 1).filter(|w| w.view == view) {
                if a.placement
                    .occupied()
                    .zip(b.placement.occupied())
                    .is_some_and(|(a, b)| a & b != 0)
                {
                    if !ids.contains(&a.id) {
                        ids.push(a.id);
                    }
                    if !ids.contains(&b.id) {
                        ids.push(b.id);
                    }
                }
            }
        }
        ids
    }

    pub fn descriptions(
        &self,
        accepted: &ValidatedConfig,
    ) -> Vec<String> {
        let mut lines: Vec<_> = self
            .raw
            .iter()
            .map(|input| format!("{:?}: {:?}", input.field, input.text))
            .collect();
        for widget in &self.config.widgets {
            let previous = accepted.config().widgets.iter().find(|w| w.id == widget.id);
            if previous.is_none_or(|w| w.placement != widget.placement || w.view != widget.view) {
                lines.push(format!(
                    "Widget {}: view {}, column {}, row {}, {}×{}{}",
                    widget.id,
                    widget.view,
                    u16::from(widget.placement.column) + 1,
                    u16::from(widget.placement.row) + 1,
                    widget.placement.span.columns(),
                    widget.placement.span.rows(),
                    if widget.placement.rect().is_none() {
                        " (outside screen; diagnostic position clamped to the grid)"
                    } else {
                        ""
                    }
                ));
            }
            if let Some(previous) = previous {
                if previous.label != widget.label {
                    lines.push(format!("Widget {} label: {:?}", widget.id, widget.label));
                }
                if previous.gauge != widget.gauge {
                    lines.push(format!("Widget {} gauge: {:?}", widget.id, widget.gauge));
                }
                for role in ColorRole::ALL {
                    if role.get(&previous.colors) != role.get(&widget.colors) {
                        lines.push(format!(
                            "Widget {} {}: {}",
                            widget.id,
                            role.label(),
                            role.get(&widget.colors).hex()
                        ));
                    }
                }
                if previous.display != widget.display {
                    lines.push(format!("Widget {} display: {:?}", widget.id, widget.display));
                }
                if previous.warning != widget.warning {
                    lines.push(format!("Widget {} warning: {:?}", widget.id, widget.warning));
                }
                if previous.advanced != widget.advanced {
                    lines.push(format!("Widget {} advanced display: {:?}", widget.id, widget.advanced));
                }
            }
            lines.extend(projection_notes(widget));
        }
        if self.config.fuel != accepted.config().fuel {
            lines.push(format!("Fuel and AFR: {:?}", self.config.fuel));
        }
        if self.config.warning_colors != accepted.config().warning_colors {
            lines.push(format!(
                "Combined warning colours: {} on {}",
                self.config.warning_colors.foreground.hex(),
                self.config.warning_colors.background.hex()
            ));
        }
        if self.config.warnings != accepted.config().warnings {
            lines.push(format!("Warning presentation: {:?}", self.config.warnings));
        }
        for channel in Channel::ALL {
            if self.config.sources[channel.index()] != accepted.config().sources[channel.index()] {
                lines.push(format!(
                    "{} source: {:?}",
                    channel.label(),
                    self.config.sources[channel.index()]
                ));
            }
        }
        for view in &self.config.views {
            if accepted.config().views.iter().find(|v| v.id == view.id) != Some(view) {
                lines.push(format!("View {} name: {:?}", view.id, view.name));
            }
        }
        if self.config.boot_view != accepted.config().boot_view {
            lines.push(format!("Boot view: {}", self.config.boot_view));
        }
        if lines.is_empty() {
            lines.push(format!("Rejected setting: {:?}", self.error));
        }
        lines
    }
}

/// Render each in-bounds placement independently so overlaps remain visible.
/// Unsafe numeric/style fields use explicit safe display fallbacks. All labels
/// (including over-capacity input) are drawn separately under clipping.
pub(crate) fn isolated_widget(
    accepted: &ValidatedConfig,
    widget: &Widget,
) -> Option<ValidatedConfig> {
    let mut config = accepted.config().clone();
    config.widgets.clear();
    let mut safe = widget.clone();
    safe.placement.column = safe.placement.column.min(4 - safe.placement.span.columns());
    safe.placement.row = safe.placement.row.min(2 - safe.placement.span.rows());
    safe.label = "INVALID".try_into().ok()?;
    safe.display.decimals = safe.display.decimals.min(3);
    safe.display.afr &= safe.channel == Channel::Lambda;
    if !safe.gauge.valid(safe.channel) {
        let kind = safe.gauge.kind;
        safe.gauge = Gauge::for_channel(safe.channel);
        safe.gauge.kind = kind;
    }
    let (low, high) = safe.channel.domain();
    if let GraphRange::Fixed { min, max } = safe.display.graph_range
        && (!min.is_finite() || !max.is_finite() || min < low || max > high || min >= max)
    {
        safe.display.graph_range = GraphRange::Auto;
    }
    safe.colors = WidgetColors::default();
    let defaults = AdvancedDisplay::for_channel(safe.channel, safe.display.afr);
    if !safe.advanced.readout.compatible(safe.channel) {
        safe.advanced.readout = defaults.readout;
    }
    safe.display.afr = safe.advanced.readout.includes_afr();
    if safe.channel != Channel::Boost {
        safe.advanced.secondary = Secondary::None;
    }
    if safe.advanced.smoothing_ms != 0 && !(50..=5000).contains(&safe.advanced.smoothing_ms) {
        safe.advanced.smoothing_ms = 0;
    }
    if !(100..=60000).contains(&safe.advanced.peak_hold_ms) {
        safe.advanced.peak_hold_ms = 750;
    }
    if !safe.advanced.reference.is_finite() || !(low..=high).contains(&safe.advanced.reference) {
        safe.advanced.reference_mode = ReferenceMode::Off;
        safe.advanced.reference = 0.;
    }
    // Preserve band count/colour inspection while projecting endpoints to
    // bounded ordered lanes. The raw RGB565 colours are reapplied only through
    // draw_diagnostic_widget, never used as a validation proof.
    let mut bands = safe.advanced.bands().to_vec();
    let valid_bounds = bands.iter().enumerate().all(|(i, b)| {
        b.min.is_finite()
            && b.max.is_finite()
            && b.min >= low
            && b.max <= high
            && b.min < b.max
            && (i == 0 || bands[i - 1].max <= b.min)
    });
    let count = bands.len();
    for (i, band) in bands.iter_mut().enumerate() {
        if !valid_bounds {
            let width = (high - low) / count as f32;
            band.min = low + i as f32 * width;
            band.max = if i + 1 == count {
                high
            } else {
                low + (i + 1) as f32 * width
            };
        }
        band.color = safe.colors.gauge;
    }
    safe.advanced.set_bands(&bands).ok()?;
    safe.warning = default_widget(safe.channel, safe.id, safe.view, safe.placement).warning;
    safe.warning.enabled = false;
    config.widgets.push(safe).ok()?;
    validate(config).ok()
}

fn projection_notes(widget: &Widget) -> Vec<String> {
    let mut notes = Vec::new();
    let a = widget.advanced;
    let (low, high) = widget.channel.domain();
    let mut add = |field: &str, fallback: &str| {
        notes.push(format!(
            "Widget {} {field}: diagnostic uses {fallback}; rejected input remains stored.",
            widget.id
        ))
    };
    if !a.readout.compatible(widget.channel) {
        add("readout", "the channel's default unit");
    }
    if widget.channel != Channel::Boost && a.secondary != Secondary::None {
        add("secondary unit", "no secondary");
    }
    if a.smoothing_ms != 0 && !(50..=5000).contains(&a.smoothing_ms) {
        add("smoothing", "Off");
    }
    if !(100..=60000).contains(&a.peak_hold_ms) {
        add("peak hold", "750 ms");
    }
    if !a.reference.is_finite() || (a.reference_mode == ReferenceMode::Manual && !(low..=high).contains(&a.reference)) {
        add("reference", "Off");
    }
    if a.bands().iter().enumerate().any(|(i, b)| {
        !b.min.is_finite()
            || !b.max.is_finite()
            || b.min < low
            || b.max > high
            || b.min >= b.max
            || (i > 0 && a.bands()[i - 1].max > b.min)
    }) {
        add(
            "band bounds/order",
            "equal ordered lanes in the canonical channel domain, retaining raw colours",
        );
    }
    notes
}
