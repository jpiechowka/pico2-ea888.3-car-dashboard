//! Bounded peer selection. Address bytes use displayed/network order, never HCI order.
use serde::{Deserialize, Serialize};

pub const BLE_CONNECT_TIMEOUT_MIN_MS: u16 = 5_000;
pub const BLE_CONNECT_TIMEOUT_MAX_MS: u16 = 30_000;
pub const BLE_RESPONSE_TIMEOUT_MIN_MS: u16 = 250;
pub const BLE_RESPONSE_TIMEOUT_MAX_MS: u16 = 5_000;
pub const BLE_POLL_GAP_MIN_MS: u16 = 10;
pub const BLE_POLL_GAP_MAX_MS: u16 = 200;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BleProfile {
    VlinkerMcPlus,
    ObdLinkCx,
    TestPeer,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BleAddressKind {
    Public,
    RandomStatic,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BlePeer {
    pub address: [u8; 6],
    pub kind: BleAddressKind,
}
impl BlePeer {
    /// Only the all-zero disabled placeholder bypasses address validation.
    pub fn valid(
        self,
        enabled: bool,
    ) -> bool {
        if self.address == [0; 6] {
            return !enabled;
        }
        if self.address == [0xFF; 6] {
            return false;
        }
        match self.kind {
            BleAddressKind::Public => true,
            BleAddressKind::RandomStatic => self.address[0] & 0xC0 == 0xC0 && self.address != [0xC0, 0, 0, 0, 0, 0],
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BleSettings {
    pub enabled: bool,
    pub profile: BleProfile,
    pub peer: BlePeer,
    pub auto_reconnect: bool,
    pub connect_timeout_ms: u16,
    pub response_timeout_ms: u16,
    pub poll_gap_ms: u16,
}
impl BleSettings {
    pub const DEFAULT: Self = Self {
        enabled: false,
        profile: BleProfile::VlinkerMcPlus,
        peer: BlePeer {
            address: [0; 6],
            kind: BleAddressKind::Public,
        },
        auto_reconnect: true,
        connect_timeout_ms: 10_000,
        response_timeout_ms: 1_500,
        poll_gap_ms: 20,
    };
}
impl Default for BleSettings {
    fn default() -> Self { Self::DEFAULT }
}
// No keys, PINs or bond secrets enter portable project files.
const _: () = assert!(core::mem::size_of::<BleSettings>() == 16);
