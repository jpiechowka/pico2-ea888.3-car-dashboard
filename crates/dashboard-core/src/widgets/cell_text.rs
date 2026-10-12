//! Shared visible text: rasterization and retained-state comparison must agree.
use core::fmt::Write;

use heapless::String;

use crate::thresholds::BAR_TO_PSI;

pub type CellText = [String<16>; 4];
fn empty() -> CellText { core::array::from_fn(|_| String::new()) }

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
pub fn lambda(value: f32) -> CellText {
    let mut text = empty();
    let _ = write!(text[0], "{value:.3}");
    // A display convention, not the measured stoichiometry of the fuel in the
    // tank. E5 is an upper ethanol-content label, not an exact composition.
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
