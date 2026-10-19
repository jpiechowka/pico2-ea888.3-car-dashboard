//! Bounded, hardware-independent BLE discovery and photographic inspection.
mod advertisement;
mod catalog;
mod inspection;
mod model;
mod telemetry;
pub use advertisement::*;
pub use catalog::*;
pub use inspection::*;
pub use model::*;
pub use telemetry::*;
mod render;
pub use render::{page_count, render};
