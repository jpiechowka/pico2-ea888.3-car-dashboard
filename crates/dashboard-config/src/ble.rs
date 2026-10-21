//! Bounded peer selection. Address bytes use displayed/network order, never HCI order.
use serde::{Deserialize, Serialize};

/// Minimum admitted connection attempt timeout in milliseconds.
pub const BLE_CONNECT_TIMEOUT_MIN_MS: u16 = 5_000;
/// Maximum admitted connection attempt timeout in milliseconds.
pub const BLE_CONNECT_TIMEOUT_MAX_MS: u16 = 30_000;
/// Minimum admitted adapter response timeout in milliseconds.
pub const BLE_RESPONSE_TIMEOUT_MIN_MS: u16 = 250;
/// Maximum admitted adapter response timeout in milliseconds.
pub const BLE_RESPONSE_TIMEOUT_MAX_MS: u16 = 5_000;
/// Minimum delay between adapter transactions in milliseconds.
pub const BLE_POLL_GAP_MIN_MS: u16 = 10;
/// Maximum configured transaction gap in milliseconds.
pub const BLE_POLL_GAP_MAX_MS: u16 = 200;

/// Closed adapter profiles; production adapter profiles currently identify/init only.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BleProfile {
    /// vLinker MC+ prototype; selecting it does not qualify ECU traffic.
    VlinkerMcPlus,
    /// OBDLink CX prototype; selecting it does not qualify ECU traffic.
    ObdLinkCx,
    /// Separate synthetic test-service peer, never a vehicle-data source.
    TestPeer,
}
/// Stable peer address kinds admitted to portable dashboard configuration.
/// Resolvable/private rotating identities are deliberately not representable here.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BleAddressKind {
    /// Public device address; validity checks do not authenticate its assignment or peer.
    Public,
    /// Random static address with top two display-order bits set and a nontrivial random part.
    RandomStatic,
}
/// Explicit peer identity; device names and saved GATT handles are not connection authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BlePeer {
    /// Six address octets in displayed/network order, most significant octet first.
    pub address: [u8; 6],
    /// Address kind participates in identity; equal bytes with different kinds are distinct peers.
    pub kind: BleAddressKind,
}
impl BlePeer {
    /// Return whether this peer is structurally admitted for the `enabled` state.
    /// Zero is accepted only when disabled; all-ones always fails. Random-static
    /// validation checks the two tag bits and rejects all-zero/all-one random parts.
    /// This does not verify peer presence, authenticity, bonds or actual controller acceptance.
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
/// Bounded optional adapter transport policy; no keys, PINs or bond secrets are serialized.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BleSettings {
    /// Opt-in transport gate; default false, independent of source/deployment qualification.
    pub enabled: bool,
    /// Closed initialization/test-service selection.
    pub profile: BleProfile,
    /// Selected address and type; required nonzero when enabled.
    pub peer: BlePeer,
    /// Permit runtime reconnect attempts after loss, subject to bounded retry/cancellation policy.
    pub auto_reconnect: bool,
    /// Connection deadline in milliseconds, 5,000..=30,000.
    pub connect_timeout_ms: u16,
    /// Adapter transaction deadline in milliseconds, 250..=5,000.
    pub response_timeout_ms: u16,
    /// Minimum inter-transaction gap in milliseconds, 10..=200.
    pub poll_gap_ms: u16,
}
impl BleSettings {
    /// Disabled MC+ placeholder with 10 s connect, 1.5 s response and 20 ms gap defaults.
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
    /// Return [`Self::DEFAULT`]; constructing settings performs no radio I/O.
    fn default() -> Self { Self::DEFAULT }
}
/// Fixed settings storage size, independent of the persistent bond-store layout.
const _: () = assert!(core::mem::size_of::<BleSettings>() == 16);
