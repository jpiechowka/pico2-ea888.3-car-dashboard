//! Deployment choices do not establish electrical, bridge or ECU qualification.
use crate::diagnostics::service::Profile;

/// Historical preprovisioned Longan UART baud rate; no automatic baud discovery.
pub const BAUD: u32 = 115_200;
/// Exclusive buffered UART receive ring capacity in bytes.
pub const RX_BYTES: usize = 1024;
/// Exclusive buffered UART transmit ring capacity in bytes; records remain serialized by the pump.
pub const TX_BYTES: usize = 64;
/// Change only after physical DLC8/standard/data/exact-filter qualification.
/// RX12 data itself cannot provide this missing metadata.
pub const PROFILE: Profile = Profile::UNQUALIFIED;
