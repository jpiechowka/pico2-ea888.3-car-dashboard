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
