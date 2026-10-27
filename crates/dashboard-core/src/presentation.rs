//! Shared display conversion and visible-cell presentation. Acquisition stays canonical.
use dashboard_config::{Readout, Secondary};

/// A resolved primary/secondary unit. FollowBoost and paired lambda are resolved
/// once at the formatting boundary, never in acquisition or warning evaluation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unit {
    /// Canonical degrees Celsius, unchanged.
    Celsius,
    /// Degrees Fahrenheit, canonical Celsius*1.8+32 for absolute values.
    Fahrenheit,
    /// Canonical gauge pressure in bar, unchanged.
    Bar,
    /// Pounds-force per square inch, converted from canonical bar.
    Psi,
    /// Kilopascals, canonical bar*100.
    Kpa,
    /// Canonical volts, unchanged.
    Volts,
    /// Dimensionless canonical lambda, unchanged.
    Lambda,
    /// Equivalent air/fuel mass ratio, lambda times the explicit stoichiometric assumption.
    Afr,
    /// Signed ignition correction in degrees; no temperature conversion.
    Degrees,
}

impl Unit {
    /// Returns concrete unit for `readout`; only FollowBoost responds to physical
    /// `boost_psi` (true PSI, false bar). LambdaAndAfr uses Lambda as its primary unit.
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
            Readout::IgnitionNumeric | Readout::IgnitionStatus | Readout::IgnitionStatusAndNumber => Self::Degrees,
        }
    }

    /// Returns the requested pressure `secondary` unless None or identical to
    /// resolved `primary`. Caller separately validates channel/widget capabilities.
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

    /// Returns scaled `canonical` difference in this unit, without Fahrenheit offset.
    /// `stoich` is the explicit air/fuel ratio used only for AFR; all other units ignore it.
    /// Inputs are not validated/clamped and nonfinite/overflow results propagate to formatting guards.
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

    /// Returns absolute `canonical` value in this unit, adding32 for Fahrenheit;
    /// `stoich` supplies only AFR's lambda multiplier. No finite/range admission occurs here.
    pub fn value(
        self,
        canonical: f32,
        stoich: f32,
    ) -> f32 {
        self.difference(canonical, stoich) + if self == Self::Fahrenheit { 32.0 } else { 0.0 }
    }

    /// Returns the fixed UTF-8 unit suffix; degrees uses U+00B0. Lambda/AFR use no
    /// suffix because labels/assumptions are separate. Callers must support the glyph.
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
            Self::Degrees => "°",
        }
    }
}

/// Fail closed rather than displaying a truncated number or partial assumption.
/// Replaces `out` with `prefix`, finite `value` at min(`decimals`,3) fractional
/// digits and `suffix`. Any nonfinite value or16-byte capacity failure replaces the
/// entire output with `--`; callers must not interpret that sentinel as numeric zero.
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

/// Replaces `out` with converted `canonical` using `unit`, explicit AFR `stoich`,
/// clamped `decimals`, and caller `prefix`/`suffix`. Conversion/format overflow or
/// nonfinite output becomes `--`; conversion never mutates canonical acquisition data.
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

/// Absolute alpha error is tested over `[0, 16]` against host f64 exp. Use explicit
/// UFCS on every platform, so host inherent exp cannot select another algorithm.
/// Returns alpha in0..=1 approximating1-exp(-`dt_ms`/`tau_ms`), both milliseconds.
/// Zero tau or elapsed>=16tau returns1 exactly; otherwise zero elapsed returns0.
/// Uses the locked micromath approximation with a bounded argument, not host libm.
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

/// One canonical-unit exponential display filter; raw statistics/warnings never use it.
#[repr(C)]
#[derive(Clone, Copy)]
struct SmoothState {
    /// Last filter preparation time in monotonic milliseconds.
    last_ms: u64,
    /// Current canonical display value, meaningful only when valid.
    filtered: f32,
    /// Widget identity occupying this visible slot; change forces a snap.
    widget_id: u16,
    /// Filter has an eligible current source sample.
    valid: bool,
    /// Explicit zero padding for stable compact representation.
    reserved: u8,
}
impl SmoothState {
    /// Invalid zeroed filter, snapping to the first eligible target.
    const EMPTY: Self = Self {
        last_ms: 0,
        filtered: 0.0,
        widget_id: 0,
        valid: false,
        reserved: 0,
    };

    /// Prepares widget `id` toward eligible canonical `target` at `now` milliseconds,
    /// with time constant `tau` milliseconds. None invalidates immediately; new/invalid
    /// identity, reversed time or zero tau snaps. Duplicate time preserves the prior
    /// value; positive elapsed approaches without overshoot. Caller supplies finite targets.
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
    /// Eight visible widget-slot filters, never64 per-configuration copies.
    cells: [SmoothState; 8],
    /// Last canonical per-channel source identities, used to invalidate changed sources.
    sources: [Source; 8],
    /// Exact configuration cache identity; None disables retained filtering across prepares.
    config: Option<ConfigCacheIdentity>,
    /// Acquisition warning-owner identity; None forces reset, appearance may ignore changes.
    run: Option<u64>,
    /// Last producer statistics-reset generation; change resets visible smoothing.
    reset: u64,
    /// Last requested named view ID, absent before preparation.
    view: Option<u8>,
    /// Last preparation time in monotonic milliseconds, frozen for all borrowed readings.
    now_ms: u64,
    /// Physical PSI toggle used only to resolve FollowBoost display units.
    boost_psi: bool,
    /// Synthetic appearance continuity mode; switching modes clears filters.
    appearance: bool,
}
impl Default for PresentationState {
    /// Returns empty visible filter state, as new.
    fn default() -> Self { Self::new() }
}
impl PresentationState {
    /// Returns invalid filters with no retained configuration/view/source/run identity.
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

    /// Updates visible filters using immutable `config`/`snapshot`, requested `view`,
    /// monotonic `now_ms` and physical `boost_psi`, then returns borrowed immutable
    /// readings for this epoch. Identity/view/reset/source changes or clock reversal
    /// snap eligible values; unavailable/stale values remain None. Invalid view yields no widgets.
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
    /// `config`, synthetic `snapshot`, selected `view`, monotonic `now_ms` and
    /// physical `boost_psi` define the returned borrowed readings. Reconstructed
    /// nonempty run identities alone do not reset appearance smoothing; all other guards remain.
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

    /// Prepares slots from `config` and canonical `snapshot` for `view` at monotonic
    /// `now_ms`, retaining `boost_psi` for unit resolution. `appearance` permits
    /// reconstructed synthetic run identities; normal mode resets on run replacement.
    /// Missing cache/run proof, view/reset/mode change or reversed time clears filters;
    /// per-channel source change clears affected filters, and reduced motion disables smoothing.
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
            let Some(channel) = widget.channel.scalar() else {
                *filter = SmoothState::EMPTY;
                continue;
            };
            let id = SensorId::from_channel(channel);
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
    /// Returns references to this prepared state, matching `config` layout and
    /// `snapshot`; callers must supply the pair just used by update.
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
    /// Immutable prepared filter epoch, time and physical unit selection.
    state: &'a PresentationState,
    /// Borrowed validated selected-view lookup, or None for absent view.
    layout: Option<Layout<'a>>,
    /// Canonical raw quality/warning/peak state, never copied or smoothed here.
    snapshot: &'a DashboardSnapshot,
    #[cfg(feature = "alloc")]
    /// Whether accepted-owner rule/peak indexing is authorized for this ephemeral view.
    rule_state: bool,
}
/// One widget's short-lived prepared reading; numeric values remain canonical until formatting.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PresentedWidget {
    /// Smoothed canonical current value, None for unavailable/invalid filter state.
    pub value: Option<f32>,
    /// Raw canonical producer peak, None when absent/stale or candidate rules disabled.
    pub peak: Option<f32>,
    /// Unsmoothed producer warning eligibility, disabled for unaccepted candidate layout.
    pub warning: WarningEligibility,
    /// Resolved primary display unit, including the physical FollowBoost toggle.
    pub unit: Unit,
    /// Distinct supported secondary pressure unit, or None.
    pub secondary: Option<Unit>,
}
impl DisplayReadings<'_> {
    /// Read-only candidate drawing may use qualified channel observations,
    /// but its widget indices cannot address an accepted owner's rules/peaks.
    /// This changes only the ephemeral readings view, never the snapshot or
    /// presentation owner. Available for alloc-enabled candidate inspection;
    /// normal target readings retain their original layout and rule path.
    /// Returns the consumed readings view with peaks omitted and warning state disabled.
    #[cfg(feature = "alloc")]
    pub fn without_rule_state(mut self) -> Self {
        self.rule_state = false;
        self
    }

    /// Returns prepared data for visible slot `cell`, or None for absent layout,
    /// out-of-range slot or invalid rule index. Does not advance filters/time; current
    /// value is smoothed while peak/warning use the raw snapshot at the prepared time.
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
/// Target-layout regression for the optional candidate-preview field.
mod layout_tests {
    use super::*;

    #[test]
    /// Verifies the no-alloc readings view retains the original borrowed size/alignment.
    fn normal_readings_keep_the_original_borrowed_layout() {
        // The normal target path has exactly the original three borrowed fields.
        #[allow(dead_code)]
        /// Original three-field representation used as the independent layout comparator.
        struct OriginalReadings<'a> {
            /// Prepared filters and frozen presentation time.
            state: &'a PresentationState,
            /// Borrowed selected-view lookup.
            layout: Option<Layout<'a>>,
            /// Canonical snapshot borrow.
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
