//! PWM slice ownership, shared-period channels and counter input modes.
//!
//! Compare/TOP writes are buffered while running; configuration changes are not an
//! atomic waveform update. Busy waits have no timeout. See the
//! [Pico SDK PWM contract](https://www.raspberrypi.com/documentation/pico-sdk/hardware.html#group_hardware_pwm).
//! Trait duty values are raw compare counts through TOP+1; TOP 65535 cannot
//! represent full-on and returns an explicit error. Polarity follows configured inversion.

use embassy_hal_internal::{Peri, PeripheralType};
pub use embedded_hal_1::pwm::SetDutyCycle;
use embedded_hal_1::pwm::{Error, ErrorKind, ErrorType};
use fixed::FixedU16;
use fixed::traits::ToFixed;
use pac::pwm::regs::{ChDiv, Intr};
use pac::pwm::vals::Divmode;

use crate::gpio::{AnyPin, Pin as GpioPin, Pull, SealedPin as _};
use crate::{RegExt, pac, peripherals};

/// The configuration of a PWM slice.
/// Note the period in clock cycles of a slice can be computed as:
/// `(top + 1) * (phase_correct ? 2 : 1) * divider` in free-running divider mode.
/// Phase-correct mode repeats endpoints (including the 0-to-0 latch transition);
/// TOP 0 holds the counter at zero. Compare 0/1 still selects inactive/active output.
/// See RP2040 datasheet sections 4.5.2.2–4.5.2.3; raw divider zero encodes 256.
#[non_exhaustive]
#[derive(Clone)]
pub struct Config {
    /// Inverts the PWM output signal on channel A.
    pub invert_a: bool,
    /// Inverts the PWM output signal on channel B.
    pub invert_b: bool,
    /// Enables phase-correct mode for PWM operation.
    /// In phase-correct mode, the PWM signal is generated in such a way that
    /// the pulse is always centered regardless of the duty cycle.
    /// The output frequency is halved when phase-correct mode is enabled.
    pub phase_correct: bool,
    /// Enables the PWM slice, allowing it to generate an output.
    pub enable: bool,
    /// A fractional clock divider, represented as a fixed-point number with
    /// 8 integer bits and 4 fractional bits. It allows precise control over
    /// the PWM output frequency by gating the PWM counter increment.
    /// A higher value will result in a slower output frequency.
    /// Values above 255.9375 panic; lower-bound and hardware zero-encoding semantics
    /// are not validated by this driver. Use at least 1 for ordinary division.
    pub divider: fixed::FixedU16<fixed::types::extra::U4>,
    /// The output on channel A goes high when `compare_a` is higher than the
    /// counter. A compare of 0 will produce an always low output, while a
    /// compare of `top + 1` will produce an always high output before inversion.
    /// At `top == u16::MAX`, that full-on compare is not representable.
    pub compare_a: u16,
    /// The output on channel B goes high when `compare_b` is higher than the
    /// counter. A compare of 0 will produce an always low output, while a
    /// compare of `top + 1` will produce an always high output before inversion.
    /// At `top == u16::MAX`, that full-on compare is not representable.
    pub compare_b: u16,
    /// The point at which the counter wraps, representing the maximum possible
    /// period. The counter will either wrap to 0 or reverse depending on the
    /// setting of `phase_correct`.
    pub top: u16,
}

impl Default for Config {
    /// Returns an enabled, noninverted, free-running slice with divider 1, TOP 65535 and both compares zero.
    fn default() -> Self {
        Self {
            invert_a: false,
            invert_b: false,
            phase_correct: false,
            enable: true, // differs from reset value
            divider: 1.to_fixed(),
            compare_a: 0,
            compare_b: 0,
            top: 0xFFFF,
        }
    }
}

/// PWM input mode.
pub enum InputMode {
    /// Advance the divider only while channel B input is high.
    Level,
    /// Advance the divider on each rising edge of channel B input.
    RisingEdge,
    /// Advance the divider on each falling edge of channel B input.
    FallingEdge,
}

impl From<InputMode> for Divmode {
    /// Returns the hardware divider gate/edge mode for `value`.
    fn from(value: InputMode) -> Self {
        match value {
            InputMode::Level => Divmode::LEVEL,
            InputMode::RisingEdge => Divmode::RISE,
            InputMode::FallingEdge => Divmode::FALL,
        }
    }
}

/// PWM error.
#[derive(Debug)]
pub enum PwmError {
    /// Requested duty exceeded the advertised scale.
    InvalidDutyCycle,
    /// TOP 65535 needs compare 65536 for full-on, which the hardware cannot represent.
    UnrepresentableFullDuty,
}

impl Error for PwmError {
    /// Returns Other for either range or unrepresentable endpoint errors.
    fn kind(&self) -> ErrorKind {
        match self {
            PwmError::InvalidDutyCycle | PwmError::UnrepresentableFullDuty => ErrorKind::Other,
        }
    }
}

/// PWM driver.
pub struct Pwm<'d> {
    /// Owned A output pin, or `None` for unconnected/input-only configurations.
    pin_a: Option<Peri<'d, AnyPin>>,
    /// Owned B pin (output or counter input), or `None` when unconnected.
    pin_b: Option<Peri<'d, AnyPin>>,
    /// Hardware slice index selected by the consumed peripheral token.
    slice: usize,
}

impl<'d> ErrorType for Pwm<'d> {
    /// Duty exceeded the scale or requested an unrepresentable full-on endpoint.
    type Error = PwmError;
}

impl<'d> SetDutyCycle for Pwm<'d> {
    /// Returns TOP+1, or 65535 as an unrepresentable full-on sentinel when TOP is 65535. Setting that sentinel returns
    /// `UnrepresentableFullDuty`.
    fn max_duty_cycle(&self) -> u16 { max_duty(pac::PWM.ch(self.slice).top().read().top()) }

    /// Writes raw compare `duty` to the controlled channel(s), returning `Ok(())`, `InvalidDutyCycle` above the scale,
    /// or `UnrepresentableFullDuty` for full-on at TOP 65535. Errors preserve prior compares; polarity follows
    /// configured inversion.
    fn set_duty_cycle(
        &mut self,
        duty: u16,
    ) -> Result<(), Self::Error> {
        let p = pac::PWM.ch(self.slice);
        let duty = duty_compare(p.top().read().top(), duty)?;
        p.cc().modify(|w| {
            w.set_a(duty);
            w.set_b(duty);
        });
        Ok(())
    }
}

impl<'d> Pwm<'d> {
    /// Returns slice `slice` with optional `a` output and `b` input/output pins, `b_pull`, timing `config` and divider
    /// source `divmode`. Resets the counter and may enable before pin mux; panics on an oversized divider.
    fn new_inner(
        slice: usize,
        a: Option<Peri<'d, AnyPin>>,
        b: Option<Peri<'d, AnyPin>>,
        b_pull: Pull,
        config: Config,
        divmode: Divmode,
    ) -> Self {
        let p = pac::PWM.ch(slice);
        p.csr().modify(|w| {
            w.set_divmode(divmode);
            w.set_en(false);
        });
        p.ctr().write(|w| w.0 = 0);
        Self::configure(p, &config);

        if let Some(pin) = &a {
            pin.gpio().ctrl().write(|w| w.set_funcsel(4));
            #[cfg(feature = "_rp235x")]
            pin.pad_ctrl().modify(|w| {
                w.set_iso(false);
            });
        }
        if let Some(pin) = &b {
            pin.gpio().ctrl().write(|w| w.set_funcsel(4));
            pin.pad_ctrl().modify(|w| {
                #[cfg(feature = "_rp235x")]
                w.set_iso(false);
                #[cfg(feature = "_rp235x")]
                if divmode != Divmode::DIV {
                    // Is in input mode and so must enable input mode for the pin
                    w.set_ie(true);
                }
                w.set_pue(b_pull == Pull::Up);
                w.set_pde(b_pull == Pull::Down);
            });
        }
        Self {
            // inner: p.into(),
            pin_a: a,
            pin_b: b,
            slice,
        }
    }

    /// Create PWM driver without any configured pins.
    #[inline]
    /// Returns a counter owning `slice`, with `config` applied and no GPIO routes. Panics on an oversized divider.
    pub fn new_free<T: Slice>(
        slice: Peri<'d, T>,
        config: Config,
    ) -> Self {
        Self::new_inner(slice.number(), None, None, Pull::None, config, Divmode::DIV)
    }

    /// Create PWM driver with a single 'a' pin as output.
    #[inline]
    /// Returns an A output owning `slice` and `a`, applying `config` and resetting its counter. Panics on an oversized
    /// divider.
    pub fn new_output_a<T: Slice>(
        slice: Peri<'d, T>,
        a: Peri<'d, impl ChannelAPin<T>>,
        config: Config,
    ) -> Self {
        Self::new_inner(slice.number(), Some(a.into()), None, Pull::None, config, Divmode::DIV)
    }

    /// Create PWM driver with a single 'b' pin as output.
    #[inline]
    /// Returns a B output owning `slice` and `b`, applying `config` and resetting its counter. Panics on an oversized
    /// divider.
    pub fn new_output_b<T: Slice>(
        slice: Peri<'d, T>,
        b: Peri<'d, impl ChannelBPin<T>>,
        config: Config,
    ) -> Self {
        Self::new_inner(slice.number(), None, Some(b.into()), Pull::None, config, Divmode::DIV)
    }

    /// Create PWM driver with a 'a' and 'b' pins as output.
    #[inline]
    /// Returns shared-period outputs owning `slice`, `a` and `b`, applying `config`. Resets the counter; panics on an
    /// oversized divider.
    pub fn new_output_ab<T: Slice>(
        slice: Peri<'d, T>,
        a: Peri<'d, impl ChannelAPin<T>>,
        b: Peri<'d, impl ChannelBPin<T>>,
        config: Config,
    ) -> Self {
        Self::new_inner(
            slice.number(),
            Some(a.into()),
            Some(b.into()),
            Pull::None,
            config,
            Divmode::DIV,
        )
    }

    /// Create PWM driver with a single 'b' as input pin.
    #[inline]
    /// Returns a counter owning `slice` and input `b`, with `b_pull`, input gate/edge `mode` and timing `config`.
    /// Resets the counter; panics on an oversized divider.
    pub fn new_input<T: Slice>(
        slice: Peri<'d, T>,
        b: Peri<'d, impl ChannelBPin<T>>,
        b_pull: Pull,
        mode: InputMode,
        config: Config,
    ) -> Self {
        Self::new_inner(slice.number(), None, Some(b.into()), b_pull, config, mode.into())
    }

    /// Create PWM driver with a 'a' and 'b' pins in the desired input mode.
    #[inline]
    /// Returns A output `a` gated by B input `b`, owning `slice` with `b_pull`, input `mode` and timing `config`.
    /// Resets the counter; panics on an oversized divider.
    pub fn new_output_input<T: Slice>(
        slice: Peri<'d, T>,
        a: Peri<'d, impl ChannelAPin<T>>,
        b: Peri<'d, impl ChannelBPin<T>>,
        b_pull: Pull,
        mode: InputMode,
        config: Config,
    ) -> Self {
        Self::new_inner(
            slice.number(),
            Some(a.into()),
            Some(b.into()),
            b_pull,
            config,
            mode.into(),
        )
    }

    /// Set the PWM config.
    /// Applies `config` timing, compares, inversion and enable state to this slice. Panics on an oversized divider
    /// before writes; does not reset its counter or change pin/input routes.
    pub fn set_config(
        &mut self,
        config: &Config,
    ) {
        Self::configure(pac::PWM.ch(self.slice), config);
    }

    /// Writes `config` into register block `p`, panicking for divider bits above 0xfff before any write. TOP/compare
    /// buffering follows running hardware; no complete-config atomicity is provided.
    fn configure(
        p: pac::pwm::Channel,
        config: &Config,
    ) {
        if config.divider > FixedU16::<fixed::types::extra::U4>::from_bits(0xFFF) {
            panic!("Requested divider is too large");
        }

        p.div().write_value(ChDiv(config.divider.to_bits() as u32));
        p.cc().write(|w| {
            w.set_a(config.compare_a);
            w.set_b(config.compare_b);
        });
        p.top().write(|w| w.set_top(config.top));
        p.csr().modify(|w| {
            w.set_a_inv(config.invert_a);
            w.set_b_inv(config.invert_b);
            w.set_ph_correct(config.phase_correct);
            w.set_en(config.enable);
        });
    }

    /// Advances a slice's output phase by one count while it is running
    /// by inserting a pulse into the clock enable. The counter
    /// will not count faster than once per cycle.
    #[inline]
    /// Waits until the requested advance completes; can spin indefinitely if the clock cannot advance (including
    /// disabled slice or divider 1).
    pub fn phase_advance(&mut self) {
        let p = pac::PWM.ch(self.slice);
        p.csr().write_set(|w| w.set_ph_adv(true));
        while p.csr().read().ph_adv() {}
    }

    /// Retards a slice's output phase by one count while it is running
    /// by deleting a pulse from the clock enable. The counter will not
    /// count backward when clock enable is permanently low.
    #[inline]
    /// Waits until the requested retard completes; can spin indefinitely if the slice clock is stopped.
    pub fn phase_retard(&mut self) {
        let p = pac::PWM.ch(self.slice);
        p.csr().write_set(|w| w.set_ph_ret(true));
        while p.csr().read().ph_ret() {}
    }

    /// Read PWM counter.
    #[inline]
    /// Returns the current raw counter value without stopping it.
    pub fn counter(&self) -> u16 { pac::PWM.ch(self.slice).ctr().read().ctr() }

    /// Write PWM counter.
    #[inline]
    /// Writes raw counter value `ctr` immediately, without validating it against TOP.
    pub fn set_counter(
        &self,
        ctr: u16,
    ) {
        pac::PWM.ch(self.slice).ctr().write(|w| w.set_ctr(ctr))
    }

    /// Wait for channel interrupt.
    #[inline]
    /// Consumes an already-pending or future wrap flag, spinning without timeout; a stopped slice may never wrap.
    pub fn wait_for_wrap(&mut self) {
        while !self.wrapped() {}
        self.clear_wrapped();
    }

    /// Check if interrupt for channel is set.
    #[inline]
    /// Returns whether this slice has a latched wrap interrupt flag, without clearing it.
    pub fn wrapped(&mut self) -> bool { pac::PWM.intr().read().0 & self.bit() != 0 }

    #[inline]
    /// Clear interrupt flag.
    /// Clears only this slice's latched wrap flag through the write-one-to-clear register.
    pub fn clear_wrapped(&mut self) { pac::PWM.intr().write_value(Intr(self.bit() as _)); }

    #[inline]
    /// Returns the enable/interrupt bit for this owned slice.
    fn bit(&self) -> u32 { 1 << self.slice as usize }

    /// Splits the PWM driver into separate `PwmOutput` instances for channels A and B.
    #[inline]
    /// Returns optional owned A/B channel handles for present pins, preserving the running slice. No-pin slices are
    /// stopped by drop; B input mode is not converted into an output mode.
    pub fn split(mut self) -> (Option<PwmOutput<'d>>, Option<PwmOutput<'d>>) {
        let outputs = (
            self.pin_a
                .take()
                .map(|pin| PwmOutput::new(PwmChannelPin::A(pin), self.slice, true)),
            self.pin_b
                .take()
                .map(|pin| PwmOutput::new(PwmChannelPin::B(pin), self.slice, true)),
        );
        if outputs.0.is_some() || outputs.1.is_some() {
            // Outputs now own the running slice. Dropping the original driver
            // would stop both channels before their owners could use them.
            core::mem::forget(self);
        }
        outputs
    }

    /// Splits the PWM driver by reference to allow for separate duty cycle control
    /// of each channel (A and B) without taking ownership of the PWM instance.
    #[inline]
    /// Returns optional borrowed A/B handles for present pins; their drop leaves hardware intact and borrowing prevents
    /// slice reconfiguration while they live.
    pub fn split_by_ref(&mut self) -> (Option<PwmOutput<'_>>, Option<PwmOutput<'_>>) {
        (
            self.pin_a
                .as_mut()
                .map(|pin| PwmOutput::new(PwmChannelPin::A(pin.reborrow()), self.slice.clone(), false)),
            self.pin_b
                .as_mut()
                .map(|pin| PwmOutput::new(PwmChannelPin::B(pin.reborrow()), self.slice.clone(), false)),
        )
    }
}

/// Tracks one channel's pin ownership after splitting a slice.
enum PwmChannelPin<'d> {
    /// Compare A and its routed output pin.
    A(
        /// Exclusive A pin token.
        Peri<'d, AnyPin>,
    ),
    /// Compare B and its routed pin, including input-mode B pins.
    B(
        /// Exclusive B pin token.
        Peri<'d, AnyPin>,
    ),
}

/// Single channel of Pwm driver.
pub struct PwmOutput<'d> {
    /// Pin token plus the compare-register half it controls.
    channel_pin: PwmChannelPin<'d>,
    /// Hardware slice index selected by the consumed peripheral token.
    slice: usize,
    /// Whether drop disconnects this pin; borrowed split outputs leave hardware intact.
    is_owned: bool,
}

impl<'d> PwmOutput<'d> {
    /// Returns a channel handle for `channel_pin` on `slice`; `is_owned` enables pin-disconnect cleanup on drop.
    fn new(
        channel_pin: PwmChannelPin<'d>,
        slice: usize,
        is_owned: bool,
    ) -> Self {
        Self {
            channel_pin,
            slice,
            is_owned,
        }
    }
}

impl<'d> Drop for PwmOutput<'d> {
    /// Owned handles zero their compare, disconnect the pin and enable pulldown; borrowed handles do nothing. The
    /// shared slice keeps running.
    fn drop(&mut self) {
        if self.is_owned {
            let p = pac::PWM.ch(self.slice);
            match &self.channel_pin {
                PwmChannelPin::A(pin) => {
                    p.cc().modify(|w| {
                        w.set_a(0);
                    });

                    pin.gpio().ctrl().write(|w| w.set_funcsel(31));
                    // Enable pin PULL-DOWN
                    pin.pad_ctrl().modify(|w| {
                        w.set_pde(true);
                    });
                }
                PwmChannelPin::B(pin) => {
                    p.cc().modify(|w| {
                        w.set_b(0);
                    });
                    pin.gpio().ctrl().write(|w| w.set_funcsel(31));
                    // Enable pin PULL-DOWN
                    pin.pad_ctrl().modify(|w| {
                        w.set_pde(true);
                    });
                }
            }
        }
    }
}

impl<'d> ErrorType for PwmOutput<'d> {
    /// Duty exceeded the scale or requested an unrepresentable full-on endpoint.
    type Error = PwmError;
}

impl<'d> SetDutyCycle for PwmOutput<'d> {
    /// Returns TOP+1, or 65535 as an unrepresentable full-on sentinel when TOP is 65535. Setting that sentinel returns
    /// `UnrepresentableFullDuty`.
    fn max_duty_cycle(&self) -> u16 { max_duty(pac::PWM.ch(self.slice).top().read().top()) }

    /// Writes raw compare `duty` to the controlled channel(s), returning `Ok(())`, `InvalidDutyCycle` above the scale,
    /// or `UnrepresentableFullDuty` for full-on at TOP 65535. Errors preserve prior compares; polarity follows
    /// configured inversion.
    fn set_duty_cycle(
        &mut self,
        duty: u16,
    ) -> Result<(), Self::Error> {
        let p = pac::PWM.ch(self.slice);
        let duty = duty_compare(p.top().read().top(), duty)?;
        match self.channel_pin {
            PwmChannelPin::A(_) => {
                p.cc().modify(|w| {
                    w.set_a(duty);
                });
            }
            PwmChannelPin::B(_) => {
                p.cc().modify(|w| {
                    w.set_b(duty);
                });
            }
        }

        Ok(())
    }
}

/// Batch representation of PWM slices.
pub struct PwmBatch(
    /// Bitmask of slices selected for one enable-register alias write.
    u32,
);

impl PwmBatch {
    #[inline]
    /// Enable a PWM slice in this batch.
    /// Adds `pwm`'s slice to this pending batch; performs no register write.
    pub fn enable(
        &mut self,
        pwm: &Pwm<'_>,
    ) {
        self.0 |= pwm.bit();
    }

    #[inline]
    /// Enable slices in this batch in a PWM.
    /// Runs `batch` to select slices, then atomically sets or clears their enable bits according to `enabled`. Other
    /// slices are unchanged; a panicking closure performs no batch write.
    pub fn set_enabled(
        enabled: bool,
        batch: impl FnOnce(&mut PwmBatch),
    ) {
        let mut en = PwmBatch(0);
        batch(&mut en);
        if enabled {
            pac::PWM.en().write_set(|w| w.0 = en.0);
        } else {
            pac::PWM.en().write_clear(|w| w.0 = en.0);
        }
    }
}

impl<'d> Drop for Pwm<'d> {
    /// Stops the slice, disconnects present pins and enables pulldowns; RP235x B input is disabled. Existing pullups
    /// are not cleared.
    fn drop(&mut self) {
        stop_slice(pac::PWM.ch(self.slice));
        if let Some(pin) = &self.pin_a {
            pin.gpio().ctrl().write(|w| w.set_funcsel(31));
            // Enable pin PULL-DOWN
            pin.pad_ctrl().modify(|w| {
                w.set_pde(true);
            });
        }
        if let Some(pin) = &self.pin_b {
            pin.gpio().ctrl().write(|w| w.set_funcsel(31));
            #[cfg(feature = "_rp235x")]
            // Disable input mode. Only pin_b can be input, so not needed for pin_a
            pin.pad_ctrl().modify(|w| {
                w.set_ie(false);
            });
            // Enable pin PULL-DOWN
            pin.pad_ctrl().modify(|w| {
                w.set_pde(true);
            });
        }
    }
}

/// Disables only register block `p`'s enable bit through its atomic clear alias.
fn stop_slice(p: pac::pwm::Channel) { p.csr().write_clear(|w| w.set_en(true)); }

/// Register-alias and transfer-of-ownership regressions without real peripherals.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    /// Checks teardown targets the enable bit in the clear alias, without touching neighboring registers.
    fn teardown_writes_only_enable_bit_to_clear_alias() {
        let mut registers = [0u32; 4096];
        let channel = unsafe { pac::pwm::Channel::from_ptr(registers.as_mut_ptr().cast()) };
        stop_slice(channel);
        assert_eq!(registers[0x3000 / 4], 1);
        assert_eq!(registers[0], 0);
        assert_eq!(registers[0x3000 / 4 + 1], 0);
    }

    #[test]
    /// Checks split moves ownership without reaching MMIO teardown; returned handles are deliberately not dropped.
    fn split_transfers_ownership_without_stopping_the_slice() {
        // No MMIO is initialized: split must only move ownership. Keep the
        // returned output from running its hardware destructor in this test.
        let pwm = Pwm {
            pin_a: Some(unsafe { AnyPin::steal(0) }),
            pin_b: None,
            slice: 0,
        };
        let outputs = core::mem::ManuallyDrop::new(pwm.split());
        assert!(outputs.0.as_ref().unwrap().is_owned);
        assert!(outputs.1.is_none());
    }
}

/// Restricts slice implementations to physical peripheral tokens.
trait SealedSlice {}

/// PWM Slice.
#[allow(private_bounds)]
pub trait Slice: PeripheralType + SealedSlice + Sized + 'static {
    /// Slice number.
    /// Returns this token's fixed hardware slice index.
    fn number(&self) -> usize;
}

/// Gives each slice token its fixed register-array index.
macro_rules! slice {
    ($name:ident, $num:expr) => {
        impl SealedSlice for peripherals::$name {}
        impl Slice for peripherals::$name {
            /// Returns this token's fixed hardware slice index.
            fn number(&self) -> usize { $num }
        }
    };
}

slice!(PWM_SLICE0, 0);
slice!(PWM_SLICE1, 1);
slice!(PWM_SLICE2, 2);
slice!(PWM_SLICE3, 3);
slice!(PWM_SLICE4, 4);
slice!(PWM_SLICE5, 5);
slice!(PWM_SLICE6, 6);
slice!(PWM_SLICE7, 7);

#[cfg(feature = "_rp235x")]
slice!(PWM_SLICE8, 8);
#[cfg(feature = "_rp235x")]
slice!(PWM_SLICE9, 9);
#[cfg(feature = "_rp235x")]
slice!(PWM_SLICE10, 10);
#[cfg(feature = "_rp235x")]
slice!(PWM_SLICE11, 11);

/// PWM Channel A.
pub trait ChannelAPin<T: Slice>: GpioPin {}
/// PWM Channel B.
pub trait ChannelBPin<T: Slice>: GpioPin {}

/// Restricts A/B pin routes to the silicon alternate-function table.
macro_rules! impl_pin {
    ($pin:ident, $channel:ident, $kind:ident) => {
        impl $kind<peripherals::$channel> for peripherals::$pin {}
    };
}

impl_pin!(PIN_0, PWM_SLICE0, ChannelAPin);
impl_pin!(PIN_1, PWM_SLICE0, ChannelBPin);
impl_pin!(PIN_2, PWM_SLICE1, ChannelAPin);
impl_pin!(PIN_3, PWM_SLICE1, ChannelBPin);
impl_pin!(PIN_4, PWM_SLICE2, ChannelAPin);
impl_pin!(PIN_5, PWM_SLICE2, ChannelBPin);
impl_pin!(PIN_6, PWM_SLICE3, ChannelAPin);
impl_pin!(PIN_7, PWM_SLICE3, ChannelBPin);
impl_pin!(PIN_8, PWM_SLICE4, ChannelAPin);
impl_pin!(PIN_9, PWM_SLICE4, ChannelBPin);
impl_pin!(PIN_10, PWM_SLICE5, ChannelAPin);
impl_pin!(PIN_11, PWM_SLICE5, ChannelBPin);
impl_pin!(PIN_12, PWM_SLICE6, ChannelAPin);
impl_pin!(PIN_13, PWM_SLICE6, ChannelBPin);
impl_pin!(PIN_14, PWM_SLICE7, ChannelAPin);
impl_pin!(PIN_15, PWM_SLICE7, ChannelBPin);
impl_pin!(PIN_16, PWM_SLICE0, ChannelAPin);
impl_pin!(PIN_17, PWM_SLICE0, ChannelBPin);
impl_pin!(PIN_18, PWM_SLICE1, ChannelAPin);
impl_pin!(PIN_19, PWM_SLICE1, ChannelBPin);
impl_pin!(PIN_20, PWM_SLICE2, ChannelAPin);
impl_pin!(PIN_21, PWM_SLICE2, ChannelBPin);
impl_pin!(PIN_22, PWM_SLICE3, ChannelAPin);
impl_pin!(PIN_23, PWM_SLICE3, ChannelBPin);
impl_pin!(PIN_24, PWM_SLICE4, ChannelAPin);
impl_pin!(PIN_25, PWM_SLICE4, ChannelBPin);
impl_pin!(PIN_26, PWM_SLICE5, ChannelAPin);
impl_pin!(PIN_27, PWM_SLICE5, ChannelBPin);
impl_pin!(PIN_28, PWM_SLICE6, ChannelAPin);
impl_pin!(PIN_29, PWM_SLICE6, ChannelBPin);
#[cfg(feature = "rp235xb")]
impl_pin!(PIN_30, PWM_SLICE7, ChannelAPin);
#[cfg(feature = "rp235xb")]
impl_pin!(PIN_31, PWM_SLICE7, ChannelBPin);
#[cfg(feature = "rp235xb")]
impl_pin!(PIN_32, PWM_SLICE8, ChannelAPin);
#[cfg(feature = "rp235xb")]
impl_pin!(PIN_33, PWM_SLICE8, ChannelBPin);
#[cfg(feature = "rp235xb")]
impl_pin!(PIN_34, PWM_SLICE9, ChannelAPin);
#[cfg(feature = "rp235xb")]
impl_pin!(PIN_35, PWM_SLICE9, ChannelBPin);
#[cfg(feature = "rp235xb")]
impl_pin!(PIN_36, PWM_SLICE10, ChannelAPin);
#[cfg(feature = "rp235xb")]
impl_pin!(PIN_37, PWM_SLICE10, ChannelBPin);
#[cfg(feature = "rp235xb")]
impl_pin!(PIN_38, PWM_SLICE11, ChannelAPin);
#[cfg(feature = "rp235xb")]
impl_pin!(PIN_39, PWM_SLICE11, ChannelBPin);
#[cfg(feature = "rp235xb")]
impl_pin!(PIN_40, PWM_SLICE8, ChannelAPin);
#[cfg(feature = "rp235xb")]
impl_pin!(PIN_41, PWM_SLICE8, ChannelBPin);
#[cfg(feature = "rp235xb")]
impl_pin!(PIN_42, PWM_SLICE9, ChannelAPin);
#[cfg(feature = "rp235xb")]
impl_pin!(PIN_43, PWM_SLICE9, ChannelBPin);
#[cfg(feature = "rp235xb")]
impl_pin!(PIN_44, PWM_SLICE10, ChannelAPin);
#[cfg(feature = "rp235xb")]
impl_pin!(PIN_45, PWM_SLICE10, ChannelBPin);
#[cfg(feature = "rp235xb")]
impl_pin!(PIN_46, PWM_SLICE11, ChannelAPin);
#[cfg(feature = "rp235xb")]
impl_pin!(PIN_47, PWM_SLICE11, ChannelBPin);

/// Returns TOP+1 for counter limit `top`, saturating to the rejected full-on sentinel 65535.
fn max_duty(top: u16) -> u16 { top.saturating_add(1) }

/// Returns raw compare for `duty` at counter limit `top`, or range/unrepresentable-full errors. Zero remains inactive
/// before inversion; modes share these endpoints.
fn duty_compare(
    top: u16,
    duty: u16,
) -> Result<u16, PwmError> {
    if top == u16::MAX && duty == u16::MAX {
        Err(PwmError::UnrepresentableFullDuty)
    } else if duty > max_duty(top) {
        Err(PwmError::InvalidDutyCycle)
    } else {
        Ok(duty)
    }
}

/// Trait defaults exercised through the production duty-scale and compare admission helpers.
#[cfg(test)]
mod duty_tests {
    use super::*;
    /// Software adapter retains the admitted compare for assertions, without peripheral MMIO.
    struct Output {
        /// Counter maximum used by both free-running and phase-correct modes.
        top: u16,
        /// Last admitted compare count.
        compare: u16,
    }
    impl ErrorType for Output {
        /// Same admission errors as the hardware adapter.
        type Error = PwmError;
    }
    impl SetDutyCycle for Output {
        /// Returns the production duty scale for this test counter limit.
        fn max_duty_cycle(&self) -> u16 { max_duty(self.top) }

        /// Applies `duty` through production admission, returning its result without MMIO.
        fn set_duty_cycle(
            &mut self,
            duty: u16,
        ) -> Result<(), PwmError> {
            self.compare = duty_compare(self.top, duty)?;
            Ok(())
        }
    }
    /// TOP 0 must still distinguish inactive and active; normal TOP exposes TOP+1.
    #[test]
    fn endpoints_and_fraction_defaults() {
        let mut output = Output { top: 0, compare: 0 };
        assert_eq!(output.max_duty_cycle(), 1);
        output.set_duty_cycle_fully_on().unwrap();
        assert_eq!(output.compare, 1);
        output.set_duty_cycle_fully_off().unwrap();
        assert_eq!(output.compare, 0);
        output.top = 9999;
        assert_eq!(output.max_duty_cycle(), 10000);
        output.set_duty_cycle_percent(100).unwrap();
        assert_eq!(output.compare, 10000);
        output.set_duty_cycle_fraction(1, 2).unwrap();
        assert_eq!(output.compare, 5000);
        output.set_duty_cycle_percent(0).unwrap();
        assert_eq!(output.compare, 0);
        assert!(output.set_duty_cycle(10001).is_err());
        assert_eq!(output.compare, 0);
    }
    /// The MAX sentinel must fail instead of pretending a pulse train is continuously active.
    #[test]
    fn maximum_top_rejects_unrepresentable_full_on() {
        let mut output = Output {
            top: u16::MAX,
            compare: 7,
        };
        assert_eq!(output.max_duty_cycle(), u16::MAX);
        assert!(matches!(
            output.set_duty_cycle_fully_on(),
            Err(PwmError::UnrepresentableFullDuty)
        ));
        assert_eq!(output.compare, 7);
        assert!(output.set_duty_cycle_percent(100).is_err());
        output.set_duty_cycle(u16::MAX - 1).unwrap();
        assert_eq!(output.compare, u16::MAX - 1);
        output.set_duty_cycle_percent(50).unwrap();
        assert_eq!(output.compare, 32767);
    }
    /// The locked embedded-hal default forbids a zero denominator before any setter call.
    #[test]
    #[should_panic]
    fn zero_fraction_denominator_panics() { Output { top: 99, compare: 0 }.set_duty_cycle_fraction(0, 0).unwrap(); }
}
