//! Boot-time clock tree configuration and cached frequency metadata in hertz.
//!
//! [`ClockConfig::crystal`] targets a 12 MHz board crystal by default;
//! [`ClockConfig::system_freq`] searches PLL ratios and may return a nearby rate.
//! Reported frequencies are calculated, not measured. ROSC rates are caller estimates;
//! atomics publish individual clocks, not a coherent runtime reconfiguration snapshot.
//! Initialization is one-shot and may leave hardware partially configured on panic.
//! Voltage/frequency choices do not establish electrical or thermal qualification.
//! See the [SDK PLL sequence](https://github.com/raspberrypi/pico-sdk/blob/master/src/rp2_common/hardware_pll/pll.c)
//! and [RP2350 clock registers](https://github.com/raspberrypi/pico-sdk/blob/master/src/rp2350/hardware_regs/include/hardware/regs/clocks.h).

use core::arch::asm;
use core::marker::PhantomData;
#[cfg(feature = "rp2040")]
use core::sync::atomic::AtomicU16;
use core::sync::atomic::{AtomicU32, Ordering};

use pac::clocks::vals::*;

use crate::gpio::{AnyPin, SealedPin};
use crate::pac::common::{RW, Reg};
use crate::{Peri, pac, reset};

// VCO range from 750 MHz to 1600 MHz (RP2040 datasheet 20 February 2025 2.18.1, RP2350 datasheet 29 July 2025 8.6.1)
/// Minimum admitted PLL VCO rate in hertz.
static VCO_MIN: u32 = 750_000_000;
/// Maximum admitted PLL VCO rate in hertz.
static VCO_MAX: u32 = 1_600_000_000;

// NOTE: all gpin handling is commented out for future reference.
// gpin is not usually safe to use during the boot init() call, so it won't
// be very useful until we have runtime clock reconfiguration. once this
// happens we can resurrect the commented-out gpin bits.

/// Clock error types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum ClockError {
    /// PLL failed to lock within the timeout period.
    PllLockTimedOut,
    /// Could not find valid PLL parameters for system clock.
    InvalidPllParameters,
    /// Reading the core voltage failed due to an unexpected value in the register.
    UnexpectedCoreVoltageRead,
}

/// Independently published boot-time rates; zero means absent or not yet initialized.
struct Clocks {
    /// Crystal oscillator frequency in hertz; zero before initialization or when omitted.
    xosc: AtomicU32,
    /// System clock frequency in hertz; zero before initialization or when omitted.
    sys: AtomicU32,
    /// Reference clock frequency in hertz; zero before initialization or when omitted.
    reference: AtomicU32,
    /// System PLL frequency in hertz; zero before initialization or when omitted.
    pll_sys: AtomicU32,
    /// USB PLL frequency in hertz; zero before initialization or when omitted.
    pll_usb: AtomicU32,
    /// USB peripheral clock frequency in hertz; zero before initialization or when omitted.
    usb: AtomicU32,
    /// ADC peripheral clock frequency in hertz; zero before initialization or when omitted.
    adc: AtomicU32,
    // See above re gpin handling being commented out
    // gpin0: AtomicU32,
    // gpin1: AtomicU32,
    /// Estimated ring oscillator frequency in hertz; zero before initialization or when omitted.
    rosc: AtomicU32,
    /// UART/SPI peripheral clock frequency in hertz; zero before initialization or when omitted.
    peri: AtomicU32,
    #[cfg(feature = "rp2040")]
    /// RTC clock admitted to 16 bits frequency in hertz; zero before initialization or when omitted.
    rtc: AtomicU16,
}

/// Global calculated rates; relaxed loads require completed startup for a coherent tree.
static CLOCKS: Clocks = Clocks {
    xosc: AtomicU32::new(0),
    sys: AtomicU32::new(0),
    reference: AtomicU32::new(0),
    pll_sys: AtomicU32::new(0),
    pll_usb: AtomicU32::new(0),
    usb: AtomicU32::new(0),
    adc: AtomicU32::new(0),
    // See above re gpin handling being commented out
    // gpin0: AtomicU32::new(0),
    // gpin1: AtomicU32::new(0),
    rosc: AtomicU32::new(0),
    peri: AtomicU32::new(0),
    #[cfg(feature = "rp2040")]
    rtc: AtomicU16::new(0),
};

/// Peripheral clock sources.
#[repr(u8)]
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PeriClkSrc {
    /// SYS.
    Sys = ClkPeriCtrlAuxsrc::CLK_SYS as _,
    /// PLL SYS.
    PllSys = ClkPeriCtrlAuxsrc::CLKSRC_PLL_SYS as _,
    /// PLL USB.
    PllUsb = ClkPeriCtrlAuxsrc::CLKSRC_PLL_USB as _,
    /// ROSC.
    Rosc = ClkPeriCtrlAuxsrc::ROSC_CLKSRC_PH as _,
    /// XOSC.
    Xosc = ClkPeriCtrlAuxsrc::XOSC_CLKSRC as _,
    // See above re gpin handling being commented out
    // Gpin0 = ClkPeriCtrlAuxsrc::CLKSRC_GPIN0 as _ ,
    // Gpin1 = ClkPeriCtrlAuxsrc::CLKSRC_GPIN1 as _ ,
}

/// Core voltage regulator settings.
///
/// The voltage regulator can be configured for different output voltages.
/// Higher voltages allow for higher clock frequencies but increase power consumption and heat.
#[cfg(feature = "rp2040")]
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum CoreVoltage {
    /// 0.80V
    V0_80 = 0b0000,
    /// 0.85V
    V0_85 = 0b0110,
    /// 0.90V
    V0_90 = 0b0111,
    /// 0.95V
    V0_95 = 0b1000,
    /// 1.00V
    V1_00 = 0b1001,
    /// 1.05V
    V1_05 = 0b1010,
    /// 1.10V - Default voltage level
    V1_10 = 0b1011,
    /// 1.15 V regulator selector; selecting it does not qualify an overclock.
    V1_15 = 0b1100,
    /// 1.20V
    V1_20 = 0b1101,
    /// 1.25V
    V1_25 = 0b1110,
    /// 1.30V
    V1_30 = 0b1111,
}

/// Core voltage regulator settings.
///
/// The voltage regulator can be configured for different output voltages.
/// Higher voltages allow for higher clock frequencies but increase power consumption and heat.
///
/// **Note**: The maximum voltage is 1.30V, unless unlocked by setting unless the voltage limit
/// is disabled using the disable_voltage_limit field in the vreg_ctrl register. For lack of practical use at this
/// point in time, this is not implemented here. So the maximum voltage in this enum is 1.30V for now.
#[cfg(feature = "_rp235x")]
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum CoreVoltage {
    /// 0.55V
    V0_55 = 0b00000,
    /// 0.60V
    V0_60 = 0b00001,
    /// 0.65V
    V0_65 = 0b00010,
    /// 0.70V
    V0_70 = 0b00011,
    /// 0.75V
    V0_75 = 0b00100,
    /// 0.80V
    V0_80 = 0b00101,
    /// 0.85V
    V0_85 = 0b00110,
    /// 0.90V
    V0_90 = 0b00111,
    /// 0.95V
    V0_95 = 0b01000,
    /// 1.00V
    V1_00 = 0b01001,
    /// 1.05V
    V1_05 = 0b01010,
    /// 1.10V - Default voltage level
    V1_10 = 0b01011,
    /// 1.15V
    V1_15 = 0b01100,
    /// 1.20V
    V1_20 = 0b01101,
    /// 1.25V
    V1_25 = 0b01110,
    /// 1.30V
    V1_30 = 0b01111,
}

impl CoreVoltage {
    /// Returns the policy BOD selector for this voltage; does not write hardware.
    /// Ratios vary by chip/voltage (approximately 72-94% on RP235x).
    fn recommended_bod(self) -> u8 {
        #[cfg(feature = "rp2040")]
        match self {
            CoreVoltage::V0_80 => 0b0100, // 0.645V (~81% of 0.80V)
            CoreVoltage::V0_85 => 0b0101, // 0.688V (~81% of 0.85V)
            CoreVoltage::V0_90 => 0b0110, // 0.731V (~81% of 0.90V)
            CoreVoltage::V0_95 => 0b0111, // 0.774V (~81% of 0.95V)
            CoreVoltage::V1_00 => 0b1000, // 0.817V (~82% of 1.00V)
            CoreVoltage::V1_05 => 0b1000, // 0.817V (~78% of 1.05V)
            CoreVoltage::V1_10 => 0b1001, // 0.860V (~78% of 1.10V), the default
            CoreVoltage::V1_15 => 0b1010, // 0.903V (~79% of 1.15V)
            CoreVoltage::V1_20 => 0b1011, // 0.946V (~79% of 1.20V)
            CoreVoltage::V1_25 => 0b1100, // 0.989V (~79% of 1.25V)
            CoreVoltage::V1_30 => 0b1101, // 1.032V (~79% of 1.30V)
        }
        #[cfg(feature = "_rp235x")]
        match self {
            CoreVoltage::V0_55 => 0b00001, // 0.516V (~94% of 0.55V)
            CoreVoltage::V0_60 => 0b00010, // 0.559V (~93% of 0.60V)
            CoreVoltage::V0_65 => 0b00011, // 0.602V (~93% of 0.65V)
            CoreVoltage::V0_70 => 0b00011, // 0.602V (~86% of 0.70V)
            CoreVoltage::V0_75 => 0b00100, // 0.645V (~86% of 0.75V)
            CoreVoltage::V0_80 => 0b00101, // 0.688V (~86% of 0.80V)
            CoreVoltage::V0_85 => 0b00110, // 0.731V (~86% of 0.85V)
            CoreVoltage::V0_90 => 0b00110, // 0.731V (~81% of 0.90V)
            CoreVoltage::V0_95 => 0b00111, // 0.774V (~81% of 0.95V)
            CoreVoltage::V1_00 => 0b01000, // 0.817V (~82% of 1.00V)
            CoreVoltage::V1_05 => 0b01000, // 0.817V (~78% of 1.05V)
            CoreVoltage::V1_10 => 0b01001, // 0.860V (~78% of 1.10V), the default
            CoreVoltage::V1_15 => 0b01001, // 0.860V (~75% of 1.15V)
            CoreVoltage::V1_20 => 0b01010, // 0.903V (~75% of 1.20V)
            CoreVoltage::V1_25 => 0b01010, // 0.903V (~72% of 1.25V)
            CoreVoltage::V1_30 => 0b01011, /* 0.946V (~73% of 1.30V)
                                             * all others: 0.946V (see CoreVoltage: we do not support setting
                                             * Voltages higher than 1.30V at this point) */
        }
    }
}

/// Clock configuration.
#[non_exhaustive]
pub struct ClockConfig {
    /// Ring oscillator setup/estimated rate; `None` leaves it unconfigured and records zero.
    pub rosc: Option<RoscConfig>,
    /// Crystal and PLL setup; `None` records all three frequencies as zero.
    pub xosc: Option<XoscConfig>,
    /// Reference clock configuration.
    pub ref_clk: RefClkConfig,
    /// System clock configuration.
    pub sys_clk: SysClkConfig,
    /// UART/SPI clock source; `None` leaves those peripherals in reset.
    pub peri_clk_src: Option<PeriClkSrc>,
    /// USB clock setup; `None` leaves USB in reset.
    pub usb_clk: Option<UsbClkConfig>,
    /// ADC clock setup; `None` leaves ADC in reset.
    pub adc_clk: Option<AdcClkConfig>,
    /// RTC clock setup; `None` leaves RTC in reset.
    #[cfg(feature = "rp2040")]
    pub rtc_clk: Option<RtcClkConfig>,
    /// Core voltage scaling. Defaults to 1.10V.
    pub core_voltage: CoreVoltage,
    /// Voltage stabilization delay in microseconds.
    /// `None`: 1000 us at 1.15 V, 2000 us at 1.20-1.30 V, otherwise zero.
    /// Applied only when voltage changes; cycles use the configured ROSC estimate.
    pub voltage_stabilization_delay_us: Option<u32>,
    // See above re gpin handling being commented out
    // gpin0: Option<(u32, Gpin<'static, AnyPin>)>,
    // gpin1: Option<(u32, Gpin<'static, AnyPin>)>,
}

impl Default for ClockConfig {
    /// Returns an incomplete manual template: no oscillator is configured, although
    /// reference/system sources select ROSC. Fill the source configuration before
    /// `init`, otherwise its nonzero reference-frequency assertion panics.
    fn default() -> Self {
        Self {
            rosc: None,
            xosc: None,
            ref_clk: RefClkConfig {
                src: RefClkSrc::Rosc,
                div: 1,
            },
            sys_clk: SysClkConfig {
                src: SysClkSrc::Rosc,
                div_int: 1,
                div_frac: 0,
            },
            peri_clk_src: None,
            usb_clk: None,
            adc_clk: None,
            #[cfg(feature = "rp2040")]
            rtc_clk: None,
            core_voltage: CoreVoltage::V1_10,
            voltage_stabilization_delay_us: None,
            // See above re gpin handling being commented out
            // gpin0: None,
            // gpin1: None,
        }
    }
}

impl ClockConfig {
    /// Returns a tree based on `crystal_hz`, without validating or touching hardware.
    /// Fixed PLL ratios yield 125 MHz (RP2040) or 150 MHz (RP235x) system and 48 MHz
    /// USB/ADC only with a 12 MHz crystal. Other crystals require reviewing all ratios.
    pub fn crystal(crystal_hz: u32) -> Self {
        Self {
            rosc: Some(RoscConfig {
                hz: 6_500_000,
                range: RoscRange::Medium,
                drive_strength: [0; 8],
                div: 16,
            }),
            xosc: Some(XoscConfig {
                hz: crystal_hz,
                sys_pll: Some(PllConfig {
                    refdiv: 1,
                    fbdiv: 125,
                    #[cfg(feature = "rp2040")]
                    post_div1: 6,
                    #[cfg(feature = "_rp235x")]
                    post_div1: 5,
                    post_div2: 2,
                }),
                usb_pll: Some(PllConfig {
                    refdiv: 1,
                    fbdiv: 120,
                    post_div1: 6,
                    post_div2: 5,
                }),
                delay_multiplier: 128,
            }),
            ref_clk: RefClkConfig {
                src: RefClkSrc::Xosc,
                div: 1,
            },
            sys_clk: SysClkConfig {
                src: SysClkSrc::PllSys,
                div_int: 1,
                div_frac: 0,
            },
            peri_clk_src: Some(PeriClkSrc::Sys),
            // CLK USB = PLL USB (48MHz) / 1 = 48MHz
            usb_clk: Some(UsbClkConfig {
                src: UsbClkSrc::PllUsb,
                div: 1,
                phase: 0,
            }),
            // CLK ADC = PLL USB (48MHZ) / 1 = 48MHz
            adc_clk: Some(AdcClkConfig {
                src: AdcClkSrc::PllUsb,
                div: 1,
                phase: 0,
            }),
            // CLK RTC = PLL USB (48MHz) / 1024 = 46875Hz
            #[cfg(feature = "rp2040")]
            rtc_clk: Some(RtcClkConfig {
                src: RtcClkSrc::PllUsb,
                div_int: 1024,
                div_frac: 0,
                phase: 0,
            }),
            core_voltage: CoreVoltage::V1_10, // Use hardware default (1.10V)
            voltage_stabilization_delay_us: None,
            // See above re gpin handling being commented out
            // gpin0: None,
            // gpin1: None,
        }
    }

    /// Returns a ROSC-only tree estimating 140 MHz system/peripheral and 46.7 MHz ADC.
    /// USB is omitted; the estimate is uncalibrated and is not a guaranteed silicon rate.
    pub fn rosc() -> Self {
        Self {
            rosc: Some(RoscConfig {
                hz: 140_000_000,
                range: RoscRange::High,
                drive_strength: [0; 8],
                div: 1,
            }),
            xosc: None,
            ref_clk: RefClkConfig {
                src: RefClkSrc::Rosc,
                div: 1,
            },
            sys_clk: SysClkConfig {
                src: SysClkSrc::Rosc,
                div_int: 1,
                div_frac: 0,
            },
            peri_clk_src: Some(PeriClkSrc::Rosc),
            usb_clk: None,
            // CLK ADC = ROSC (140MHz) / 3 ≅ 48MHz
            adc_clk: Some(AdcClkConfig {
                src: AdcClkSrc::Rosc,
                div: 3,
                phase: 0,
            }),
            // CLK RTC = ROSC (140MHz) / 2986.667969 ≅ 46875Hz
            #[cfg(feature = "rp2040")]
            rtc_clk: Some(RtcClkConfig {
                src: RtcClkSrc::Rosc,
                div_int: 2986,
                div_frac: 171,
                phase: 0,
            }),
            core_voltage: CoreVoltage::V1_10, // Use hardware default (1.10V)
            voltage_stabilization_delay_us: None,
            // See above re gpin handling being commented out
            // gpin0: None,
            // gpin1: None,
        }
    }

    /// Returns a 12 MHz crystal tree targeting `hz`; nearest achievable PLL rate is
    /// accepted without a tolerance bound. `Err(InvalidPllParameters)` means no candidate.
    /// RP2040 selects 1.15 V above requested 133 MHz, otherwise 1.10 V; RP235x stays
    /// at 1.10 V. This policy does not validate overclock safety or every peripheral rate.
    pub fn system_freq(hz: u32) -> Result<Self, ClockError> {
        // Start with the standard configuration from crystal()
        /// Board crystal assumed by automatic PLL search, in hertz.
        const DEFAULT_CRYSTAL_HZ: u32 = 12_000_000;
        let mut config = Self::crystal(DEFAULT_CRYSTAL_HZ);

        // No need to modify anything if target frequency is already 125MHz
        // (which is what crystal() configures by default)
        #[cfg(feature = "rp2040")]
        if hz == 125_000_000 {
            return Ok(config);
        }
        #[cfg(feature = "_rp235x")]
        if hz == 150_000_000 {
            return Ok(config);
        }

        // Find optimal PLL parameters for the requested frequency
        let sys_pll_params = find_pll_params(DEFAULT_CRYSTAL_HZ, hz).ok_or(ClockError::InvalidPllParameters)?;

        // Replace the sys_pll configuration with our custom parameters
        if let Some(xosc) = &mut config.xosc {
            xosc.sys_pll = Some(sys_pll_params);
        }

        // Set the voltage scale based on the target frequency
        // Higher frequencies require higher voltage
        #[cfg(feature = "rp2040")]
        {
            config.core_voltage = match hz {
                freq if freq > 133_000_000 => CoreVoltage::V1_15,
                _ => CoreVoltage::V1_10, // Use default voltage (V1_10)
            };
        }
        #[cfg(feature = "_rp235x")]
        {
            config.core_voltage = match hz {
                // There is no official support for running the chip on other core voltages and/or other clock speeds
                // than the defaults. So for now we have not way of knowing what the voltage should be.
                // Change this if the manufacturer provides more information.
                _ => CoreVoltage::V1_10, // Use default voltage (V1_10)
            };
        }

        Ok(config)
    }

    /// Returns a crystal tree using `xosc_hz`, system `pll_config` and `core_voltage`.
    /// Panics if system PLL ratios are invalid. USB PLL retains fixed ratios for a
    /// 12 MHz crystal; a different input needs separate USB/ADC/RTC review. No MMIO.
    #[cfg(feature = "rp2040")]
    pub fn manual_pll(
        xosc_hz: u32,
        pll_config: PllConfig,
        core_voltage: CoreVoltage,
    ) -> Self {
        // Validate PLL parameters
        assert!(pll_config.is_valid(xosc_hz), "Invalid PLL parameters");

        let mut config = Self::default();

        config.xosc = Some(XoscConfig {
            hz: xosc_hz,
            sys_pll: Some(pll_config),
            usb_pll: Some(PllConfig {
                refdiv: 1,
                fbdiv: 120,
                post_div1: 6,
                post_div2: 5,
            }),
            delay_multiplier: 128,
        });

        config.ref_clk = RefClkConfig {
            src: RefClkSrc::Xosc,
            div: 1,
        };

        config.sys_clk = SysClkConfig {
            src: SysClkSrc::PllSys,
            div_int: 1,
            div_frac: 0,
        };

        config.core_voltage = core_voltage;
        config.peri_clk_src = Some(PeriClkSrc::Sys);

        // Set reasonable defaults for other clocks
        config.usb_clk = Some(UsbClkConfig {
            src: UsbClkSrc::PllUsb,
            div: 1,
            phase: 0,
        });

        config.adc_clk = Some(AdcClkConfig {
            src: AdcClkSrc::PllUsb,
            div: 1,
            phase: 0,
        });

        config.rtc_clk = Some(RtcClkConfig {
            src: RtcClkSrc::PllUsb,
            div_int: 1024,
            div_frac: 0,
            phase: 0,
        });

        config
    }
}

/// ROSC freq range.
#[repr(u16)]
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum RoscRange {
    /// Low range.
    Low = pac::rosc::vals::FreqRange::LOW.0,
    /// Medium range (1.33x low)
    Medium = pac::rosc::vals::FreqRange::MEDIUM.0,
    /// High range (2x low)
    High = pac::rosc::vals::FreqRange::HIGH.0,
    /// Too high. Should not be used.
    TooHigh = pac::rosc::vals::FreqRange::TOOHIGH.0,
}

/// On-chip ring oscillator configuration.
pub struct RoscConfig {
    /// Final frequency of the oscillator, after the divider has been applied.
    /// The oscillator has a nominal frequency of 6.5MHz at medium range with
    /// divider 16 and all drive strengths set to 0, other values should be
    /// measured in situ.
    pub hz: u32,
    /// Oscillator range.
    pub range: RoscRange,
    /// Eight oscillator stage drive selectors, each encoded in three bits.
    pub drive_strength: [u8; 8],
    /// Encoded oscillator divider; 0 denotes divide-by-32, otherwise 1..=31.
    pub div: u16,
}

/// Crystal oscillator configuration.
pub struct XoscConfig {
    /// Input crystal frequency in hertz (used without measurement).
    pub hz: u32,
    /// System PLL ratios; `None` leaves the reset PLL unavailable.
    pub sys_pll: Option<PllConfig>,
    /// USB PLL ratios; `None` leaves the reset PLL unavailable.
    pub usb_pll: Option<PllConfig>,
    /// Startup delay multiplier: register cycles are `(hz / 1000 * multiplier + 128) / 256`.
    /// Must fit intermediate arithmetic and the hardware delay field; no range check.
    pub delay_multiplier: u32,
}

/// PLL configuration.
#[derive(Clone, Copy, Debug)]
pub struct PllConfig {
    /// Input reference divisor, 1..=63.
    pub refdiv: u8,
    /// VCO feedback multiplier, 16..=320.
    pub fbdiv: u16,
    /// First VCO output divisor, 1..=7 and at least `post_div2`.
    pub post_div1: u8,
    /// Second output divisor, 1..=7 and at most `post_div1`.
    pub post_div2: u8,
}

impl PllConfig {
    /// Returns integer output hertz for `input_hz`, truncating reference division first.
    ///
    /// # Panics
    /// Panics if [`Self::is_valid`] rejects the configuration.
    pub fn output_frequency(
        &self,
        input_hz: u32,
    ) -> u32 {
        assert!(self.is_valid(input_hz), "invalid PLL configuration");
        let ref_freq = input_hz / self.refdiv as u32;
        let vco_freq = ref_freq * self.fbdiv as u32;
        vco_freq / ((self.post_div1 * self.post_div2) as u32)
    }

    /// Returns whether `input_hz` and ratios satisfy divisor/reference/VCO bounds.
    /// Uses wide VCO arithmetic; does not establish achievable physical operation.
    pub fn is_valid(
        &self,
        input_hz: u32,
    ) -> bool {
        // Check divisor constraints
        if self.refdiv < 1 || self.refdiv > 63 {
            return false;
        }
        if self.fbdiv < 16 || self.fbdiv > 320 {
            return false;
        }
        if self.post_div1 < 1 || self.post_div1 > 7 {
            return false;
        }
        if self.post_div2 < 1 || self.post_div2 > 7 {
            return false;
        }
        if self.post_div2 > self.post_div1 {
            return false;
        }

        // Calculate reference frequency
        let ref_freq = input_hz / self.refdiv as u32;

        // Check reference frequency range
        if ref_freq < 5_000_000 || ref_freq > 800_000_000 {
            return false;
        }

        // Calculate VCO frequency
        let vco_freq = ref_freq as u64 * self.fbdiv as u64;

        // Check before narrowing: extreme but well-shaped inputs can overflow u32.
        vco_freq >= VCO_MIN as u64 && vco_freq <= VCO_MAX as u64
    }
}

/// Reference clock config.
pub struct RefClkConfig {
    /// Reference clock source.
    pub src: RefClkSrc,
    /// Integer divider 1..=4; initialization panics outside this range.
    pub div: u8,
}

/// Reference clock source.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum RefClkSrc {
    /// XOSC.
    Xosc,
    /// ROSC.
    Rosc,
    /// PLL USB.
    PllUsb,
    // See above re gpin handling being commented out
    // Gpin0,
    // Gpin1,
}

/// SYS clock source.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum SysClkSrc {
    /// REF.
    Ref,
    /// PLL SYS.
    PllSys,
    /// PLL USB.
    PllUsb,
    /// ROSC.
    Rosc,
    /// XOSC.
    Xosc,
    // See above re gpin handling being commented out
    // Gpin0,
    // Gpin1,
}

/// SYS clock config.
pub struct SysClkConfig {
    /// SYS clock source.
    pub src: SysClkSrc,
    /// Encoded integer divider; zero means the maximum field value plus one.
    #[cfg(feature = "rp2040")]
    pub div_int: u32,
    /// Fractional divider bits: /256 on RP2040, /65536 on RP235x.
    #[cfg(feature = "rp2040")]
    pub div_frac: u8,
    /// Encoded integer divider; zero means the maximum field value plus one.
    #[cfg(feature = "_rp235x")]
    pub div_int: u16,
    /// Fractional divider bits: /256 on RP2040, /65536 on RP235x.
    #[cfg(feature = "_rp235x")]
    pub div_frac: u16,
}

/// USB clock source.
#[repr(u8)]
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum UsbClkSrc {
    /// PLL USB.
    PllUsb = ClkUsbCtrlAuxsrc::CLKSRC_PLL_USB as _,
    /// PLL SYS.
    PllSys = ClkUsbCtrlAuxsrc::CLKSRC_PLL_SYS as _,
    /// ROSC.
    Rosc = ClkUsbCtrlAuxsrc::ROSC_CLKSRC_PH as _,
    /// XOSC.
    Xosc = ClkUsbCtrlAuxsrc::XOSC_CLKSRC as _,
    // See above re gpin handling being commented out
    // Gpin0 = ClkUsbCtrlAuxsrc::CLKSRC_GPIN0 as _ ,
    // Gpin1 = ClkUsbCtrlAuxsrc::CLKSRC_GPIN1 as _ ,
}

/// USB clock config.
pub struct UsbClkConfig {
    /// USB clock source.
    pub src: UsbClkSrc,
    /// Integer divider 1..=4; initialization panics outside this range.
    pub div: u8,
    /// Initial enable delay in input-clock cycles, 0..=3.
    pub phase: u8,
}

/// ADC clock source.
#[repr(u8)]
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum AdcClkSrc {
    /// PLL USB.
    PllUsb = ClkAdcCtrlAuxsrc::CLKSRC_PLL_USB as _,
    /// PLL SYS.
    PllSys = ClkAdcCtrlAuxsrc::CLKSRC_PLL_SYS as _,
    /// ROSC.
    Rosc = ClkAdcCtrlAuxsrc::ROSC_CLKSRC_PH as _,
    /// XOSC.
    Xosc = ClkAdcCtrlAuxsrc::XOSC_CLKSRC as _,
    // See above re gpin handling being commented out
    // Gpin0 = ClkAdcCtrlAuxsrc::CLKSRC_GPIN0 as _ ,
    // Gpin1 = ClkAdcCtrlAuxsrc::CLKSRC_GPIN1 as _ ,
}

/// ADC clock config.
pub struct AdcClkConfig {
    /// ADC clock source.
    pub src: AdcClkSrc,
    /// Integer divider 1..=4; initialization panics outside this range.
    pub div: u8,
    /// Initial enable delay in input-clock cycles, 0..=3.
    pub phase: u8,
}

/// RTC clock source.
#[repr(u8)]
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[cfg(feature = "rp2040")]
pub enum RtcClkSrc {
    /// PLL USB.
    PllUsb = ClkRtcCtrlAuxsrc::CLKSRC_PLL_USB as _,
    /// PLL SYS.
    PllSys = ClkRtcCtrlAuxsrc::CLKSRC_PLL_SYS as _,
    /// ROSC.
    Rosc = ClkRtcCtrlAuxsrc::ROSC_CLKSRC_PH as _,
    /// XOSC.
    Xosc = ClkRtcCtrlAuxsrc::XOSC_CLKSRC as _,
    // See above re gpin handling being commented out
    // Gpin0 = ClkRtcCtrlAuxsrc::CLKSRC_GPIN0 as _ ,
    // Gpin1 = ClkRtcCtrlAuxsrc::CLKSRC_GPIN1 as _ ,
}

/// RTC clock config.
#[cfg(feature = "rp2040")]
pub struct RtcClkConfig {
    /// RTC clock source.
    pub src: RtcClkSrc,
    /// Raw 24-bit integer divider; zero encodes 65536. Resulting whole hertz must be 1..=65535.
    pub div_int: u32,
    /// Fractional divider in units of 1/256.
    pub div_frac: u8,
    /// Initial enable delay in input-clock cycles, 0..=3.
    pub phase: u8,
}

/// Returns PLL ratios for `input_hz` and desired `target_hz`, or `None` without a VCO candidate.
/// Searches fixed refdiv=1, descending feedback/postdiv ratios: exact integer rate
/// first, then minimum absolute integer-hertz error without a tolerance limit.
/// Ties retain the first candidate; no hardware or voltage validation is performed.
fn find_pll_params(
    input_hz: u32,
    target_hz: u32,
) -> Option<PllConfig> {
    // Fixed reference divider for system PLL
    /// Fixed reference divider used by this search.
    const PLL_SYS_REFDIV: u8 = 1;

    // Calculate reference frequency
    let reference_freq = input_hz as u64 / PLL_SYS_REFDIV as u64;

    // Start from highest fbdiv for better stability (like SDK does)
    for fbdiv in (16..=320).rev() {
        let vco_freq = reference_freq * fbdiv;

        // Check VCO frequency is within valid range
        if vco_freq < VCO_MIN as u64 || vco_freq > VCO_MAX as u64 {
            continue;
        }

        // Try all possible postdiv combinations starting from larger values
        // (more conservative/stable approach)
        for post_div1 in (1..=7).rev() {
            for post_div2 in (1..=post_div1).rev() {
                let out_freq = vco_freq / (post_div1 * post_div2);

                // Check if we get the exact target frequency without remainder
                if out_freq == target_hz as u64 && (vco_freq % (post_div1 * post_div2) == 0) {
                    return Some(PllConfig {
                        refdiv: PLL_SYS_REFDIV,
                        fbdiv: fbdiv as u16,
                        post_div1: post_div1 as u8,
                        post_div2: post_div2 as u8,
                    });
                }
            }
        }
    }

    // If we couldn't find an exact match, find the closest match
    let mut best_config = None;
    let mut min_diff = u32::MAX;

    for fbdiv in (16..=320).rev() {
        let vco_freq = reference_freq * fbdiv;

        if vco_freq < VCO_MIN as u64 || vco_freq > VCO_MAX as u64 {
            continue;
        }

        for post_div1 in (1..=7).rev() {
            for post_div2 in (1..=post_div1).rev() {
                let out_freq = (vco_freq / (post_div1 * post_div2) as u64) as u32;
                let diff = if out_freq > target_hz {
                    out_freq - target_hz
                } else {
                    target_hz - out_freq
                };

                // If this is closer to the target, save it
                if diff < min_diff {
                    min_diff = diff;
                    best_config = Some(PllConfig {
                        refdiv: PLL_SYS_REFDIV,
                        fbdiv: fbdiv as u16,
                        post_div1: post_div1 as u8,
                        post_div2: post_div2 as u8,
                    });
                }
            }
        }
    }

    // Return the closest match if we found one
    best_config
}

/// Applies `config`, resets dependent peripherals and publishes calculated clock rates.
/// Panics on missing selected sources, invalid ratios/dividers or PLL timeout; no rollback.
/// XOSC/mux/reset waits are unbounded. PLL lock limit counts one million polls, not time.
/// Invalid delay arithmetic can overflow; phase/ROSC fields are not fully validated.
///
/// # Safety
/// Call exactly once during boot, before drivers/other cores/IRQs depend on these clocks.
/// Hardware crystal, voltage and clock limits must match `config`; cached rates are estimates.
pub(crate) unsafe fn init(config: ClockConfig) {
    // Reset everything except:
    // - QSPI (we're using it to run this code!)
    // - PLLs (it may be suicide if that's what's clocking us)
    // - USB, SYSCFG (breaks usb-to-swd on core1)
    // - RTC (else there would be no more time...)
    let mut peris = reset::ALL_PERIPHERALS;
    peris.set_io_qspi(false);
    // peris.set_io_bank0(false); // might be suicide if we're clocked from gpin
    peris.set_pads_qspi(false);
    peris.set_pll_sys(false);
    peris.set_pll_usb(false);
    peris.set_usbctrl(false);
    peris.set_syscfg(false);
    // peris.set_rtc(false);
    reset::reset(peris);

    // Disable resus that may be enabled from previous software
    let c = pac::CLOCKS;
    c.clk_sys_resus_ctrl()
        .write_value(pac::clocks::regs::ClkSysResusCtrl(0));

    // Before we touch PLLs, switch sys and ref cleanly away from their aux sources.
    c.clk_sys_ctrl().modify(|w| w.set_src(ClkSysCtrlSrc::CLK_REF));
    #[cfg(feature = "rp2040")]
    while c.clk_sys_selected().read() != 1 {}
    #[cfg(feature = "_rp235x")]
    while c.clk_sys_selected().read() != pac::clocks::regs::ClkSysSelected(1) {}
    c.clk_ref_ctrl().modify(|w| w.set_src(ClkRefCtrlSrc::ROSC_CLKSRC_PH));
    #[cfg(feature = "rp2040")]
    while c.clk_ref_selected().read() != 1 {}
    #[cfg(feature = "_rp235x")]
    while c.clk_ref_selected().read() != pac::clocks::regs::ClkRefSelected(1) {}

    // Reset the PLLs
    let mut peris = reset::Peripherals(0);
    peris.set_pll_sys(true);
    peris.set_pll_usb(true);
    reset::reset(peris);
    reset::unreset_wait(peris);

    // See above re gpin handling being commented out
    // let gpin0_freq = config.gpin0.map_or(0, |p| {
    //     core::mem::forget(p.1);
    //     p.0
    // });
    // CLOCKS.gpin0.store(gpin0_freq, Ordering::Relaxed);
    // let gpin1_freq = config.gpin1.map_or(0, |p| {
    //     core::mem::forget(p.1);
    //     p.0
    // });
    // CLOCKS.gpin1.store(gpin1_freq, Ordering::Relaxed);

    let rosc_freq = match config.rosc {
        Some(config) => configure_rosc(config),
        None => 0,
    };
    CLOCKS.rosc.store(rosc_freq, Ordering::Relaxed);

    // Set Core Voltage, if we have config for it and we're not using the default
    {
        let voltage = config.core_voltage;

        #[cfg(feature = "rp2040")]
        let vreg = pac::VREG_AND_CHIP_RESET;
        #[cfg(feature = "_rp235x")]
        let vreg = pac::POWMAN;

        let current_vsel = vreg.vreg().read().vsel();
        let target_vsel = voltage as u8;

        // If the target voltage is different from the current one, we need to change it
        if target_vsel != current_vsel {
            // Set the voltage regulator to the target voltage
            #[cfg(feature = "rp2040")]
            vreg.vreg().modify(|w| w.set_vsel(target_vsel));
            #[cfg(feature = "_rp235x")]
            // For rp235x changes to the voltage regulator are protected by a password, see datasheet section 6.4 Power
            // Management (POWMAN) Registers The password is "5AFE" (0x5AFE), it must be set in the top 16
            // bits of the register
            vreg.vreg().modify(|w| {
                w.0 = (w.0 & 0x0000FFFF) | (0x5AFE << 16); // Set the password
                w.set_vsel(target_vsel);
                *w
            });

            // Wait for the voltage to stabilize. Use the provided delay or default based on voltage
            let settling_time_us = config.voltage_stabilization_delay_us.unwrap_or_else(|| {
                match voltage {
                    CoreVoltage::V1_15 => 1000,                                           // 1ms for 1.15V
                    CoreVoltage::V1_20 | CoreVoltage::V1_25 | CoreVoltage::V1_30 => 2000, // 2ms for higher voltages
                    _ => 0,                                                               // no delay for all others
                }
            });

            if settling_time_us != 0 {
                // Delay in microseconds, using the ROSC frequency to calculate cycles
                let cycles_per_us = rosc_freq / 1_000_000;
                let delay_cycles = settling_time_us * cycles_per_us;
                cortex_m::asm::delay(delay_cycles);
            }

            // Only now set the BOD level. At this point the voltage is considered stable.
            #[cfg(feature = "rp2040")]
            vreg.bod().write(|w| {
                w.set_vsel(voltage.recommended_bod());
                w.set_en(true); // Enable brownout detection
            });
            #[cfg(feature = "_rp235x")]
            vreg.bod().write(|w| {
                w.0 = (w.0 & 0x0000FFFF) | (0x5AFE << 16); // Set the password
                w.set_vsel(voltage.recommended_bod());
                w.set_en(true); // Enable brownout detection
            });
        }
    }

    let (xosc_freq, pll_sys_freq, pll_usb_freq) = match config.xosc {
        Some(config) => {
            // start XOSC
            start_xosc(config.hz, config.delay_multiplier);

            let pll_sys_freq = match config.sys_pll {
                Some(sys_pll_config) => {
                    match configure_pll(pac::PLL_SYS, config.hz, sys_pll_config) {
                        Ok(freq) => freq,
                        Err(e) => panic!("Failed to configure PLL_SYS: {:?}", e),
                    }
                }
                None => 0,
            };
            let pll_usb_freq = match config.usb_pll {
                Some(usb_pll_config) => {
                    match configure_pll(pac::PLL_USB, config.hz, usb_pll_config) {
                        Ok(freq) => freq,
                        Err(e) => panic!("Failed to configure PLL_USB: {:?}", e),
                    }
                }
                None => 0,
            };

            (config.hz, pll_sys_freq, pll_usb_freq)
        }
        None => (0, 0, 0),
    };

    CLOCKS.xosc.store(xosc_freq, Ordering::Relaxed);
    CLOCKS.pll_sys.store(pll_sys_freq, Ordering::Relaxed);
    CLOCKS.pll_usb.store(pll_usb_freq, Ordering::Relaxed);

    let (ref_src, ref_aux, clk_ref_freq) = {
        use ClkRefCtrlAuxsrc as Aux;
        use ClkRefCtrlSrc as Src;
        let div = config.ref_clk.div as u32;
        assert!(div >= 1 && div <= 4);
        match config.ref_clk.src {
            RefClkSrc::Xosc => (Src::XOSC_CLKSRC, Aux::CLKSRC_PLL_USB, xosc_freq / div),
            RefClkSrc::Rosc => (Src::ROSC_CLKSRC_PH, Aux::CLKSRC_PLL_USB, rosc_freq / div),
            RefClkSrc::PllUsb => (Src::CLKSRC_CLK_REF_AUX, Aux::CLKSRC_PLL_USB, pll_usb_freq / div),
            // See above re gpin handling being commented out
            // RefClkSrc::Gpin0 => (Src::CLKSRC_CLK_REF_AUX, Aux::CLKSRC_GPIN0, gpin0_freq / div),
            // RefClkSrc::Gpin1 => (Src::CLKSRC_CLK_REF_AUX, Aux::CLKSRC_GPIN1, gpin1_freq / div),
        }
    };
    assert!(clk_ref_freq != 0);
    CLOCKS.reference.store(clk_ref_freq, Ordering::Relaxed);
    c.clk_ref_ctrl().write(|w| {
        w.set_src(ref_src);
        w.set_auxsrc(ref_aux);
    });
    #[cfg(feature = "rp2040")]
    while c.clk_ref_selected().read() != (1 << ref_src as u32) {}
    #[cfg(feature = "_rp235x")]
    while c.clk_ref_selected().read() != pac::clocks::regs::ClkRefSelected(1 << ref_src as u32) {}
    c.clk_ref_div().write(|w| {
        w.set_int(config.ref_clk.div);
    });

    // Configure tick generation on the 2040.
    #[cfg(feature = "rp2040")]
    pac::WATCHDOG.tick().write(|w| {
        w.set_cycles((clk_ref_freq / 1_000_000) as u16);
        w.set_enable(true);
    });
    // Configure tick generator on the 2350
    #[cfg(feature = "_rp235x")]
    {
        let cycle_count = clk_ref_freq / 1_000_000;

        pac::TICKS.timer0_cycles().write(|w| w.0 = cycle_count);
        pac::TICKS.timer0_ctrl().write(|w| w.set_enable(true));

        pac::TICKS.watchdog_cycles().write(|w| w.0 = cycle_count);
        pac::TICKS.watchdog_ctrl().write(|w| w.set_enable(true));
    }

    let (sys_src, sys_aux, clk_sys_freq) = {
        use ClkSysCtrlAuxsrc as Aux;
        use ClkSysCtrlSrc as Src;
        let (src, aux, freq) = match config.sys_clk.src {
            SysClkSrc::Ref => (Src::CLK_REF, Aux::CLKSRC_PLL_SYS, clk_ref_freq),
            SysClkSrc::PllSys => (Src::CLKSRC_CLK_SYS_AUX, Aux::CLKSRC_PLL_SYS, pll_sys_freq),
            SysClkSrc::PllUsb => (Src::CLKSRC_CLK_SYS_AUX, Aux::CLKSRC_PLL_USB, pll_usb_freq),
            SysClkSrc::Rosc => (Src::CLKSRC_CLK_SYS_AUX, Aux::ROSC_CLKSRC, rosc_freq),
            SysClkSrc::Xosc => (Src::CLKSRC_CLK_SYS_AUX, Aux::XOSC_CLKSRC, xosc_freq),
            // See above re gpin handling being commented out
            // SysClkSrc::Gpin0 => (Src::CLKSRC_CLK_SYS_AUX, Aux::CLKSRC_GPIN0, gpin0_freq),
            // SysClkSrc::Gpin1 => (Src::CLKSRC_CLK_SYS_AUX, Aux::CLKSRC_GPIN1, gpin1_freq),
        };
        (
            src,
            aux,
            divided_clock_frequency(freq, config.sys_clk.div_int as u64, config.sys_clk.div_frac as u64),
        )
    };
    assert!(clk_sys_freq != 0);
    CLOCKS.sys.store(clk_sys_freq, Ordering::Relaxed);
    if sys_src != ClkSysCtrlSrc::CLK_REF {
        c.clk_sys_ctrl().write(|w| w.set_src(ClkSysCtrlSrc::CLK_REF));
        #[cfg(feature = "rp2040")]
        while c.clk_sys_selected().read() != (1 << ClkSysCtrlSrc::CLK_REF as u32) {}
        #[cfg(feature = "_rp235x")]
        while c.clk_sys_selected().read() != pac::clocks::regs::ClkSysSelected(1 << ClkSysCtrlSrc::CLK_REF as u32) {}
    }
    c.clk_sys_ctrl().write(|w| {
        w.set_auxsrc(sys_aux);
        w.set_src(sys_src);
    });

    #[cfg(feature = "rp2040")]
    while c.clk_sys_selected().read() != (1 << sys_src as u32) {}
    #[cfg(feature = "_rp235x")]
    while c.clk_sys_selected().read() != pac::clocks::regs::ClkSysSelected(1 << sys_src as u32) {}

    c.clk_sys_div().write(|w| {
        w.set_int(config.sys_clk.div_int);
        w.set_frac(config.sys_clk.div_frac);
    });

    let mut peris = reset::ALL_PERIPHERALS;

    if let Some(src) = config.peri_clk_src {
        c.clk_peri_ctrl().write(|w| {
            w.set_enable(true);
            w.set_auxsrc(ClkPeriCtrlAuxsrc::from_bits(src as _));
        });
        let peri_freq = match src {
            PeriClkSrc::Sys => clk_sys_freq,
            PeriClkSrc::PllSys => pll_sys_freq,
            PeriClkSrc::PllUsb => pll_usb_freq,
            PeriClkSrc::Rosc => rosc_freq,
            PeriClkSrc::Xosc => xosc_freq,
            // See above re gpin handling being commented out
            // PeriClkSrc::Gpin0 => gpin0_freq,
            // PeriClkSrc::Gpin1 => gpin1_freq,
        };
        assert!(peri_freq != 0);
        CLOCKS.peri.store(peri_freq, Ordering::Relaxed);
    } else {
        peris.set_spi0(false);
        peris.set_spi1(false);
        peris.set_uart0(false);
        peris.set_uart1(false);
        CLOCKS.peri.store(0, Ordering::Relaxed);
    }

    if let Some(conf) = config.usb_clk {
        c.clk_usb_div().write(|w| w.set_int(conf.div));
        c.clk_usb_ctrl().write(|w| {
            w.set_phase(conf.phase);
            w.set_enable(true);
            w.set_auxsrc(ClkUsbCtrlAuxsrc::from_bits(conf.src as _));
        });
        let usb_freq = match conf.src {
            UsbClkSrc::PllUsb => pll_usb_freq,
            UsbClkSrc::PllSys => pll_sys_freq,
            UsbClkSrc::Rosc => rosc_freq,
            UsbClkSrc::Xosc => xosc_freq,
            // See above re gpin handling being commented out
            // UsbClkSrc::Gpin0 => gpin0_freq,
            // UsbClkSrc::Gpin1 => gpin1_freq,
        };
        assert!(usb_freq != 0);
        assert!(conf.div >= 1 && conf.div <= 4);
        CLOCKS.usb.store(usb_freq / conf.div as u32, Ordering::Relaxed);
    } else {
        peris.set_usbctrl(false);
        CLOCKS.usb.store(0, Ordering::Relaxed);
    }

    if let Some(conf) = config.adc_clk {
        c.clk_adc_div().write(|w| w.set_int(conf.div));
        c.clk_adc_ctrl().write(|w| {
            w.set_phase(conf.phase);
            w.set_enable(true);
            w.set_auxsrc(ClkAdcCtrlAuxsrc::from_bits(conf.src as _));
        });
        let adc_in_freq = match conf.src {
            AdcClkSrc::PllUsb => pll_usb_freq,
            AdcClkSrc::PllSys => pll_sys_freq,
            AdcClkSrc::Rosc => rosc_freq,
            AdcClkSrc::Xosc => xosc_freq,
            // See above re gpin handling being commented out
            // AdcClkSrc::Gpin0 => gpin0_freq,
            // AdcClkSrc::Gpin1 => gpin1_freq,
        };
        assert!(adc_in_freq != 0);
        assert!(conf.div >= 1 && conf.div <= 4);
        CLOCKS.adc.store(adc_in_freq / conf.div as u32, Ordering::Relaxed);
    } else {
        peris.set_adc(false);
        CLOCKS.adc.store(0, Ordering::Relaxed);
    }

    // rp2040 specific clocks
    #[cfg(feature = "rp2040")]
    if let Some(conf) = config.rtc_clk {
        let rtc_in_freq = match conf.src {
            RtcClkSrc::PllUsb => pll_usb_freq,
            RtcClkSrc::PllSys => pll_sys_freq,
            RtcClkSrc::Rosc => rosc_freq,
            RtcClkSrc::Xosc => xosc_freq,
            // See above re gpin handling being commented out
            // RtcClkSrc::Gpin0 => gpin0_freq,
            // RtcClkSrc::Gpin1 => gpin1_freq,
        };
        assert!(rtc_in_freq != 0);
        let rtc_freq = rtc_clock_frequency(rtc_in_freq, conf.div_int, conf.div_frac);
        c.clk_rtc_div().write(|w| {
            w.set_int(conf.div_int);
            w.set_frac(conf.div_frac);
        });
        c.clk_rtc_ctrl().write(|w| {
            w.set_phase(conf.phase);
            w.set_enable(true);
            w.set_auxsrc(ClkRtcCtrlAuxsrc::from_bits(conf.src as _));
        });
        CLOCKS.rtc.store(rtc_freq, Ordering::Relaxed);
    } else {
        peris.set_rtc(false);
        CLOCKS.rtc.store(0, Ordering::Relaxed);
    }

    // rp235x specific clocks
    #[cfg(feature = "_rp235x")]
    {
        // TODO hstx clock
        peris.set_hstx(false);
    }

    // Peripheral clocks should now all be running
    reset::unreset_wait(peris);
}

/// Programs oscillator fields from `config` and returns its claimed hertz, without waiting or measurement.
fn configure_rosc(config: RoscConfig) -> u32 {
    let p = pac::ROSC;

    p.freqa().write(|w| {
        w.set_passwd(pac::rosc::vals::Passwd::PASS);
        w.set_ds0(config.drive_strength[0]);
        w.set_ds1(config.drive_strength[1]);
        w.set_ds2(config.drive_strength[2]);
        w.set_ds3(config.drive_strength[3]);
    });

    p.freqb().write(|w| {
        w.set_passwd(pac::rosc::vals::Passwd::PASS);
        w.set_ds4(config.drive_strength[4]);
        w.set_ds5(config.drive_strength[5]);
        w.set_ds6(config.drive_strength[6]);
        w.set_ds7(config.drive_strength[7]);
    });

    p.div().write(|w| {
        w.set_div(pac::rosc::vals::Div(config.div + pac::rosc::vals::Div::PASS.0));
    });

    p.ctrl().write(|w| {
        w.set_enable(pac::rosc::vals::Enable::ENABLE);
        w.set_freq_range(pac::rosc::vals::FreqRange(config.range as u16));
    });

    config.hz
}

/// Returns cached ROSC hertz; zero means absent/uninitialized, not a measured stopped clock.
pub fn rosc_freq() -> u32 { CLOCKS.rosc.load(Ordering::Relaxed) }

/// Returns cached XOSC hertz; zero means absent/uninitialized, not a measured stopped clock.
pub fn xosc_freq() -> u32 { CLOCKS.xosc.load(Ordering::Relaxed) }

// See above re gpin handling being commented out
// pub fn gpin0_freq() -> u32 {
//     CLOCKS.gpin0.load(Ordering::Relaxed)
// }
// pub fn gpin1_freq() -> u32 {
//     CLOCKS.gpin1.load(Ordering::Relaxed)
// }

/// Returns cached PLL SYS hertz; zero means absent/uninitialized, not a measured stopped clock.
pub fn pll_sys_freq() -> u32 { CLOCKS.pll_sys.load(Ordering::Relaxed) }

/// Returns cached PLL USB hertz; zero means absent/uninitialized, not a measured stopped clock.
pub fn pll_usb_freq() -> u32 { CLOCKS.pll_usb.load(Ordering::Relaxed) }

/// Returns cached SYS hertz; zero means absent/uninitialized, not a measured stopped clock.
pub fn clk_sys_freq() -> u32 { CLOCKS.sys.load(Ordering::Relaxed) }

/// Returns cached REF hertz; zero means absent/uninitialized, not a measured stopped clock.
pub fn clk_ref_freq() -> u32 { CLOCKS.reference.load(Ordering::Relaxed) }

/// Returns cached Peripheral hertz; zero means absent/uninitialized, not a measured stopped clock.
pub fn clk_peri_freq() -> u32 { CLOCKS.peri.load(Ordering::Relaxed) }

/// Returns cached USB hertz; zero means absent/uninitialized, not a measured stopped clock.
pub fn clk_usb_freq() -> u32 { CLOCKS.usb.load(Ordering::Relaxed) }

/// Returns cached ADC hertz; zero means absent/uninitialized, not a measured stopped clock.
pub fn clk_adc_freq() -> u32 { CLOCKS.adc.load(Ordering::Relaxed) }

/// Returns cached RTC hertz; zero means absent/uninitialized, not a measured stopped clock.
#[cfg(feature = "rp2040")]
pub fn clk_rtc_freq() -> u16 { CLOCKS.rtc.load(Ordering::Relaxed) }

/// The core voltage of the chip.
///
/// Returns the current core voltage or an error if the voltage register
/// contains an unknown value.
pub fn core_voltage() -> Result<CoreVoltage, ClockError> {
    #[cfg(feature = "rp2040")]
    {
        let vreg = pac::VREG_AND_CHIP_RESET;
        let vsel = vreg.vreg().read().vsel();
        match vsel {
            0b0000 => Ok(CoreVoltage::V0_80),
            0b0110 => Ok(CoreVoltage::V0_85),
            0b0111 => Ok(CoreVoltage::V0_90),
            0b1000 => Ok(CoreVoltage::V0_95),
            0b1001 => Ok(CoreVoltage::V1_00),
            0b1010 => Ok(CoreVoltage::V1_05),
            0b1011 => Ok(CoreVoltage::V1_10),
            0b1100 => Ok(CoreVoltage::V1_15),
            0b1101 => Ok(CoreVoltage::V1_20),
            0b1110 => Ok(CoreVoltage::V1_25),
            0b1111 => Ok(CoreVoltage::V1_30),
            _ => Err(ClockError::UnexpectedCoreVoltageRead),
        }
    }

    #[cfg(feature = "_rp235x")]
    {
        let vreg = pac::POWMAN;
        let vsel = vreg.vreg().read().vsel();
        match vsel {
            0b00000 => Ok(CoreVoltage::V0_55),
            0b00001 => Ok(CoreVoltage::V0_60),
            0b00010 => Ok(CoreVoltage::V0_65),
            0b00011 => Ok(CoreVoltage::V0_70),
            0b00100 => Ok(CoreVoltage::V0_75),
            0b00101 => Ok(CoreVoltage::V0_80),
            0b00110 => Ok(CoreVoltage::V0_85),
            0b00111 => Ok(CoreVoltage::V0_90),
            0b01000 => Ok(CoreVoltage::V0_95),
            0b01001 => Ok(CoreVoltage::V1_00),
            0b01010 => Ok(CoreVoltage::V1_05),
            0b01011 => Ok(CoreVoltage::V1_10),
            0b01100 => Ok(CoreVoltage::V1_15),
            0b01101 => Ok(CoreVoltage::V1_20),
            0b01110 => Ok(CoreVoltage::V1_25),
            0b01111 => Ok(CoreVoltage::V1_30),
            _ => Err(ClockError::UnexpectedCoreVoltageRead),
            // see CoreVoltage: we do not support setting Voltages higher than 1.30V at this point
        }
    }
}

/// Starts the crystal at assumed `crystal_hz` with `delay_multiplier`; waits indefinitely for stable.
/// Delay arithmetic may overflow or truncate for unsupported values; hardware must match the input.
fn start_xosc(
    crystal_hz: u32,
    delay_multiplier: u32,
) {
    let startup_delay = (((crystal_hz / 1000) * delay_multiplier) + 128) / 256;
    pac::XOSC.startup().write(|w| w.set_delay(startup_delay as u16));
    pac::XOSC.ctrl().write(|w| {
        w.set_freq_range(pac::xosc::vals::CtrlFreqRange::_1_15MHZ);
        w.set_enable(pac::xosc::vals::Enable::ENABLE);
    });
    while !pac::XOSC.status().read().stable() {}
}

/// Configures PLL register bank `p` using `input_freq` hertz and `config` ratios.
/// Returns output hertz or `PllLockTimedOut` after one million failed lock polls;
/// timeout leaves VCO powered with output disabled. Invalid ratios panic before PLL writes.
#[inline(always)]
fn configure_pll(
    p: pac::pll::Pll,
    input_freq: u32,
    config: PllConfig,
) -> Result<u32, ClockError> {
    // Calculate reference frequency
    let ref_freq = input_freq / config.refdiv as u32;

    // Validate PLL parameters
    // Feedback divider (FBDIV) must be between 16 and 320
    assert!(config.fbdiv >= 16 && config.fbdiv <= 320);

    // Post divider 1 (POSTDIV1) must be between 1 and 7
    assert!(config.post_div1 >= 1 && config.post_div1 <= 7);

    // Post divider 2 (POSTDIV2) must be between 1 and 7
    assert!(config.post_div2 >= 1 && config.post_div2 <= 7);

    // Post divider 2 (POSTDIV2) must be less than or equal to post divider 1 (POSTDIV1)
    assert!(config.post_div2 <= config.post_div1);

    // Reference divider (REFDIV) must be between 1 and 63
    assert!(config.refdiv >= 1 && config.refdiv <= 63);

    // Reference frequency (REF_FREQ) must be between 5MHz and 800MHz
    assert!(ref_freq >= 5_000_000 && ref_freq <= 800_000_000);

    // Calculate VCO frequency
    let vco_freq = ref_freq.saturating_mul(config.fbdiv as u32);

    // VCO (Voltage Controlled Oscillator) frequency must be between 750MHz and 1600MHz
    assert!(vco_freq >= VCO_MIN && vco_freq <= VCO_MAX);

    // We follow the SDK's approach to PLL configuration which is:
    // 1. Power down PLL
    // 2. Configure the reference divider
    // 3. Configure the feedback divider
    // 4. Power up PLL and VCO
    // 5. Wait for PLL to lock
    // 6. Configure post-dividers
    // 7. Enable post-divider output

    // 1. Power down PLL before configuration
    p.pwr().write(|w| {
        w.set_pd(true); // Power down the PLL
        w.set_vcopd(true); // Power down the VCO
        w.set_postdivpd(true); // Power down the post divider
        w.set_dsmpd(true); // Disable fractional mode
        *w
    });

    // Short delay after powering down
    cortex_m::asm::delay(10);

    // 2. Configure reference divider first
    p.cs().write(|w| w.set_refdiv(config.refdiv as _));

    // 3. Configure feedback divider
    p.fbdiv_int().write(|w| w.set_fbdiv_int(config.fbdiv));

    // 4. Power up PLL and VCO, but keep post divider powered down during initial lock
    p.pwr().write(|w| {
        w.set_pd(false); // Power up the PLL
        w.set_vcopd(false); // Power up the VCO
        w.set_postdivpd(true); // Keep post divider powered down during initial lock
        w.set_dsmpd(true); // Disable fractional mode (simpler configuration)
        *w
    });

    // 5. Wait for PLL to lock with a timeout
    let mut timeout = 1_000_000;
    while !p.cs().read().lock() {
        timeout -= 1;
        if timeout == 0 {
            // PLL failed to lock; leave its post-divider disabled and report failure.
            return Err(ClockError::PllLockTimedOut);
        }
    }

    // 6. Configure post dividers after PLL is locked
    p.prim().write(|w| {
        w.set_postdiv1(config.post_div1);
        w.set_postdiv2(config.post_div2);
    });

    // 7. Enable the post divider output
    p.pwr().modify(|w| {
        w.set_postdivpd(false); // Power up post divider
        *w
    });

    // Final delay to ensure everything is stable
    cortex_m::asm::delay(100);

    // Calculate and return actual output frequency
    Ok(vco_freq / ((config.post_div1 * config.post_div2) as u32))
}

/// General purpose input clock pin.
pub trait GpinPin: crate::gpio::Pin {
    /// Clock input index (0 or 1), not the GPIO number.
    const NR: usize;
}

/// Implements the clock-input index for each supported pin token.
macro_rules! impl_gpinpin {
    ($name:ident, $gpin_num:expr) => {
        impl GpinPin for crate::peripherals::$name {
            /// Clock input index (0 or 1), not the GPIO number.
            const NR: usize = $gpin_num;
        }
    };
}

#[cfg(feature = "_rp235x")]
impl_gpinpin!(PIN_12, 0);
#[cfg(feature = "_rp235x")]
impl_gpinpin!(PIN_14, 1);

impl_gpinpin!(PIN_20, 0);
impl_gpinpin!(PIN_22, 1);

/// General purpose clock input driver.
pub struct Gpin<'d, T: GpinPin> {
    /// Owned GPIO routed to clock input until drop.
    gpin: Peri<'d, AnyPin>,
    /// Retains the compile-time GPIO-to-clock-input mapping.
    _phantom: PhantomData<T>,
}

impl<'d, T: GpinPin> Gpin<'d, T> {
    /// Returns ownership of `gpin` routed to its GPIN function; does not select it as a clock source.
    pub fn new(gpin: Peri<'d, T>) -> Self {
        #[cfg(feature = "rp2040")]
        gpin.gpio().ctrl().write(|w| w.set_funcsel(0x08));

        // On RP2350 GPIN changed from F8 toF9
        #[cfg(feature = "_rp235x")]
        gpin.gpio().ctrl().write(|w| w.set_funcsel(0x09));

        #[cfg(feature = "_rp235x")]
        gpin.pad_ctrl().write(|w| {
            w.set_iso(false);
        });

        Gpin {
            gpin: gpin.into(),
            _phantom: PhantomData,
        }
    }
}

impl<'d, T: GpinPin> Drop for Gpin<'d, T> {
    /// Restores pad defaults and disconnects the GPIO function; does not reconfigure clock consumers.
    fn drop(&mut self) {
        self.gpin.pad_ctrl().write(|_| {});
        self.gpin
            .gpio()
            .ctrl()
            .write(|w| w.set_funcsel(pac::io::vals::Gpio0ctrlFuncsel::NULL as _));
    }
}

/// General purpose clock output pin.
pub trait GpoutPin: crate::gpio::Pin {
    /// Returns clock-output index 0..=3, not the GPIO number.
    fn number(&self) -> usize;
}

/// Implements clock-output index lookup for each supported GPIO token.
macro_rules! impl_gpoutpin {
    ($name:ident, $gpout_num:expr) => {
        impl GpoutPin for crate::peripherals::$name {
            /// Returns the fixed clock-output index for this pin.
            fn number(&self) -> usize { $gpout_num }
        }
    };
}

#[cfg(feature = "_rp235x")]
impl_gpoutpin!(PIN_13, 0);
#[cfg(feature = "_rp235x")]
impl_gpoutpin!(PIN_15, 1);

impl_gpoutpin!(PIN_21, 0);
impl_gpoutpin!(PIN_23, 1);
impl_gpoutpin!(PIN_24, 2);
impl_gpoutpin!(PIN_25, 3);

/// Gpout clock source.
#[repr(u8)]
pub enum GpoutSrc {
    /// Sys PLL.
    PllSys = ClkGpoutCtrlAuxsrc::CLKSRC_PLL_SYS as _,
    // See above re gpin handling being commented out
    // Gpin0 = ClkGpoutCtrlAuxsrc::CLKSRC_GPIN0 as _ ,
    // Gpin1 = ClkGpoutCtrlAuxsrc::CLKSRC_GPIN1 as _ ,
    /// USB PLL.
    PllUsb = ClkGpoutCtrlAuxsrc::CLKSRC_PLL_USB as _,
    /// ROSC.
    Rosc = ClkGpoutCtrlAuxsrc::ROSC_CLKSRC as _,
    /// XOSC.
    Xosc = ClkGpoutCtrlAuxsrc::XOSC_CLKSRC as _,
    /// LPOSC.
    #[cfg(feature = "_rp235x")]
    Lposc = ClkGpoutCtrlAuxsrc::LPOSC_CLKSRC as _,
    /// SYS.
    Sys = ClkGpoutCtrlAuxsrc::CLK_SYS as _,
    /// USB.
    Usb = ClkGpoutCtrlAuxsrc::CLK_USB as _,
    /// ADC.
    Adc = ClkGpoutCtrlAuxsrc::CLK_ADC as _,
    /// RTC.
    #[cfg(feature = "rp2040")]
    Rtc = ClkGpoutCtrlAuxsrc::CLK_RTC as _,
    /// REF.
    Ref = ClkGpoutCtrlAuxsrc::CLK_REF as _,
}

/// General purpose clock output driver.
pub struct Gpout<'d, T: GpoutPin> {
    /// Owned output GPIO and its associated clock-output index.
    gpout: Peri<'d, T>,
}

impl<'d, T: GpoutPin> Gpout<'d, T> {
    /// Returns ownership of `gpout` routed to GPOUT; retains prior clock source/divider/enable registers.
    pub fn new(gpout: Peri<'d, T>) -> Self {
        #[cfg(feature = "rp2040")]
        gpout.gpio().ctrl().write(|w| w.set_funcsel(0x08));

        // On RP2350 GPOUT changed from F8 toF9
        #[cfg(feature = "_rp235x")]
        gpout.gpio().ctrl().write(|w| w.set_funcsel(0x09));

        #[cfg(feature = "_rp235x")]
        gpout.pad_ctrl().write(|w| {
            w.set_iso(false);
        });

        Self { gpout }
    }

    /// Writes raw integer `int` and fractional `frac` divider fields without disabling output.
    /// Zero integer encodes the field maximum plus one; fractional units are 1/256
    /// (RP2040) or 1/65536 (RP235x). RP2040 `int` must fit 24 bits.
    #[cfg(feature = "rp2040")]
    pub fn set_div(
        &self,
        int: u32,
        frac: u8,
    ) {
        let c = pac::CLOCKS;
        c.clk_gpout_div(self.gpout.number()).write(|w| {
            w.set_int(int);
            w.set_frac(frac);
        });
    }

    /// Writes raw integer `int` and fractional `frac` divider fields without disabling output.
    /// Zero integer encodes the field maximum plus one; fractional units are 1/256
    /// (RP2040) or 1/65536 (RP235x). RP2040 `int` must fit 24 bits.
    #[cfg(feature = "_rp235x")]
    pub fn set_div(
        &self,
        int: u16,
        frac: u16,
    ) {
        let c = pac::CLOCKS;
        c.clk_gpout_div(self.gpout.number()).write(|w| {
            w.set_int(int);
            w.set_frac(frac);
        });
    }

    /// Selects `src` immediately; switch while disabled to avoid clock glitches.
    pub fn set_src(
        &self,
        src: GpoutSrc,
    ) {
        let c = pac::CLOCKS;
        c.clk_gpout_ctrl(self.gpout.number()).modify(|w| {
            w.set_auxsrc(ClkGpoutCtrlAuxsrc::from_bits(src as _));
        });
    }

    /// Requests clean clock-output enable; does not wait for a physical edge.
    pub fn enable(&self) {
        let c = pac::CLOCKS;
        c.clk_gpout_ctrl(self.gpout.number()).modify(|w| {
            w.set_enable(true);
        });
    }

    /// Requests clean clock-output disable; does not wait for hardware acknowledgment.
    pub fn disable(&self) {
        let c = pac::CLOCKS;
        c.clk_gpout_ctrl(self.gpout.number()).modify(|w| {
            w.set_enable(false);
        });
    }

    /// Returns calculated output hertz from cached source rate and current divider, even if disabled.
    /// LPOSC returns zero because no rate is cached. Raw selectors outside `GpoutSrc`
    /// panic; this performs no frequency measurement.
    pub fn get_freq(&self) -> u32 {
        let c = pac::CLOCKS;
        let src = c.clk_gpout_ctrl(self.gpout.number()).read().auxsrc();

        let base = gpout_source_frequency(src);

        let div = c.clk_gpout_div(self.gpout.number()).read();
        let int = div.int() as u64;
        let frac = div.frac() as u64;

        divided_clock_frequency(base, int, frac)
    }
}

impl<'d, T: GpoutPin> Drop for Gpout<'d, T> {
    /// Disables clock output, restores pad defaults and disconnects the GPIO function.
    fn drop(&mut self) {
        self.disable();
        self.gpout.pad_ctrl().write(|_| {});
        self.gpout
            .gpio()
            .ctrl()
            .write(|w| w.set_funcsel(pac::io::vals::Gpio0ctrlFuncsel::NULL as _));
    }
}

/// Random number generator based on the ROSC RANDOMBIT register.
///
/// This will not produce random values if the ROSC is stopped or run at some
/// harmonic of the bus frequency. With default clock settings these are not
/// issues. No conditioning or online entropy test is performed; the CryptoRng marker
/// does not provide measured entropy assurance for arbitrary clock settings.
pub struct RoscRng;

impl RoscRng {
    /// Returns eight consecutive ROSC bit samples packed most-significant bit first.
    pub fn next_u8() -> u8 {
        let random_reg = pac::ROSC.randombit();
        let mut acc = 0;
        for _ in 0..u8::BITS {
            acc <<= 1;
            acc |= random_reg.read().randombit() as u8;
        }
        acc
    }

    /// Returns four sampled bytes interpreted little-endian by rand_core.
    pub fn next_u32(&mut self) -> u32 { rand_core_09::impls::next_u32_via_fill(self) }

    /// Returns eight sampled bytes interpreted little-endian by rand_core.
    pub fn next_u64(&mut self) -> u64 { rand_core_09::impls::next_u64_via_fill(self) }

    /// Fills `dest` with eight ROSC reads per byte; empty slices perform no reads.
    pub fn fill_bytes(
        &mut self,
        dest: &mut [u8],
    ) {
        dest.fill_with(Self::next_u8)
    }
}

impl rand_core_06::RngCore for RoscRng {
    /// Returns four sampled bytes via the inherent ROSC generator.
    fn next_u32(&mut self) -> u32 { self.next_u32() }

    /// Returns eight sampled bytes via the inherent ROSC generator.
    fn next_u64(&mut self) -> u64 { self.next_u64() }

    /// Fills `dest` from raw ROSC samples without health testing.
    fn fill_bytes(
        &mut self,
        dest: &mut [u8],
    ) {
        self.fill_bytes(dest);
    }

    /// Fills `dest` and returns `Ok(())`; oscillator entropy failures are not detected.
    fn try_fill_bytes(
        &mut self,
        dest: &mut [u8],
    ) -> Result<(), rand_core_06::Error> {
        self.fill_bytes(dest);
        Ok(())
    }
}

impl rand_core_06::CryptoRng for RoscRng {}

impl rand_core_09::RngCore for RoscRng {
    /// Returns four sampled bytes via the inherent ROSC generator.
    fn next_u32(&mut self) -> u32 { self.next_u32() }

    /// Returns eight sampled bytes via the inherent ROSC generator.
    fn next_u64(&mut self) -> u64 { self.next_u64() }

    /// Fills `dest` from raw ROSC samples without health testing.
    fn fill_bytes(
        &mut self,
        dest: &mut [u8],
    ) {
        self.fill_bytes(dest);
    }
}

impl rand_core_09::CryptoRng for RoscRng {}

/// Enters ROSC dormant after gating ADC/USB/peripheral clocks and stopping PLLs/XOSC.
/// Returns after a configured dormant wake and restores saved clocks, with unbounded stability waits.
/// Call only after quiescing both cores, IRQ users and DMA: this global sequence can power down SRAM.
/// Configure wake sources first; RP2040 RTC needs an external clock to continue. RP235x LPOSC/
/// POWMAN are not managed here. No wake source or failed oscillator relock can leave this blocked.
#[cfg(all(target_arch = "arm"))]
pub fn dormant_sleep() {
    /// Restores a saved register and then runs its stabilization callback on drop.
    struct Set<T: Copy, F: Fn()>(
        /// Register to restore.
        Reg<T, RW>,
        /// Value captured before the temporary change.
        T,
        /// Post-restore callback, potentially waiting indefinitely for stabilization.
        F,
    );

    impl<T: Copy, F: Fn()> Drop for Set<T, F> {
        /// Restores the saved value and waits through its callback, without a timeout.
        fn drop(&mut self) {
            self.0.write_value(self.1);
            self.2();
        }
    }

    /// Updates `reg` using `f`; returns a guard restoring the old value then invoking
    /// the callback returned by `f`. A callback panic prevents returning the guard.
    fn set_with_post_restore<T: Copy, After: Fn(), F: FnOnce(&mut T) -> After>(
        reg: Reg<T, RW>,
        f: F,
    ) -> Set<T, impl Fn()> {
        reg.modify(|w| {
            let old = *w;
            let after = f(w);
            Set(reg, old, after)
        })
    }

    /// Updates `reg` through `f`; returns a guard restoring the old register value on drop.
    fn set<T: Copy, F: FnOnce(&mut T)>(
        reg: Reg<T, RW>,
        f: F,
    ) -> Set<T, impl Fn()> {
        set_with_post_restore(reg, |r| {
            f(r);
            || ()
        })
    }

    // disable all clocks that are not vital in preparation for disabling clock sources.
    // we'll keep gpout and rtc clocks untouched, gpout because we don't care about them
    // and rtc because it's a possible wakeup source. if clk_rtc is not configured for
    // gpin we'll never wake from rtc, but that's what the user asked for then.
    let _stop_adc = set(pac::CLOCKS.clk_adc_ctrl(), |w| w.set_enable(false));
    let _stop_usb = set(pac::CLOCKS.clk_usb_ctrl(), |w| w.set_enable(false));
    let _stop_peri = set(pac::CLOCKS.clk_peri_ctrl(), |w| w.set_enable(false));
    // set up rosc. we could ask the user to tell us which clock source to wake from like
    // the C SDK does, but that seems rather unfriendly. we *may* disturb rtc by changing
    // rosc configuration if it's currently the rtc clock source, so we'll configure rosc
    // to the slowest frequency to minimize that impact.
    let _configure_rosc = (
        set(pac::ROSC.ctrl(), |w| {
            w.set_enable(pac::rosc::vals::Enable::ENABLE);
            w.set_freq_range(pac::rosc::vals::FreqRange::LOW);
        }),
        // div=32
        set(pac::ROSC.div(), |w| w.set_div(pac::rosc::vals::Div(0xAA0))),
    );
    while !pac::ROSC.status().read().stable() {}
    // switch over to rosc as the system clock source. this will change clock sources for
    // watchdog and timer clocks, but timers won't be a concern and the watchdog won't
    // speed up by enough to worry about (unless it's clocked from gpin, which we don't
    // support anyway).
    let _switch_clk_ref = set(pac::CLOCKS.clk_ref_ctrl(), |w| {
        w.set_src(pac::clocks::vals::ClkRefCtrlSrc::ROSC_CLKSRC_PH);
    });
    let _switch_clk_sys = set(pac::CLOCKS.clk_sys_ctrl(), |w| {
        w.set_src(pac::clocks::vals::ClkSysCtrlSrc::CLK_REF);
    });
    // oscillator dormancy does not power down plls, we have to do that ourselves. we'll
    // restore them to their prior glory when woken though since the system may be clocked
    // from either (and usb/adc will probably need the USB PLL anyway)
    let _stop_pll_sys = set_with_post_restore(pac::PLL_SYS.pwr(), |w| {
        let wake = !w.pd() && !w.vcopd();
        w.set_pd(true);
        w.set_vcopd(true);
        move || while wake && !pac::PLL_SYS.cs().read().lock() {}
    });
    let _stop_pll_usb = set_with_post_restore(pac::PLL_USB.pwr(), |w| {
        let wake = !w.pd() && !w.vcopd();
        w.set_pd(true);
        w.set_vcopd(true);
        move || while wake && !pac::PLL_USB.cs().read().lock() {}
    });
    // dormancy only stops the oscillator we're telling to go dormant, the other remains
    // running. nothing can use xosc at this point any more. not doing this costs an 200µA.
    let _stop_xosc = set_with_post_restore(pac::XOSC.ctrl(), |w| {
        let wake = w.enable() == pac::xosc::vals::Enable::ENABLE;
        if wake {
            w.set_enable(pac::xosc::vals::Enable::DISABLE);
        }
        move || while wake && !pac::XOSC.status().read().stable() {}
    });
    let _power_down_xip_cache = set(pac::XIP_CTRL.ctrl(), |w| w.set_power_down(true));

    // only power down memory if we're running from XIP (or ROM? how?).
    // powering down memory otherwise would require a lot of exacting checks that
    // are better done by the user in a local copy of this function.
    // powering down memories saves ~100µA, so it's well worth doing.
    unsafe {
        let is_in_flash = {
            // we can't rely on the address of this function as rust sees it since linker
            // magic or even boot2 may place it into ram.
            let pc: usize;
            asm!(
                "mov {pc}, pc",
                pc = out (reg) pc
            );
            pc < 0x20000000
        };
        if is_in_flash {
            // we will be powering down memories, so we must be *absolutely*
            // certain that we're running entirely from XIP and registers until
            // memories are powered back up again. accessing memory that's powered
            // down may corrupt memory contents (see section 2.11.4 of the manual).
            // additionally a 20ns wait time is needed after powering up memories
            // again. rosc is likely to run at only a few MHz at most, so the
            // inter-instruction delay alone will be enough to satisfy this bound.
            asm!(
                "ldr {old_mem}, [{mempowerdown}]",
                "str {power_down_mems}, [{mempowerdown}]",
                "str {coma}, [{dormant}]",
                "str {old_mem}, [{mempowerdown}]",
                old_mem = out (reg) _,
                mempowerdown = in (reg) pac::SYSCFG.mempowerdown().as_ptr(),
                power_down_mems = in (reg) 0b11111111,
                dormant = in (reg) pac::ROSC.dormant().as_ptr(),
                coma = in (reg) 0x636f6d61,
            );
        } else {
            pac::ROSC.dormant().write_value(rp_pac::rosc::regs::Dormant(0x636F6D61));
        }
    }
}

/// Returns cached hertz for output selector `src`, or zero for unmeasured LPOSC.
/// Unsupported raw register selectors panic.
fn gpout_source_frequency(src: ClkGpoutCtrlAuxsrc) -> u32 {
    match src {
        ClkGpoutCtrlAuxsrc::CLKSRC_PLL_SYS => pll_sys_freq(),
        // See above re gpin handling being commented out
        // ClkGpoutCtrlAuxsrc::CLKSRC_GPIN0 => gpin0_freq(),
        // ClkGpoutCtrlAuxsrc::CLKSRC_GPIN1 => gpin1_freq(),
        ClkGpoutCtrlAuxsrc::CLKSRC_PLL_USB => pll_usb_freq(),
        ClkGpoutCtrlAuxsrc::ROSC_CLKSRC => rosc_freq(),
        ClkGpoutCtrlAuxsrc::XOSC_CLKSRC => xosc_freq(),
        ClkGpoutCtrlAuxsrc::CLK_SYS => clk_sys_freq(),
        ClkGpoutCtrlAuxsrc::CLK_USB => clk_usb_freq(),
        ClkGpoutCtrlAuxsrc::CLK_ADC => clk_adc_freq(),
        ClkGpoutCtrlAuxsrc::CLK_REF => clk_ref_freq(),
        #[cfg(feature = "rp2040")]
        ClkGpoutCtrlAuxsrc::CLK_RTC => clk_rtc_freq() as u32,
        #[cfg(feature = "_rp235x")]
        ClkGpoutCtrlAuxsrc::LPOSC_CLKSRC => 0,
        _ => unreachable!(),
    }
}

/// Returns RTC hertz from source `base`, integer `int` and fractional `frac` divider fields.
/// Panics for divider/rate values not representable by the retained RTC API.
#[cfg(feature = "rp2040")]
fn rtc_clock_frequency(
    base: u32,
    int: u32,
    frac: u8,
) -> u16 {
    assert!(int <= 0x00FF_FFFF, "RTC divider exceeds its 24-bit field");
    // CLK_RTC_DIV documents zero as divide-by-2^16, unlike SYS/GPOUT.
    let int = if int == 0 { 65_536 } else { int };
    let hz = (base as u64 * 256) / (int as u64 * 256 + frac as u64);
    assert!(
        hz > 0 && hz <= u16::MAX as u64,
        "RTC frequency must fit nonzero u16 hertz"
    );
    hz as u16
}

/// Returns floor hertz for source `base`, raw integer `int` and fractional `frac`.
/// Inputs must fit hardware fields; integer zero encodes max+1, fractions use chip-specific width.
fn divided_clock_frequency(
    base: u32,
    int: u64,
    frac: u64,
) -> u32 {
    #[cfg(feature = "rp2040")]
    /// Number of fractional divider bits on this chip family.
    const FRAC_BITS: u32 = 8;
    #[cfg(feature = "_rp235x")]
    /// Number of fractional divider bits on this chip family.
    const FRAC_BITS: u32 = 16;
    // SYS and GPOUT encode the maximum integer divisor as zero.
    let int = if int == 0 { 1u64 << (32 - FRAC_BITS) } else { int };
    ((base as u64 * (1 << FRAC_BITS)) / ((int << FRAC_BITS) + frac)) as u32
}

#[cfg(test)]
/// Pure clock arithmetic/policy regressions; no oscillator or voltage measurements.
mod tests {
    extern crate std;

    use super::*;

    /// Accepted public output sources must not panic when frequency metadata is unavailable.
    #[test]
    fn gpout_supported_uncached_sources() {
        #[cfg(feature = "rp2040")]
        assert_eq!(
            gpout_source_frequency(ClkGpoutCtrlAuxsrc::CLK_RTC),
            clk_rtc_freq() as u32
        );
        #[cfg(feature = "_rp235x")]
        assert_eq!(gpout_source_frequency(ClkGpoutCtrlAuxsrc::LPOSC_CLKSRC), 0);
    }

    /// Checks RTC zero encoding, fractional rate and rejection before narrowing or field truncation.
    #[cfg(feature = "rp2040")]
    #[test]
    fn rtc_divider_boundaries() {
        assert_eq!(rtc_clock_frequency(48_000_000, 1024, 0), 46_875);
        assert_eq!(rtc_clock_frequency(65_536_000, 0, 0), 1000);
        assert_eq!(rtc_clock_frequency(48_000, 1, 128), 32_000);
        assert_eq!(rtc_clock_frequency(65_535, 1, 0), 65_535);
        for (base, int) in [(65_536, 1), (1, 2), (48_000_000, 0x1000000)] {
            assert!(std::panic::catch_unwind(|| rtc_clock_frequency(base, int, 0)).is_err());
        }
    }

    #[test]
    /// Rejects extreme references without narrowing an overflowing VCO.
    fn pll_validation_rejects_overflowing_vco() {
        let config = PllConfig {
            refdiv: 1,
            fbdiv: 320,
            post_div1: 7,
            post_div2: 7,
        };
        assert!(!config.is_valid(800_000_000));
        assert!(!config.is_valid(u32::MAX));
    }

    #[cfg(feature = "_rp235x")]
    #[test]
    /// Checks fractional division and maximum-divisor zero encoding.
    fn rp2350_fractional_clock_and_zero_integer_encoding() {
        assert_eq!(divided_clock_frequency(150_000_000, 1, 32768), 100_000_000);
        assert_eq!(divided_clock_frequency(150_000_000, 2, 0), 75_000_000);
        assert_eq!(divided_clock_frequency(65_536_000, 0, 0), 1000);
        assert_eq!(divided_clock_frequency(u32::MAX, 1, 0), u32::MAX);
    }

    #[cfg(feature = "rp2040")]
    #[test]
    /// Checks exact stock/USB/overclock ratios and nearest-rate search bounds.
    fn test_find_pll_params() {
        #[cfg(feature = "rp2040")]
        {
            // Test standard 125 MHz configuration with 12 MHz crystal
            let params = find_pll_params(12_000_000, 125_000_000).unwrap();
            assert_eq!(params.refdiv, 1);
            assert_eq!(params.fbdiv, 125);

            // Test USB PLL configuration for 48MHz
            // The algorithm may find different valid parameters than the SDK defaults
            // We'll check that it generates a valid configuration that produces 48MHz
            let params = find_pll_params(12_000_000, 48_000_000).unwrap();
            assert_eq!(params.refdiv, 1);

            // Calculate the actual output frequency
            let ref_freq = 12_000_000 / params.refdiv as u32;
            let vco_freq = ref_freq as u64 * params.fbdiv as u64;
            let output_freq = (vco_freq / ((params.post_div1 * params.post_div2) as u64)) as u32;

            // Verify the output frequency is correct
            assert_eq!(output_freq, 48_000_000);

            // Verify VCO frequency is in valid range
            assert!(vco_freq >= VCO_MIN as u64 && vco_freq <= VCO_MAX as u64);

            // Test overclocked configuration for 200 MHz
            let params = find_pll_params(12_000_000, 200_000_000).unwrap();
            assert_eq!(params.refdiv, 1);
            let vco_freq = 12_000_000 as u64 * params.fbdiv as u64;
            let output_freq = (vco_freq / ((params.post_div1 * params.post_div2) as u64)) as u32;
            assert_eq!(output_freq, 200_000_000);
            assert!(vco_freq >= VCO_MIN as u64 && vco_freq <= VCO_MAX as u64); // VCO in valid range

            // Test non-standard crystal with 16 MHz
            let params = find_pll_params(16_000_000, 125_000_000).unwrap();
            let vco_freq = (16_000_000 / params.refdiv as u32) as u64 * params.fbdiv as u64;
            let output_freq = (vco_freq / ((params.post_div1 * params.post_div2) as u64)) as u32;

            // Test non-standard crystal with 15 MHz
            let params = find_pll_params(15_000_000, 125_000_000).unwrap();
            let vco_freq = (15_000_000 / params.refdiv as u32) as u64 * params.fbdiv as u64;
            let output_freq = (vco_freq / ((params.post_div1 * params.post_div2) as u64)) as u32;

            // With a 15 MHz crystal, we might not get exactly 125 MHz
            // Check that it's close enough (within 0.2% margin)
            let freq_diff = if output_freq > 125_000_000 {
                output_freq - 125_000_000
            } else {
                125_000_000 - output_freq
            };
            let error_percentage = (freq_diff as f64 / 125_000_000.0) * 100.0;
            assert!(
                error_percentage < 0.2,
                "Output frequency {} is not close enough to target 125 MHz. Error: {:.2}%",
                output_freq,
                error_percentage
            );

            assert!(vco_freq >= VCO_MIN as u64 && vco_freq <= VCO_MAX as u64);
        }
    }

    #[cfg(feature = "rp2040")]
    #[test]
    /// Checks each PLL ratio/reference/VCO admission boundary.
    fn test_pll_config_validation() {
        // Test PLL configuration validation logic
        let valid_config = PllConfig {
            refdiv: 1,
            fbdiv: 125,
            post_div1: 6,
            post_div2: 2,
        };

        // Valid configuration should pass validation
        assert!(valid_config.is_valid(12_000_000));

        // Test fbdiv constraints
        let mut invalid_config = valid_config;
        invalid_config.fbdiv = 15; // Below minimum of 16
        assert!(!invalid_config.is_valid(12_000_000));

        invalid_config.fbdiv = 321; // Above maximum of 320
        assert!(!invalid_config.is_valid(12_000_000));

        // Test post_div constraints
        invalid_config = valid_config;
        invalid_config.post_div1 = 0; // Below minimum of 1
        assert!(!invalid_config.is_valid(12_000_000));

        invalid_config = valid_config;
        invalid_config.post_div1 = 8; // Above maximum of 7
        assert!(!invalid_config.is_valid(12_000_000));

        // Test post_div2 must be <= post_div1
        invalid_config = valid_config;
        invalid_config.post_div2 = 7;
        invalid_config.post_div1 = 3;
        assert!(!invalid_config.is_valid(12_000_000));

        // Test reference frequency constraints
        invalid_config = valid_config;
        assert!(!invalid_config.is_valid(4_000_000)); // Below minimum of 5 MHz
        assert!(!invalid_config.is_valid(900_000_000)); // Above maximum of 800 MHz

        // Test VCO frequency constraints
        invalid_config = valid_config;
        invalid_config.fbdiv = 16;
        assert!(!invalid_config.is_valid(12_000_000)); // VCO too low: 12MHz * 16 = 192MHz

        // Test VCO frequency constraints - too high
        invalid_config = valid_config;
        invalid_config.fbdiv = 200;
        invalid_config.refdiv = 1;
        // This should be INVALID: 12MHz * 200 = 2400MHz exceeds max VCO of 1600MHz
        assert!(!invalid_config.is_valid(12_000_000));

        // Test a valid high VCO configuration
        invalid_config.fbdiv = 160; // 10MHz * 160 = 1600MHz, exactly at the limit
        assert!(invalid_config.is_valid(10_000_000));
    }

    #[cfg(feature = "rp2040")]
    #[test]
    /// Checks manual ratios, selected voltage and resulting calculated frequency.
    fn test_manual_pll_helper() {
        {
            // Test the new manual_pll helper method
            let config = ClockConfig::manual_pll(
                12_000_000,
                PllConfig {
                    refdiv: 1,
                    fbdiv: 100,
                    post_div1: 3,
                    post_div2: 2,
                },
                CoreVoltage::V1_15,
            );

            // Check voltage scale was set correctly
            assert_eq!(config.core_voltage, CoreVoltage::V1_15);

            // Check PLL config was set correctly
            assert_eq!(config.xosc.as_ref().unwrap().sys_pll.as_ref().unwrap().refdiv, 1);
            assert_eq!(config.xosc.as_ref().unwrap().sys_pll.as_ref().unwrap().fbdiv, 100);
            assert_eq!(config.xosc.as_ref().unwrap().sys_pll.as_ref().unwrap().post_div1, 3);
            assert_eq!(config.xosc.as_ref().unwrap().sys_pll.as_ref().unwrap().post_div2, 2);

            // Check we get the expected frequency
            assert_eq!(
                config
                    .xosc
                    .as_ref()
                    .unwrap()
                    .sys_pll
                    .as_ref()
                    .unwrap()
                    .output_frequency(12_000_000),
                200_000_000
            );
        }
    }

    #[cfg(feature = "rp2040")]
    #[test]
    /// Checks automatic RP2040 voltage policy across requested-rate thresholds.
    fn test_auto_voltage_scaling() {
        {
            // Test automatic voltage scaling based on frequency
            // Under 133 MHz should use default voltage (V1_10)
            let config = ClockConfig::system_freq(125_000_000).unwrap();
            assert_eq!(config.core_voltage, CoreVoltage::V1_10);

            // 133-200 MHz should use V1_15
            let config = ClockConfig::system_freq(150_000_000).unwrap();
            assert_eq!(config.core_voltage, CoreVoltage::V1_15);
            let config = ClockConfig::system_freq(200_000_000).unwrap();
            assert_eq!(config.core_voltage, CoreVoltage::V1_15);

            // Above 200 MHz should use V1_15
            let config = ClockConfig::system_freq(250_000_000).unwrap();
            assert_eq!(config.core_voltage, CoreVoltage::V1_15);

            // Below 125 MHz should use V1_10
            let config = ClockConfig::system_freq(100_000_000).unwrap();
            assert_eq!(config.core_voltage, CoreVoltage::V1_10);
        }
    }
}
