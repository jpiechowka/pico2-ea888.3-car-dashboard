//! Compact host faults for the bounded dashboard log.
use core::fmt;

use trouble_host::{BleHostError, Error};

pub struct RunnerFault<'a, E>(pub &'a Result<(), BleHostError<E>>);
impl<E: fmt::Debug> fmt::Display for RunnerFault<'_, E> {
    fn fmt(
        &self,
        f: &mut fmt::Formatter<'_>,
    ) -> fmt::Result {
        match self.0 {
            Err(BleHostError::BleHost(Error::HardwareError(code))) => write!(f, "BLE HCI hardware 0x{code:02X}"),
            Err(BleHostError::BleHost(Error::HciDecode(reason))) => write!(f, "BLE HCI decode {reason:?}"),
            _ => write!(f, "BLE HCI stopped: {:?}", self.0),
        }
    }
}
