pub const OIL_LOW_TEMP: f32 = 75.0;

pub const OIL_DSG_ELEVATED: f32 = 90.0;

pub const OIL_DSG_HIGH: f32 = 100.0;

pub const OIL_DSG_CRITICAL: f32 = 110.0;

const _: () = assert!(OIL_LOW_TEMP < OIL_DSG_ELEVATED);
const _: () = assert!(OIL_DSG_ELEVATED < OIL_DSG_HIGH);
const _: () = assert!(OIL_DSG_HIGH < OIL_DSG_CRITICAL);

pub const COOLANT_COLD_MAX: f32 = 75.0;

pub const COOLANT_CRITICAL: f32 = 90.0;

const _: () = assert!(COOLANT_COLD_MAX < COOLANT_CRITICAL);

pub const IAT_EXTREME_COLD: f32 = -20.0;

pub const IAT_COLD: f32 = 0.0;

pub const IAT_WARM: f32 = 25.0;

pub const IAT_HOT: f32 = 45.0;

pub const IAT_CRITICAL: f32 = 60.0;

const _: () = assert!(IAT_EXTREME_COLD < IAT_COLD);
const _: () = assert!(IAT_COLD < IAT_WARM);
const _: () = assert!(IAT_WARM < IAT_HOT);
const _: () = assert!(IAT_HOT < IAT_CRITICAL);

pub const EGT_COLD_MAX: f32 = 300.0;

pub const EGT_SPIRITED: f32 = 600.0;

pub const EGT_HIGH_LOAD: f32 = 850.0;

pub const EGT_CRITICAL: f32 = 950.0;

pub const EGT_DANGER_MANIFOLD: f32 = 1050.0;

const _: () = assert!(EGT_COLD_MAX < EGT_SPIRITED);
const _: () = assert!(EGT_SPIRITED < EGT_HIGH_LOAD);
const _: () = assert!(EGT_HIGH_LOAD < EGT_CRITICAL);
const _: () = assert!(EGT_CRITICAL < EGT_DANGER_MANIFOLD);

pub const BATT_CRITICAL: f32 = 12.0;

pub const BATT_WARNING: f32 = 12.5;

const _: () = assert!(BATT_CRITICAL < BATT_WARNING);

#[inline]
#[allow(dead_code)]
pub fn is_critical_battery(voltage: f32) -> bool { voltage < BATT_CRITICAL }

pub const AFR_RICH_AF: f32 = 12.0;

pub const AFR_RICH: f32 = 14.0;

pub const AFR_OPTIMAL_MAX: f32 = 14.9;

pub const AFR_LEAN_CRITICAL: f32 = 15.5;

pub const AFR_STOICH: f32 = 14.7;

const _: () = assert!(AFR_RICH_AF < AFR_RICH);
const _: () = assert!(AFR_RICH < AFR_OPTIMAL_MAX);
const _: () = assert!(AFR_OPTIMAL_MAX < AFR_LEAN_CRITICAL);

pub const BOOST_EASTER_EGG_BAR: f32 = 1.95;

pub const BOOST_EASTER_EGG_PSI: f32 = 29.0;

pub const BAR_TO_PSI: f32 = 14.5038;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_oil_dsg_threshold_ordering() {
        assert!(OIL_LOW_TEMP < OIL_DSG_ELEVATED);
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
        assert!(EGT_CRITICAL < EGT_DANGER_MANIFOLD);
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
        assert!(is_critical_battery(11.9));
        assert!(is_critical_battery(11.0));
        assert!(!is_critical_battery(12.0));
        assert!(!is_critical_battery(12.5));
        assert!(!is_critical_battery(14.0));
    }

    #[test]
    fn test_bar_to_psi_conversion() {
        assert!((BAR_TO_PSI - 14.5).abs() < 0.1);
        let two_bar_psi = 2.0 * BAR_TO_PSI;
        assert!((two_bar_psi - 29.0).abs() < 0.1);
    }

    #[test]
    fn test_afr_stoich_in_optimal_range() {
        assert!(AFR_STOICH > AFR_RICH);
        assert!(AFR_STOICH < AFR_OPTIMAL_MAX);
    }
}
