//! Deployment choices do not establish electrical, bridge or ECU qualification.
use crate::diagnostics::service::Profile;

pub const BAUD: u32 = 115_200;
pub const RX_BYTES: usize = 1024;
pub const TX_BYTES: usize = 64;
/// Change only after physical DLC8/standard/data/exact-filter qualification.
/// RX12 data itself cannot provide this missing metadata.
pub const PROFILE: Profile = Profile::UNQUALIFIED;
