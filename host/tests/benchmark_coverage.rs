//! Acceptance gates for the exact shared benchmark fixtures, without collecting timings.
/// Fill/scene/diagnostic/history workloads shared with Criterion benches.
#[allow(dead_code)]
#[path = "../benches/coverage/mod.rs"]
mod coverage;

/// Fill output matches every coordinate while visible pixels and source iterator consumption remain exact.
#[test]
fn fill_workloads_match_coordinate_reference_and_record_work_counts() { coverage::fills::verify(); }

/// Retained scene transitions and replayed damage ranges reconstruct every full-render pixel.
#[test]
fn scene_transitions_and_damage_replay_match_every_full_pixel() { coverage::scenes::verify(); }

/// Negative, malformed, timeout and pending-then-negative fixtures terminate without late partial results.
#[test]
fn diagnostic_terminal_fixtures_have_exact_outcomes_and_no_partial_result() {
    coverage::diagnostics::verify_terminals();
}

/// Split/burst response replay retains expected quality ages, solicited extrema and complete framebuffer equality.
#[test]
fn slow_bursty_diagnostics_preserve_quality_ages_peaks_and_pixels() { coverage::diagnostics::verify_replay(); }

/// Constant history draws its exact expected row and repeated no-sample ticks preserve sensor state.
#[test]
fn constant_history_and_no_new_sample_workloads_have_fixed_outputs() { coverage::verify_history(); }
