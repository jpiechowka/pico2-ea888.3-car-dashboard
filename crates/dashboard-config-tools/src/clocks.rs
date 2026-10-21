//! Build-script clock generation, with no independent clock policy.
use std::ffi::OsString;

use dashboard_build_profiles::{ClockSelection, CustomClock, ProfileId, ResolvedClock, Voltage, catalogue, resolve};
use serde::{Deserialize, Serialize};

/// Ordered build-environment keys: core Hz, SPI upper-bound Hz, regulator mV.
pub const CUSTOM_KEYS: [&str; 3] = ["DASHBOARD_CPU_HZ", "DASHBOARD_SPI_HZ", "DASHBOARD_VSEL_MV"];
/// Parse `value` as ASCII unsigned decimal; return `u32` or a diagnostic string.
///
/// Empty, signed, spaced, non-ASCII and overflowing input is rejected; leading zeros
/// are accepted. Frequency/setpoint admission belongs to the subsequent resolver.
pub fn decimal(value: &str) -> Result<u32, String> {
    if value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
        return Err("clock inputs require nonempty ASCII decimal integers".into());
    }
    value.parse().map_err(|_| "clock integer exceeds u32".into())
}
/// Return the integer mV setpoint of `v`, forwarding the shared voltage definition.
pub fn voltage_mv(v: Voltage) -> u32 { v.millivolts() }
/// Parse ordered `raw` environment values; return no selection when all are absent.
///
/// Partial triples, non-Unicode/decimal values or unsupported voltage setpoints
/// return an error. CPU/SPI ranges and representability are deferred to `resolve`.
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
/// Return the clock selection from Cargo `features` and ordered custom `raw` values.
///
/// Unknown/non-clock features are ignored; duplicate copies of one feature do not
/// select two profiles. Multiple distinct clock features or any named/custom mix
/// fail. No clock feature and no custom values selects Stock; resolution is separate.
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
/// Build sidecar tying selected and resolved clocks to optimization/features.
///
/// Deserialization enforces shape only. Consumers must recompute the selection and
/// compare fields before trusting metadata supplied outside the checked build.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClockMetadata {
    /// Sidecar format version; generation emits version 1.
    pub schema_version: u16,
    /// Named or explicit custom request before resolution.
    pub selection: ClockSelection,
    /// Resolved actual frequencies and divider registers for that request.
    pub clock: ResolvedClock,
    /// Caller-supplied Cargo optimization description; generation does not validate it.
    pub optimization: String,
    /// Sorted, deduplicated caller feature names; not independent feature admission.
    pub features: Vec<String>,
}
/// Resolve `selection`; return `(Rust source, JSON sidecar)` UTF-8 bytes.
///
/// `optimization` is recorded verbatim; `features` are sorted/deduplicated. Only
/// typed/resolved numeric values and a closed voltage identifier enter Rust source.
/// Invalid clocks or a JSON sidecar over 4096 bytes fail; this function writes no
/// files and cannot flash or program clocks. The generated constants carry docs.
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
        r#"// Generated from dashboard-build-profiles; do not edit.
/// Requested core frequency, Hz; legacy rounding may change the actual rate.
pub const REQUESTED_CPU_HZ: u32 = {};
/// Exact resolved PLL/core frequency, Hz; not a runtime measurement.
pub const ACTUAL_CPU_HZ: u32 = {};
/// SPI maximum frequency requested from Embassy, Hz.
pub const REQUESTED_SPI_HZ: u32 = {};
/// Floor of the rational divider-derived SPI rate, Hz; not measured scanout.
pub const ACTUAL_SPI_HZ: u32 = {};
/// Integer core-regulator setpoint, mV; not a measured voltage.
pub const VOLTAGE_MV: u32 = {};
/// Typed HAL regulator selection paired with the admitted clock recipe.
pub const CORE_VOLTAGE: CoreVoltage = CoreVoltage::{};
/// Checked PLL tuple for the board's 12 MHz crystal.
pub const PLL: PllConfig = PllConfig {{ refdiv: {}, fbdiv: {}, post_div1: {}, post_div2: {} }};
/// Expected SPI even prescaler register for the generated request.
pub const CPSDVSR: u8 = {};
/// Expected SPI serial-rate register; effective second divisor is SCR + 1.
pub const SCR: u8 = {};
/// True only for the named legacy 290 MHz request resolved to 290.4 MHz.
pub const LEGACY_ROUNDING: bool = {};
"#,
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
