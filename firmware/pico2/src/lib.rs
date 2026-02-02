//! Dashboard library - testable modules for the OBD-II dashboard.
//!
//! This library contains the core logic that can be tested on the host machine.
//! The binary (`main.rs`) uses this library and adds the embedded-specific code.
//!
//! # Testing
//!
//! Run tests on host with:
//! ```bash
//! cargo test -p dashboard-pico2 --lib --target x86_64-unknown-linux-gnu  # Linux/macOS
//! cargo test -p dashboard-pico2 --lib --target x86_64-pc-windows-msvc    # Windows
//! ```
//!
//! Tests run with `std` enabled (via `cfg_attr`), allowing use of the standard
//! test framework while the actual firmware runs as `no_std`.

// Use no_std only when NOT testing (tests need std for the test harness)
#![cfg_attr(not(test), no_std)]
// Crate-level lints
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_sign_loss)]

// === Pure logic modules (testable on host, no ARM dependencies) ===

pub mod colors;
pub mod config;
pub mod cpu_cycles;
pub mod memory;
pub mod pages;
pub mod render;
pub mod sensor_state;
pub mod thresholds;
