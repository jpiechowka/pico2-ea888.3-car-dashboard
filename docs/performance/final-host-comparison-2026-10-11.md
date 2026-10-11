# Full-suite repeatability comparison

Same implementation and fixtures, not a code-change speedup comparison.
Source commit: `0f60e0614fe934352d4f871f3eeac7cadc0f8278`. Default outline.
72 cases, 30 samples each, identical 179-file input hashes and compiler/configuration.
The predeclared 20% review threshold passes for every row (exit 0).

```text
# Run from host/
python scripts/compare_benchmarks.py baselines/stage08_final_a baselines/stage08_final_b --budget-percent 20 --gate
```

Raw measurements and source archives: [run A](../../host/baselines/stage08_final_a/metadata.json)
and [run B](../../host/baselines/stage08_final_b/metadata.json).
[Interpretation and matched rendering paths](final-host-results-2026-10-11.md).
Criterion's portable folder names are used below. Times are median ns per named
operation, including batches where the fixture name states multiple frames.

| Fixture | Run A ns | Run B ns | Change | Result |
|---|---:|---:|---:|---|
| acquisition_batch_and_snapshot | 541.357 | 552.953 | +2.14% | pass |
| acquisition_typed_sample_and_snapshot | 273.684 | 273.746 | +0.02% | pass |
| acquisition_watch_copy_roundtrip | 79.429 | 79.728 | +0.38% | pass |
| alarms_eight_channel_three_samples | 211.485 | 215.822 | +2.05% | pass |
| boost_cell_psi_peak | 6992.497 | 7063.192 | +1.01% | pass |
| cell_render_battery | 10542.246 | 10674.499 | +1.25% | pass |
| cell_render_boost | 8579.106 | 8596.157 | +0.20% | pass |
| cell_render_lambda | 13519.754 | 13589.062 | +0.51% | pass |
| cell_render_temperature | 11052.840 | 11201.497 | +1.34% | pass |
| client_isotp_128_bytes | 661.312 | 678.280 | +2.57% | pass |
| client_single_frame_obd | 70.332 | 74.620 | +6.10% | pass |
| client_terminal_malformed | 49.853 | 48.265 | -3.19% | pass |
| client_terminal_negative | 64.798 | 63.539 | -1.94% | pass |
| client_terminal_pendingthennegative | 103.025 | 101.491 | -1.49% | pass |
| client_terminal_timeout | 31.274 | 31.361 | +0.28% | pass |
| clipped_contiguous/left_8_px | 5084.501 | 5180.071 | +1.88% | pass |
| damage_dense | 261.062 | 276.258 | +5.82% | pass |
| damage_digit_width_with_ranges | 2850.205 | 2729.863 | -4.22% | pass |
| damage_disjoint_full_fallback_with_ranges | 2696.622 | 2773.103 | +2.84% | pass |
| damage_disjoint_sparse_cells_with_ranges | 2952.535 | 2762.037 | -6.45% | pass |
| damage_one_cell | 2573.300 | 2707.253 | +5.21% | pass |
| damage_one_digit_with_ranges | 2883.261 | 2885.103 | +0.06% | pass |
| damage_popup_removal_with_ranges | 2537.992 | 2444.075 | -3.70% | pass |
| damage_stationary | 2774.758 | 2897.840 | +4.44% | pass |
| fill_contiguous/clipped_four_edges | 75986.349 | 75571.176 | -0.55% | pass |
| fill_contiguous/full | 76781.990 | 76959.866 | +0.23% | pass |
| fill_contiguous/narrow_1px | 526.164 | 514.583 | -2.20% | pass |
| fill_contiguous/odd_x_width | 9814.531 | 9958.268 | +1.46% | pass |
| fill_solid/clipped_four_edges | 13638.821 | 13768.912 | +0.95% | pass |
| fill_solid/full | 13638.773 | 13666.826 | +0.21% | pass |
| fill_solid/narrow_1px | 64.429 | 62.180 | -3.49% | pass |
| fill_solid/odd_x_width | 1513.748 | 1473.453 | -2.66% | pass |
| frame_window_record_all_metrics | 41.210 | 40.712 | -1.21% | pass |
| frame_window_summarize_1500_frames | 1031.940 | 1058.968 | +2.62% | pass |
| framebuffer/clear_full | 2070.100 | 2093.110 | +1.11% | pass |
| input_coherent_mailbox_roundtrip | 33.963 | 34.079 | +0.34% | pass |
| input_legacy_atomic_roundtrip | 14.618 | 14.833 | +1.47% | pass |
| logging_full_page | 54033.838 | 55341.008 | +2.42% | pass |
| logging_insert_and_snapshot | 176.165 | 179.133 | +1.68% | pass |
| mini_graph_60_samples_constant | 1965.938 | 1917.528 | -2.46% | pass |
| mini_graph_60_samples_newest_impulse | 2472.488 | 2505.313 | +1.33% | pass |
| protocol_isotp_128_bytes | 360.541 | 377.512 | +4.71% | pass |
| protocol_rx12_record | 26.684 | 26.182 | -1.88% | pass |
| protocol_single_frame_obd | 32.862 | 33.515 | +1.99% | pass |
| replay_demo_model_to_full_eight_frames | 1012538.542 | 1029084.473 | +1.63% | pass |
| replay_demo_model_to_retained_eight_frames | 993558.250 | 1024576.703 | +3.12% | pass |
| replay_slow_bursty_session_to_full_seventeen_frames | 678703.912 | 664923.182 | -2.03% | pass |
| replay_slow_bursty_session_to_retained_seventeen_frames | 193380.398 | 194789.924 | +0.73% | pass |
| scene_full_all_changed_eight_frames | 991051.527 | 994666.396 | +0.36% | pass |
| scene_full_dashboard | 100061.157 | 103605.115 | +3.54% | pass |
| scene_full_one_cell | 101384.510 | 103256.638 | +1.85% | pass |
| scene_retained_all_changed_eight_frames | 983906.944 | 1013573.295 | +3.02% | pass |
| scene_retained_one_cell | 10276.673 | 10620.127 | +3.34% | pass |
| scene_retained_stationary | 1893.209 | 1895.617 | +0.13% | pass |
| scene_transition_cold_cache_one_frame | 105948.221 | 103098.241 | -2.69% | pass |
| scene_transition_digit_width_one_frame | 15474.960 | 15552.463 | +0.50% | pass |
| scene_transition_quality_transitions_three_frames | 19808.976 | 19502.633 | -1.55% | pass |
| scene_transition_same_rounded_text_one_frame | 2072.837 | 2098.674 | +1.25% | pass |
| scene_transition_unit_toggle_one_frame | 10622.370 | 10616.923 | -0.05% | pass |
| sensor_state_next_measurement_update | 3.709 | 3.720 | +0.31% | pass |
| sensor_state_no_new_sample_tick_snapshot | 35.492 | 34.148 | -3.79% | pass |
| session_coolant_transaction_and_snapshot | 418.825 | 464.917 | +11.01% | pass |
| session_diagnostic_distribution_export | 313.094 | 319.378 | +2.01% | pass |
| settings_load_full_journal | 14163.514 | 14342.005 | +1.26% | pass |
| settings_maintenance_transaction_model | 33.811 | 35.367 | +4.60% | pass |
| settings_rollover_transaction_model | 17525.167 | 17637.868 | +0.64% | pass |
| target_kernel_clear_eight_iterations | 16306.877 | 16357.475 | +0.31% | pass |
| target_kernel_clipped_eight_iterations | 2380.869 | 2403.606 | +0.95% | pass |
| target_kernel_graphwidget_eight_iterations | 91551.035 | 92312.056 | +0.83% | pass |
| target_kernel_history_eight_iterations | 5233.372 | 5330.644 | +1.86% | pass |
| uart_rx12_ring_copy | 8.381 | 8.643 | +3.12% | pass |
| vehicle_coolant_io_session_snapshot | 528.637 | 546.414 | +3.36% | pass |
