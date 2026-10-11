#[path = "../benches/scene_replay/mod.rs"]
mod scene_replay;

#[test]
fn fixed_timestamped_benchmark_replay_changes_all_channels_and_matches_every_pixel() { scene_replay::verify(); }
