#![cfg_attr(windows, feature(windows_process_extensions_raw_attribute))]
//! Studio job core and bounded native prerequisite checks.
pub mod build_input;
pub mod devices;
mod host;
pub mod jobs;
pub mod prerequisites;
mod process;
pub mod server;

mod terminal;
pub mod relay;

mod artifacts;

mod file_locks;

mod device_inspection;

pub mod flash_output;
