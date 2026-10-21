//! Bounded JSON primitives. Bounds are UTF-8 bytes or element counts, not authentication.
//! [`Wire`] admission checks semantic invariants after deserialization; callers must
//! still bind messages to the current origin/session and authorize operations.

use std::fmt;
use std::marker::PhantomData;
use std::ops::Deref;

use serde::de::{self, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};

/// Stable local admission/state failures; wire endpoints choose their own status mapping.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// Malformed JSON, unsupported schema, or inconsistent derived fields.
    Schema,
    /// An encoded byte, string byte, or collection element limit was exceeded.
    Bounds,
    /// The operation is not allowed in the current lifecycle state.
    InvalidState,
    /// Another operation owns the required resource or slot.
    Busy,
    /// A lease, ticket, or retained event window has expired.
    Expired,
    /// Identity is absent, inaccessible, or deliberately not disclosed.
    NotFoundOrUnavailable,
    /// Requested clock policy requires explicit Expert admission.
    ExpertRequired,
    /// Clock inputs cannot be represented by the supported PLL/SPI rules.
    Clock,
    /// The session or stream has already closed.
    Closed,
    /// Monotonic sequence space is exhausted; wrapping is forbidden.
    SequenceExhausted,
}
impl fmt::Display for Error {
    /// Write the variant name to `f`; return its formatting result without extra context.
    fn fmt(
        &self,
        f: &mut fmt::Formatter<'_>,
    ) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Error {}

/// Owned UTF-8 text of at most `N` bytes; empty strings and control characters are allowed.
/// This type bounds storage only. Context validators must reject unsafe path/URL content.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct Text<const N: usize>(
    /// Validated bytes, exposed only through immutable string access.
    String,
);
impl<const N: usize> Text<N> {
    /// Consume `s` and return bounded text, or [`Error::Bounds`] after conversion if too long.
    /// Conversion may allocate before the byte limit is checked.
    pub fn new(s: impl Into<String>) -> Result<Self, Error> {
        let s = s.into();
        if s.len() > N { Err(Error::Bounds) } else { Ok(Self(s)) }
    }

    /// Return the complete borrowed UTF-8 contents without normalization or truncation.
    pub fn as_str(&self) -> &str { &self.0 }
}
impl<const N: usize> Deref for Text<N> {
    /// Immutable string view; mutation cannot bypass construction bounds.
    type Target = str;

    /// Return the immutable validated string slice.
    fn deref(&self) -> &str { &self.0 }
}
impl<'de, const N: usize> Deserialize<'de> for Text<N> {
    /// Read a string from `d`; return bounded owned text or a deserializer error.
    /// The visitor checks length before copying; `d` may have buffered input already.
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        /// Visitor carrying the compile-time UTF-8 byte limit.
        struct V<const N: usize>;
        impl<'de, const N: usize> Visitor<'de> for V<N> {
            /// Validated owned text produced on success.
            type Value = Text<N>;

            /// Describe the byte limit to `f`; return the formatter's result.
            fn expecting(
                &self,
                f: &mut fmt::Formatter,
            ) -> fmt::Result {
                write!(f, "at most {N} UTF-8 bytes")
            }

            /// Copy `s` only within the byte limit; return bounded text or a length error.
            fn visit_str<E: de::Error>(
                self,
                s: &str,
            ) -> Result<Self::Value, E> {
                if s.len() > N {
                    return Err(E::custom("text bound"));
                }
                Ok(Text(s.to_owned()))
            }
        }
        d.deserialize_str(V::<N>)
    }
}

/// Owned collection containing at most `N` elements; element byte sizes are not bounded here.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct BoundedVec<T, const N: usize>(
    /// Validated elements; only immutable access preserves the count invariant.
    Vec<T>,
);
impl<T, const N: usize> BoundedVec<T, N> {
    /// Consume `v`; return it wrapped without copying, or [`Error::Bounds`] if its length exceeds `N`.
    pub fn new(v: Vec<T>) -> Result<Self, Error> { if v.len() > N { Err(Error::Bounds) } else { Ok(Self(v)) } }

    /// Return ownership of the underlying vector, relinquishing this wrapper's count invariant.
    pub fn into_vec(self) -> Vec<T> { self.0 }
}
impl<T, const N: usize> Default for BoundedVec<T, N> {
    /// Return an empty collection, including when `N` is zero.
    fn default() -> Self { Self(Vec::new()) }
}
impl<T, const N: usize> Deref for BoundedVec<T, N> {
    /// Immutable slice view that cannot grow the collection.
    type Target = [T];

    /// Return the validated elements in their original order.
    fn deref(&self) -> &[T] { &self.0 }
}
impl<'de, T: Deserialize<'de>, const N: usize> Deserialize<'de> for BoundedVec<T, N> {
    /// Read a sequence from `d`; return at most `N` decoded elements or an element/count error.
    /// One additional element is consumed as `IgnoredAny` to detect overflow; use an outer byte cap.
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        /// Visitor retaining element type and compile-time count without allocating a bound-sized array.
        struct V<T, const N: usize>(
            /// Type-only marker for decoded elements.
            PhantomData<T>,
        );
        impl<'de, T: Deserialize<'de>, const N: usize> Visitor<'de> for V<T, N> {
            /// Collection returned only after the terminating sequence has been checked.
            type Value = BoundedVec<T, N>;

            /// Describe the element count to `f`; return the formatter's result.
            fn expecting(
                &self,
                f: &mut fmt::Formatter,
            ) -> fmt::Result {
                write!(f, "at most {N} entries")
            }

            /// Consume `a` in order; return bounded elements or reject the first excess entry.
            /// A failed element or excess entry discards the partially collected vector.
            fn visit_seq<A: SeqAccess<'de>>(
                self,
                mut a: A,
            ) -> Result<Self::Value, A::Error> {
                let mut v = Vec::new();
                while v.len() < N {
                    match a.next_element()? {
                        Some(x) => v.push(x),
                        None => return Ok(BoundedVec(v)),
                    }
                }
                if a.next_element::<de::IgnoredAny>()?.is_some() {
                    return Err(de::Error::custom("list bound"));
                }
                Ok(BoundedVec(v))
            }
        }
        d.deserialize_seq(V::<T, N>(PhantomData))
    }
}

/// Define a role-specific exact-width lowercase hexadecimal identifier and its strict decoder.
/// `$name` is the Rust type, `$n` the ASCII byte count, and `$doc` its identity/authority contract.
macro_rules! hex_id {
    ($name:ident, $n:expr, $doc:literal) => {
        #[doc = $doc]
        #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
        #[serde(transparent)]
        pub struct $name(
            /// Exact-width ASCII hexadecimal bytes; never normalized from uppercase input.
            String,
        );
        impl $name {
            /// Consume `s`; return a validated identifier or [`Error::Schema`] for width/alphabet errors.
            /// Validation establishes syntax only, not existence, randomness, or authorization.
            pub fn new(s: impl Into<String>) -> Result<Self, Error> {
                let s = s.into();
                if s.len() != $n
                    || !s
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                {
                    return Err(Error::Schema);
                }
                Ok(Self(s))
            }

            /// Return the exact lowercase hexadecimal representation.
            pub fn as_str(&self) -> &str { &self.0 }
        }
        impl<'de> Deserialize<'de> for $name {
            /// Decode bounded text from `d`; return the identifier or a width/alphabet error.
            fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                let s = Text::<$n>::deserialize(d)?;
                Self::new(s.as_str()).map_err(de::Error::custom)
            }
        }
    };
}
hex_id!(
    JobId,
    32,
    "128-bit job correlation identifier; possession alone grants no job access."
);
hex_id!(
    ProjectId,
    32,
    "128-bit project identity used to correlate admitted project bytes and requests."
);
hex_id!(
    AttemptId,
    32,
    "128-bit build/flash attempt identity, distinct from its job or artifact."
);
hex_id!(
    SessionId,
    32,
    "128-bit session correlation identity; session authority is checked separately."
);
hex_id!(
    DeviceId,
    32,
    "128-bit helper device identity; not a hardware capacity or authenticity certificate."
);
hex_id!(
    ArtifactId,
    64,
    "256-bit artifact identity in lowercase hexadecimal; admission still verifies its binding."
);
hex_id!(
    Sha256,
    64,
    "SHA-256 digest representation; syntax validation does not compute or authenticate the digest."
);
hex_id!(
    FileId128,
    32,
    "128-bit filesystem file identity representation, independent of a path string."
);

/// JSON message contract with an encoded byte cap and explicit semantic admission.
/// Direct Serde use bypasses [`Wire::validate`]; untrusted ingress must call [`decode`].
pub trait Wire: Serialize + for<'de> Deserialize<'de> {
    /// Maximum compact or received JSON bytes; incoming whitespace also counts.
    const MAX_BYTES: usize;
    /// Return success only for supported schema and state-independent field invariants.
    /// This check does not establish current-session authority or perform an operation.
    fn validate(&self) -> Result<(), Error>;
}
/// Decode `bytes` only within `T`'s cap; return a semantically validated message.
/// Reject oversized input before parsing; JSON errors map to `Schema`, then validation errors propagate.
pub fn decode<T: Wire>(bytes: &[u8]) -> Result<T, Error> {
    if bytes.len() > T::MAX_BYTES {
        return Err(Error::Bounds);
    }
    let value: T = serde_json::from_slice(bytes).map_err(|_| Error::Schema)?;
    value.validate()?;
    Ok(value)
}
/// Validate `value`, serialize compact JSON, then return bytes within `T`'s cap.
/// Serialization allocates before the final byte check; failures publish no partial message.
pub fn encode<T: Wire>(value: &T) -> Result<Vec<u8>, Error> {
    value.validate()?;
    let bytes = serde_json::to_vec(value).map_err(|_| Error::Schema)?;
    if bytes.len() > T::MAX_BYTES {
        Err(Error::Bounds)
    } else {
        Ok(bytes)
    }
}
/// Serialize `value` temporarily and return success if its compact JSON fits `max` bytes.
/// Serialization errors map to `Schema`; excess maps to `Bounds`. This is not an allocation cap.
pub fn serialized_bound<T: Serialize>(
    value: &T,
    max: usize,
) -> Result<(), Error> {
    if serde_json::to_vec(value).map_err(|_| Error::Schema)?.len() > max {
        Err(Error::Bounds)
    } else {
        Ok(())
    }
}
