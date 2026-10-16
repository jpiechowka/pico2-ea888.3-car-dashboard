use std::fmt;
use std::marker::PhantomData;
use std::ops::Deref;

use serde::de::{self, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Schema,
    Bounds,
    InvalidState,
    Busy,
    Expired,
    NotFoundOrUnavailable,
    ExpertRequired,
    Clock,
    Closed,
    SequenceExhausted,
}
impl fmt::Display for Error {
    fn fmt(
        &self,
        f: &mut fmt::Formatter<'_>,
    ) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Error {}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct Text<const N: usize>(String);
impl<const N: usize> Text<N> {
    pub fn new(s: impl Into<String>) -> Result<Self, Error> {
        let s = s.into();
        if s.len() > N { Err(Error::Bounds) } else { Ok(Self(s)) }
    }

    pub fn as_str(&self) -> &str { &self.0 }
}
impl<const N: usize> Deref for Text<N> {
    type Target = str;

    fn deref(&self) -> &str { &self.0 }
}
impl<'de, const N: usize> Deserialize<'de> for Text<N> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V<const N: usize>;
        impl<'de, const N: usize> Visitor<'de> for V<N> {
            type Value = Text<N>;

            fn expecting(
                &self,
                f: &mut fmt::Formatter,
            ) -> fmt::Result {
                write!(f, "at most {N} UTF-8 bytes")
            }

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

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct BoundedVec<T, const N: usize>(Vec<T>);
impl<T, const N: usize> BoundedVec<T, N> {
    pub fn new(v: Vec<T>) -> Result<Self, Error> { if v.len() > N { Err(Error::Bounds) } else { Ok(Self(v)) } }

    pub fn into_vec(self) -> Vec<T> { self.0 }
}
impl<T, const N: usize> Default for BoundedVec<T, N> {
    fn default() -> Self { Self(Vec::new()) }
}
impl<T, const N: usize> Deref for BoundedVec<T, N> {
    type Target = [T];

    fn deref(&self) -> &[T] { &self.0 }
}
impl<'de, T: Deserialize<'de>, const N: usize> Deserialize<'de> for BoundedVec<T, N> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V<T, const N: usize>(PhantomData<T>);
        impl<'de, T: Deserialize<'de>, const N: usize> Visitor<'de> for V<T, N> {
            type Value = BoundedVec<T, N>;

            fn expecting(
                &self,
                f: &mut fmt::Formatter,
            ) -> fmt::Result {
                write!(f, "at most {N} entries")
            }

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

macro_rules! hex_id {
    ($name:ident, $n:expr) => {
        #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
        #[serde(transparent)]
        pub struct $name(String);
        impl $name {
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

            pub fn as_str(&self) -> &str { &self.0 }
        }
        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                let s = Text::<$n>::deserialize(d)?;
                Self::new(s.as_str()).map_err(de::Error::custom)
            }
        }
    };
}
hex_id!(JobId, 32);
hex_id!(ProjectId, 32);
hex_id!(AttemptId, 32);
hex_id!(SessionId, 32);
hex_id!(DeviceId, 32);
hex_id!(ArtifactId, 64);
hex_id!(Sha256, 64);
hex_id!(FileId128, 32);

pub trait Wire: Serialize + for<'de> Deserialize<'de> {
    const MAX_BYTES: usize;
    fn validate(&self) -> Result<(), Error>;
}
pub fn decode<T: Wire>(bytes: &[u8]) -> Result<T, Error> {
    if bytes.len() > T::MAX_BYTES {
        return Err(Error::Bounds);
    }
    let value: T = serde_json::from_slice(bytes).map_err(|_| Error::Schema)?;
    value.validate()?;
    Ok(value)
}
pub fn encode<T: Wire>(value: &T) -> Result<Vec<u8>, Error> {
    value.validate()?;
    let bytes = serde_json::to_vec(value).map_err(|_| Error::Schema)?;
    if bytes.len() > T::MAX_BYTES {
        Err(Error::Bounds)
    } else {
        Ok(bytes)
    }
}
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
