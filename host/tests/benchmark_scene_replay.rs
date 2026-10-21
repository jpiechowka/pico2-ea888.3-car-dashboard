//! Acceptance check for the exact prepared/live workload used by scene benchmarks.
/// Fixed replay shared with measured workloads; no timing assertion is made here.
#[path = "../benches/scene_replay/mod.rs"]
mod scene_replay;

/// All channels and history timestamps advance while retained and complete raster paths remain pixel-identical.
#[test]
fn fixed_timestamped_benchmark_replay_changes_all_channels_and_matches_every_pixel() { scene_replay::verify(); }
