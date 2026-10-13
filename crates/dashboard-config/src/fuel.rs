use core::ops::ControlFlow;

use crate::validate::in_range;
use crate::*;

/// Independent inputs are all inspected; arithmetic runs only after their proof.
pub(crate) fn visit_stoichiometry(
    fuel: Fuel,
    mut emit: impl FnMut(ValidationError) -> ControlFlow<()>,
) -> ControlFlow<(), Option<f32>> {
    let mut valid = true;
    macro_rules! bounded {
        ($value:expr, $low:expr, $high:expr, $field:expr) => {
            if !in_range($value, $low, $high) {
                valid = false;
                emit(ValidationError::new(ErrorCode::Bounds, $field))?;
            }
        };
    }
    let ratio = match fuel {
        Fuel::EquivalentGasoline => 14.7,
        Fuel::Stoichiometric { ratio } => ratio,
        Fuel::EthanolBlend(blend) => {
            bounded!(blend.percent, 0.0, 100.0, Field::FuelPercent);
            bounded!(blend.gasoline_stoich, 5.0, 25.0, Field::FuelGasolineRatio);
            bounded!(blend.ethanol_stoich, 5.0, 25.0, Field::FuelEthanolRatio);
            bounded!(blend.gasoline_density, 0.5, 1.2, Field::FuelGasolineDensity);
            bounded!(blend.ethanol_density, 0.5, 1.2, Field::FuelEthanolDensity);
            if in_range(blend.gasoline_stoich, 5.0, 25.0)
                && in_range(blend.ethanol_stoich, 5.0, 25.0)
                && blend.gasoline_stoich <= blend.ethanol_stoich
            {
                valid = false;
                emit(ValidationError::new(ErrorCode::FuelBasis, Field::FuelGasolineRatio))?;
            }
            match blend.density_reference {
                DensityReference::CommonCelsius(value) => bounded!(value, -30.0, 60.0, Field::FuelReference),
                DensityReference::UnknownAssumed
                    if blend.basis == BlendBasis::Volume && blend.composition_origin == CompositionOrigin::Measured =>
                {
                    valid = false;
                    emit(ValidationError::new(ErrorCode::FuelBasis, Field::FuelReference))?;
                }
                _ => {}
            }
            if !valid {
                return ControlFlow::Continue(None);
            }
            let fraction = blend.percent / 100.0;
            let mass_fraction = match blend.basis {
                BlendBasis::Mass => fraction,
                BlendBasis::Volume => {
                    let ethanol = fraction * blend.ethanol_density;
                    ethanol / ((1.0 - fraction) * blend.gasoline_density + ethanol)
                }
            };
            (1.0 - mass_fraction) * blend.gasoline_stoich + mass_fraction * blend.ethanol_stoich
        }
    };
    bounded!(ratio, 5.0, 25.0, Field::FuelRatio);
    ControlFlow::Continue(valid.then_some(ratio))
}
