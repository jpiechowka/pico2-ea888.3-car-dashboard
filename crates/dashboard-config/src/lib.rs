#![no_std]

#[cfg(feature = "json")]
extern crate alloc;

mod catalog;
mod codec;
mod defaults;
mod fuel;
mod model;
mod validate;

pub use catalog::*;
pub use codec::*;
pub use defaults::{default_config, default_widget};
pub use model::*;
pub use validate::*;
