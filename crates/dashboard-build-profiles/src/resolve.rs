use crate::ProfileId;
use crate::catalogue::profile;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum Voltage {
    V1_10,
    V1_15,
    V1_20,
    V1_25,
    V1_30,
}

impl Voltage {
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct CustomClock {
    pub cpu_hz: u32,
    pub spi_max_hz: u32,
    pub voltage: Voltage,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum ClockSelection {
    Named(ProfileId),
    Custom(CustomClock),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct Pll {
    pub refdiv: u8,
    pub fbdiv: u16,
    pub postdiv1: u8,
    pub postdiv2: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct SpiRate {
    /// An allowed integer request that reproduces these exact dividers.
    pub request_hz: u32,
    /// Truncated display value; compare rates using the divisors instead.
    pub actual_hz: u32,
    pub cpsdvsr: u8,
    pub scr: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct SpiNeighbors {
    pub below_or_equal: Option<SpiRate>,
    pub above: Option<SpiRate>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct ResolvedClock {
    pub requested_cpu_hz: u32,
    pub actual_cpu_hz: u32,
    pub requested_spi_hz: u32,
    pub actual_spi_hz: u32,
    pub pll: Pll,
    pub cpsdvsr: u8,
    pub scr: u8,
    pub voltage: Voltage,
    pub legacy_rounding: bool,
    pub spi_neighbors: SpiNeighbors,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum ClockError {
    CpuOutOfRange,
    SpiOutOfRange,
    SpiAbovePeripheralLimit,
    NoExactPll,
    SpiTooLow,
    ArithmeticOverflow,
    InvalidDivider,
}

const XOSC_HZ: u64 = 12_000_000;
const VCO_MIN_HZ: u64 = 750_000_000;
const VCO_MAX_HZ: u64 = 1_600_000_000;

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

fn ceil_div(
    numerator: u64,
    denominator: u64,
) -> Result<u64, ClockError> {
    if denominator == 0 {
        return Err(ClockError::InvalidDivider);
    }
    Ok(numerator / denominator + u64::from(!numerator.is_multiple_of(denominator)))
}

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

fn rate_denominator(rate: SpiRate) -> Result<u64, ClockError> {
    u64::from(rate.cpsdvsr)
        .checked_mul(u64::from(rate.scr) + 1)
        .ok_or(ClockError::ArithmeticOverflow)
}

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
