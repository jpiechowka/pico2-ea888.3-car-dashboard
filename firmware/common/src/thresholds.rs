//! Centralized sensor threshold configuration.
//!
//! All thresholds are compile-time constants with validation assertions.
//! This ensures consistency across color functions, critical checks, and
//! main loop logic.
//!
//! # Compile-Time Validation
//!
//! Each threshold group includes `const` assertions that verify threshold
//! ordering at compile time. If thresholds are configured incorrectly
//! (e.g., `CRITICAL < WARNING`), compilation will fail with a clear error.
//!
//! # Usage
//!
//! Import individual thresholds or use glob import:
//! ```ignore
//! use dashboard_common::thresholds::{OIL_DSG_CRITICAL, is_critical_battery};
//! // or
//! use dashboard_common::thresholds::*;
//! ```

// =============================================================================
// Oil/DSG Temperature Thresholds (shared by both sensors)
// =============================================================================

/// Temperature where oil/DSG enters elevated state (90-100C = YELLOW).
/// Below this value, background is BLACK (normal operation).
pub const OIL_DSG_ELEVATED: f32 = 90.0;

/// Temperature where oil/DSG enters high state (100-110C = ORANGE).
pub const OIL_DSG_HIGH: f32 = 100.0;

/// Temperature where oil/DSG enters critical state (>=110C = RED, blink + shake).
/// This is the danger zone - potential engine/transmission damage.
pub const OIL_DSG_CRITICAL: f32 = 110.0;

// Compile-time validation: thresholds must be in ascending order
const _: () = assert!(OIL_DSG_ELEVATED < OIL_DSG_HIGH);
const _: () = assert!(OIL_DSG_HIGH < OIL_DSG_CRITICAL);

// =============================================================================
// Coolant/Water Temperature Thresholds
// =============================================================================

/// Temperature where coolant transitions from cold (ORANGE) to optimal (GREEN).
/// Below this value, engine is still warming up.
pub const COOLANT_COLD_MAX: f32 = 75.0;

/// Temperature where coolant enters critical state (>90C = RED, blink + shake).
/// Indicates overheating - stop driving immediately.
pub const COOLANT_CRITICAL: f32 = 90.0;

const _: () = assert!(COOLANT_COLD_MAX < COOLANT_CRITICAL);

// =============================================================================
// Intake Air Temperature (IAT) Thresholds
// =============================================================================

/// Extreme cold threshold (<=-20C triggers critical blink).
/// Risk of ice formation in intake system.
pub const IAT_EXTREME_COLD: f32 = -20.0;

/// Cold threshold (<0C = BLUE).
/// Potential icing risk, dense air for power.
pub const IAT_COLD: f32 = 0.0;

/// Warm threshold (25-45C = YELLOW).
/// Air getting warm, less dense.
pub const IAT_WARM: f32 = 25.0;

/// Hot threshold (45-60C = ORANGE).
/// Heat soak affecting performance.
pub const IAT_HOT: f32 = 45.0;

/// Critical threshold (>=60C = RED, blink + shake).
/// Severe heat soak - significant power loss risk.
pub const IAT_CRITICAL: f32 = 60.0;

const _: () = assert!(IAT_EXTREME_COLD < IAT_COLD);
const _: () = assert!(IAT_COLD < IAT_WARM);
const _: () = assert!(IAT_WARM < IAT_HOT);
const _: () = assert!(IAT_HOT < IAT_CRITICAL);

// =============================================================================
// Exhaust Gas Temperature (EGT) Thresholds
// =============================================================================

/// Cold/warming threshold (<300C = BLUE).
/// Engine and catalyst still warming up.
pub const EGT_COLD_MAX: f32 = 300.0;

/// Spirited driving threshold (500-700C = YELLOW).
/// Normal for enthusiastic driving.
pub const EGT_SPIRITED: f32 = 500.0;

/// High load threshold (700-850C = ORANGE).
/// Hard acceleration, track use.
pub const EGT_HIGH_LOAD: f32 = 700.0;

/// Critical threshold (>=850C = RED, blink + shake).
/// Risk of catalyst/turbo damage, possible lean condition.
pub const EGT_CRITICAL: f32 = 850.0;

const _: () = assert!(EGT_COLD_MAX < EGT_SPIRITED);
const _: () = assert!(EGT_SPIRITED < EGT_HIGH_LOAD);
const _: () = assert!(EGT_HIGH_LOAD < EGT_CRITICAL);

// =============================================================================
// Battery Voltage Thresholds
// =============================================================================

/// Critical threshold (<12.0V = RED, blink + shake).
/// Indicates alternator failure or severe battery drain.
pub const BATT_CRITICAL: f32 = 12.0;

/// Warning threshold (12.0-12.5V = ORANGE).
/// Battery not fully charged or slight alternator issue.
pub const BATT_WARNING: f32 = 12.5;

const _: () = assert!(BATT_CRITICAL < BATT_WARNING);

/// Check if battery voltage is critical.
///
/// Returns `true` if voltage is below `BATT_CRITICAL` (12.0V).
#[inline]
#[allow(dead_code)]
pub fn is_critical_battery(voltage: f32) -> bool { voltage < BATT_CRITICAL }

// =============================================================================
// Air-Fuel Ratio (AFR) Thresholds
// =============================================================================

/// Very rich threshold (<12.0 = BLUE, "RICH AF").
/// Risk of fuel washing cylinder walls, catalyst damage.
pub const AFR_RICH_AF: f32 = 12.0;

/// Rich threshold (12.0-14.0 = `DARK_TEAL`, "RICH").
/// Safe for power under boost/load.
pub const AFR_RICH: f32 = 14.0;

/// Optimal ceiling (14.0-14.9 = GREEN).
/// Efficient cruise operation.
pub const AFR_OPTIMAL_MAX: f32 = 14.9;

/// Lean/critical threshold (>15.5 = RED, "LEAN AF", blink + shake).
/// Risk of detonation/engine damage under load.
pub const AFR_LEAN_CRITICAL: f32 = 15.5;

/// Stoichiometric air-fuel ratio (14.7:1).
/// Theoretical perfect combustion ratio.
pub const AFR_STOICH: f32 = 14.7;

const _: () = assert!(AFR_RICH_AF < AFR_RICH);
const _: () = assert!(AFR_RICH < AFR_OPTIMAL_MAX);
const _: () = assert!(AFR_OPTIMAL_MAX < AFR_LEAN_CRITICAL);

// =============================================================================
// Boost Pressure Thresholds
// =============================================================================

/// Easter egg threshold in bar (~2.0 bar).
/// Triggers "Fast AF Boi!" message.
pub const BOOST_EASTER_EGG_BAR: f32 = 1.95;

/// Easter egg threshold in PSI (~29.0 PSI).
/// Triggers "Fast AF Boi!" message when displaying PSI.
pub const BOOST_EASTER_EGG_PSI: f32 = 29.0;

/// Bar to PSI conversion factor.
/// 1 bar = 14.5038 PSI.
pub const BAR_TO_PSI: f32 = 14.5038;

// =============================================================================
// Unit Tests
// =============================================================================

#[cfg(test)]
#[allow(clippy::assertions_on_constants)] // Intentional compile-time validation of threshold ordering
mod tests {
    use super::*;

    #[test]
    fn test_oil_dsg_threshold_ordering() {
        assert!(OIL_DSG_ELEVATED < OIL_DSG_HIGH);
        assert!(OIL_DSG_HIGH < OIL_DSG_CRITICAL);
    }

    #[test]
    fn test_coolant_threshold_ordering() {
        assert!(COOLANT_COLD_MAX < COOLANT_CRITICAL);
    }

    #[test]
    fn test_iat_threshold_ordering() {
        assert!(IAT_EXTREME_COLD < IAT_COLD);
        assert!(IAT_COLD < IAT_WARM);
        assert!(IAT_WARM < IAT_HOT);
        assert!(IAT_HOT < IAT_CRITICAL);
    }

    #[test]
    fn test_egt_threshold_ordering() {
        assert!(EGT_COLD_MAX < EGT_SPIRITED);
        assert!(EGT_SPIRITED < EGT_HIGH_LOAD);
        assert!(EGT_HIGH_LOAD < EGT_CRITICAL);
    }

    #[test]
    fn test_battery_threshold_ordering() {
        assert!(BATT_CRITICAL < BATT_WARNING);
    }

    #[test]
    fn test_afr_threshold_ordering() {
        assert!(AFR_RICH_AF < AFR_RICH);
        assert!(AFR_RICH < AFR_OPTIMAL_MAX);
        assert!(AFR_OPTIMAL_MAX < AFR_LEAN_CRITICAL);
    }

    #[test]
    fn test_is_critical_battery() {
        assert!(is_critical_battery(11.9), "11.9V should be critical");
        assert!(is_critical_battery(11.0), "11.0V should be critical");
        assert!(!is_critical_battery(12.0), "12.0V should not be critical");
        assert!(!is_critical_battery(12.5), "12.5V should not be critical");
        assert!(!is_critical_battery(14.0), "14.0V should not be critical");
    }

    #[test]
    fn test_bar_to_psi_conversion() {
        // 1 bar should be approximately 14.5 PSI
        assert!((BAR_TO_PSI - 14.5).abs() < 0.1);
        // 2 bar (easter egg threshold) should be approximately 29 PSI
        let two_bar_psi = 2.0 * BAR_TO_PSI;
        assert!((two_bar_psi - 29.0).abs() < 0.1);
    }

    #[test]
    fn test_easter_egg_thresholds_consistent() {
        // Easter egg PSI threshold should be reasonably close to BAR threshold converted
        // BAR = 1.95, PSI = 29.0, actual conversion = 1.95 * 14.5038 ≈ 28.28 PSI
        // The PSI threshold is a rounded value, so allow larger tolerance
        let bar_as_psi = BOOST_EASTER_EGG_BAR * BAR_TO_PSI;
        assert!(
            (bar_as_psi - BOOST_EASTER_EGG_PSI).abs() < 1.0,
            "Easter egg thresholds should be within ~1 PSI"
        );
    }

    #[test]
    fn test_afr_stoich_in_optimal_range() {
        // Stoichiometric should be in the optimal range
        assert!(AFR_STOICH > AFR_RICH);
        assert!(AFR_STOICH < AFR_OPTIMAL_MAX);
    }
}
