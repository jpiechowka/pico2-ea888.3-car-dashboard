//! Explicit adapter-identity utility entrypoint; vehicle data remains unavailable.
//!
//! Core0 owns hardware, radio, read-only USB export and capture flash; core1 owns
//! the catalogue and rendering. Two affine framebuffers and bounded revision-tagged
//! messages cross cores. Connections require selected-peer inspection; no ECU polling.
#![no_std]
#![no_main]

include!("../application.rs");
use defmt_rtt as _;
use panic_probe as _;

/// Fixed identity labels and image marker, paired with the INFO inspector below.
const TOOL_KIND: ToolKind = ToolKind::NgmIdentity;

/// Fixed INFO-only inspection entrypoint; never selected from runtime input.
pub use identity_radio::inspect_peer as inspect_selected_peer;

#[unsafe(link_section = ".bi_entries")]
#[used]
/// Read-only picotool identity/build metadata for the adapter identity image.
static INFO: [embassy_rp::binary_info::EntryAddr; 4] = [
    embassy_rp::binary_info::rp_program_name!(c"pico2w-ngm-identity"),
    embassy_rp::binary_info::rp_program_description!(c"Manual NGM adapter identity trial; no ECU requests"),
    embassy_rp::binary_info::rp_cargo_version!(),
    embassy_rp::binary_info::rp_program_build_attribute!(),
];

#[used]
#[unsafe(no_mangle)]
#[unsafe(link_section = ".probe_tool")]
/// Linked utility discriminator: one identifies the explicit NGM identity image.
pub static BLE_PROBE_TOOL_KIND: u8 = TOOL_KIND as u8;

#[embassy_executor::main]
/// Starts shared owners with `spawner` and the fixed identity policy; boot scans
/// without inspecting a peer. Returns after initialization. Shared initialization
/// panics on ownership failure; identity transport requires explicit selected-peer input.
async fn main(spawner: Spawner) { run(spawner).await; }
