//! Shared display conversion and visible-cell presentation. Acquisition stays canonical.
use dashboard_config::{Readout, Secondary};

/// A resolved primary/secondary unit. FollowBoost and paired lambda are resolved
/// once at the formatting boundary, never in acquisition or warning evaluation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unit {
    Celsius,
    Fahrenheit,
    Bar,
    Psi,
    Kpa,
    Volts,
    Lambda,
    Afr,
}

impl Unit {
    pub const fn resolve(
        readout: Readout,
        boost_psi: bool,
    ) -> Self {
        match readout {
            Readout::Celsius => Self::Celsius,
            Readout::Fahrenheit => Self::Fahrenheit,
            Readout::FollowBoost => {
                if boost_psi {
                    Self::Psi
                } else {
                    Self::Bar
                }
            }
            Readout::Bar => Self::Bar,
            Readout::Psi => Self::Psi,
            Readout::Kpa => Self::Kpa,
            Readout::Volts => Self::Volts,
            Readout::Lambda | Readout::LambdaAndAfr => Self::Lambda,
            Readout::Afr => Self::Afr,
        }
    }

    pub fn secondary(
        secondary: Secondary,
        primary: Self,
    ) -> Option<Self> {
        let unit = match secondary {
            Secondary::None => return None,
            Secondary::Bar => Self::Bar,
            Secondary::Psi => Self::Psi,
            Secondary::Kpa => Self::Kpa,
        };
        (unit != primary).then_some(unit)
    }

    pub fn difference(
        self,
        canonical: f32,
        stoich: f32,
    ) -> f32 {
        canonical
            * match self {
                Self::Fahrenheit => 1.8,
                Self::Psi => (100000.0_f64 / 6894.757293168_f64) as f32,
                Self::Kpa => 100.0,
                Self::Afr => stoich,
                _ => 1.0,
            }
    }

    pub fn value(
        self,
        canonical: f32,
        stoich: f32,
    ) -> f32 {
        self.difference(canonical, stoich) + if self == Self::Fahrenheit { 32.0 } else { 0.0 }
    }

    pub const fn suffix(self) -> &'static str {
        match self {
            Self::Celsius => "C",
            Self::Fahrenheit => "F",
            Self::Bar => " BAR",
            Self::Psi => " PSI",
            Self::Kpa => " kPa",
            Self::Volts => "V",
            Self::Lambda => "",
            Self::Afr => "",
        }
    }
}

/// Fail closed rather than displaying a truncated number or partial assumption.
pub fn number(
    out: &mut heapless::String<16>,
    prefix: &str,
    value: f32,
    decimals: usize,
    suffix: &str,
) {
    use core::fmt::Write;
    out.clear();
    if !value.is_finite() || write!(out, "{prefix}{value:.decimals$}{suffix}", decimals = decimals.min(3)).is_err() {
        out.clear();
        let _ = out.push_str("--");
    }
}

pub fn format_value(
    out: &mut heapless::String<16>,
    canonical: f32,
    unit: Unit,
    stoich: f32,
    decimals: usize,
    prefix: &str,
    suffix: &str,
) {
    number(out, prefix, unit.value(canonical, stoich), decimals, suffix);
}

use dashboard_config::{ConfigCacheIdentity, ValidatedConfig};

use crate::acquisition::DashboardSnapshot;
use crate::alarms::RuleIndex;
use crate::configured_scene::Layout;
use crate::samples::{SensorId, Source};
use crate::warning_state::WarningEligibility;

/// Absolute alpha error is tested over [0,16] against host f64 exp. Use explicit
/// UFCS on every platform, so host inherent exp cannot select another algorithm.
pub fn smoothing_alpha(
    dt_ms: u64,
    tau_ms: u16,
) -> f32 {
    if tau_ms == 0 || dt_ms >= u64::from(tau_ms) * 16 {
        return 1.0;
    }
    if dt_ms == 0 {
        return 0.0;
    }
    // The direct micromath approximation exceeds our 0.001 alpha tolerance
    // near -ln(2). Quarter the bounded argument and square twice; this retains
    // the same locked implementation on ARM/Wasm/host and bounds the measured
    // full-domain absolute error below 0.0006 without a dependency change.
    let quarter = micromath::F32Ext::exp(-(dt_ms as f32 / f32::from(tau_ms)) * 0.25);
    let half = quarter * quarter;
    (1.0 - half * half).clamp(0.0, 1.0)
}

#[repr(C)]
#[derive(Clone, Copy)]
struct SmoothState {
    last_ms: u64,
    filtered: f32,
    widget_id: u16,
    valid: bool,
    reserved: u8,
}
impl SmoothState {
    const EMPTY: Self = Self {
        last_ms: 0,
        filtered: 0.0,
        widget_id: 0,
        valid: false,
        reserved: 0,
    };

    fn update(
        &mut self,
        id: u16,
        target: Option<f32>,
        now: u64,
        tau: u16,
    ) {
        let Some(target) = target else {
            self.valid = false;
            return;
        };
        if !self.valid || self.widget_id != id || now < self.last_ms || tau == 0 {
            self.filtered = target;
        } else if now != self.last_ms {
            let alpha = smoothing_alpha(now - self.last_ms, tau);
            self.filtered = if alpha == 1.0 {
                target
            } else {
                (self.filtered + alpha * (target - self.filtered))
                    .clamp(self.filtered.min(target), self.filtered.max(target))
            };
        }
        self.widget_id = id;
        self.last_ms = now;
        self.valid = true;
    }
}

/// Eight visible cells only. No history, snapshot or configuration copies.
#[derive(Clone)]
pub struct PresentationState {
    cells: [SmoothState; 8],
    sources: [Source; 8],
    config: Option<ConfigCacheIdentity>,
    run: Option<u64>,
    reset: u64,
    view: Option<u8>,
    now_ms: u64,
    boost_psi: bool,
    appearance: bool,
}
impl Default for PresentationState {
    fn default() -> Self { Self::new() }
}
impl PresentationState {
    pub const fn new() -> Self {
        Self {
            cells: [SmoothState::EMPTY; 8],
            sources: [Source::Unknown; 8],
            config: None,
            run: None,
            reset: 0,
            view: None,
            now_ms: 0,
            boost_psi: false,
            appearance: false,
        }
    }

    /// Explicit hide/mode boundary. Returning to the dashboard snaps.
    pub fn reset(&mut self) { *self = Self::new(); }

    pub fn prepare<'a>(
        &'a mut self,
        config: &'a ValidatedConfig,
        snapshot: &'a DashboardSnapshot,
        view: u8,
        now_ms: u64,
        boost_psi: bool,
    ) -> DisplayReadings<'a> {
        self.update(config, snapshot, view, now_ms, boost_psi, false);
        self.readings(config, snapshot)
    }

    /// Appearance reconstructs its synthetic acquisition each frame. Its caller
    /// owns continuity and resets on mode/config/sample-program changes. This
    /// never rewrites a snapshot's graph/publication identity or retains history.
    pub fn prepare_appearance<'a>(
        &'a mut self,
        config: &'a ValidatedConfig,
        snapshot: &'a DashboardSnapshot,
        view: u8,
        now_ms: u64,
        boost_psi: bool,
    ) -> DisplayReadings<'a> {
        self.update(config, snapshot, view, now_ms, boost_psi, true);
        self.readings(config, snapshot)
    }

    fn update(
        &mut self,
        config: &ValidatedConfig,
        snapshot: &DashboardSnapshot,
        view: u8,
        now_ms: u64,
        boost_psi: bool,
        appearance: bool,
    ) {
        let run = snapshot.warning_run_id();
        let sources = snapshot.metadata.map(|meta| meta.source);
        if self.config != config.cache_identity()
            || config.cache_identity().is_none()
            || self.view != Some(view)
            || self.reset != snapshot.reset_generation
            || self.appearance != appearance
            || now_ms < self.now_ms
            || run.is_none()
            || (!appearance && self.run != run)
        {
            self.cells = [SmoothState::EMPTY; 8];
        }
        self.config = config.cache_identity();
        self.run = run;
        self.reset = snapshot.reset_generation;
        self.view = Some(view);
        self.now_ms = now_ms;
        self.boost_psi = boost_psi;
        self.appearance = appearance;
        let Some(layout) = Layout::new(config, view, false) else {
            self.cells = [SmoothState::EMPTY; 8];
            self.sources = sources;
            return;
        };
        for (cell, filter) in self.cells.iter_mut().enumerate() {
            let Some(widget) = layout.widget(cell) else {
                *filter = SmoothState::EMPTY;
                continue;
            };
            let id = SensorId::from_channel(widget.channel);
            if sources[id as usize] != self.sources[id as usize] {
                filter.valid = false;
            }
            filter.update(
                widget.id,
                snapshot.value_at(id, now_ms),
                now_ms,
                crate::ui::animations::MotionPolicy::new(config.config().reduced_motion)
                    .smoothing_ms(widget.advanced.smoothing_ms),
            );
        }
        self.sources = sources;
    }

    /// Immutable projection of the already prepared epoch. Repeated key/draw
    /// access cannot advance time, update targets, or copy acquisition storage.
    fn readings<'a>(
        &'a self,
        config: &'a ValidatedConfig,
        snapshot: &'a DashboardSnapshot,
    ) -> DisplayReadings<'a> {
        DisplayReadings {
            state: self,
            layout: self.view.and_then(|view| Layout::new(config, view, false)),
            snapshot,
            #[cfg(feature = "alloc")]
            rule_state: true,
        }
    }
}

/// Borrowed view over eight prepared filters. A PresentedWidget is short-lived
/// synchronous scratch; no second eight-record array survives an async yield.
pub struct DisplayReadings<'a> {
    state: &'a PresentationState,
    layout: Option<Layout<'a>>,
    snapshot: &'a DashboardSnapshot,
    #[cfg(feature = "alloc")]
    rule_state: bool,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PresentedWidget {
    pub value: Option<f32>,
    pub peak: Option<f32>,
    pub warning: WarningEligibility,
    pub unit: Unit,
    pub secondary: Option<Unit>,
}
impl DisplayReadings<'_> {
    /// Read-only candidate drawing may use qualified channel observations,
    /// but its widget indices cannot address an accepted owner's rules/peaks.
    /// This changes only the ephemeral readings view, never the snapshot or
    /// presentation owner. Available for alloc-enabled candidate inspection;
    /// normal target readings retain their original layout and rule path.
    #[cfg(feature = "alloc")]
    pub fn without_rule_state(mut self) -> Self {
        self.rule_state = false;
        self
    }

    pub fn widget(
        &self,
        cell: usize,
    ) -> Option<PresentedWidget> {
        let layout = self.layout.as_ref()?;
        let widget = layout.widget(cell)?;
        let filter = self.state.cells.get(cell)?;
        #[cfg(not(feature = "alloc"))]
        let index = RuleIndex::new(layout.rule_index(cell)?)?;
        #[cfg(feature = "alloc")]
        let (peak, warning) = if self.rule_state {
            let index = RuleIndex::new(layout.rule_index(cell)?)?;
            (
                self.snapshot.peak_at(index, self.state.now_ms),
                self.snapshot.warning_eligibility(index, self.state.now_ms),
            )
        } else {
            (
                None,
                WarningEligibility {
                    status: crate::alarms::Status::Disabled,
                    latched: false,
                    cleared: false,
                    acknowledged: false,
                    snoozed: false,
                },
            )
        };
        let unit = Unit::resolve(widget.advanced.readout, self.state.boost_psi);
        Some(PresentedWidget {
            value: filter.valid.then_some(filter.filtered),
            #[cfg(not(feature = "alloc"))]
            peak: self.snapshot.peak_at(index, self.state.now_ms),
            #[cfg(feature = "alloc")]
            peak,
            #[cfg(not(feature = "alloc"))]
            warning: self.snapshot.warning_eligibility(index, self.state.now_ms),
            #[cfg(feature = "alloc")]
            warning,
            unit,
            secondary: if widget.capabilities().secondary {
                Unit::secondary(widget.advanced.secondary, unit)
            } else {
                None
            },
        })
    }
}

#[cfg(all(test, not(feature = "alloc")))]
mod layout_tests {
    use super::*;

    #[test]
    fn normal_readings_keep_the_original_borrowed_layout() {
        // The normal target path has exactly the original three borrowed fields.
        #[allow(dead_code)]
        struct OriginalReadings<'a> {
            state: &'a PresentationState,
            layout: Option<Layout<'a>>,
            snapshot: &'a DashboardSnapshot,
        }
        assert_eq!(
            core::mem::size_of::<DisplayReadings<'_>>(),
            core::mem::size_of::<OriginalReadings<'_>>()
        );
        assert_eq!(
            core::mem::align_of::<DisplayReadings<'_>>(),
            core::mem::align_of::<OriginalReadings<'_>>()
        );
    }
}
