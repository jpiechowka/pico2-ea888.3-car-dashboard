#![cfg_attr(windows, feature(windows_process_extensions_raw_attribute))]
//! Studio job core and bounded native prerequisite checks.
mod host;
pub mod jobs;
pub mod prerequisites;
mod process;
pub mod server;
