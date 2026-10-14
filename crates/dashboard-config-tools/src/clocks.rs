//! Build-script clock generation, with no independent clock policy.
use std::ffi::OsString;

use dashboard_build_profiles::{ClockSelection, CustomClock, ProfileId, ResolvedClock, Voltage, catalogue, resolve};
use serde::{Deserialize, Serialize};

pub const CUSTOM_KEYS: [&str; 3] = ["DASHBOARD_CPU_HZ", "DASHBOARD_SPI_HZ", "DASHBOARD_VSEL_MV"];
pub fn decimal(value: &str) -> Result<u32, String> {
    if value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
        return Err("clock inputs require nonempty ASCII decimal integers".into());
    }
    value.parse().map_err(|_| "clock integer exceeds u32".into())
}
pub fn voltage_mv(v: Voltage) -> u32 {
    match v {
        Voltage::V1_10 => 1100,
        Voltage::V1_15 => 1150,
        Voltage::V1_20 => 1200,
        Voltage::V1_25 => 1250,
        Voltage::V1_30 => 1300,
    }
}
pub fn custom_clock(raw: &[Option<OsString>; 3]) -> Result<Option<ClockSelection>, String> {
    let count = raw.iter().filter(|v| v.is_some()).count();
    if count == 0 {
        return Ok(None);
    }
    if count != 3 {
        return Err(
            "custom clock requires all three DASHBOARD_CPU_HZ/DASHBOARD_SPI_HZ/DASHBOARD_VSEL_MV values".into(),
        );
    }
    let numbers = raw
        .iter()
        .map(|v| {
            decimal(
                v.as_ref()
                    .unwrap()
                    .to_str()
                    .ok_or("clock inputs must be Unicode decimal")?,
            )
        })
        .collect::<Result<Vec<_>, String>>()?;
    let voltage = match numbers[2] {
        1100 => Voltage::V1_10,
        1150 => Voltage::V1_15,
        1200 => Voltage::V1_20,
        1250 => Voltage::V1_25,
        1300 => Voltage::V1_30,
        _ => return Err("voltage requires 1100, 1150, 1200, 1250 or 1300 mV".into()),
    };
    Ok(Some(ClockSelection::Custom(CustomClock {
        cpu_hz: numbers[0],
        spi_max_hz: numbers[1],
        voltage,
    })))
}
pub fn selection(
    features: &[String],
    raw: &[Option<OsString>; 3],
) -> Result<ClockSelection, String> {
    let selected = catalogue()
        .iter()
        .filter(|p| features.iter().any(|f| f == p.feature))
        .collect::<Vec<_>>();
    if selected.len() > 1 {
        return Err("Only one clock profile feature can be enabled".into());
    }
    if !selected.is_empty() && raw.iter().any(Option::is_some) {
        return Err("named profile cannot be combined with custom clock".into());
    }
    Ok(custom_clock(raw)?.unwrap_or(ClockSelection::Named(
        selected.first().map_or(ProfileId::Stock, |p| p.id),
    )))
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClockMetadata {
    pub schema_version: u16,
    pub selection: ClockSelection,
    pub clock: ResolvedClock,
    pub optimization: String,
    pub features: Vec<String>,
}
pub fn generate(
    selection: ClockSelection,
    optimization: &str,
    mut features: Vec<String>,
) -> Result<(Vec<u8>, Vec<u8>), String> {
    let r = resolve(selection).map_err(|e| format!("{e:?}"))?;
    features.sort();
    features.dedup();
    let voltage = match r.voltage {
        Voltage::V1_10 => "V1_10",
        Voltage::V1_15 => "V1_15",
        Voltage::V1_20 => "V1_20",
        Voltage::V1_25 => "V1_25",
        Voltage::V1_30 => "V1_30",
    };
    let source = format!(
        "// Generated from dashboard-build-profiles; do not edit.\npub const REQUESTED_CPU_HZ: u32 = {};\npub const \
         ACTUAL_CPU_HZ: u32 = {};\npub const REQUESTED_SPI_HZ: u32 = {};\npub const ACTUAL_SPI_HZ: u32 = {};\npub \
         const VOLTAGE_MV: u32 = {};\npub const CORE_VOLTAGE: CoreVoltage = CoreVoltage::{};\npub const PLL: \
         PllConfig = PllConfig {{ refdiv: {}, fbdiv: {}, post_div1: {}, post_div2: {} }};\npub const CPSDVSR: u8 = \
         {};\npub const SCR: u8 = {};\npub const LEGACY_ROUNDING: bool = {};\n",
        r.requested_cpu_hz,
        r.actual_cpu_hz,
        r.requested_spi_hz,
        r.actual_spi_hz,
        voltage_mv(r.voltage),
        voltage,
        r.pll.refdiv,
        r.pll.fbdiv,
        r.pll.postdiv1,
        r.pll.postdiv2,
        r.cpsdvsr,
        r.scr,
        r.legacy_rounding
    );
    let metadata = ClockMetadata {
        schema_version: 1,
        selection,
        clock: r,
        optimization: optimization.into(),
        features,
    };
    Ok((
        source.into_bytes(),
        crate::build_options::bounded_json(&metadata, 4096)?,
    ))
}
