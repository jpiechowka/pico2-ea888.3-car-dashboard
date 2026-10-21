//! Checked 12 MHz crystal PLL selection and Embassy-compatible SPI quantization.
//!
//! All calculations are integer/rational; displayed Hz truncate fractional SPI
//! rates. Public selections bound CPU/SPI requests before internal arithmetic.

use crate::ProfileId;
use crate::catalogue::profile;

/// Admitted core-voltage setpoints; selection does not prove overclock stability.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum Voltage {
    /// 1100 mV setpoint.
    V1_10,
    /// 1150 mV setpoint.
    V1_15,
    /// 1200 mV setpoint.
    V1_20,
    /// 1250 mV setpoint.
    V1_25,
    /// 1300 mV setpoint.
    V1_30,
}

impl Voltage {
    /// Return the exact integer setpoint in mV; this is not an ADC measurement.
    pub const fn millivolts(self) -> u32 {
        match self {
            Self::V1_10 => 1100,
            Self::V1_15 => 1150,
            Self::V1_20 => 1200,
            Self::V1_25 => 1250,
            Self::V1_30 => 1300,
        }
    }
}

/// Unresolved Expert request; constructing/deserializing it does not validate clocks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct CustomClock {
    /// Exact requested core Hz; [`resolve`] admits 12–300 MHz and requires a PLL solution.
    pub cpu_hz: u32,
    /// Maximum SPI Hz, 1–75 MHz and at most half the resolved peripheral clock.
    pub spi_max_hz: u32,
    /// Typed regulator setpoint, independent of frequency representability.
    pub voltage: Voltage,
}

/// Recipe input; all variants require [`resolve`] before register generation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum ClockSelection {
    /// Use the canonical profile, including its deliberate legacy rounding rule.
    Named(
        /// Catalogue identity.
        ProfileId,
    ),
    /// Require exact CPU synthesis from an explicitly supplied request.
    Custom(
        /// Unvalidated custom request.
        CustomClock,
    ),
}

/// PLL dividers: `12 MHz * fbdiv / refdiv / postdiv1 / postdiv2`.
///
/// Fields are public for transport. Only values produced by [`resolve`] carry
/// its validation; deserialization alone does not validate VCO or divider limits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct Pll {
    /// Crystal reference divisor; this resolver searches 1 then 2.
    pub refdiv: u8,
    /// Feedback multiplier, 16–320, additionally restricted by the VCO range.
    pub fbdiv: u16,
    /// First post-divider, 1–7.
    pub postdiv1: u8,
    /// Second post-divider, 1–`postdiv1`.
    pub postdiv2: u8,
}

/// One SPI divider result with an integer request that reproduces it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct SpiRate {
    /// An allowed integer request that reproduces these exact dividers.
    pub request_hz: u32,
    /// Truncated display value; compare rates using the divisors instead.
    pub actual_hz: u32,
    /// Even prescaler, 2–254; resolved public requests currently select 2.
    pub cpsdvsr: u8,
    /// Register value for the second divisor minus one, 0–255.
    pub scr: u8,
}

/// Nearest reproducible SPI rates on each side of a requested rational rate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct SpiNeighbors {
    /// Fastest rate no greater than the request, or `None` if no admitted witness exists.
    pub below_or_equal: Option<SpiRate>,
    /// Slowest rate strictly above the request, or `None` at the admitted ceiling.
    pub above: Option<SpiRate>,
}

/// Resolved register recipe and honest requested/actual metadata.
///
/// Successful [`resolve`] validates this value. Public fields and deserialization
/// do not preserve that guarantee for values assembled by another caller.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct ResolvedClock {
    /// Original core request, Hz; differs from actual only for the legacy profile.
    pub requested_cpu_hz: u32,
    /// Exact integral PLL output, Hz; also used as `clk_peri` by the firmware recipe.
    pub actual_cpu_hz: u32,
    /// Original SPI maximum request, Hz.
    pub requested_spi_hz: u32,
    /// Integer floor of `actual_cpu_hz / (cpsdvsr * (scr + 1))`, Hz.
    pub actual_spi_hz: u32,
    /// Validated PLL divider tuple used to produce the core/peripheral clock.
    pub pll: Pll,
    /// SPI prescaler register, an even integer from 2 through 254.
    pub cpsdvsr: u8,
    /// SPI serial clock rate register; the effective divisor is this value plus one.
    pub scr: u8,
    /// Regulator setpoint passed through from the selected recipe.
    pub voltage: Voltage,
    /// True only for named legacy 290 MHz, retaining its 290.4 MHz PLL behavior.
    pub legacy_rounding: bool,
    /// Reproducible neighboring SPI choices for UI explanation and correction.
    pub spi_neighbors: SpiNeighbors,
}

/// Resolution failure; no hardware or externally visible state has been changed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum ClockError {
    /// CPU request lies outside the inclusive 12–300 MHz admission range.
    CpuOutOfRange,
    /// SPI request lies outside the inclusive 1–75 MHz admission range.
    SpiOutOfRange,
    /// SPI request exceeds half the resolved `clk_peri` frequency.
    SpiAbovePeripheralLimit,
    /// No exact allowed PLL tuple exists; custom requests are never rounded.
    NoExactPll,
    /// Requested SPI rate would need a divisor beyond the register capacity.
    SpiTooLow,
    /// Checked intermediate arithmetic or integer conversion cannot be represented.
    ArithmeticOverflow,
    /// Zero, out-of-range or nonintegral divider configuration was encountered.
    InvalidDivider,
}

/// Pico 2 W crystal frequency in Hz; this resolver is not crystal-configurable.
const XOSC_HZ: u64 = 12_000_000;
/// Minimum admitted PLL VCO Hz, matching the vendored RP2350 HAL.
const VCO_MIN_HZ: u64 = 750_000_000;
/// Maximum admitted PLL VCO Hz, matching the vendored RP2350 HAL.
const VCO_MAX_HZ: u64 = 1_600_000_000;

/// Resolve `selection` into checked PLL/SPI values; return a recipe or [`ClockError`].
///
/// CPU requests must be exact except named legacy 290 MHz; SPI is rounded down by
/// the HAL's ceiling-divisor algorithm. The maximum SPI request is checked against
/// the actual peripheral clock, not its label. Pure and allocation-free; success
/// establishes representability, not voltage, panel timing or electrical safety.
pub fn resolve(selection: ClockSelection) -> Result<ResolvedClock, ClockError> {
    let (cpu_hz, spi_max_hz, voltage, legacy_rounding) = match selection {
        ClockSelection::Named(id) => {
            let selected = profile(id);
            (
                selected.requested_cpu_hz,
                selected.requested_spi_hz,
                selected.voltage,
                id == ProfileId::Legacy290Spi72,
            )
        }
        ClockSelection::Custom(custom) => (custom.cpu_hz, custom.spi_max_hz, custom.voltage, false),
    };
    if !(12_000_000..=300_000_000).contains(&cpu_hz) {
        return Err(ClockError::CpuOutOfRange);
    }
    if !(1_000_000..=75_000_000).contains(&spi_max_hz) {
        return Err(ClockError::SpiOutOfRange);
    }

    let pll = if legacy_rounding {
        Pll {
            refdiv: 1,
            fbdiv: 121,
            postdiv1: 5,
            postdiv2: 1,
        }
    } else {
        exact_pll(cpu_hz)?
    };
    let actual_cpu_hz = pll_output(pll)?;
    if !legacy_rounding && actual_cpu_hz != cpu_hz {
        return Err(ClockError::NoExactPll);
    }
    if spi_max_hz > actual_cpu_hz / 2 {
        return Err(ClockError::SpiAbovePeripheralLimit);
    }
    let (cpsdvsr, scr) = spi_dividers(actual_cpu_hz, spi_max_hz)?;
    let denominator = u32::from(cpsdvsr)
        .checked_mul(u32::from(scr) + 1)
        .ok_or(ClockError::ArithmeticOverflow)?;
    let actual_spi_hz = actual_cpu_hz / denominator;
    let spi_neighbors = spi_neighbors(actual_cpu_hz, spi_max_hz)?;
    Ok(ResolvedClock {
        requested_cpu_hz: cpu_hz,
        actual_cpu_hz,
        requested_spi_hz: spi_max_hz,
        actual_spi_hz,
        pll,
        cpsdvsr,
        scr,
        voltage,
        legacy_rounding,
        spi_neighbors,
    })
}

/// Return an exact PLL tuple for `cpu_hz`, or `NoExactPll` without rounding.
///
/// Search order is reference divisor 1 then 2, descending feedback and descending
/// post-dividers. Checked products avoid a truncated quotient appearing exact.
fn exact_pll(cpu_hz: u32) -> Result<Pll, ClockError> {
    for refdiv in [1u8, 2] {
        let reference_hz = XOSC_HZ / u64::from(refdiv);
        if !(5_000_000..=800_000_000).contains(&reference_hz) {
            continue;
        }
        for fbdiv in (16u16..=320).rev() {
            let vco_hz = reference_hz
                .checked_mul(u64::from(fbdiv))
                .ok_or(ClockError::ArithmeticOverflow)?;
            if !(VCO_MIN_HZ..=VCO_MAX_HZ).contains(&vco_hz) {
                continue;
            }
            for postdiv1 in (1u8..=7).rev() {
                for postdiv2 in (1u8..=postdiv1).rev() {
                    let output_product = u64::from(cpu_hz)
                        .checked_mul(u64::from(refdiv))
                        .and_then(|v| v.checked_mul(u64::from(postdiv1)))
                        .and_then(|v| v.checked_mul(u64::from(postdiv2)))
                        .ok_or(ClockError::ArithmeticOverflow)?;
                    let oscillator_product = XOSC_HZ
                        .checked_mul(u64::from(fbdiv))
                        .ok_or(ClockError::ArithmeticOverflow)?;
                    if oscillator_product == output_product {
                        return Ok(Pll {
                            refdiv,
                            fbdiv,
                            postdiv1,
                            postdiv2,
                        });
                    }
                }
            }
        }
    }
    Err(ClockError::NoExactPll)
}

/// Validate `pll` and return integral output Hz; reject invalid VCO/divisors/fractions.
///
/// Restricts reference divisors to the resolver's 1/2 search domain. Checked
/// arithmetic/conversion failures return `ArithmeticOverflow`; never programs a PLL.
fn pll_output(pll: Pll) -> Result<u32, ClockError> {
    if !(1..=2).contains(&pll.refdiv)
        || !(16..=320).contains(&pll.fbdiv)
        || !(1..=7).contains(&pll.postdiv1)
        || !(1..=pll.postdiv1).contains(&pll.postdiv2)
    {
        return Err(ClockError::InvalidDivider);
    }
    let reference_hz = XOSC_HZ / u64::from(pll.refdiv);
    if reference_hz < 5_000_000 {
        return Err(ClockError::InvalidDivider);
    }
    let vco_hz = reference_hz
        .checked_mul(u64::from(pll.fbdiv))
        .ok_or(ClockError::ArithmeticOverflow)?;
    if !(VCO_MIN_HZ..=VCO_MAX_HZ).contains(&vco_hz) {
        return Err(ClockError::InvalidDivider);
    }
    let divisor = u64::from(pll.postdiv1)
        .checked_mul(u64::from(pll.postdiv2))
        .ok_or(ClockError::ArithmeticOverflow)?;
    if vco_hz % divisor != 0 {
        return Err(ClockError::InvalidDivider);
    }
    u32::try_from(vco_hz / divisor).map_err(|_| ClockError::ArithmeticOverflow)
}

/// Return the ceiling of `numerator` divided by `denominator`; a zero denominator is invalid.
///
/// Quotient-plus-remainder avoids overflow from the common add-before-divide formula.
fn ceil_div(
    numerator: u64,
    denominator: u64,
) -> Result<u64, ClockError> {
    if denominator == 0 {
        return Err(ClockError::InvalidDivider);
    }
    Ok(numerator / denominator + u64::from(!numerator.is_multiple_of(denominator)))
}

/// Return `(CPSDVSR, SCR)` for `clk_peri_hz` and maximum `requested_hz`, both Hz.
///
/// Mirrors vendored Embassy `calc_prescs`: round ratios upward so the rational
/// output never exceeds the request. Reject zero clocks/requests, exhausted
/// register range or overflow instead of truncating a divisor into a register.
fn spi_dividers(
    clk_peri_hz: u32,
    requested_hz: u32,
) -> Result<(u8, u8), ClockError> {
    let double_request = u64::from(requested_hz)
        .checked_mul(2)
        .ok_or(ClockError::ArithmeticOverflow)?;
    let ratio = ceil_div(u64::from(clk_peri_hz), double_request)?;
    if ratio == 0 {
        return Err(ClockError::InvalidDivider);
    }
    if ratio > 127 * 256 {
        return Err(ClockError::SpiTooLow);
    }
    let half_prescaler = ceil_div(ratio, 256)?;
    let postdiv = if half_prescaler == 1 {
        ratio
    } else {
        ceil_div(ratio, half_prescaler)?
    };
    let prescaler = half_prescaler.checked_mul(2).ok_or(ClockError::ArithmeticOverflow)?;
    if !(2..=254).contains(&prescaler) || !(1..=256).contains(&postdiv) {
        return Err(ClockError::InvalidDivider);
    }
    let denominator = prescaler.checked_mul(postdiv).ok_or(ClockError::ArithmeticOverflow)?;
    if u64::from(clk_peri_hz)
        > u64::from(requested_hz)
            .checked_mul(denominator)
            .ok_or(ClockError::ArithmeticOverflow)?
    {
        return Err(ClockError::InvalidDivider);
    }
    Ok((
        u8::try_from(prescaler).map_err(|_| ClockError::InvalidDivider)?,
        u8::try_from(postdiv - 1).map_err(|_| ClockError::InvalidDivider)?,
    ))
}

/// Return checked `CPSDVSR * (SCR + 1)` for `rate`; callers supply validated fields.
fn rate_denominator(rate: SpiRate) -> Result<u64, ClockError> {
    u64::from(rate.cpsdvsr)
        .checked_mul(u64::from(rate.scr) + 1)
        .ok_or(ClockError::ArithmeticOverflow)
}

/// Return whether `left` is faster than `right` under common `cpu_hz` clock Hz.
///
/// Cross-products preserve fractional ordering; displayed integer Hz can be equal.
/// Callers supply admitted divider rates; checked multiplication can return overflow.
fn rate_is_faster(
    cpu_hz: u32,
    left: SpiRate,
    right: SpiRate,
) -> Result<bool, ClockError> {
    let left_product = u64::from(cpu_hz)
        .checked_mul(rate_denominator(right)?)
        .ok_or(ClockError::ArithmeticOverflow)?;
    let right_product = u64::from(cpu_hz)
        .checked_mul(rate_denominator(left)?)
        .ok_or(ClockError::ArithmeticOverflow)?;
    Ok(left_product > right_product)
}

/// Return nearest reproducible SPI neighbors for `cpu_hz` and `requested_hz`, in Hz.
///
/// Enumerates at most 256 second divisors under public 1–75 MHz request limits.
/// Each witness is fed back through the HAL-matching selector before acceptance;
/// comparisons use rational rates, never the floored display Hz. Propagates
/// arithmetic/divider errors; absence on either side is represented by `None`.
fn spi_neighbors(
    cpu_hz: u32,
    requested_hz: u32,
) -> Result<SpiNeighbors, ClockError> {
    let mut neighbors = SpiNeighbors {
        below_or_equal: None,
        above: None,
    };
    let maximum_request = core::cmp::min(75_000_000, cpu_hz / 2);
    // Under the public 1 MHz minimum and 300 MHz CPU maximum, Embassy always
    // chooses CPSDVSR=2. Enumerate its possible SCR values, retaining only
    // those reproducible by an allowed integer request.
    for postdiv in 1u16..=256 {
        let denominator = u64::from(postdiv)
            .checked_mul(2)
            .ok_or(ClockError::ArithmeticOverflow)?;
        let witness = ceil_div(u64::from(cpu_hz), denominator)?.max(1_000_000);
        if witness > u64::from(maximum_request) {
            continue;
        }
        let request_hz = u32::try_from(witness).map_err(|_| ClockError::ArithmeticOverflow)?;
        let (cpsdvsr, scr) = spi_dividers(cpu_hz, request_hz)?;
        if cpsdvsr != 2 || u16::from(scr) + 1 != postdiv {
            continue;
        }
        let rate = SpiRate {
            request_hz,
            actual_hz: u32::try_from(u64::from(cpu_hz) / denominator).map_err(|_| ClockError::ArithmeticOverflow)?,
            cpsdvsr,
            scr,
        };
        let at_or_below = u64::from(cpu_hz)
            <= u64::from(requested_hz)
                .checked_mul(denominator)
                .ok_or(ClockError::ArithmeticOverflow)?;
        if at_or_below {
            let replace = match neighbors.below_or_equal {
                Some(previous) => rate_is_faster(cpu_hz, rate, previous)?,
                None => true,
            };
            if replace {
                neighbors.below_or_equal = Some(rate);
            }
        } else {
            let replace = match neighbors.above {
                Some(previous) => rate_is_faster(cpu_hz, previous, rate)?,
                None => true,
            };
            if replace {
                neighbors.above = Some(rate);
            }
        }
    }
    Ok(neighbors)
}
