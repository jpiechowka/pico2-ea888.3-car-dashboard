//! Bounded, hardware-independent BLE discovery and photographic inspection.
mod advertisement;
mod catalog;
mod inspection;
mod model;
pub use advertisement::*;
pub use catalog::*;
pub use inspection::*;
pub use model::*;
mod render;
pub use render::{page_count, render};
