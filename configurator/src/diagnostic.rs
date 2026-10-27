//! Browser-only inspection of rejected drafts, never a configuration proof.
use core::ops::ControlFlow;

use dashboard_config::*;

use crate::editor::{EditorError, RawField, RawInput, WidgetField};

/// Shared retention cap for semantic and raw-input diagnostics; traversal still counts omitted issues.
const ISSUE_LIMIT: usize = 128;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Why a retained raw control could not be replayed into the candidate.
pub enum RawDiagnosticCategory {
    /// Text does not parse as the control's expected scalar/color/address.
    Parse,
    /// Text or bounded collection capacity was exceeded.
    Capacity,
    /// Owner/variant/index is missing, ambiguous or inapplicable.
    Unavailable,
    /// Parsed input failed immediate shared semantic validation.
    SemanticInput,
}

#[derive(Clone, Debug, PartialEq)]
/// Repair location for one failed raw replay; original text stays in the candidate owner.
pub struct RawDiagnostic {
    /// Index into the exact draft's retained raw vector, not stable across revisions.
    pub raw_index: usize,
    /// Typed control identity used to recheck an inspection capability.
    pub field: RawField,
    /// User-facing failure grouping.
    pub category: RawDiagnosticCategory,
    /// Original typed replay error.
    pub error: EditorError,
}

/// Retention is shared across categories; traversal continues after the cap.
/// No raw text is copied into the report. The candidate owns the repair input.
#[derive(Debug, Default)]
pub struct ValidationReport {
    /// Shared semantic issues in validator traversal order within the shared retention cap.
    pub issues: Vec<ValidationIssue>,
    /// Raw replay errors retained before semantic traversal, sharing the same cap.
    pub raw_errors: Vec<RawDiagnostic>,
    /// Further issues encountered after the combined 128-entry cap.
    pub omitted: usize,
}

impl ValidationReport {
    /// Returns whether one more raw or semantic diagnostic fits the shared cap.
    fn has_capacity(&self) -> bool { self.issues.len() + self.raw_errors.len() < ISSUE_LIMIT }

    /// Traverses all shared validation issues in `config`, retaining bounded entries and counting the remainder.
    /// This inspection visitor does not create a ValidatedConfig proof.
    fn collect(
        &mut self,
        config: &Config,
    ) {
        let _ = visit_validation(config, |issue| {
            if self.has_capacity() {
                self.issues.push(issue);
            } else {
                self.omitted += 1;
            }
            ControlFlow::Continue(())
        });
    }

    /// Returns the first retained semantic issue as a codec error, or `None`; raw errors are not folded into it.
    pub fn first_config_error(&self) -> Option<ConfigError> {
        self.issues.first().map(|issue| ConfigError::Validation(issue.error))
    }

    /// Returns an inspection label for `issue` using indexed owners in `config`, so duplicate IDs do not redirect
    /// labels. Missing indices omit owner detail; returned user labels must be rendered as text, never markup.
    pub fn describe(
        &self,
        config: &Config,
        issue: &ValidationIssue,
    ) -> String {
        let label = if let Some(widget) = issue.location.widget_index().and_then(|i| config.widgets.get(i)) {
            format!(
                " (view {}, widget {}: {:?})",
                widget.view,
                widget.id,
                widget.label.as_str()
            )
        } else if let Some(view) = issue.location.view_index().and_then(|i| config.views.get(i)) {
            format!(" (view {}: {:?})", view.id, view.name.as_str())
        } else if let Some(channel) = issue.location.channel_index().and_then(|i| Channel::ALL.get(i)) {
            format!(" ({})", channel.label())
        } else {
            String::new()
        };
        format!("{}{label}: {:?}", issue.location, issue.error.code)
    }
}

/// Returns all retained/countable semantic issues for candidate `config`, without accepting it or copying raw text.
pub fn collect_candidate(config: &Config) -> ValidationReport {
    let mut report = ValidationReport::default();
    report.collect(config);
    report
}

/// Inspection-only rejected visual configuration and retained raw controls; never export/build authority.
pub struct Draft {
    /// Candidate configuration after best-effort raw replay; may remain invalid.
    pub config: Config,
    /// Original raw repair text in replay order.
    pub raw: Vec<RawInput>,
    /// Reducer's retained rejection cause, used when no finer descriptive differences are available.
    pub error: EditorError,
}

impl Draft {
    /// Returns bounded raw/semantic diagnostics after replaying every raw field into a disposable clone.
    /// Replay continues after individual failures; successful unrelated values remain inspectable without acceptance.
    pub fn validation_report(&self) -> ValidationReport {
        let mut report = ValidationReport::default();
        let mut scratch = self.config.clone();
        for (raw_index, input) in self.raw.iter().enumerate() {
            if let Err(error) = crate::editor::apply_raw(&mut scratch, input) {
                let category = match error {
                    EditorError::Parse(_) => RawDiagnosticCategory::Parse,
                    EditorError::TextCapacity | EditorError::RawTooLong | EditorError::RawCapacity => {
                        RawDiagnosticCategory::Capacity
                    }
                    EditorError::Validation(_) => RawDiagnosticCategory::SemanticInput,
                    _ => RawDiagnosticCategory::Unavailable,
                };
                if report.has_capacity() {
                    report.raw_errors.push(RawDiagnostic {
                        raw_index,
                        field: input.field,
                        category,
                        error,
                    });
                } else {
                    report.omitted += 1;
                }
            }
        }
        report.collect(&scratch);
        report
    }

    /// Returns retained raw label for uniquely identified `widget`, otherwise its stored label.
    /// Missing/duplicate candidate identity prevents borrowing another widget's raw text.
    pub fn label<'a>(
        &'a self,
        widget: &'a Widget,
    ) -> &'a str {
        if self
            .config
            .widgets
            .iter()
            .filter(|candidate| candidate.id == widget.id)
            .count()
            != 1
        {
            return &widget.label;
        }
        self.raw
            .iter()
            .find_map(|input| {
                (input.field == RawField::Widget(widget.id, WidgetField::Label)).then_some(input.text.as_str())
            })
            .unwrap_or(&widget.label)
    }

    /// Returns raw/stored name for `view`, or explicit ambiguous/unknown text; duplicate IDs never select arbitrarily.
    pub fn name(
        &self,
        view: u8,
    ) -> &str {
        if self
            .config
            .views
            .iter()
            .filter(|candidate| candidate.id == view)
            .count()
            > 1
        {
            return "AMBIGUOUS VIEW";
        }
        self.raw
            .iter()
            .find_map(|input| (input.field == RawField::ViewName(view)).then_some(input.text.as_str()))
            .or_else(|| self.config.views.iter().find(|v| v.id == view).map(|v| v.name.as_str()))
            .unwrap_or("UNKNOWN VIEW")
    }

    /// Returns candidate widget IDs in `view` affected by differences from `accepted`, relevant raw inputs or
    /// collisions. Both overlap participants are included; the list is diagnostic and duplicate candidate IDs are
    /// not unique authority.
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
                    || self.config.view_orientation(w.view) != accepted.config().view_orientation(w.view)
                    || (w.channel == WidgetChannel::Lambda && self.config.fuel != accepted.config().fuel)
                    || w.channel.scalar().is_some_and(|channel| {
                        self.config.sources[channel.index()] != accepted.config().sources[channel.index()]
                    })
                    || self.config.warnings != accepted.config().warnings
                    || w.channel.scalar().is_some_and(|channel| {
                        self.config.history_seconds[channel.index()]
                            != accepted.config().history_seconds[channel.index()]
                    })
                    || self.config.reduced_motion != accepted.config().reduced_motion
                    || self.config.views.iter().find(|v| v.id == w.view)
                        != accepted.config().views.iter().find(|v| v.id == w.view)
                    || self.raw.iter().any(|input| {
                        match input.field {
                            RawField::Widget(id, _) => id == w.id,
                            RawField::Fuel(_) => w.channel == WidgetChannel::Lambda,
                            RawField::Source(channel, _) => Some(channel) == w.channel.scalar(),
                            RawField::ViewName(id) => id == w.view,
                            RawField::WarningPageMilliseconds | RawField::WarningColor { .. } => true,
                            RawField::HistorySeconds(channel) => Some(channel) == w.channel.scalar(),
                            RawField::ControlHoldMilliseconds | RawField::Led { .. } | RawField::Ble(_) => false,
                        }
                    })
            })
            .map(|w| w.id)
            .collect();
        // Mark both participants of a collision, even if only one was moved.
        for (i, a) in self.config.widgets.iter().enumerate().filter(|(_, w)| w.view == view) {
            for b in self.config.widgets.iter().skip(i + 1).filter(|w| w.view == view) {
                if a.placement
                    .occupied_in(
                        self.config
                            .view_orientation(view)
                            .unwrap_or(self.config.orientation)
                            .geometry(),
                    )
                    .zip(
                        b.placement.occupied_in(
                            self.config
                                .view_orientation(view)
                                .unwrap_or(self.config.orientation)
                                .geometry(),
                        ),
                    )
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

    /// Returns human-inspectable raw/candidate differences from `accepted`, plus explicit display-fallback notes.
    /// Text may contain retained user input and is for escaped local UI only; it is not suitable for helper telemetry.
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
                    if widget
                        .placement
                        .rect_in(
                            self.config
                                .view_orientation(widget.view)
                                .unwrap_or(self.config.orientation)
                                .geometry()
                        )
                        .is_none()
                    {
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
            if self.config.history_seconds[channel.index()] != accepted.config().history_seconds[channel.index()] {
                lines.push(format!(
                    "{} shared history: {} seconds; diagnostic uses the last accepted {} seconds.",
                    channel.label(),
                    self.config.history_seconds[channel.index()],
                    accepted.config().history_seconds[channel.index()]
                ));
            }
            if self.config.sources[channel.index()] != accepted.config().sources[channel.index()] {
                lines.push(format!(
                    "{} source: {:?}",
                    channel.label(),
                    self.config.sources[channel.index()]
                ));
            }
        }
        if self.config.ble != accepted.config().ble {
            lines.push(format!(
                "BLE settings: {:?}; physical connection status is unavailable in this browser.",
                self.config.ble
            ));
        }
        if self.config.led != accepted.config().led {
            lines.push(format!(
                "RGB indicator settings: {:?}; live timing uses the accepted settings until repair.",
                self.config.led
            ));
        }
        if self.config.reduced_motion != accepted.config().reduced_motion {
            lines.push(format!(
                "Reduced motion: {}; diagnostic uses the accepted global motion setting.",
                self.config.reduced_motion
            ));
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
/// Returns a safe one-widget rendering fixture from `accepted` and candidate `widget`, or `None` for missing geometry
/// or projection/admission failure. The fixture is never used as proof of the original rejected candidate.
#[cfg(test)]
pub(crate) fn isolated_widget(
    accepted: &ValidatedConfig,
    widget: &Widget,
) -> Option<ValidatedConfig> {
    isolated_widget_in(
        accepted,
        widget,
        accepted.config().view_orientation(widget.view)?.geometry(),
    )
}

/// Returns a validated one-widget display projection of `widget` in `geometry`, borrowing accepted global
/// acquisition settings from `accepted`. Clamps placement, substitutes invalid numeric/contrast settings and disables
/// warnings; rejected text/colors are drawn separately by the diagnostic renderer. Returns `None` if construction
/// fails.
pub(crate) fn isolated_widget_in(
    accepted: &ValidatedConfig,
    widget: &Widget,
    geometry: Geometry,
) -> Option<ValidatedConfig> {
    let mut config = accepted.config().clone();
    // Use accepted acquisition/style fallbacks without requiring the rejected
    // candidate's view identity to exist in the accepted document.
    let mut view = config.views.first()?.clone();
    view.id = widget.view;
    view.orientation = Some(match geometry {
        Geometry::Landscape => Orientation::Landscape,
        Geometry::Portrait => Orientation::Portrait,
    });
    config.views.clear();
    config.views.push(view).ok()?;
    config.boot_view = widget.view;
    config.widgets.clear();
    let mut safe = widget.clone();
    let geometry = config.view_orientation(widget.view)?.geometry();
    safe.placement.column = safe
        .placement
        .column
        .min(geometry.columns() - safe.placement.span.columns());
    safe.placement.row = safe.placement.row.min(geometry.rows() - safe.placement.span.rows());
    safe.label = "INVALID".try_into().ok()?;
    safe.display.decimals = safe.display.decimals.min(3);
    let Some(channel) = safe.channel.scalar() else {
        // This is a disposable diagnostic projection; the full rejected widget stays in the editor.
        let mut projection = default_ignition_widget(safe.id, safe.view, safe.placement);
        projection.label = safe.label;
        projection.display.decimals = safe.display.decimals;
        projection.warning.enabled = false;
        if safe.advanced.readout.compatible_widget(safe.channel) {
            projection.advanced.readout = safe.advanced.readout;
        }
        config.widgets.push(projection).ok()?;
        return validate(config).ok();
    };
    safe.display.afr &= safe.channel == WidgetChannel::Lambda;
    if !safe.gauge.valid(channel) {
        let kind = safe.gauge.kind;
        safe.gauge = Gauge::for_channel(channel);
        safe.gauge.kind = kind;
    }
    let (low, high) = safe.channel.domain();
    if let GraphRange::Fixed { min, max } = safe.display.graph_range
        && (!min.is_finite() || !max.is_finite() || min < low || max > high || min >= max)
    {
        safe.display.graph_range = GraphRange::Auto;
    }
    safe.colors = WidgetColors::default();
    let defaults = AdvancedDisplay::for_channel(channel, safe.display.afr);
    if !safe.advanced.readout.compatible_widget(safe.channel) {
        safe.advanced.readout = defaults.readout;
    }
    safe.display.afr = safe.advanced.readout.includes_afr();
    if safe.channel != WidgetChannel::Boost {
        safe.advanced.secondary = Secondary::None;
    }
    if safe.advanced.smoothing_ms != 0 && !(50..=5000).contains(&safe.advanced.smoothing_ms) {
        safe.advanced.smoothing_ms = 0;
    }
    if !(100..=60000).contains(&safe.advanced.peak_hold_ms) {
        safe.advanced.peak_hold_ms = 750;
    }
    if !safe.advanced.reference.is_finite()
        || (safe.advanced.reference_mode == ReferenceMode::Manual && !(low..=high).contains(&safe.advanced.reference))
    {
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
    safe.warning = default_widget_for_channel(safe.channel, safe.id, safe.view, safe.placement).warning;
    safe.warning.enabled = false;
    config.widgets.push(safe).ok()?;
    validate(config).ok()
}

/// Returns explicit advanced-setting fallback descriptions for invalid `widget` readout, smoothing, markers or bands.
/// Does not alter the widget; finite inactive references are retained and therefore receive no fallback note.
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
    if widget.channel == WidgetChannel::IgnitionRetard {
        add(
            "group settings",
            "a bounded four-cylinder projection with scalar features disabled",
        );
    }
    if !a.readout.compatible_widget(widget.channel) {
        add("readout", "the channel's default unit");
    }
    if widget.channel != WidgetChannel::Boost && a.secondary != Secondary::None {
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

#[cfg(test)]
mod tests {
    //! Diagnostic projection preserves inactive finite values while isolating unsafe numeric rendering inputs.
    use super::*;

    #[test]
    /// Only Manual enforces channel reference bounds, while every mode projects nonfinite references to Off/zero.
    fn review_reference_projection_bounds_only_manual_and_always_bounds_nonfinite() {
        let accepted = validate(default_config()).unwrap();
        let mut widget = accepted.config().widgets[0].clone();
        for mode in [ReferenceMode::Off, ReferenceMode::Zero] {
            widget.advanced.reference_mode = mode;
            widget.advanced.reference = 999.;
            let projected = isolated_widget(&accepted, &widget).unwrap();
            assert_eq!(projected.config().widgets[0].advanced.reference_mode, mode);
            assert_eq!(projected.config().widgets[0].advanced.reference, 999.);
            assert!(
                !projection_notes(&widget)
                    .iter()
                    .any(|line| line.contains(" reference:"))
            );
        }
        widget.advanced.reference_mode = ReferenceMode::Manual;
        let projected = isolated_widget(&accepted, &widget).unwrap();
        assert_eq!(
            projected.config().widgets[0].advanced.reference_mode,
            ReferenceMode::Off
        );
        assert!(
            projection_notes(&widget)
                .iter()
                .any(|line| line.contains(" reference:"))
        );
        for mode in [ReferenceMode::Off, ReferenceMode::Zero, ReferenceMode::Manual] {
            for number in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
                widget.advanced.reference_mode = mode;
                widget.advanced.reference = number;
                let projected = isolated_widget(&accepted, &widget).unwrap();
                assert_eq!(
                    projected.config().widgets[0].advanced.reference_mode,
                    ReferenceMode::Off
                );
                assert_eq!(projected.config().widgets[0].advanced.reference, 0.);
                assert!(
                    projection_notes(&widget)
                        .iter()
                        .any(|line| line.contains(" reference:"))
                );
            }
        }
    }
}
