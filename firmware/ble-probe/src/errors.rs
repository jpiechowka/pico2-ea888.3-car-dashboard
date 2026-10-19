//! Bounded classifications used by the radio and exact-dependency host tests.
use dashboard_core::discovery::{FailureReason, ReadResult};
use trouble_host::att::AttErrorCode;
use trouble_host::{BleHostError, Error};

pub fn failure_reason<E>(error: BleHostError<E>) -> FailureReason {
    match error {
        BleHostError::Controller(_) => FailureReason::Controller,
        BleHostError::BleHost(error) => {
            match error {
                Error::Timeout => FailureReason::Timeout,
                Error::Disconnected | Error::ChannelClosed => FailureReason::Disconnected,
                Error::InsufficientSpace
                | Error::OutOfMemory
                | Error::ConnectionLimitReached
                | Error::GattSubscriberLimitReached
                | Error::NoChannelAvailable => FailureReason::Capacity,
                Error::Att(code) => FailureReason::Att(code.to_u8()),
                Error::Hci(code) => FailureReason::Hci(code.into()),
                Error::HciDecode(_) => FailureReason::HciDecode,
                Error::HardwareError(code) => FailureReason::Hardware(code),
                Error::UnexpectedGattResponse => FailureReason::UnexpectedResponse,
                Error::CodecError(_) => FailureReason::Codec,
                Error::InvalidState => FailureReason::InvalidState,
                Error::NotFound => FailureReason::NotFound,
                Error::NotSupported | Error::ExtendedAdvertisingNotSupported => FailureReason::Unsupported,
                Error::Busy | Error::NoPermits | Error::AlreadyInUse => FailureReason::Busy,
                Error::InvalidValue
                | Error::InvalidUuidLength(_)
                | Error::UnexpectedDataLength { .. }
                | Error::CannotConstructGattValue(_)
                | Error::MalformedCharacteristicDeclaration { .. }
                | Error::InvalidCharacteristicDeclarationData
                | Error::FailedToFinalize { .. }
                | Error::Advertisement(_)
                | Error::InvalidChannelId
                | Error::ConfigFilterAcceptListIsEmpty => FailureReason::InvalidValue,
                Error::L2capConnectError(_) | Error::Other => FailureReason::Other,
            }
        }
    }
}
/// Protected DIS values remain evidence; inspection never escalates security.
pub fn device_info_att_result(code: AttErrorCode) -> ReadResult {
    if matches!(
        code,
        AttErrorCode::INSUFFICIENT_AUTHENTICATION
            | AttErrorCode::INSUFFICIENT_ENCRYPTION
            | AttErrorCode::INSUFFICIENT_ENCRYPTION_KEY_SIZE
            | AttErrorCode::INSUFFICIENT_AUTHORISATION
    ) {
        ReadResult::Protected
    } else {
        ReadResult::Unavailable
    }
}
