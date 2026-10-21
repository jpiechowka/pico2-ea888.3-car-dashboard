#![cfg_attr(windows, feature(windows_process_extensions_raw_attribute))]
//! Studio jobs, bounded native tool checks, immutable builds and explicit device operations.
/// Bounded immutable project snapshots and firmware build recipe admission.
pub mod build_input;
/// BOOTSEL identity enumeration and explicit device selection.
pub mod devices;
/// Authenticated bounded browser diagnostic ingress.
mod diagnostics;
/// Platform-owned tool process execution and output disposition.
mod host;
/// Serialized native job ownership, cancellation and bounded event journals.
pub mod jobs;
/// Installed-tool discovery with bounded native probes.
pub mod prerequisites;
/// Owned native process supervision and cancellation contracts.
mod process;
/// Loopback HTTP framing, capability admission and operation dispatch.
pub mod server;

/// Bounded terminal relay of browser diagnostics.
pub mod relay;
/// Console formatting and diagnostic presentation.
mod terminal;

/// Validated local ELF/UF2 publication and retained artifact selection.
mod artifacts;

/// Windows file sharing, stable handles and delete-by-handle ownership.
mod file_locks;

/// Explicit picotool device interrogation and bounded report parsing.
mod device_inspection;

/// Conservative parsing of picotool transfer/verification/reboot observations.
pub mod flash_output;

/// Durable attempt intent and restart recovery without automatic retry.
mod flash_intent;

/// Explicit admitted-image/device transfer, verification and reboot stages.
mod flash;

/// Bounded build-attempt inventory and safe explicit cleanup.
mod build_storage;
/// Preflighted removal of selected local attempt records.
mod flash_cleanup;
