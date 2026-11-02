//! Backend-selecting assertions and logging; log and defmt features are mutually exclusive.
#![macro_use]
#![allow(unused)]

use core::fmt::{Debug, Display, LowerHex};

#[cfg(all(feature = "defmt", feature = "log"))]
compile_error!("You may not enable both `defmt` and `log` features.");

#[collapse_debuginfo(yes)]
/// Asserts the supplied condition using core or defmt; false conditions panic.
macro_rules! assert {
    ($($x:tt)*) => {
        {
            #[cfg(not(feature = "defmt"))]
            ::core::assert!($($x)*);
            #[cfg(feature = "defmt")]
            ::defmt::assert!($($x)*);
        }
    };
}

#[collapse_debuginfo(yes)]
/// Asserts equality through core or defmt and panics with the supplied diagnostic on mismatch.
macro_rules! assert_eq {
    ($($x:tt)*) => {
        {
            #[cfg(not(feature = "defmt"))]
            ::core::assert_eq!($($x)*);
            #[cfg(feature = "defmt")]
            ::defmt::assert_eq!($($x)*);
        }
    };
}

#[collapse_debuginfo(yes)]
/// Asserts inequality through core or defmt and panics when the compared values match.
macro_rules! assert_ne {
    ($($x:tt)*) => {
        {
            #[cfg(not(feature = "defmt"))]
            ::core::assert_ne!($($x)*);
            #[cfg(feature = "defmt")]
            ::defmt::assert_ne!($($x)*);
        }
    };
}

#[collapse_debuginfo(yes)]
/// Delegates a condition check to the selected backend's debug assertion configuration.
macro_rules! debug_assert {
    ($($x:tt)*) => {
        {
            #[cfg(not(feature = "defmt"))]
            ::core::debug_assert!($($x)*);
            #[cfg(feature = "defmt")]
            ::defmt::debug_assert!($($x)*);
        }
    };
}

#[collapse_debuginfo(yes)]
/// Delegates an equality check to the selected backend's debug assertion configuration.
macro_rules! debug_assert_eq {
    ($($x:tt)*) => {
        {
            #[cfg(not(feature = "defmt"))]
            ::core::debug_assert_eq!($($x)*);
            #[cfg(feature = "defmt")]
            ::defmt::debug_assert_eq!($($x)*);
        }
    };
}

#[collapse_debuginfo(yes)]
/// Delegates an inequality check to the selected backend's debug assertion configuration.
macro_rules! debug_assert_ne {
    ($($x:tt)*) => {
        {
            #[cfg(not(feature = "defmt"))]
            ::core::debug_assert_ne!($($x)*);
            #[cfg(feature = "defmt")]
            ::defmt::debug_assert_ne!($($x)*);
        }
    };
}

#[collapse_debuginfo(yes)]
/// Panics through the selected backend to mark an unimplemented path.
macro_rules! todo {
    ($($x:tt)*) => {
        {
            #[cfg(not(feature = "defmt"))]
            ::core::todo!($($x)*);
            #[cfg(feature = "defmt")]
            ::defmt::todo!($($x)*);
        }
    };
}

#[collapse_debuginfo(yes)]
/// Panics through the selected backend if a supposedly unreachable branch executes.
macro_rules! unreachable {
    ($($x:tt)*) => {
        {
            #[cfg(not(feature = "defmt"))]
            ::core::unreachable!($($x)*);
            #[cfg(feature = "defmt")]
            ::defmt::unreachable!($($x)*);
        }
    };
}

#[collapse_debuginfo(yes)]
/// Emits the supplied panic diagnostic through core or defmt and does not return.
macro_rules! panic {
    ($($x:tt)*) => {
        {
            #[cfg(not(feature = "defmt"))]
            ::core::panic!($($x)*);
            #[cfg(feature = "defmt")]
            ::defmt::panic!($($x)*);
        }
    };
}

#[collapse_debuginfo(yes)]
/// Logs the supplied format and expressions at trace level; without a backend evaluates and borrows expressions only.
macro_rules! trace {
    ($s:literal $(, $x:expr)* $(,)?) => {
        {
            #[cfg(feature = "log")]
            ::log::trace!($s $(, $x)*);
            #[cfg(feature = "defmt")]
            ::defmt::trace!($s $(, $x)*);
            #[cfg(not(any(feature = "log", feature="defmt")))]
            let _ = ($( & $x ),*);
        }
    };
}

#[collapse_debuginfo(yes)]
/// Logs the supplied format and expressions at debug level; without a backend evaluates and borrows expressions only.
macro_rules! debug {
    ($s:literal $(, $x:expr)* $(,)?) => {
        {
            #[cfg(feature = "log")]
            ::log::debug!($s $(, $x)*);
            #[cfg(feature = "defmt")]
            ::defmt::debug!($s $(, $x)*);
            #[cfg(not(any(feature = "log", feature="defmt")))]
            let _ = ($( & $x ),*);
        }
    };
}

#[collapse_debuginfo(yes)]
/// Logs the supplied format and expressions at info level; without a backend evaluates and borrows expressions only.
macro_rules! info {
    ($s:literal $(, $x:expr)* $(,)?) => {
        {
            #[cfg(feature = "log")]
            ::log::info!($s $(, $x)*);
            #[cfg(feature = "defmt")]
            ::defmt::info!($s $(, $x)*);
            #[cfg(not(any(feature = "log", feature="defmt")))]
            let _ = ($( & $x ),*);
        }
    };
}

#[collapse_debuginfo(yes)]
/// Logs the supplied format and expressions at warning level; without a backend evaluates and borrows expressions only.
macro_rules! warn {
    ($s:literal $(, $x:expr)* $(,)?) => {
        {
            #[cfg(feature = "log")]
            ::log::warn!($s $(, $x)*);
            #[cfg(feature = "defmt")]
            ::defmt::warn!($s $(, $x)*);
            #[cfg(not(any(feature = "log", feature="defmt")))]
            let _ = ($( & $x ),*);
        }
    };
}

#[collapse_debuginfo(yes)]
/// Logs the supplied format and expressions at error level; without a backend evaluates and borrows expressions only.
macro_rules! error {
    ($s:literal $(, $x:expr)* $(,)?) => {
        {
            #[cfg(feature = "log")]
            ::log::error!($s $(, $x)*);
            #[cfg(feature = "defmt")]
            ::defmt::error!($s $(, $x)*);
            #[cfg(not(any(feature = "log", feature="defmt")))]
            let _ = ($( & $x ),*);
        }
    };
}

#[cfg(feature = "defmt")]
#[collapse_debuginfo(yes)]
/// Unwraps Option/Result through defmt, returning the value or panicking with its diagnostic.
macro_rules! unwrap {
    ($($x:tt)*) => {
        ::defmt::unwrap!($($x)*)
    };
}

#[cfg(not(feature = "defmt"))]
#[collapse_debuginfo(yes)]
/// Returns the supplied Option/Result success value; failure panics with expression and optional message context.
macro_rules! unwrap {
    ($arg:expr) => {
        match $crate::fmt::Try::into_result($arg) {
            ::core::result::Result::Ok(t) => t,
            ::core::result::Result::Err(e) => {
                ::core::panic!("unwrap of `{}` failed: {:?}", ::core::stringify!($arg), e);
            }
        }
    };
    ($arg:expr, $($msg:expr),+ $(,)? ) => {
        match $crate::fmt::Try::into_result($arg) {
            ::core::result::Result::Ok(t) => t,
            ::core::result::Result::Err(e) => {
                ::core::panic!("unwrap of `{}` failed: {}: {:?}", ::core::stringify!($arg), ::core::format_args!($($msg,)*), e);
            }
        }
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
/// Marker error produced when a missing Option is passed to the local unwrap adapter.
pub struct NoneError;

/// Converts Option and Result into a common result for diagnostic unwrapping.
pub trait Try {
    /// Value returned by successful unwrapping.
    type Ok;
    /// Failure value included in the panic diagnostic.
    type Error;
    /// Returns the success value or the original/adapted failure without panicking.
    fn into_result(self) -> Result<Self::Ok, Self::Error>;
}

impl<T> Try for Option<T> {
    /// Marker representing None without an invented payload.
    type Error = NoneError;
    /// Value stored by Some.
    type Ok = T;

    #[inline]
    /// Returns Ok for Some and NoneError for None; consumes the Option.
    fn into_result(self) -> Result<T, NoneError> { self.ok_or(NoneError) }
}

impl<T, E> Try for Result<T, E> {
    /// Original result failure type.
    type Error = E;
    /// Original result success type.
    type Ok = T;

    #[inline]
    /// Returns this result unchanged, preserving its success value or error.
    fn into_result(self) -> Self { self }
}

/// Borrowed byte slice rendered as hexadecimal by either logging backend.
pub(crate) struct Bytes<'a>(
    /// Borrowed initialized bytes; formatting does not retain them beyond the call.
    pub &'a [u8],
);

impl Debug for Bytes<'_> {
    /// Writes alternate hexadecimal debug bytes to `f`; returns the formatter result.
    fn fmt(
        &self,
        f: &mut core::fmt::Formatter<'_>,
    ) -> core::fmt::Result {
        write!(f, "{:#02x?}", self.0)
    }
}

impl Display for Bytes<'_> {
    /// Writes the same hexadecimal byte representation to `f`; returns the formatter result.
    fn fmt(
        &self,
        f: &mut core::fmt::Formatter<'_>,
    ) -> core::fmt::Result {
        write!(f, "{:#02x?}", self.0)
    }
}

impl LowerHex for Bytes<'_> {
    /// Writes the byte slice in hexadecimal to `f`; returns the formatter result.
    fn fmt(
        &self,
        f: &mut core::fmt::Formatter<'_>,
    ) -> core::fmt::Result {
        write!(f, "{:#02x?}", self.0)
    }
}

#[cfg(feature = "defmt")]
impl defmt::Format for Bytes<'_> {
    /// Writes the borrowed bytes as hexadecimal through defmt `fmt`; no allocation or ownership transfer.
    fn format(
        &self,
        fmt: defmt::Formatter,
    ) {
        defmt::write!(fmt, "{:02x}", self.0)
    }
}
