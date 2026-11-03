//! Explicit NGM research image: adapter-only Inspect and separately confirmed fixed ECU reads.
//! No startup probing, ECU writes, unlock, session changes, generic commands or automatic retries.
#![no_std]
#![no_main]

include!("../application.rs");
use defmt_rtt as _;
use panic_probe as _;

/// Fixed collector identity; enabling its feature cannot change another binary's policy.
const TOOL_KIND: ToolKind = ToolKind::NgmCollector;

/// Shared selected-peer owner; only an explicit Collect action may enter the read sub-session.
pub use identity_radio::inspect_peer as inspect_selected_peer;

#[unsafe(link_section = ".bi_entries")]
#[used]
/// Read-only image metadata; not adapter/ECU qualification or authority from a saved capture.
static INFO: [embassy_rp::binary_info::EntryAddr; 4] = [
    embassy_rp::binary_info::rp_program_name!(c"pico2w-ngm-collector"),
    embassy_rp::binary_info::rp_program_description!(c"Manual NGM INFO and separately confirmed fixed ECU reads"),
    embassy_rp::binary_info::rp_cargo_version!(),
    embassy_rp::binary_info::rp_program_build_attribute!(),
];

#[used]
#[unsafe(no_mangle)]
#[unsafe(link_section = ".probe_tool")]
/// Linked discriminator2: the only image allowed to accept confirmed Collect requests.
pub static BLE_PROBE_TOOL_KIND: u8 = TOOL_KIND as u8;

#[embassy_executor::main]
/// Initializes owners using `spawner`; boot scans only. Returns after owner launch.
/// Shared initialization panics on invalid hardware ownership or missing image feature.
async fn main(spawner: Spawner) { run(spawner).await; }
