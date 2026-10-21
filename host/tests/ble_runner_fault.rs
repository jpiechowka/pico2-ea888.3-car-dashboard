//! Dashboard BLE runner failures preserve numeric hardware/decode detail in real bounded log messages.
/// Production formatter compiled without the hardware task itself.
#[path = "../../firmware/pico2/src/tasks/ble_error.rs"]
mod ble_error;
use core::fmt::Write;

use dashboard_core::logging::Message;
use trouble_host::{BleHostError, Error};

/// Returns `error` formatted as a host-runner failure in the production bounded
/// Message; any formatting failure leaves its prefix/truncation state inspectable.
fn render(error: Error) -> Message {
    let result: Result<(), BleHostError<()>> = Err(BleHostError::BleHost(error));
    let mut message = Message::new();
    let _ = write!(message, "{}", ble_error::RunnerFault(&result));
    message
}
/// Every controller hardware byte remains complete and visible despite the real dashboard log capacity.
#[test]
fn every_hardware_vendor_code_survives_the_real_dashboard_log_buffer() {
    for code in 0..=255 {
        let message = render(Error::HardwareError(code));
        assert!(!message.truncated(), "{:?}", message.as_str());
        assert!(message.as_str().contains(&format!("0x{code:02X}")));
    }
}
/// Both HCI decode subtypes survive bounded formatting without collapsing to an undifferentiated runner failure.
#[test]
fn hci_decode_subtype_survives_the_real_dashboard_log_buffer() {
    for error in [
        bt_hci::FromHciBytesError::InvalidSize,
        bt_hci::FromHciBytesError::InvalidValue,
    ] {
        let message = render(Error::HciDecode(error));
        assert!(!message.truncated(), "{:?}", message.as_str());
        assert!(message.as_str().contains(&format!("{error:?}")));
    }
}
