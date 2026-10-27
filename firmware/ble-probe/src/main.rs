//! Standard-only standalone BLE probe entrypoint, independent of package features.
//!
//! Core0 owns hardware, radio, read-only USB export and capture flash; core1 owns
//! the catalogue and rendering. Two affine framebuffers and bounded revision-tagged
//! messages cross cores. Connections require selected-peer inspection; no ECU polling.
#![no_std]
#![no_main]

include!("application.rs");
use defmt_rtt as _;
use panic_probe as _;

/// Fixed standard labels and image marker, paired with the standard inspector below.
const TOOL_KIND: ToolKind = ToolKind::StandardProbe;

/// Fixed standard inspection entrypoint, independent of package feature unification.
pub use radio::inspect_peer as inspect_selected_peer;

#[unsafe(link_section = ".bi_entries")]
#[used]
/// Read-only picotool identity/build metadata for the standard discovery image.
static INFO: [embassy_rp::binary_info::EntryAddr; 4] = [
    embassy_rp::binary_info::rp_program_name!(c"pico2w-ble-probe"),
    embassy_rp::binary_info::rp_program_description!(c"BLE discovery and manual read-only GATT inspection"),
    embassy_rp::binary_info::rp_cargo_version!(),
    embassy_rp::binary_info::rp_program_build_attribute!(),
];

#[used]
#[unsafe(no_mangle)]
#[unsafe(link_section = ".probe_tool")]
/// Linked utility discriminator: zero is the standard-only probe, regardless of features.
pub static BLE_PROBE_TOOL_KIND: u8 = TOOL_KIND as u8;

#[embassy_executor::main]
/// Starts shared owners with `spawner` and the fixed standard-only policy.
/// Returns after initialization; shared owners run indefinitely. Panics if the
/// one-time hardware/task initialization fails. Never selects identity by feature.
async fn main(spawner: Spawner) { run(spawner).await; }
