//! Shared visible text: rasterization and retained-state comparison must agree.
use core::fmt::Write;

use heapless::String;

use crate::thresholds::BAR_TO_PSI;

/// Six text lanes, each bounded to16 UTF-8 bytes. Scalar lanes are visible text;
/// ignition uses lanes0..4 for cylinders and lanes4/5 for key-only quality/source
/// metadata. Unused lanes are empty; consumers must select the matching renderer.
pub type CellText = [String<16>; 6];
/// Returns six empty owned text lanes without allocation.
fn empty() -> CellText { core::array::from_fn(|_| String::new()) }

/// Returns legacy current/alternate-unit/maximum text for `value` and `maximum`
/// in relative bar; `psi` chooses PSI primary with one decimal, otherwise bar
/// with two. Inputs are not qualified here; oversized formatting retains a bounded prefix.
pub fn boost(
    value: f32,
    maximum: f32,
    psi: bool,
) -> CellText {
    let mut text = empty();
    if psi {
        let _ = write!(text[0], "{:.1}", value * BAR_TO_PSI);
        let _ = write!(text[1], "{value:.2} BAR");
        let _ = write!(text[2], "MAX {:.1}", maximum * BAR_TO_PSI);
    } else {
        let _ = write!(text[0], "{value:.2}");
        let _ = write!(text[1], "{:.1} PSI", value * BAR_TO_PSI);
        let _ = write!(text[2], "MAX {maximum:.2}");
    }
    text
}
/// Returns lambda-ratio `value` to three decimals, gasoline-equivalent AFR and
/// explicit 14.7 reference text. AFR outside `0.01..=999.99` (including nonfinite)
/// becomes `--`; the lambda lane itself is not validated here.
pub fn lambda(value: f32) -> CellText {
    let mut text = empty();
    let _ = write!(text[0], "{value:.3}");
    // A display convention, not the measured stoichiometry of the fuel in the
    // tank. E5 is an upper ethanol-content label, not an exact composition.
    /// Assumed gasoline stoichiometric mass ratio, never a measured fuel/ECU target.
    const GASOLINE_EQUIVALENT_SCALE: f32 = 14.7;
    let afr = value * GASOLINE_EQUIVALENT_SCALE;
    if (0.01..=999.99).contains(&afr) {
        let _ = write!(text[1], "AFR {afr:.2}");
    } else {
        let _ = text[1].push_str("AFR --");
    }
    let _ = write!(text[2], "EQ {GASOLINE_EQUIVALENT_SCALE:.1}");
    text
}
/// Returns current `value`, `minimum` and `maximum` volts to one decimal with V
/// suffixes. Caller supplies qualified values; formatting failure leaves bounded prefixes.
pub fn battery(
    value: f32,
    minimum: f32,
    maximum: f32,
) -> CellText {
    let mut text = empty();
    let _ = write!(text[0], "{value:.1}V");
    let _ = write!(text[1], "MIN {minimum:.1}V");
    let _ = write!(text[2], "MAX {maximum:.1}V");
    text
}
/// Returns current `value`, optional `average` and `maximum` degrees Celsius
/// rounded to whole degrees; `None` leaves the average lane empty. No qualification
/// occurs here, and oversized formatting keeps only the successful bounded prefix.
pub fn temperature(
    value: f32,
    maximum: f32,
    average: Option<f32>,
) -> CellText {
    let mut text = empty();
    let _ = write!(text[0], "{value:.0}C");
    if let Some(average) = average {
        let _ = write!(text[1], "AVG {average:.0}C");
    }
    let _ = write!(text[2], "MAX {maximum:.0}C");
    text
}
