//! Common types.

/// Traits for conversion between types and their GATT representations
pub mod gatt_traits;
/// L2CAP headers, signaling payloads and codec constants.
pub(crate) mod l2cap;
/// Primitive little-endian codec implementations.
pub(crate) mod primitives;

/// Pairing I/O-capability octets and labels.
pub(crate) mod capabilities;
/// Bluetooth16-bit and128-bit UUID representation and conversion.
pub mod uuid;
