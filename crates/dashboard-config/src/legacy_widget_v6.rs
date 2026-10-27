//! Frozen schema-four through schema-six widget wire boundary; new selector/readout tags reject.
use heapless::String;
use serde::{Deserialize, Serialize};

use crate::*;

/// One channel presentation and warning rule, placed within its referenced view.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LegacyWidget {
    /// Unique nonzero identifier across every view.
    pub id: u16,
    /// Existing view ID; not an array index.
    pub view: u8,
    /// Shared measurement channel; multiple widgets may present the same sample.
    pub channel: Channel,
    /// Logical cell position and span; 2×1 remains horizontal in portrait.
    pub placement: Placement,
    /// Printable nonblank label; admission allows twelve bytes per spanned column.
    pub label: String<24>,
    /// Numeric precision and optional graph/statistic visibility.
    pub display: DisplayOptions,
    /// Raw-sample warning rule, independent of presentation smoothing.
    pub warning: WarningSettings,
    /// Gauge shape and canonical range; not an engine operating-limit recommendation.
    pub gauge: crate::Gauge,
    /// Per-role RGB565 colors with required pairwise contrast.
    pub colors: crate::WidgetColors,
    /// Readout units, timing, peak/reference and bounded gauge-band presentation.
    pub advanced: LegacyAdvancedDisplay,
}

/// Display units only; validation rejects combinations incompatible with the measurement channel.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum LegacyReadout {
    /// Temperature in degrees Celsius.
    Celsius,
    /// Temperature converted to degrees Fahrenheit.
    Fahrenheit,
    /// Pressure follows the runtime bar/PSI toggle.
    FollowBoost,
    /// Fixed bar display, unaffected by the toggle.
    Bar,
    /// Fixed PSI display, unaffected by the toggle.
    Psi,
    /// Fixed kilopascal display.
    Kpa,
    /// ECU supply voltage in volts.
    Volts,
    /// Dimensionless lambda without a fuel conversion.
    Lambda,
    /// Fuel-assumption-derived equivalent AFR.
    Afr,
    /// Lambda primary readout with equivalent AFR decoration.
    LambdaAndAfr,
}

/// Historical eight-field display wire order, independent of current readout variants.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LegacyAdvancedDisplay {
    /// At most three retained bands, with finite canonical endpoints validated after migration.
    bands: heapless::Vec<Band, 3>,
    /// Finite canonical reference; manual mode additionally requires its scalar domain.
    reference: f32,
    /// Smoothing milliseconds: zero or 50..=5000 after admission.
    smoothing_ms: u16,
    /// Peak-hold milliseconds, 100..=60000 after admission.
    peak_hold_ms: u16,
    /// Closed historical scalar readout vocabulary; channel compatibility is validated after migration.
    readout: LegacyReadout,
    /// Retained pressure companion selection; non-boost widgets require None.
    secondary: Secondary,
    /// Retained raw-extremum marker selection.
    peak_mode: PeakSelection,
    /// Retained reference-marker selection; manual values never establish an ECU target.
    reference_mode: ReferenceMode,
}

impl LegacyWidget {
    /// Return the current widget by consuming the frozen widget, preserving every
    /// scalar field and mapping closed readout variants.
    /// The band-copy assertion cannot fail while both reviewed capacities remain three.
    pub(crate) fn upgrade(self) -> Widget {
        let old = self.advanced;
        let mut advanced = AdvancedDisplay::for_channel(self.channel, self.display.afr);
        advanced.reference = old.reference;
        advanced.smoothing_ms = old.smoothing_ms;
        advanced.peak_hold_ms = old.peak_hold_ms;
        advanced.readout = match old.readout {
            LegacyReadout::Celsius => Readout::Celsius,
            LegacyReadout::Fahrenheit => Readout::Fahrenheit,
            LegacyReadout::FollowBoost => Readout::FollowBoost,
            LegacyReadout::Bar => Readout::Bar,
            LegacyReadout::Psi => Readout::Psi,
            LegacyReadout::Kpa => Readout::Kpa,
            LegacyReadout::Volts => Readout::Volts,
            LegacyReadout::Lambda => Readout::Lambda,
            LegacyReadout::Afr => Readout::Afr,
            LegacyReadout::LambdaAndAfr => Readout::LambdaAndAfr,
        };
        advanced.secondary = old.secondary;
        advanced.peak_mode = old.peak_mode;
        advanced.reference_mode = old.reference_mode;
        advanced.set_bands(&old.bands).expect("frozen three-band capacity");
        Widget {
            id: self.id,
            view: self.view,
            channel: self.channel.into(),
            placement: self.placement,
            label: self.label,
            display: self.display,
            warning: self.warning,
            gauge: self.gauge,
            colors: self.colors,
            advanced,
        }
    }
}

/// Return upgraded `widgets` without dropping or reordering entries; input and
/// output capacities are identical.
pub(crate) fn upgrade_widgets(widgets: heapless::Vec<LegacyWidget, MAX_WIDGETS>) -> heapless::Vec<Widget, MAX_WIDGETS> {
    widgets.into_iter().map(LegacyWidget::upgrade).collect()
}

#[cfg(test)]
mod tests {
    //! Frozen widget wire boundaries reject schema-seven group tags before migration.

    use super::*;

    #[test]
    /// Historical advanced widgets reject every group readout/channel binary tag before upgrade.
    fn new_tags_cannot_enter_the_frozen_wire_boundary() {
        let placement = Placement {
            column: 0,
            row: 0,
            span: Span::OneByOne,
        };
        let mut buffer = [0; 1024];
        let scalar = default_widget(Channel::Oil, 1, 1, placement);
        let bytes = postcard::to_slice(&scalar, &mut buffer).unwrap();
        // The current widget appends the schema-seven popup selector; frozen bytes stop before it.
        let frozen: LegacyWidget = postcard::from_bytes(&bytes[..bytes.len() - 1]).unwrap();
        assert_eq!(frozen.upgrade(), scalar);
        for readout in [
            Readout::IgnitionNumeric,
            Readout::IgnitionStatus,
            Readout::IgnitionStatusAndNumber,
        ] {
            let mut candidate = scalar.clone();
            candidate.advanced.readout = readout;
            let bytes = postcard::to_slice(&candidate, &mut buffer).unwrap();
            assert!(postcard::from_bytes::<LegacyWidget>(bytes).is_err());
        }
        let group = default_ignition_widget(1, 1, placement);
        let bytes = postcard::to_slice(&group, &mut buffer).unwrap();
        assert!(postcard::from_bytes::<LegacyWidget>(bytes).is_err());
    }
}
