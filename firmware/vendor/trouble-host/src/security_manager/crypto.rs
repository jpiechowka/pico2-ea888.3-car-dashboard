//! Bluetooth SMP AES-CMAC, legacy key derivation and P-256 agreement primitives.
//!
//! Integers and curve coordinates use numeric big-endian input to cryptographic functions;
//! SMP packet helpers convert to little-endian wire order. See Bluetooth Core Vol 3, Part H
//! ([Security Manager specification](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/Core-61/out/en/host/security-manager-specification.html)).
//! LTK/IRK Display and Debug expose key material; secret-scalar/MacKey/DHKey formatters redact it.
#![warn(missing_docs)]
// This file contains code from Blackrock User-Mode Bluetooth LE Library (https://github.com/mxk/burble)

use core::num::NonZeroU128;

use aes::cipher::{BlockEncrypt, KeyInit};
use aes::Aes128;
use bt_hci::param::BdAddr;
use cmac::digest;
#[cfg(not(feature = "security-p256-cortex-m4"))]
use p256::ecdh;
use rand_core::{CryptoRng, RngCore};

use crate::{Address, Error};

/// 128-bit LE encryption key, derived through Secure Connections or legacy key distribution.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[must_use]
#[repr(transparent)]
pub struct LongTermKey(
    /// Numeric key value; zero is representable and not rejected here.
    pub u128,
);

impl LongTermKey {
    /// Returns a key containing numeric `k` unchanged, including zero.
    #[inline(always)]
    pub const fn new(k: u128) -> Self {
        Self(k)
    }
    /// Returns the key decoded from little-endian wire bytes `k`, without validation.
    #[inline(always)]
    pub const fn from_le_bytes(k: [u8; 16]) -> Self {
        Self(u128::from_le_bytes(k))
    }
    /// Returns the key's sixteen little-endian wire bytes.
    #[inline(always)]
    pub const fn to_le_bytes(self) -> [u8; 16] {
        self.0.to_le_bytes()
    }

    /// Returns `d1(er, div, 0)` for encryption root `er` and 16-bit diversifier `div`
    /// (Vol 3, Part H, B.2.2); no entropy or root-key validation occurs.
    #[inline]
    pub fn from_encryption_root(er: u128, div: u16) -> Self {
        Self(d1(er, div, 0))
    }
}

impl From<&LongTermKey> for u128 {
    /// Returns `k`'s numeric key value without changing it.
    #[inline(always)]
    fn from(k: &LongTermKey) -> Self {
        k.0
    }
}

impl core::fmt::Display for LongTermKey {
    /// Writes the unredacted key to `f` as lowercase hex with at least 16 digits.
    /// Returns the formatter's write result; avoid use in credential-safe diagnostics.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{:016x}", self.0)
    }
}

#[cfg(feature = "defmt")]
impl defmt::Format for LongTermKey {
    /// Writes the unredacted numeric key to `fmt` as hexadecimal.
    fn format(&self, fmt: defmt::Formatter) {
        defmt::write!(fmt, "{:016x}", self.0)
    }
}

/// Identity Resolving Key.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[must_use]
#[repr(transparent)]
pub struct IdentityResolvingKey(
    /// Nonzero numeric IRK; zero represents an unavailable key at external boundaries.
    pub NonZeroU128,
);

impl IdentityResolvingKey {
    /// Returns the IRK for numeric `k`, or None when `k` is zero.
    #[inline(always)]
    pub const fn new(k: u128) -> Option<Self> {
        match NonZeroU128::new(k) {
            Some(k) => Some(Self(k)),
            None => None,
        }
    }

    /// Returns the IRK decoded from little-endian `k`, or None for sixteen zero bytes.
    #[inline(always)]
    pub const fn from_le_bytes(k: [u8; 16]) -> Option<Self> {
        Self::new(u128::from_le_bytes(k))
    }

    /// Returns `d1(ir, 1, 0)` for identity root `ir` (Vol 3, Part H, B.2.3),
    /// or None if that derived value is zero; the input root is not validated.
    #[inline]
    pub fn from_identity_root(ir: u128) -> Option<Self> {
        Self::new(d1(ir, 1, 0))
    }

    /// Returns the Identity Resolving Key as `[u8; 16]` value in little endian.
    #[inline(always)]
    pub const fn to_le_bytes(self) -> [u8; 16] {
        self.0.get().to_le_bytes()
    }

    /// Draws up to four three-byte random parts from `rng`; Returns an RPA in BdAddr wire order.
    ///
    /// The generated address follows the format described in
    /// Bluetooth Core Specification [Vol 3] Part C, Section 10.8.2.
    ///
    /// Returns `InvalidValue` on RNG failure or four consecutive invalid random
    /// parts. Callers must propagate failure rather than expose an identity address.
    pub fn generate_resolvable_address<T: RngCore + CryptoRng>(&self, rng: &mut T) -> Result<[u8; 6], Error> {
        for _ in 0..4 {
            let mut prand = [0u8; 3];
            rng.try_fill_bytes(&mut prand).map_err(|_| Error::InvalidValue)?;
            prand[2] = (prand[2] & 0x3f) | 0x40;
            // Core Vol 6 Part B 1.3.2.2 excludes all-zero/all-one random bits.
            // Rejection preserves the distribution without an unbounded loop.
            if prand == [0, 0, 0x40] || prand == [0xff, 0xff, 0x7f] {
                continue;
            }
            let mut address = [0u8; 6];
            address[3..].copy_from_slice(&prand);
            // ah uses numeric big endian; BdAddr uses least-significant byte first.
            prand.reverse();
            let mut hash = self.ah(prand);
            hash.reverse();
            address[..3].copy_from_slice(&hash);
            return Ok(address);
        }
        Err(Error::InvalidValue)
    }

    /// Resolves a resolvable private address.
    ///
    /// Returns whether `address` has RPA type bits and its 24-bit hash matches this IRK.
    /// This is hash-based resolution, not proof of origin; it does not validate the random-part
    /// all-zero/all-one exclusions or an external HCI address-kind field.
    pub fn resolve_address(&self, address: &BdAddr) -> bool {
        // Extract prand (top 24 bits) and hash (bottom 24 bits)
        let mut prand = [0u8; 3];
        prand.copy_from_slice(&address.raw()[3..6]);

        // Verify the address type bits (top 2 bits should be 0b01)
        if (prand[2] & 0b11000000) != 0b01000000 {
            return false; // Not a resolvable private address
        }

        prand.reverse();

        // Calculate local hash
        let mut local_hash = self.ah(prand);
        local_hash.reverse();

        // Compare with the hash in the address
        let mut address_hash = [0u8; 3];
        address_hash.copy_from_slice(&address.raw()[0..3]);
        local_hash == address_hash
    }

    /// Returns ah(IRK, `r`): AES-encrypt the zero-padded big-endian 24-bit random value `r`
    /// and retain the least-significant three bytes in big-endian order (Vol 3, Part H, §2.2.2).
    fn ah(&self, r: [u8; 3]) -> [u8; 3] {
        let mut r_prime = [0u8; 16];
        r_prime[13..].copy_from_slice(&r);

        let cipher = Aes128::new_from_slice(&self.0.get().to_be_bytes()).unwrap();
        cipher.encrypt_block((&mut r_prime).into());
        // Extract least significant 24 bits (3 bytes) as the result
        r_prime[13..16].try_into().unwrap()
    }
}

impl From<&IdentityResolvingKey> for u128 {
    /// Returns `k`'s nonzero numeric IRK without changing it.
    #[inline(always)]
    fn from(k: &IdentityResolvingKey) -> Self {
        k.0.get()
    }
}

impl core::fmt::Display for IdentityResolvingKey {
    /// Writes the unredacted IRK to `f` as lowercase hex with at least 16 digits.
    /// Returns the formatter's write result; avoid use in credential-safe diagnostics.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{:016x}", self.0)
    }
}

#[cfg(feature = "defmt")]
impl defmt::Format for IdentityResolvingKey {
    /// Writes the unredacted numeric IRK to `fmt` as hexadecimal.
    fn format(&self, fmt: defmt::Formatter) {
        defmt::write!(fmt, "{:016x}", self.0)
    }
}

/// [RFC4493 AES-CMAC](https://www.rfc-editor.org/info/rfc4493/) streaming state (Vol 3, Part H, §2.2.5).
#[derive(Debug)]
#[repr(transparent)]
pub struct AesCmac(
    /// Keyed chaining state and buffered final block.
    cmac::Cmac<aes::Aes128>,
);

impl AesCmac {
    /// Returns empty AES-CMAC state keyed by `k`'s big-endian 128-bit value.
    #[inline(always)]
    #[must_use]
    pub(super) fn new(k: &Key) -> Self {
        Self(digest::KeyInit::new(&k.0))
    }

    /// Returns empty AES-CMAC state with the all-zero key for the GATT Database Hash
    /// (Vol 3, Part G, §7.3.1); callers must serialize the specified attribute fields.
    #[inline(always)]
    #[must_use]
    pub fn db_hash() -> Self {
        Self::new(&Key::new(0))
    }

    /// Appends all bytes from `b` to the authenticated message; Returns this same mutable state.
    #[inline(always)]
    pub fn update(&mut self, b: impl AsRef<[u8]>) -> &mut Self {
        digest::Update::update(&mut self.0, b.as_ref());
        self
    }

    /// Consumes this state; Returns the full 128-bit MAC interpreted as a big-endian integer.
    #[inline(always)]
    #[must_use]
    pub fn finalize(self) -> u128 {
        u128::from_be_bytes(*digest::FixedOutput::finalize_fixed(self.0).as_ref())
    }

    /// Returns the full MAC as a new key and resets message state while retaining the original key.
    #[inline(always)]
    pub(super) fn finalize_key(&mut self) -> Key {
        // Best effort to avoid leaving copies
        let mut k = Key::new(0);
        digest::FixedOutputReset::finalize_into_reset(&mut self.0, &mut k.0);
        k
    }
}

/// LE Secure Connections check value generated by [`MacKey::f6`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[must_use]
#[repr(transparent)]
pub struct Check(
    /// Full numeric f6 output; wire encoding is little-endian.
    pub u128,
);

#[derive(Clone, Copy)]
#[repr(transparent)]
/// Internal AES128 key in cipher-input byte order.
pub(super) struct Key(
    /// Sixteen big-endian key bytes.
    aes::cipher::Key<aes::Aes128>,
);

impl Key {
    /// Returns `k` encoded as sixteen big-endian AES key bytes, including zero unchanged.
    #[inline(always)]
    pub fn new(k: u128) -> Self {
        Self(k.to_be_bytes().into())
    }
}

impl From<&Key> for u128 {
    /// Returns `k` decoded as a big-endian numeric key.
    #[inline(always)]
    fn from(k: &Key) -> Self {
        Self::from_be_bytes(k.0.into())
    }
}

/// Concatenated `AuthReq`, OOB data flag, and IO capability parameters used by
/// [`MacKey::f6`] function ([Vol 3] Part H, Section 2.2.8).
#[repr(transparent)]
#[derive(Clone, Copy, Debug)]
pub struct IoCap(
    /// AuthReq, OOB flag and I/O capability in f6 concatenation order.
    pub(crate) [u8; 3],
);

impl IoCap {
    /// Returns the f6 triplet from raw `auth_req`, boolean `oob_data` and raw `io_cap`.
    /// Reserved authentication/I/O values are preserved rather than validated.
    #[inline(always)]
    pub fn new(auth_req: u8, oob_data: bool, io_cap: u8) -> Self {
        Self([auth_req, u8::from(oob_data), io_cap])
    }
}

/// 128-bit key used to compute LE Secure Connections check value
/// ([Vol 3] Part H, Section 2.2.8).
#[derive(Clone, Copy)]
#[must_use]
#[repr(transparent)]
pub struct MacKey(
    /// f5-derived AES-CMAC authentication key.
    Key,
);

impl core::fmt::Debug for MacKey {
    /// Writes a redacted key marker to `f`; Returns its write result.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_tuple("MacKey").field(&"***").finish()
    }
}

#[cfg(feature = "defmt")]
impl defmt::Format for MacKey {
    /// Writes a redacted key marker to `fmt`.
    fn format(&self, fmt: defmt::Formatter) {
        defmt::write!(fmt, "MacKey(***)");
    }
}

impl MacKey {
    /// Returns f6 over ordered nonces `n1`/`n2`, model-specific 128-bit secret `r`,
    /// sender feature triplet `io_cap` and ordered link addresses `a1`/`a2` (Vol 3, Part H, §2.2.8).
    /// Caller must select initiator/responder ordering for the check being generated.
    #[inline]
    pub fn f6(&self, n1: Nonce, n2: Nonce, r: u128, io_cap: IoCap, a1: Address, a2: Address) -> Check {
        let mut m = AesCmac::new(&self.0);
        m.update(n1.0.to_be_bytes())
            .update(n2.0.to_be_bytes())
            .update(r.to_be_bytes())
            .update(io_cap.0)
            .update(a1.to_bytes())
            .update(a2.to_bytes());
        Check(m.finalize())
    }
}

/// 128-bit random nonce value ([Vol 3] Part H, Section 2.3.5.6).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[repr(transparent)]
pub struct Nonce(
    /// Numeric nonce; direct construction permits zero for protocol placeholders.
    pub u128,
);

impl Nonce {
    /// Returns a nonce from sixteen `rng` bytes interpreted in native endian order.
    /// Callers supply a cryptographically secure source; the RngCore bound alone does not enforce it.
    ///
    /// # Panics
    ///
    /// Panics if `rng.fill_bytes` fails by panicking, or its sixteen output bytes are all zero.
    #[allow(clippy::new_without_default)]
    #[inline]
    pub fn new<T: RngCore>(rng: &mut T) -> Self {
        let mut b = [0; core::mem::size_of::<u128>()];
        rng.fill_bytes(b.as_mut_slice());
        let n = u128::from_ne_bytes(b);
        assert_ne!(n, 0);
        Self(n)
    }

    /// Returns f4 keyed by this nonce over big-endian X coordinates `u`/`v` and final octet `z`.
    /// `z` is zero for comparison/OOB or 0x80 plus the current passkey bit; no value validation occurs.
    #[inline]
    pub fn f4(&self, u: &PublicKeyX, v: &PublicKeyX, z: u8) -> Confirm {
        let mut m = AesCmac::new(&Key::new(self.0));
        m.update(u.as_be_bytes()).update(v.as_be_bytes()).update([z]);
        Confirm(m.finalize())
    }

    /// Returns g2's low 32-bit CMAC value modulo 1000000, keyed by this initiator nonce.
    /// `pkax`/`pkbx` are initiator/responder X coordinates and `nb` is the responder nonce.
    #[inline]
    pub fn g2(&self, pkax: &PublicKeyX, pkbx: &PublicKeyX, nb: &Self) -> NumCompare {
        let mut m = AesCmac::new(&Key::new(self.0));
        m.update(pkax.as_be_bytes())
            .update(pkbx.as_be_bytes())
            .update(nb.0.to_be_bytes());
        #[allow(clippy::cast_possible_truncation)]
        NumCompare(m.finalize() as u32 % 1_000_000)
    }
}

/// LE Secure Connections confirm value generated by [`Nonce::f4`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[must_use]
#[repr(transparent)]
pub struct Confirm(
    /// Full numeric f4 output; wire encoding is little-endian.
    pub u128,
);

/// 6-digit LE Secure Connections numeric comparison value generated by
/// [`Nonce::g2`].
#[derive(Clone, Copy, Eq, PartialEq, Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[must_use]
#[repr(transparent)]
pub struct NumCompare(
    /// Decimal comparison value; g2 produces 0 through 999999, direct construction is unchecked.
    pub u32,
);

/// P-256 elliptic curve secret key.
#[cfg(not(feature = "security-p256-cortex-m4"))]
#[derive(Clone)]
#[must_use]
#[repr(transparent)]
pub struct SecretKey(
    /// Valid nonzero scalar in the P-256 subgroup.
    p256::NonZeroScalar,
);

/// P-256 elliptic curve secret key.
#[cfg(feature = "security-p256-cortex-m4")]
#[derive(Clone)]
#[must_use]
#[repr(transparent)]
pub struct SecretKey(
    /// Cortex-M backend's validated private scalar.
    p256_cortex_m4::SecretKey,
);

impl core::fmt::Debug for SecretKey {
    /// Writes a redacted scalar marker to `f`; Returns its write result.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_tuple("SecretKey").field(&"***").finish()
    }
}

#[cfg(feature = "defmt")]
impl defmt::Format for SecretKey {
    /// Writes a redacted scalar marker to `fmt`.
    fn format(&self, fmt: defmt::Formatter) {
        defmt::write!(fmt, "SecretKey(***)");
    }
}

#[cfg(feature = "security-p256-cortex-m4")]
impl SecretKey {
    /// Returns a valid random P-256 scalar drawn from `rng`; backend rejection sampling may retry.
    /// Infallible RNG methods may panic on entropy-source failure.
    #[allow(clippy::new_without_default)]
    pub fn new<T: RngCore + CryptoRng>(rng: &mut T) -> Self {
        Self(p256_cortex_m4::SecretKey::random(rng))
    }

    /// Returns this scalar's affine public point with both coordinates in big-endian order.
    pub fn public_key(&self) -> PublicKey {
        let pk = self.0.public_key();
        PublicKey {
            x: PublicKeyX(Coord(pk.x())),
            y: Coord(pk.y()),
        }
    }

    /// Returns ECDH agreement with remote `pk`, or None for an invalid point, the Bluetooth
    /// debug point or X-coordinate equality with `local_pk` (Vol 3, Part H, §2.3.5.6.1).
    ///
    /// `local_pk` must be the public key of `self`; passing it in avoids an
    /// expensive scalar multiplication to recompute it.
    #[must_use]
    pub fn dh_key(&self, pk: PublicKey, local_pk: &PublicKey) -> Option<DHKey> {
        if pk.is_debug() {
            return None; // TODO: Compile-time option for debug-only mode
        }
        if pk.x == local_pk.x {
            return None;
        }

        let mut bytes = [0u8; 64];
        bytes[..32].copy_from_slice(pk.x.as_be_bytes());
        bytes[32..].copy_from_slice(&pk.y.0);
        // from_untagged_bytes validates that the point is on the curve
        let rpk = p256_cortex_m4::PublicKey::from_untagged_bytes(&bytes).ok()?;
        let mut secret = [0u8; 32];
        secret.copy_from_slice(self.0.agree(&rpk).as_bytes());
        Some(DHKey(secret))
    }
}

#[cfg(not(feature = "security-p256-cortex-m4"))]
impl SecretKey {
    /// Returns a valid random P-256 scalar drawn from `rng`; backend rejection sampling may retry.
    /// Infallible RNG methods may panic on entropy-source failure.
    #[allow(clippy::new_without_default)]
    #[inline(never)]
    pub fn new<T: RngCore + CryptoRng>(rng: &mut T) -> Self {
        Self(p256::NonZeroScalar::random(rng))
    }

    /// Returns this scalar's affine public point with both coordinates in big-endian order.
    /// The unreachable branch assumes the backend preserves its nonzero-scalar invariant.
    #[inline(never)]
    pub fn public_key(&self) -> PublicKey {
        use p256::elliptic_curve::sec1::Coordinates::Uncompressed;
        use p256::elliptic_curve::sec1::ToEncodedPoint;
        let p = p256::PublicKey::from_secret_scalar(&self.0).to_encoded_point(false);
        match p.coordinates() {
            Uncompressed { x, y } => PublicKey {
                x: PublicKeyX(Coord(*x.as_ref())),
                y: Coord(*y.as_ref()),
            },
            _ => unreachable!("invalid secret key"),
        }
    }

    /// Returns ECDH agreement with remote `pk`, or None for invalid `pk`/`local_pk`, the Bluetooth
    /// debug peer point or X-coordinate equality with `local_pk` (Vol 3, Part H, §2.3.5.6.1).
    ///
    /// `local_pk` must be the public key of `self`; passing it in avoids an
    /// expensive scalar multiplication to recompute it.
    #[must_use]
    #[inline(never)]
    pub fn dh_key(&self, pk: PublicKey, local_pk: &PublicKey) -> Option<DHKey> {
        use p256::elliptic_curve::sec1::FromEncodedPoint;
        if pk.is_debug() || pk.x == local_pk.x {
            return None; // TODO: Compile-time option for debug-only mode
        }

        let (x, y) = (&pk.x.0 .0.into(), &pk.y.0.into());
        let rep = p256::EncodedPoint::from_affine_coordinates(x, y, false);
        let (lx, ly) = (&local_pk.x.0 .0.into(), &local_pk.y.0.into());
        let lrep = p256::EncodedPoint::from_affine_coordinates(lx, ly, false);
        let lpk: Option<p256::PublicKey> = Option::from(p256::PublicKey::from_encoded_point(&lrep));
        // Constant-time ops not required:
        // https://github.com/RustCrypto/traits/issues/1227
        let rpk: Option<p256::PublicKey> = Option::from(p256::PublicKey::from_encoded_point(&rep));
        match (rpk, lpk) {
            (Some(rpk), Some(lpk)) if rpk != lpk => Some(DHKey(ecdh::diffie_hellman(self.0, rpk.as_affine()))),
            _ => None,
        }
    }
}

/// P-256 elliptic curve public key ([Vol 3] Part H, Section 3.5.6).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[must_use]
pub struct PublicKey {
    /// Affine X coordinate in big-endian order; raw construction does not validate the point.
    pub x: PublicKeyX,
    /// Affine Y coordinate in big-endian order; raw construction does not validate the point.
    pub y: Coord,
}

impl PublicKey {
    /// Returns the point decoded from little-endian X then Y in `bytes`, without curve validation.
    /// Panics unless `bytes` is exactly 64 bytes; DH-key derivation performs subsequent validation.
    pub fn from_bytes(bytes: &[u8]) -> Self {
        let mut x = [0u8; 32];
        let mut y = [0u8; 32];

        x.copy_from_slice(&bytes[..32]);
        y.copy_from_slice(&bytes[32..]);

        x.reverse();
        y.reverse();

        Self {
            x: PublicKeyX(Coord(x)),
            y: Coord(y),
        }
    }

    /// Returns the public key X coordinate.
    #[inline(always)]
    pub const fn x(&self) -> &PublicKeyX {
        &self.x
    }

    /// Returns whether `self` is the debug public key
    /// ([Vol 3] Part H, Section 2.3.5.6.1).
    #[allow(clippy::unreadable_literal)]
    #[allow(clippy::unusual_byte_groupings)]
    fn is_debug(&self) -> bool {
        let (x, y) = (&self.x.0 .0, &self.y.0);
        x[..16] == u128::to_be_bytes(0x20b003d2_f297be2c_5e2c83a7_e9f9a5b9)
            && x[16..] == u128::to_be_bytes(0xeff49111_acf4fddb_cc030148_0e359de6)
            && y[..16] == u128::to_be_bytes(0xdc809c49_652aeb6d_63329abf_5a52155c)
            && y[16..] == u128::to_be_bytes(0x766345c2_8fed3024_741c8ed0_1589d28b)
    }
}

/// 256-bit elliptic curve coordinate in big-endian byte order.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[repr(transparent)]
pub struct Coord(
    /// Thirty-two big-endian coordinate bytes.
    [u8; 256 / u8::BITS as usize],
);

impl Coord {
    /// Returns the coordinate in big-endian byte order.
    #[inline(always)]
    pub(super) const fn as_be_bytes(&self) -> &[u8; core::mem::size_of::<Self>()] {
        &self.0
    }
}

/// P-256 elliptic curve public key affine X coordinate.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[must_use]
#[repr(transparent)]
pub struct PublicKeyX(
    /// Unvalidated big-endian affine X coordinate.
    Coord,
);

impl PublicKeyX {
    /// Returns an unvalidated X coordinate containing big-endian test bytes `x`.
    #[cfg(test)]
    #[inline]
    pub(super) const fn from_be_bytes(x: [u8; core::mem::size_of::<Self>()]) -> Self {
        Self(Coord(x))
    }

    /// Returns the coordinate in big-endian byte order.
    #[inline(always)]
    pub(super) const fn as_be_bytes(&self) -> &[u8; core::mem::size_of::<Self>()] {
        &self.0 .0
    }
}

/// P-256 elliptic curve shared secret ([Vol 3] Part H, Section 2.3.5.6.1).
#[cfg(not(feature = "security-p256-cortex-m4"))]
#[must_use]
#[repr(transparent)]
pub struct DHKey(
    /// RustCrypto shared-point X coordinate, retained for f5.
    ecdh::SharedSecret,
);

/// P-256 elliptic curve shared secret ([Vol 3] Part H, Section 2.3.5.6.1).
#[cfg(feature = "security-p256-cortex-m4")]
#[derive(Clone)]
#[must_use]
#[repr(transparent)]
pub struct DHKey(
    /// Shared-point X coordinate in big-endian order.
    [u8; 32],
);

#[cfg(not(feature = "security-p256-cortex-m4"))]
impl Clone for DHKey {
    /// Returns a new shared-secret wrapper containing an independent copy of the same 32 bytes.
    fn clone(&self) -> Self {
        Self(ecdh::SharedSecret::from(*self.0.raw_secret_bytes()))
    }
}

impl core::fmt::Debug for DHKey {
    /// Writes a redacted shared-secret marker to `f`; Returns its write result.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_tuple("DHKey").field(&"***").finish()
    }
}

#[cfg(feature = "defmt")]
impl defmt::Format for DHKey {
    /// Writes a redacted shared-secret marker to `fmt`.
    fn format(&self, fmt: defmt::Formatter) {
        defmt::write!(fmt, "DHKey(***)");
    }
}

impl DHKey {
    /// Returns a copy of the shared point's32-byte big-endian X coordinate.
    #[cfg(not(feature = "security-p256-cortex-m4"))]
    fn secret_bytes(&self) -> [u8; 32] {
        (*self.0.raw_secret_bytes()).into()
    }

    /// Returns a copy of the shared point's32-byte big-endian X coordinate.
    #[cfg(feature = "security-p256-cortex-m4")]
    fn secret_bytes(&self) -> [u8; 32] {
        self.0
    }

    /// Returns f5's (MacKey, LTK), binding initiator `n1`/`a1` and responder `n2`/`a2`
    /// to this shared secret (Vol 3, Part H, §2.2.7). Address values are actual link addresses.
    #[inline]
    pub fn f5(&self, n1: Nonce, n2: Nonce, a1: Address, a2: Address) -> (MacKey, LongTermKey) {
        let n1 = n1.0.to_be_bytes();
        let n2 = n2.0.to_be_bytes();
        let half = |m: &mut AesCmac, counter: u8| {
            m.update([counter])
                .update(b"btle")
                .update(n1)
                .update(n2)
                .update(a1.to_bytes())
                .update(a2.to_bytes())
                .update(256_u16.to_be_bytes())
                .finalize_key()
        };
        let mut m = AesCmac::new(&Key::new(0x6C88_8391_AAF5_A538_6037_0BDB_5A60_83BE));
        m.update(self.secret_bytes());
        let mut m = AesCmac::new(&m.finalize_key());
        (MacKey(half(&mut m, 0)), LongTermKey(u128::from(&half(&mut m, 1))))
    }
}

#[cfg(feature = "legacy-pairing")]
#[allow(clippy::too_many_arguments)]
/// Confirm value generation function `c1` for LE Legacy Pairing
/// ([Vol 3] Part H, Section 2.2.3).
///
/// Uses two rounds of AES-128 with XOR combining:
///   c1(k, r, preq, pres, iat, ia, rat, ra) = e(k, e(k, r XOR p1) XOR p2)
/// where:
///   p1 = pres || preq || rat || iat
///   p2 = padding || ia || ra
///
/// - `preq`/`pres`: 7 bytes each in wire order (command code at index 0)
/// - `iat`/`rat`: address type (0=public, 1=random)
/// - `ia`/`ra`: 6-byte address in MSO order (reversed from BdAddr raw)
/// - `k`: numeric 128-bit temporary key; `r`: numeric 128-bit random challenge.
///
/// Returns the full 128-bit confirm value; caller validates the type flags and feature bytes.
pub(super) fn c1(
    k: u128,
    r: u128,
    preq: &[u8; 7],
    pres: &[u8; 7],
    iat: u8,
    ia: &[u8; 6],
    rat: u8,
    ra: &[u8; 6],
) -> u128 {
    // p1 = pres || preq || rat || iat (MSO to LSO, 16 bytes)
    // preq/pres are treated as LE integers in the concatenation, so the wire-order
    // bytes must be reversed to place the most significant byte at the MSO position.
    let mut pres_rev = *pres;
    pres_rev.reverse();
    let mut preq_rev = *preq;
    preq_rev.reverse();

    let mut p1 = [0u8; 16];
    p1[0..7].copy_from_slice(&pres_rev);
    p1[7..14].copy_from_slice(&preq_rev);
    p1[14] = rat;
    p1[15] = iat;

    // p2 = padding(4) || ia(6) || ra(6) (MSO to LSO)
    let mut p2 = [0u8; 16];
    p2[4..10].copy_from_slice(ia);
    p2[10..16].copy_from_slice(ra);

    let cipher = Aes128::new_from_slice(&k.to_be_bytes()).unwrap();

    // e(k, r XOR p1)
    let r_bytes = r.to_be_bytes();
    let mut block = [0u8; 16];
    for i in 0..16 {
        block[i] = r_bytes[i] ^ p1[i];
    }
    cipher.encrypt_block((&mut block).into());

    // e(k, result XOR p2)
    for i in 0..16 {
        block[i] ^= p2[i];
    }
    cipher.encrypt_block((&mut block).into());

    u128::from_be_bytes(block)
}

#[cfg(feature = "legacy-pairing")]
/// Short Term Key (STK) generation function `s1` for LE Legacy Pairing
/// ([Vol 3] Part H, Section 2.2.4).
///
///   s1(k, r1, r2) = e(k, r1' || r2')
/// where r1' and r2' are the least significant 64 bits of r1 and r2.
/// Returns the STK using temporary key `k`, responder random `r1` and initiator random `r2`.
pub(super) fn s1(k: u128, r1: u128, r2: u128) -> u128 {
    let r1_bytes = r1.to_be_bytes();
    let r2_bytes = r2.to_be_bytes();

    // r1' = r1[63:0] = least significant 8 bytes (bytes [8..16] in big-endian)
    // r2' = r2[63:0] = least significant 8 bytes
    let mut r_prime = [0u8; 16];
    r_prime[0..8].copy_from_slice(&r1_bytes[8..16]);
    r_prime[8..16].copy_from_slice(&r2_bytes[8..16]);

    let cipher = Aes128::new_from_slice(&k.to_be_bytes()).unwrap();
    cipher.encrypt_block((&mut r_prime).into());

    u128::from_be_bytes(r_prime)
}

/// Diversifying function `d1` ([Vol 3] Part H, Section B.2.1).
///
///   d1(k, d, r) = e(k, d')
/// where d' = padding(96) || r || d, with the least significant octet of `d`
/// becoming the least significant octet of `d'`.
/// Returns the 128-bit diversified key using AES key `k`, low 16-bit diversifier `d` and next 16 bits `r`.
pub(super) fn d1(k: u128, d: u16, r: u16) -> u128 {
    let mut d_prime = [0u8; 16];
    d_prime[12..14].copy_from_slice(&r.to_be_bytes());
    d_prime[14..16].copy_from_slice(&d.to_be_bytes());

    let cipher = Aes128::new_from_slice(&k.to_be_bytes()).unwrap();
    cipher.encrypt_block((&mut d_prime).into());

    u128::from_be_bytes(d_prime)
}

/// Returns big-endian high half `hi` followed by low half `lo`, converted into the test's requested type.
#[allow(clippy::redundant_pub_crate)]
#[cfg(test)]
pub(super) fn u256<T: From<[u8; 32]>>(hi: u128, lo: u128) -> T {
    let mut b = [0; 32];
    b[..16].copy_from_slice(&hi.to_be_bytes());
    b[16..].copy_from_slice(&lo.to_be_bytes());
    T::from(b)
}

#[allow(clippy::unreadable_literal)]
#[allow(clippy::unusual_byte_groupings)]
#[cfg(test)]
/// Bluetooth sample vectors, key admission and bounded RPA generation failure contracts.
mod tests {
    use p256::elliptic_curve::rand_core::OsRng;

    use super::*;
    extern crate std;
    use bt_hci::param::{AddrKind, BdAddr};

    #[test]
    /// Crypto wrappers preserve the fixed 32/64-byte coordinate/key storage assumptions.
    fn sizes() {
        assert_eq!(core::mem::size_of::<Coord>(), 32);
        assert_eq!(core::mem::size_of::<PublicKey>(), 64);
        assert_eq!(core::mem::size_of::<SecretKey>(), 32);
        assert_eq!(core::mem::size_of::<DHKey>(), 32);
    }

    /// Debug mode key ([Vol 3] Part H, Section 2.3.5.6.1).
    #[test]
    fn debug_key() {
        let sk = secret_key(
            0x3f49f6d4_a3c55f38_74c9b3e3_d2103f50,
            0x4aff607b_eb40b799_5899b8a6_cd3c1abd,
        );
        let pk = PublicKey {
            x: PublicKeyX(Coord(u256(
                0x20b003d2_f297be2c_5e2c83a7_e9f9a5b9,
                0xeff49111_acf4fddb_cc030148_0e359de6,
            ))),
            y: Coord(u256(
                0xdc809c49_652aeb6d_63329abf_5a52155c,
                0x766345c2_8fed3024_741c8ed0_1589d28b,
            )),
        };
        assert_eq!(sk.public_key(), pk);
        assert!(pk.is_debug());
    }

    /// P-256 data set 1 ([Vol 2] Part G, Section 7.1.2.1).
    #[test]
    fn p256_1() {
        let (ska, skb) = (
            secret_key(
                0x3f49f6d4_a3c55f38_74c9b3e3_d2103f50,
                0x4aff607b_eb40b799_5899b8a6_cd3c1abd,
            ),
            secret_key(
                0x55188b3d_32f6bb9a_900afcfb_eed4e72a,
                0x59cb9ac2_f19d7cfb_6b4fdd49_f47fc5fd,
            ),
        );
        let (pka, pkb) = (
            PublicKey {
                x: PublicKeyX(Coord(u256(
                    0x20b003d2_f297be2c_5e2c83a7_e9f9a5b9,
                    0xeff49111_acf4fddb_cc030148_0e359de6,
                ))),
                y: Coord(u256(
                    0xdc809c49_652aeb6d_63329abf_5a52155c,
                    0x766345c2_8fed3024_741c8ed0_1589d28b,
                )),
            },
            PublicKey {
                x: PublicKeyX(Coord(u256(
                    0x1ea1f0f0_1faf1d96_09592284_f19e4c00,
                    0x47b58afd_8615a69f_559077b2_2faaa190,
                ))),
                y: Coord(u256(
                    0x4c55f33e_429dad37_7356703a_9ab85160,
                    0x472d1130_e28e3676_5f89aff9_15b1214a,
                )),
            },
        );
        let dh_key = shared_secret(
            0xec0234a3_57c8ad05_341010a6_0a397d9b,
            0x99796b13_b4f866f1_868d34f3_73bfa698,
        );
        assert_eq!(ska.public_key(), pka);
        assert_eq!(skb.public_key(), pkb);
        assert_eq!(
            ska.dh_key(pkb, &pka).unwrap().0.raw_secret_bytes(),
            dh_key.0.raw_secret_bytes()
        );

        assert!(!pkb.is_debug());
        assert!(skb.dh_key(pkb, &pkb).is_none());
    }

    /// P-256 data set 2 ([Vol 2] Part G, Section 7.1.2.2).
    #[test]
    fn p256_2() {
        let (ska, skb) = (
            secret_key(
                0x06a51669_3c9aa31a_6084545d_0c5db641,
                0xb48572b9_7203ddff_b7ac73f7_d0457663,
            ),
            secret_key(
                0x529aa067_0d72cd64_97502ed4_73502b03,
                0x7e8803b5_c60829a5_a3caa219_505530ba,
            ),
        );
        let (pka, pkb) = (
            PublicKey {
                x: PublicKeyX(Coord(u256(
                    0x2c31a47b_5779809e_f44cb5ea_af5c3e43,
                    0xd5f8faad_4a8794cb_987e9b03_745c78dd,
                ))),
                y: Coord(u256(
                    0x91951218_3898dfbe_cd52e240_8e43871f,
                    0xd0211091_17bd3ed4_eaf84377_43715d4f,
                )),
            },
            PublicKey {
                x: PublicKeyX(Coord(u256(
                    0xf465e43f_f23d3f1b_9dc7dfc0_4da87581,
                    0x84dbc966_204796ec_cf0d6cf5_e16500cc,
                ))),
                y: Coord(u256(
                    0x0201d048_bcbbd899_eeefc424_164e33c2,
                    0x01c2b010_ca6b4d43_a8a155ca_d8ecb279,
                )),
            },
        );
        let dh_key = shared_secret(
            0xab85843a_2f6d883f_62e5684b_38e30733,
            0x5fe6e194_5ecd1960_4105c6f2_3221eb69,
        );
        assert_eq!(ska.public_key(), pka);
        assert_eq!(skb.public_key(), pkb);
        assert_eq!(
            ska.dh_key(pkb, &pka).unwrap().0.raw_secret_bytes(),
            dh_key.0.raw_secret_bytes()
        );
    }

    /// Core Vol 3 Part H 2.3.5.6.1 rejects equal X even when the peer's valid Y is negated.
    #[test]
    fn dh_key_rejects_opposite_point_with_same_x() {
        let local = secret_key(0, 1);
        let opposite = SecretKey(-local.0);
        let local_pk = local.public_key();
        let opposite_pk = opposite.public_key();
        assert_eq!(local_pk.x, opposite_pk.x);
        assert_ne!(local_pk.y, opposite_pk.y);
        assert!(!local_pk.is_debug());
        assert!(!opposite_pk.is_debug());
        assert!(local.dh_key(opposite_pk, &local_pk).is_none());
    }

    /// Key generation function ([Vol 3] Part H, Section D.3).
    #[test]
    fn dh_key_f5() {
        let w = shared_secret(
            0xec0234a3_57c8ad05_341010a6_0a397d9b,
            0x99796b13_b4f866f1_868d34f3_73bfa698,
        );
        let n1 = Nonce(0xd5cb8454_d177733e_ffffb2ec_712baeab);
        let n2 = Nonce(0xa6e8e7cc_25a75f6e_216583f7_ff3dc4cf);
        let a1 = Address::new(AddrKind::PUBLIC, BdAddr::new([0xce, 0xbf, 0x37, 0x37, 0x12, 0x56]));
        let a2 = Address::new(AddrKind::PUBLIC, BdAddr::new([0xc1, 0xcf, 0x2d, 0x70, 0x13, 0xa7]));
        let (mk, ltk) = w.f5(n1, n2, a1, a2);
        assert_eq!(ltk.0, 0x69867911_69d7cd23_980522b5_94750a38);
        assert_eq!(u128::from(&mk.0), 0x2965f176_a1084a02_fd3f6a20_ce636e20);
    }

    #[inline]
    /// Returns the valid scalar composed from `hi`/`lo`; panics for zero or out-of-range test vectors.
    fn secret_key(hi: u128, lo: u128) -> SecretKey {
        SecretKey(p256::NonZeroScalar::from_repr(u256(hi, lo)).unwrap())
    }

    #[inline]
    /// Returns a shared-secret test wrapper from big-endian `hi`/`lo`, without curve derivation.
    fn shared_secret(hi: u128, lo: u128) -> DHKey {
        DHKey(ecdh::SharedSecret::from(u256::<p256::FieldBytes>(hi, lo)))
    }

    #[test]
    /// Independent OS-generated keypairs can complete the remote-point validation/agreement path.
    fn testtest() {
        let skb = SecretKey::new(&mut OsRng);
        let pkb = skb.public_key();

        let ska = SecretKey::new(&mut OsRng);
        let pka = ska.public_key();

        let _dh_key = skb.dh_key(pka, &pkb).unwrap();
    }

    #[test]
    /// A frozen little-endian public-key payload is admitted for ECDH with a generated local key.
    fn testtest2() {
        let bytes = [
            0x1eu8, 0x3b, 0x26, 0x40, 0x0e, 0xba, 0x72, 0x51, 0x81, 0xf9, 0x3d, 0x16, 0xb3, 0xc4, 0x11, 0x55, 0x3f,
            0xa8, 0x88, 0x47, 0x08, 0x1c, 0x4a, 0x42, 0x88, 0xbb, 0x68, 0x1d, 0x93, 0xe5, 0xab, 0xb3, 0x72, 0xfa, 0x93,
            0xb4, 0xa0, 0xfe, 0x3f, 0x83, 0x9c, 0x85, 0x5b, 0x5f, 0xb6, 0x30, 0x09, 0x85, 0x47, 0xfd, 0xa8, 0xfa, 0x11,
            0x71, 0xe4, 0x95, 0x17, 0x71, 0x98, 0x82, 0x8f, 0xf8, 0x79, 0x94,
        ];

        let skb = SecretKey::new(&mut OsRng);
        let pkb = skb.public_key();

        let pka = PublicKey::from_bytes(&bytes);

        let _dh_key = skb.dh_key(pka, &pkb).unwrap();
    }

    #[test]
    /// Two OS-random nonces differ; this is a probabilistic smoke check, not an entropy assessment.
    fn nonce() {
        // No fair dice rolls for us!
        assert_ne!(Nonce::new(&mut OsRng), Nonce::new(&mut OsRng));
    }

    /// Confirm value generation function ([Vol 3] Part H, Section D.2).
    #[test]
    fn nonce_f4() {
        let u = PublicKeyX::from_be_bytes(u256(
            0x20b003d2_f297be2c_5e2c83a7_e9f9a5b9,
            0xeff49111_acf4fddb_cc030148_0e359de6,
        ));
        let v = PublicKeyX::from_be_bytes(u256(
            0x55188b3d_32f6bb9a_900afcfb_eed4e72a,
            0x59cb9ac2_f19d7cfb_6b4fdd49_f47fc5fd,
        ));
        let x = Nonce(0xd5cb8454_d177733e_ffffb2ec_712baeab);
        assert_eq!(x.f4(&u, &v, 0).0, 0xf2c916f1_07a9bd1c_f1eda1be_a974872d);
    }

    /// Numeric comparison generation function ([Vol 3] Part H, Section D.5).
    #[allow(clippy::unreadable_literal)]
    #[test]
    fn nonce_g2() {
        let u = PublicKeyX::from_be_bytes(u256(
            0x20b003d2_f297be2c_5e2c83a7_e9f9a5b9,
            0xeff49111_acf4fddb_cc030148_0e359de6,
        ));
        let v = PublicKeyX::from_be_bytes(u256(
            0x55188b3d_32f6bb9a_900afcfb_eed4e72a,
            0x59cb9ac2_f19d7cfb_6b4fdd49_f47fc5fd,
        ));
        let x = Nonce(0xd5cb8454_d177733e_ffffb2ec_712baeab);
        let y = Nonce(0xa6e8e7cc_25a75f6e_216583f7_ff3dc4cf);
        assert_eq!(x.g2(&u, &v, &y), NumCompare(0x2f9ed5ba % 1_000_000));
    }

    /// Check value generation function ([Vol 3] Part H, Section D.4).
    #[test]
    fn mac_key_f6() {
        let k = MacKey(Key::new(0x2965f176_a1084a02_fd3f6a20_ce636e20));
        let n1 = Nonce(0xd5cb8454_d177733e_ffffb2ec_712baeab);
        let n2 = Nonce(0xa6e8e7cc_25a75f6e_216583f7_ff3dc4cf);
        let r = 0x12a3343b_b453bb54_08da42d2_0c2d0fc8;
        let io_cap = IoCap([0x01, 0x01, 0x02]);
        let a1 = Address::new(AddrKind::PUBLIC, BdAddr::new([0xce, 0xbf, 0x37, 0x37, 0x12, 0x56]));
        let a2 = Address::new(AddrKind::PUBLIC, BdAddr::new([0xc1, 0xcf, 0x2d, 0x70, 0x13, 0xa7]));
        let c = k.f6(n1, n2, r, io_cap, a1, a2);
        assert_eq!(c.0, 0xe3c47398_9cd0e8c5_d26c0b09_da958f61);
    }

    #[test]
    /// Frozen wire-order public keys/nonces produce numeric comparison 991180 through g2.
    fn nonce_f4_test() {
        let ra = [
            0x11u8, 0x3a, 0x7a, 0x69, 0x11, 0xcd, 0x44, 0x15, 0x52, 0xf7, 0x47, 0xe8, 0x26, 0x67, 0x72, 0xca,
        ];

        let rb = [
            0xa5u8, 0x9e, 0x9a, 0x32, 0xc0, 0x97, 0x1c, 0xf7, 0x72, 0x1c, 0x29, 0xa7, 0x8c, 0x1e, 0xfd, 0x18,
        ];

        let pkb = [
            0xd, 0x80, 0x33, 0x93, 0xad, 0x1f, 0x7e, 0x9a, 0x30, 0xc9, 0x6e, 0x1, 0x78, 0xf3, 0x43, 0x14, 0xa0, 0x57,
            0xae, 0xa5, 0xa8, 0xee, 0x75, 0x51, 0x3f, 0xaa, 0xb1, 0x80, 0x75, 0xc7, 0x14, 0x50, 0x73, 0x9a, 0x98, 0x95,
            0x36, 0x2e, 0xe6, 0x81, 0x5f, 0xbf, 0x16, 0xa2, 0x8c, 0xf6, 0x9d, 0xdc, 0x1f, 0xb8, 0x84, 0x8c, 0x7d, 0x37,
            0x36, 0xe4, 0x36, 0x3c, 0xb3, 0xe8, 0xfe, 0x4a, 0x73, 0xc6,
        ];

        let pka = [
            0x97, 0x20, 0x0f, 0xfe, 0xf0, 0xec, 0xdd, 0x11, 0xda, 0xa8, 0xa8, 0x07, 0x3e, 0xd7, 0xc6, 0xf2, 0x68, 0x5d,
            0xc2, 0x58, 0x71, 0x1e, 0x34, 0x4f, 0xa1, 0xc4, 0x44, 0xa9, 0x7c, 0x71, 0xee, 0x54, 0x0d, 0xad, 0xb7, 0x69,
            0x89, 0x9d, 0x4f, 0x83, 0x37, 0xcd, 0x43, 0xd3, 0x9f, 0x05, 0x13, 0x99, 0x6f, 0xbc, 0x1a, 0x89, 0xed, 0xb4,
            0x7f, 0x80, 0x98, 0xcf, 0xad, 0x7c, 0x4c, 0x57, 0xbf, 0xe1,
        ];

        let mut pkb_x = [0u8; 32];
        pkb_x.copy_from_slice(&pkb[..32]);
        pkb_x.reverse();
        let mut pka_x = [0u8; 32];
        pka_x.copy_from_slice(&pka[..32]);
        pka_x.reverse();

        let pkbx = PublicKeyX::from_be_bytes(pkb_x);
        let pkax = PublicKeyX::from_be_bytes(pka_x);
        extern crate std;

        let x = Nonce(u128::from_le_bytes(ra));
        let y = Nonce(u128::from_le_bytes(rb));

        assert_eq!(x.g2(&pkax, &pkbx, &y).0, 991180);
    }

    #[test]
    /// Core ah sample maps big-endian random 0x708194 to hash 0x0dfbaa with the specified IRK.
    pub fn irk_test() {
        let irk = IdentityResolvingKey::new(0xec0234a3_57c8ad05_341010a6_0a397d9b).unwrap();
        let prand = [0x70, 0x81, 0x94];

        let hash = irk.ah(prand);
        assert_eq!(hash, [0x0d, 0xfb, 0xaa]);
    }

    #[test]
    /// A frozen least-significant-byte-first RPA resolves under its corresponding IRK.
    pub fn rpa_test() {
        let irk = IdentityResolvingKey::new(0x8b3958c158ed64467bd27bc90d3cf54d).unwrap();
        let address = BdAddr::new([0x92, 0xF2, 0x8F, 0x84, 0x72, 0x4F]);
        let re = irk.resolve_address(&address);
        assert_eq!(re, true);
    }

    #[test]
    /// Generation matches the Core wire fixture, resolves only under the expected tested key,
    /// and rejects repeated all-zero/all-one random parts.
    fn generated_rpa_resolves_and_preserves_wire_order() {
        /// Repeats one three-byte draw to exercise RPA rejection and byte order.
        struct FixedRandom(
            /// Wire-order random-part bytes returned for every draw.
            [u8; 3],
        );
        impl RngCore for FixedRandom {
            /// Panics if the implementation requests a word; no value is returned by this byte-only fixture.
            fn next_u32(&mut self) -> u32 {
                panic!("only address bytes are requested")
            }
            /// Panics if the implementation requests a double word; no value is returned.
            fn next_u64(&mut self) -> u64 {
                panic!("only address bytes are requested")
            }
            /// Copies the scripted draw into `dest`; panics unless its length is three.
            fn fill_bytes(&mut self, dest: &mut [u8]) {
                dest.copy_from_slice(&self.0);
            }
            /// Copies the scripted draw into `dest`; Returns Ok, or panics on non-three-byte requests.
            fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), rand_core::Error> {
                self.fill_bytes(dest);
                Ok(())
            }
        }
        impl CryptoRng for FixedRandom {}
        let irk = IdentityResolvingKey::new(0xec0234a3_57c8ad05_341010a6_0a397d9b).unwrap();
        // Core Vol 3 Part H Appendix D.7: prand708194 -> hash0dfbaa.
        // BdAddr stores the least-significant byte first.
        let address = irk
            .generate_resolvable_address(&mut FixedRandom([0x94, 0x81, 0x70]))
            .unwrap();
        assert_eq!(address, [0xaa, 0xfb, 0x0d, 0x94, 0x81, 0x70]);
        assert!(irk.resolve_address(&BdAddr::new(address)));
        let other = IdentityResolvingKey::new(1).unwrap();
        assert!(!other.resolve_address(&BdAddr::new(address)));
        assert!(matches!(
            irk.generate_resolvable_address(&mut FixedRandom([0; 3])),
            Err(Error::InvalidValue)
        ));
        assert!(matches!(
            irk.generate_resolvable_address(&mut FixedRandom([0xff; 3])),
            Err(Error::InvalidValue)
        ));
    }

    #[test]
    /// Rejected random parts consume attempts and an RNG failure stops immediately without fallback.
    fn rpa_retries_invalid_random_parts_and_bounds_rng_failure() {
        /// Scripted zero/ones/valid sequence followed by an optional entropy-source error.
        struct RandomStream {
            /// Number of fallible byte requests, including failures.
            calls: usize,
            /// Make the next request fail before writing any bytes.
            fail: bool,
        }
        impl RngCore for RandomStream {
            /// Panics on a word request; no value is returned by this byte-only fixture.
            fn next_u32(&mut self) -> u32 {
                unreachable!()
            }
            /// Panics on a double-word request; no value is returned.
            fn next_u64(&mut self) -> u64 {
                unreachable!()
            }
            /// Panics on infallible byte access to require the entropy-error propagation path.
            fn fill_bytes(&mut self, _: &mut [u8]) {
                panic!("use fallible RNG boundary")
            }
            /// Increments attempts and fills three-byte `dest` from the script; Returns the injected
            /// error before mutation when enabled, otherwise Ok. Panics on another destination size.
            fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), rand_core::Error> {
                self.calls += 1;
                if self.fail {
                    return Err(rand_core::Error::from(
                        core::num::NonZeroU32::new(rand_core::Error::CUSTOM_START).unwrap(),
                    ));
                }
                dest.copy_from_slice(match self.calls {
                    1 => &[0, 0, 0],
                    2 => &[0xff, 0xff, 0xff],
                    _ => &[0x94, 0x81, 0x70],
                });
                Ok(())
            }
        }
        impl CryptoRng for RandomStream {}
        let irk = IdentityResolvingKey::new(0xec0234a3_57c8ad05_341010a6_0a397d9b).unwrap();
        let mut rng = RandomStream { calls: 0, fail: false };
        assert_eq!(
            irk.generate_resolvable_address(&mut rng).unwrap(),
            [0xaa, 0xfb, 0x0d, 0x94, 0x81, 0x70]
        );
        assert_eq!(rng.calls, 3);
        rng.fail = true;
        assert!(matches!(
            irk.generate_resolvable_address(&mut rng),
            Err(Error::InvalidValue)
        ));
        assert_eq!(rng.calls, 4);
    }

    /// LE Legacy Pairing c1 test vector ([Vol 3] Part H, Appendix D.1).
    /// preq/pres in wire order (command code at index 0).
    #[cfg(feature = "legacy-pairing")]
    #[allow(clippy::unreadable_literal)]
    #[test]
    fn legacy_c1() {
        let k: u128 = 0;
        let r: u128 = 0x5783D52156AD6F0E6388274EC6702EE0;
        // Wire order: [Code, IO, OOB, Auth, MaxKey, InitDist, RespDist]
        let preq: [u8; 7] = [0x01, 0x01, 0x00, 0x00, 0x10, 0x07, 0x07];
        let pres: [u8; 7] = [0x02, 0x03, 0x00, 0x00, 0x08, 0x00, 0x05];
        let iat: u8 = 0x01;
        let rat: u8 = 0x00;
        let ia: [u8; 6] = [0xA1, 0xA2, 0xA3, 0xA4, 0xA5, 0xA6];
        let ra: [u8; 6] = [0xB1, 0xB2, 0xB3, 0xB4, 0xB5, 0xB6];

        let result = c1(k, r, &preq, &pres, iat, &ia, rat, &ra);
        assert_eq!(result, 0x1E1E3FEF878988EAD2A74DC5BEF13B86);
    }

    /// LE Legacy Pairing s1 test vector ([Vol 3] Part H, Appendix D.2).
    #[cfg(feature = "legacy-pairing")]
    #[allow(clippy::unreadable_literal)]
    #[test]
    fn legacy_s1() {
        let k: u128 = 0;
        let r1: u128 = 0x000F0E0D0C0B0A091122334455667788;
        let r2: u128 = 0x010203040506070899AABBCCDDEEFF00;

        let result = s1(k, r1, r2);
        assert_eq!(result, 0x9a1fe1f0e8b0f49b5b4216ae796da062);
    }

    /// Diversifying function d1 ([Vol 3] Part H, Section B.2.1).
    /// Spec example: d=0x1234, r=0xABCD => d' = 0x000000000000000000000000ABCD1234.
    #[allow(clippy::unreadable_literal)]
    #[test]
    fn diversify_d1() {
        let k: u128 = 0x000102030405060708090a0b0c0d0e0f;
        let d: u16 = 0x1234;
        let r: u16 = 0xABCD;
        assert_eq!(d1(k, d, r), 0xb66854fa3dd35aadf83a6c59e22b52fd);
    }
}
