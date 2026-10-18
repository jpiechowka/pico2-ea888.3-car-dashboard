#![cfg_attr(windows, feature(windows_process_extensions_raw_attribute))]
//! Studio jobs, bounded native tool checks, immutable builds and explicit device operations.
pub mod build_input;
pub mod devices;
mod diagnostics;
mod host;
pub mod jobs;
pub mod prerequisites;
mod process;
pub mod server;

pub mod relay;
mod terminal;

mod artifacts;

mod file_locks;

mod device_inspection;

pub mod flash_output;

mod flash_intent;

mod flash;

mod build_storage;
mod flash_cleanup;
