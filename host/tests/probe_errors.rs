//! Probe failure taxonomy against the locked host dependency, preserving codes without private payload bytes.
/// Actual probe classification adapter, compiled on the host without radio operations.
#[path = "../../firmware/ble-probe/src/errors.rs"]
mod errors;

use bt_hci::FromHciBytesError;
use bt_hci::param::Status;
use dashboard_core::discovery::{FailureReason, ReadResult};
use trouble_host::advertise::AdvertisementDataError;
use trouble_host::att::{Att, AttErrorCode};
use trouble_host::l2cap::LeCreditConnResultCode;
use trouble_host::{BleHostError, Error};

/// Returns production bounded classification for host-stack `error`; controller
/// I/O errors are exercised separately because they use the outer error variant.
fn classify(error: Error) -> FailureReason { errors::failure_reason(BleHostError::<()>::BleHost(error)) }

/// Every ATT error byte remains the exact remote code rather than a generic local failure.
#[test]
fn all_att_bytes_preserve_the_remote_code() {
    for code in u8::MIN..=u8::MAX {
        assert_eq!(classify(Error::Att(AttErrorCode::new(code))), FailureReason::Att(code));
    }
}

/// Only the four ATT security-related codes classify standard values as protected, without initiating security
/// escalation.
#[test]
fn only_authentication_authorization_and_encryption_errors_mark_values_protected() {
    for code in u8::MIN..=u8::MAX {
        let expected = match code {
            0x05 | 0x08 | 0x0C | 0x0F => ReadResult::Protected,
            _ => ReadResult::Unavailable,
        };
        assert_eq!(
            errors::device_info_att_result(AttErrorCode::new(code)),
            expected,
            "ATT {code:02x}"
        );
    }
}

/// HCI success remains success and all nonzero controller statuses retain their exact error code.
#[test]
fn all_nonzero_hci_statuses_preserve_the_controller_code() {
    assert!(Status::SUCCESS.to_result().is_ok());
    for code in 1..=u8::MAX {
        let error = Status::new(code).to_result().unwrap_err();
        assert_eq!(classify(Error::Hci(error)), FailureReason::Hci(code));
    }
}

/// Hardware Error event bytes retain their distinct family and complete vendor code, including zero.
#[test]
fn all_hardware_error_bytes_preserve_the_vendor_code() {
    for code in u8::MIN..=u8::MAX {
        assert_eq!(classify(Error::HardwareError(code)), FailureReason::Hardware(code));
    }
}

/// Controller I/O, HCI byte decoding and ATT codec failures keep separate actionable classifications.
#[test]
fn controller_io_and_decode_failures_remain_distinct() {
    assert_eq!(
        errors::failure_reason(BleHostError::Controller(())),
        FailureReason::Controller
    );
    for error in [FromHciBytesError::InvalidSize, FromHciBytesError::InvalidValue] {
        assert_eq!(classify(Error::HciDecode(error)), FailureReason::HciDecode);
    }
    // Public ATT decoding supplies both variants of Trouble's private codec type.
    for packet in [&[][..], &[0x09, 1, 0][..]] {
        let error = Att::decode(packet).unwrap_err();
        assert_eq!(classify(Error::CodecError(error)), FailureReason::Codec);
    }
}

/// Lifecycle/resource/availability errors preserve stable action families across the locked host-stack variants.
#[test]
fn host_lifecycle_capacity_and_availability_families_remain_actionable() {
    for (error, expected) in [
        (Error::Timeout, FailureReason::Timeout),
        (Error::Disconnected, FailureReason::Disconnected),
        (Error::ChannelClosed, FailureReason::Disconnected),
        (Error::InsufficientSpace, FailureReason::Capacity),
        (Error::OutOfMemory, FailureReason::Capacity),
        (Error::ConnectionLimitReached, FailureReason::Capacity),
        (Error::GattSubscriberLimitReached, FailureReason::Capacity),
        (Error::NoChannelAvailable, FailureReason::Capacity),
        (Error::UnexpectedGattResponse, FailureReason::UnexpectedResponse),
        (Error::InvalidState, FailureReason::InvalidState),
        (Error::NotFound, FailureReason::NotFound),
        (Error::NotSupported, FailureReason::Unsupported),
        (Error::ExtendedAdvertisingNotSupported, FailureReason::Unsupported),
        (Error::Busy, FailureReason::Busy),
        (Error::NoPermits, FailureReason::Busy),
        (Error::AlreadyInUse, FailureReason::Busy),
        (Error::Other, FailureReason::Other),
    ] {
        let label = format!("{error:?}");
        assert_eq!(classify(error), expected, "{label}");
    }
}

/// Malformed payload variants retain only InvalidValue classification, deliberately excluding embedded/private bytes.
#[test]
fn invalid_payload_variants_discard_private_bytes_but_retain_failure_family() {
    for error in [
        Error::InvalidValue,
        Error::InvalidUuidLength(3),
        Error::UnexpectedDataLength { expected: 5, actual: 4 },
        Error::CannotConstructGattValue([0xA5; 16]),
        Error::MalformedCharacteristicDeclaration { expected: 5, actual: 2 },
        Error::InvalidCharacteristicDeclarationData,
        Error::FailedToFinalize {
            expected: 23,
            actual: 22,
        },
        Error::Advertisement(AdvertisementDataError::TooLong),
        Error::InvalidChannelId,
        Error::ConfigFilterAcceptListIsEmpty,
    ] {
        let label = format!("{error:?}");
        assert_eq!(classify(error), FailureReason::InvalidValue, "{label}");
    }
}

/// L2CAP connection refusals remain host errors and cannot be misrepresented as protected ATT read values.
#[test]
fn l2cap_refusals_remain_host_errors_without_becoming_protected_att_values() {
    for code in [
        LeCreditConnResultCode::SpsmNotSupported,
        LeCreditConnResultCode::NoResources,
        LeCreditConnResultCode::InsufficientAuthentication,
        LeCreditConnResultCode::InsufficientAuthorization,
        LeCreditConnResultCode::EncryptionKeyTooShort,
        LeCreditConnResultCode::InsufficientEncryption,
        LeCreditConnResultCode::InvalidSourceId,
        LeCreditConnResultCode::ScidAlreadyAllocated,
        LeCreditConnResultCode::UnacceptableParameters,
    ] {
        assert_eq!(classify(Error::L2capConnectError(code)), FailureReason::Other);
    }
}
