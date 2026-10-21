//! Internal logging/panic adapters; `defmt` and `log` are mutually exclusive.
#![macro_use]
#![allow(unused_macros)]

use core::fmt::{Debug, Display, LowerHex};

#[cfg(all(feature = "defmt", feature = "log"))]
compile_error!("You may not enable both `defmt` and `log` features.");

#[collapse_debuginfo(yes)]
/// Checks the supplied condition and optional message with core or defmt; false panics.
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
/// Checks supplied operands for equality with core or defmt; unequal values panic.
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
/// Checks supplied operands for inequality with core or defmt; equal values panic.
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
/// Checks the supplied condition only with debug assertions enabled; failure panics through the selected backend.
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
/// Checks supplied operands for equality only with debug assertions enabled; failure panics.
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
/// Checks supplied operands for inequality only with debug assertions enabled; failure panics.
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
/// Panics through core or defmt with the optional supplied message; never returns.
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
#[cfg(not(feature = "defmt"))]
/// Panics if the marked branch executes; the optional message uses the selected backend. Never returns.
macro_rules! unreachable {
    ($($x:tt)*) => {
        ::core::unreachable!($($x)*)
    };
}

#[collapse_debuginfo(yes)]
#[cfg(feature = "defmt")]
/// Panics if the marked branch executes; the optional message uses the selected backend. Never returns.
macro_rules! unreachable {
    ($($x:tt)*) => {
        ::defmt::unreachable!($($x)*)
    };
}

#[collapse_debuginfo(yes)]
/// Panics through core or defmt using the supplied message; never returns.
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
/// Emits the format literal and expressions at trace level through the selected backend.
/// With no logger, expressions are still evaluated and borrowed, but no output is produced.
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
/// Emits the format literal and expressions at debug level through the selected backend.
/// With no logger, expressions are still evaluated and borrowed, but no output is produced.
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
/// Emits the format literal and expressions at info level through the selected backend.
/// With no logger, expressions are still evaluated and borrowed, but no output is produced.
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
/// Emits the format literal and expressions at warn level through the selected backend.
/// With no logger, expressions are still evaluated and borrowed, but no output is produced.
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
/// Emits the format literal and expressions at error level through the selected backend.
/// With no logger, expressions are still evaluated and borrowed, but no output is produced.
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

#[collapse_debuginfo(yes)]
#[cfg(feature = "defmt")]
/// Returns the supplied Option/Result success value; None/Err panics with the optional message.
macro_rules! unwrap {
    ($($x:tt)*) => {
        ::defmt::unwrap!($($x)*)
    };
}

#[collapse_debuginfo(yes)]
#[cfg(not(feature = "defmt"))]
/// Returns the supplied Option/Result success value; None/Err panics with the optional message.
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

/// Failure marker used when an Option passed to unwrap contains no value.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct NoneError;

/// Converts Option and Result into one unwrap-compatible result without allocation.
pub trait Try {
    /// Value returned by a successful conversion.
    type Ok;
    /// Failure retained for panic diagnostics.
    type Error;
    /// Consumes self and returns the success value or diagnostic error.
    fn into_result(self) -> Result<Self::Ok, Self::Error>;
}

impl<T> Try for Option<T> {
    /// Original contained success type.
    type Ok = T;
    /// None maps to a dedicated absence marker.
    type Error = NoneError;

    #[inline]
    /// Returns Ok for Some, or NoneError for None, consuming self.
    fn into_result(self) -> Result<T, NoneError> {
        self.ok_or(NoneError)
    }
}

impl<T, E> Try for Result<T, E> {
    /// Original contained success type.
    type Ok = T;
    /// Original error type, unchanged.
    type Error = E;

    #[inline]
    /// Returns self unchanged, preserving both value and error variants.
    fn into_result(self) -> Self {
        self
    }
}

#[allow(unused)]
/// Borrowed byte sequence formatted as hexadecimal without allocation.
pub(crate) struct Bytes<'a>(#[doc = "Bytes retained for the lifetime of the formatting call."] pub &'a [u8]);

impl Debug for Bytes<'_> {
    /// Writes the byte slice as a lower-hex debug list to `f`; returns any formatter error.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{:#02x?}", self.0)
    }
}

impl Display for Bytes<'_> {
    /// Writes the byte slice as a lower-hex debug list to `f`; returns any formatter error.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{:#02x?}", self.0)
    }
}

impl LowerHex for Bytes<'_> {
    /// Writes the byte slice as a lower-hex debug list to `f`; returns any formatter error.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{:#02x?}", self.0)
    }
}

#[cfg(feature = "defmt")]
impl<'a> defmt::Format for Bytes<'a> {
    /// Writes two-digit hexadecimal bytes through defmt formatter `fmt`.
    fn format(&self, fmt: defmt::Formatter) {
        defmt::write!(fmt, "{:02x}", self.0)
    }
}

#[cfg(not(feature = "defmt"))]
/// Backend-independent format bound using core Debug when defmt is disabled.
pub trait Format: Debug {}

#[cfg(not(feature = "defmt"))]
impl<T: Debug> Format for T {}

#[cfg(feature = "defmt")]
/// Backend-independent format bound using defmt encoding when enabled.
pub trait Format: defmt::Format {}

#[cfg(feature = "defmt")]
impl<T: defmt::Format> Format for T {}
