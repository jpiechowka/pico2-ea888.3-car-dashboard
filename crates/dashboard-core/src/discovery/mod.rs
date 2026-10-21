//! Bounded, hardware-independent BLE discovery, inspection and saved capture evidence.
/// Legacy advertising framing and peer identity.
mod advertisement;
/// Bounded saved-capture encoding and semantic admission.
pub mod capture;
/// Two-copy persistent capture journal and selection policy.
pub mod capture_store;
/// User-confirmed capture operations with nonreused request identities.
pub mod capture_ui;
/// Read-only serial recovery framing and bounded transfer state.
pub mod capture_wire;
/// Stable peer catalogue with per-packet observation provenance.
mod catalog;
/// Bounded operation failure and rejected declaration evidence.
mod failure;
/// Shared fault-bit assignments and LED priority classification.
mod faults;
/// GATT inventory admission and standard-information read allowlist.
mod inspection;
/// Generation-checked discovery controls and owner events.
mod model;
/// Name selection with packet/read/cache provenance.
pub mod names;
/// Controller batch framing diagnostics retained across pauses.
mod scan_diagnostics;
/// Physical display/work/memory observations kept separate from saved evidence.
mod telemetry;
pub use advertisement::*;
pub use catalog::*;
pub use failure::*;
pub use faults::*;
pub use inspection::*;
pub use model::*;
pub use scan_diagnostics::*;
pub use telemetry::*;
/// Shared live, saved-capture and slot screens.
mod render;
pub use render::{page_count, render, render_saved, render_slots, saved_page_count};
