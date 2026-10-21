//! QMI CS1 peripheral for RP235x
//!
//! This module provides access to the QMI CS1 functionality for use with external memory devices
//! such as PSRAM. The QMI (Quad SPI) controller supports CS1 as a second chip select signal.
//!
//! This peripheral is only available on RP235x chips.

#![cfg(feature = "_rp235x")]

use embassy_hal_internal::{Peri, PeripheralType};

use crate::gpio::Pin as GpioPin;
use crate::{pac, peripherals};

/// QMI CS1 driver.
pub struct QmiCs1<'d> {
    /// Exclusive CS1 peripheral ownership; external memory configuration is separate.
    _inner: Peri<'d, peripherals::QMI_CS1>,
}

impl<'d> QmiCs1<'d> {
    /// Returns CS1 ownership from `qmi_cs1` after selecting function 9 on `cs1`.
    /// Sets 12mA drive, fast slew, input enable and clears isolation; does not configure memory timing.
    pub fn new(
        qmi_cs1: Peri<'d, peripherals::QMI_CS1>,
        cs1: Peri<'d, impl QmiCs1Pin>,
    ) -> Self {
        // Configure CS1 pin for QMI function (funcsel = 9)
        cs1.gpio().ctrl().write(|w| w.set_funcsel(9));

        // Configure pad settings for high-speed operation
        cs1.pad_ctrl().write(|w| {
            #[cfg(feature = "_rp235x")]
            w.set_iso(false);
            w.set_ie(true);
            w.set_drive(pac::pads::vals::Drive::_12M_A);
            w.set_slewfast(true);
        });

        Self { _inner: qmi_cs1 }
    }
}

/// Restricts instances to the HAL QMI_CS1 singleton.
trait SealedInstance {}

/// QMI CS1 instance trait.
#[allow(private_bounds)]
pub trait Instance: SealedInstance + PeripheralType {}

impl SealedInstance for peripherals::QMI_CS1 {}

impl Instance for peripherals::QMI_CS1 {}

/// CS1 pin trait for QMI.
pub trait QmiCs1Pin: GpioPin {}

// Implement pin traits for CS1-capable GPIO pins
impl QmiCs1Pin for peripherals::PIN_0 {}
impl QmiCs1Pin for peripherals::PIN_8 {}
impl QmiCs1Pin for peripherals::PIN_19 {}
#[cfg(feature = "rp235xb")]
impl QmiCs1Pin for peripherals::PIN_47 {}
