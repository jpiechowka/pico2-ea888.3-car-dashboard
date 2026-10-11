#[allow(dead_code)]
#[path = "../benches/coverage/mod.rs"]
mod coverage;

#[test]
fn fill_workloads_match_coordinate_reference_and_record_work_counts() { coverage::fills::verify(); }

#[test]
fn scene_transitions_and_damage_replay_match_every_full_pixel() { coverage::scenes::verify(); }

#[test]
fn diagnostic_terminal_fixtures_have_exact_outcomes_and_no_partial_result() {
    coverage::diagnostics::verify_terminals();
}

#[test]
fn slow_bursty_diagnostics_preserve_quality_ages_peaks_and_pixels() { coverage::diagnostics::verify_replay(); }

#[test]
fn constant_history_and_no_new_sample_workloads_have_fixed_outputs() { coverage::verify_history(); }
