//! Fixed local CLI adapters; the protocol owns all build option policy.
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;

use dashboard_build_profiles::{ClockSelection, Profile, ProfileId, ResolvedClock, catalogue, resolve};
use dashboard_helper_protocol::{BuildOptions, Mode, NormalizedBuildOptions, Optimization, decode, encode, normalize};
use serde::{Deserialize, Serialize};

use crate::clocks::custom_clock;

/// Maximum serialized profile catalogue bytes accepted by the CLI.
pub const PROFILES_LIMIT: usize = 65_536;
/// Maximum serialized capture-options bytes accepted by the CLI.
pub const CAPTURE_LIMIT: usize = 8192;

/// Catalogue identity paired with freshly resolved divider metadata.
#[derive(Serialize)]
struct ProfileRow {
    /// Canonical shared profile; labels and owner observations retain their provenance.
    profile: &'static Profile,
    /// Resolved requested/actual rates and register divisors for that profile.
    clock: ResolvedClock,
}
/// Versioned CLI profile-list envelope.
#[derive(Serialize)]
struct Profiles {
    /// Wire format version, currently 1.
    schema_version: u16,
    /// All catalogue rows in shared UI order.
    profiles: Vec<ProfileRow>,
}

/// Return bounded JSON for every profile and its freshly resolved clock recipe.
/// Any resolver/serialization/length failure rejects the entire output.
pub fn profiles_json() -> Result<Vec<u8>, String> {
    let profiles = catalogue()
        .iter()
        .map(|p| {
            Ok(ProfileRow {
                profile: p,
                clock: resolve(ClockSelection::Named(p.id)).map_err(|e| format!("{e:?}"))?,
            })
        })
        .collect::<Result<_, String>>()?;
    bounded_json(
        &Profiles {
            schema_version: 1,
            profiles,
        },
        PROFILES_LIMIT,
    )
}

/// Serialize `value`; return UTF-8 JSON bytes only when their length is at most `limit`.
///
/// The limit is checked after serialization and is not an allocation-time bound;
/// callers must supply already bounded typed data. Serializer errors propagate.
pub fn bounded_json(
    value: &impl Serialize,
    limit: usize,
) -> Result<Vec<u8>, String> {
    let bytes = serde_json::to_vec(value).map_err(|e| e.to_string())?;
    if bytes.len() > limit {
        return Err("JSON output exceeds bound".into());
    }
    Ok(bytes)
}

/// Parse `args` as key/value pairs restricted to `allowed`; return a sorted owned map.
///
/// Reject odd counts, non-Unicode text, unknown/duplicate keys and values over
/// 4096 UTF-8 bytes. Empty values remain available for option-specific validation.
fn pairs(
    args: &[OsString],
    allowed: &[&str],
) -> Result<BTreeMap<String, String>, String> {
    if !args.len().is_multiple_of(2) {
        return Err("every option requires one value".into());
    }
    let mut values = BTreeMap::new();
    for pair in args.as_chunks::<2>().0 {
        let key = pair[0].to_str().ok_or("non-Unicode option")?;
        let value = pair[1].to_str().ok_or("non-Unicode value")?;
        if !allowed.contains(&key) || values.insert(key.into(), value.into()).is_some() {
            return Err("unknown or duplicate option".into());
        }
        if value.len() > 4096 {
            return Err("option exceeds bound".into());
        }
    }
    Ok(values)
}
/// Read `key` from `values`; return `default` when absent or exact lowercase booleans.
/// Other spellings, including empty strings and numeric booleans, return an error.
fn boolean(
    values: &BTreeMap<String, String>,
    key: &str,
    default: bool,
) -> Result<bool, String> {
    match values.get(key).map(String::as_str) {
        None => Ok(default),
        Some("true") => Ok(true),
        Some("false") => Ok(false),
        _ => Err(format!("{key} requires true or false")),
    }
}
/// Return `--optimization` from `values`, defaulting to 3; reject anything but 3/2/s/z.
fn optimization(values: &BTreeMap<String, String>) -> Result<Optimization, String> {
    match values.get("--optimization").map(String::as_str).unwrap_or("3") {
        "3" => Ok(Optimization::Speed3),
        "2" => Ok(Optimization::Speed2),
        "s" => Ok(Optimization::SizeS),
        "z" => Ok(Optimization::SizeZ),
        _ => Err("optimization requires 3, 2, s or z".into()),
    }
}
/// Return the all-or-none custom clock triple in `values`; defer ranges to normalization.
fn custom(values: &BTreeMap<String, String>) -> Result<Option<ClockSelection>, String> {
    let raw = ["--cpu-hz", "--spi-hz", "--vsel-mv"].map(|key| values.get(key).map(OsString::from));
    custom_clock(&raw)
}

/// Parse fixed CLI `args`; return a normalized recipe or a rejection diagnostic.
///
/// Defaults come from the shared protocol. Only catalogue aliases, typed modes,
/// optimization and explicit booleans are admitted; named/custom conflicts and
/// missing Expert acknowledgement fail. Arbitrary environment/features are excluded.
pub fn build_options(args: &[OsString]) -> Result<NormalizedBuildOptions, String> {
    let values = pairs(
        args,
        &[
            "--mode",
            "--profile",
            "--cpu-hz",
            "--spi-hz",
            "--vsel-mv",
            "--optimization",
            "--simple-outline",
            "--serial-display",
            "--full-frame",
            "--frame-trace",
            "--te-trace",
            "--expert",
        ],
    )?;
    let mut requested = BuildOptions::default();
    if let Some(mode) = values.get("--mode") {
        requested.mode = match mode.as_str() {
            "demo" => Mode::Demo,
            "vehicle" => Mode::Vehicle,
            _ => return Err("mode requires demo or vehicle".into()),
        };
    }
    let clock = custom(&values)?;
    if let Some(profile) = values.get("--profile") {
        if clock.is_some() {
            return Err("named profile cannot be combined with custom clock".into());
        }
        let p = catalogue()
            .iter()
            .find(|p| p.aliases.contains(&profile.as_str()))
            .ok_or("unknown catalogue profile")?;
        requested.clock = ClockSelection::Named(p.id);
    } else if let Some(clock) = clock {
        requested.clock = clock;
    }
    requested.optimization = optimization(&values)?;
    requested.simple_outline = boolean(&values, "--simple-outline", requested.simple_outline)?;
    requested.serial_display = boolean(&values, "--serial-display", requested.serial_display)?;
    requested.full_frame = boolean(&values, "--full-frame", requested.full_frame)?;
    requested.frame_trace = boolean(&values, "--frame-trace", requested.frame_trace)?;
    requested.te_trace = boolean(&values, "--te-trace", requested.te_trace)?;
    requested.expert = boolean(&values, "--expert", false)?;
    normalize(&requested).map_err(|e| format!("invalid build options: {e:?}"))
}

/// Decimal environment values derived from an admitted custom clock recipe.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CustomEnvironment {
    /// Core frequency in ASCII decimal Hz.
    pub cpu_hz: String,
    /// SPI maximum request in ASCII decimal Hz, not the quantized actual rate.
    pub spi_hz: String,
    /// Regulator setpoint in ASCII decimal mV.
    pub vsel_mv: String,
}
/// Versioned capture/build replay options; raw legacy capture flags stay explicit.
///
/// Deserialization checks shape only; obtain admitted values via [`capture_options`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CaptureOptions {
    /// Capture-options schema, currently 2.
    pub schema_version: u16,
    /// Shared normalized recipe, before explicit legacy/benchmark feature adaptation.
    pub normalized: NormalizedBuildOptions,
    /// Sorted actual Cargo feature list, including any recorded capture adaptations.
    pub features: Vec<String>,
    /// Always true for generated options: defaults are expanded into explicit features.
    pub no_default_features: bool,
    /// Cargo optimization spelling: 3, 2, s or z.
    pub optimization: String,
    /// Custom clock environment triple; absent for named selections.
    pub custom_clock: Option<CustomEnvironment>,
    /// Capture adds `target-bench`; it is not a normal Studio build feature.
    pub benchmark: bool,
    /// Raw capture omitted a clock feature/custom triple; explicit stock feature is omitted.
    pub legacy_stock: bool,
    /// Raw capture selected no acquisition backend; do not silently enable BLE.
    pub legacy_unavailable_backend: bool,
    /// Raw capture explicitly selected historical `longan-rx12` instead of BLE.
    pub legacy_longan_backend: bool,
    /// Regulator setpoint in mV, derived from the normalized clock.
    pub voltage_mv: u32,
}
/// Return replay options from `normalized` plus explicitly identified capture exceptions.
///
/// `benchmark` adds target measurements; `legacy_stock` omits a stock feature;
/// `legacy_unavailable_backend` removes BLE and `legacy_longan_backend` substitutes
/// Longan. Callers must validate mutually exclusive backend flags before this helper.
fn capture(
    normalized: NormalizedBuildOptions,
    benchmark: bool,
    legacy_stock: bool,
    legacy_unavailable_backend: bool,
    legacy_longan_backend: bool,
) -> CaptureOptions {
    let mut features: Vec<String> = normalized.feature_names().into_iter().map(str::to_owned).collect();
    if legacy_stock {
        features.retain(|f| f != "cpu150-spi62-1v10");
    }
    if legacy_unavailable_backend {
        features.retain(|f| f != "ble-obd");
    }
    if legacy_longan_backend {
        features.retain(|f| f != "ble-obd");
        features.push("longan-rx12".into());
    }
    if benchmark {
        features.push("target-bench".into());
    }
    features.sort();
    let custom_clock = match normalized.requested.clock {
        ClockSelection::Custom(c) => {
            Some(CustomEnvironment {
                cpu_hz: c.cpu_hz.to_string(),
                spi_hz: c.spi_max_hz.to_string(),
                vsel_mv: crate::clocks::voltage_mv(c.voltage).to_string(),
            })
        }
        ClockSelection::Named(_) => None,
    };
    let optimization = normalized.requested.optimization.cargo_value().to_owned();
    let voltage_mv = crate::clocks::voltage_mv(normalized.clock.voltage);
    CaptureOptions {
        schema_version: 2,
        normalized,
        features,
        no_default_features: true,
        optimization,
        custom_clock,
        benchmark,
        legacy_stock,
        legacy_unavailable_backend,
        legacy_longan_backend,
        voltage_mv,
    }
}
/// Return checked capture options from fixed CLI `args`, or a diagnostic error.
///
/// Either accept one normalized JSON argument through protocol validation, or
/// reconstruct a recipe from a closed raw feature set. Reject mixed input forms,
/// duplicate/unknown features, backend conflicts and unacknowledged Expert clocks.
/// Preserve historical absent/Longan backends instead of inventing a BLE selection.
pub fn capture_options(args: &[OsString]) -> Result<CaptureOptions, String> {
    let values = pairs(
        args,
        &[
            "--normalized-json",
            "--features",
            "--no-default-features",
            "--optimization",
            "--expert",
            "--cpu-hz",
            "--spi-hz",
            "--vsel-mv",
        ],
    )?;
    if let Some(json) = values.get("--normalized-json") {
        if values.len() != 1 {
            return Err("normalized options cannot be combined with raw capture options".into());
        }
        let normalized = decode::<NormalizedBuildOptions>(json.as_bytes())
            .map_err(|e| format!("invalid normalized options: {e:?}"))?;
        return Ok(capture(normalized, false, false, false, false));
    }
    let raw = values
        .get("--features")
        .ok_or("capture requires features or normalized-json")?;
    let mut features = BTreeSet::new();
    if !raw.is_empty() {
        for feature in raw.split(',') {
            if feature.is_empty() || !features.insert(feature) {
                return Err("empty or duplicate capture feature".into());
            }
        }
    }
    let benchmark = features.remove("target-bench");
    let mut selected = None;
    for profile in catalogue() {
        if features.remove(profile.feature) && selected.replace(profile.id).is_some() {
            return Err("Only one clock profile feature can be enabled".into());
        }
    }
    let custom = custom(&values)?;
    if selected.is_some() && custom.is_some() {
        return Err("named profile cannot be combined with custom clock".into());
    }
    let legacy_stock = selected.is_none() && custom.is_none();
    let demo = features.remove("demo");
    let vehicle = features.remove("ble-obd");
    let legacy_longan = features.remove("longan-rx12");
    if u8::from(demo) + u8::from(vehicle) + u8::from(legacy_longan) > 1 {
        return Err("Select exactly one acquisition backend: demo, ble-obd or longan-rx12".into());
    }
    let no_default = boolean(&values, "--no-default-features", false)?;
    let requested = BuildOptions {
        mode: if demo { Mode::Demo } else { Mode::Vehicle },
        clock: custom.unwrap_or(ClockSelection::Named(selected.unwrap_or(ProfileId::Stock))),
        optimization: optimization(&values)?,
        simple_outline: features.remove("simple-outline") || !no_default,
        serial_display: features.remove("serial-display"),
        full_frame: features.remove("full-frame"),
        frame_trace: features.remove("frame-trace"),
        te_trace: features.remove("te-trace"),
        expert: boolean(&values, "--expert", false)?,
    };
    if !features.is_empty() {
        return Err("unknown capture feature".into());
    }
    Ok(capture(
        normalize(&requested).map_err(|e| format!("invalid build options: {e:?}"))?,
        benchmark,
        legacy_stock,
        !demo && !vehicle && !legacy_longan,
        legacy_longan,
    ))
}
/// Validate and encode `options`; return bounded protocol JSON or its rejection diagnostic.
pub fn normalized_json(options: &NormalizedBuildOptions) -> Result<Vec<u8>, String> {
    encode(options).map_err(|e| format!("{e:?}"))
}
