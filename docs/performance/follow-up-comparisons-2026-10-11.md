# Complete follow-up host comparisons

All values are median nanoseconds, with the existing 20% policy. Labels describe
the raw samples, not proven causality. See [the decision and limitations](follow-up-host-results-2026-10-11.md).
No capture was discarded and no metadata was altered.

## audit2_before to audit2_band_candidate

Before measurement records are restricted in memory to the matching candidate keys; full metadata is validated.

Counts: 10 pass, 0 review, 1 regression. Gate failed.

| Fixture | Before ns | After ns | Change | Result |
|---|---:|---:|---:|---|
| damage_dense | 660.135 | 717.605 | +8.71% | pass |
| damage_digit_width_with_ranges | 6272.994 | 5538.031 | -11.72% | pass |
| damage_disjoint_full_fallback_with_ranges | 6319.656 | 6900.626 | +9.19% | pass |
| damage_disjoint_sparse_cells_with_ranges | 6256.230 | 5454.996 | -12.81% | pass |
| damage_first_byte | 6296.698 | 5422.232 | -13.89% | pass |
| damage_last_byte | 6602.677 | 5656.954 | -14.32% | pass |
| damage_last_byte_each_band | 6482.984 | 10937.032 | +68.70% | regression |
| damage_one_cell | 6334.887 | 3615.184 | -42.93% | pass |
| damage_one_digit_with_ranges | 6409.320 | 5781.403 | -9.80% | pass |
| damage_popup_removal_with_ranges | 5385.526 | 4958.890 | -7.92% | pass |
| damage_stationary | 8182.434 | 4353.625 | -46.79% | pass |

## audit2_before to audit2_band_repeat

Before measurement records are restricted in memory to the matching candidate keys; full metadata is validated.

Counts: 10 pass, 0 review, 1 regression. Gate failed.

| Fixture | Before ns | After ns | Change | Result |
|---|---:|---:|---:|---|
| damage_dense | 660.135 | 762.083 | +15.44% | pass |
| damage_digit_width_with_ranges | 6272.994 | 5654.240 | -9.86% | pass |
| damage_disjoint_full_fallback_with_ranges | 6319.656 | 6943.100 | +9.87% | pass |
| damage_disjoint_sparse_cells_with_ranges | 6256.230 | 5521.164 | -11.75% | pass |
| damage_first_byte | 6296.698 | 5380.359 | -14.55% | pass |
| damage_last_byte | 6602.677 | 5667.039 | -14.17% | pass |
| damage_last_byte_each_band | 6482.984 | 10672.005 | +64.62% | regression |
| damage_one_cell | 6334.887 | 3467.618 | -45.26% | pass |
| damage_one_digit_with_ranges | 6409.320 | 5742.696 | -10.40% | pass |
| damage_popup_removal_with_ranges | 5385.526 | 4989.127 | -7.36% | pass |
| damage_stationary | 8182.434 | 4433.282 | -45.82% | pass |

## audit2_before to audit2_fill_candidate

Both full captures have identical measurement coverage.

Counts: 74 pass, 1 review, 0 regression. Gate failed.

| Fixture | Before ns | After ns | Change | Result |
|---|---:|---:|---:|---|
| acquisition_batch_and_snapshot | 1057.157 | 1014.411 | -4.04% | pass |
| acquisition_typed_sample_and_snapshot | 481.213 | 527.883 | +9.70% | pass |
| acquisition_watch_copy_roundtrip | 206.627 | 209.300 | +1.29% | pass |
| alarms_eight_channel_three_samples | 474.622 | 437.083 | -7.91% | pass |
| boost_cell_psi_peak | 12317.695 | 11140.677 | -9.56% | pass |
| cell_render_battery | 25880.877 | 13344.033 | -48.44% | pass |
| cell_render_boost | 20464.593 | 10037.483 | -50.95% | pass |
| cell_render_lambda | 37861.629 | 19777.237 | -47.76% | pass |
| cell_render_temperature | 30209.817 | 15772.104 | -47.79% | pass |
| client_isotp_128_bytes | 1306.708 | 1596.354 | +22.17% | review |
| client_single_frame_obd | 129.967 | 140.268 | +7.93% | pass |
| client_terminal_malformed | 103.807 | 53.401 | -48.56% | pass |
| client_terminal_negative | 125.129 | 72.556 | -42.01% | pass |
| client_terminal_pendingthennegative | 192.082 | 116.841 | -39.17% | pass |
| client_terminal_timeout | 59.909 | 34.866 | -41.80% | pass |
| clipped_contiguous/left_8_px | 5369.452 | 5350.960 | -0.34% | pass |
| damage_dense | 660.135 | 652.563 | -1.15% | pass |
| damage_digit_width_with_ranges | 6272.994 | 3230.097 | -48.51% | pass |
| damage_disjoint_full_fallback_with_ranges | 6319.656 | 3299.022 | -47.80% | pass |
| damage_disjoint_sparse_cells_with_ranges | 6256.230 | 3289.110 | -47.43% | pass |
| damage_first_byte | 6296.698 | 6526.432 | +3.65% | pass |
| damage_last_byte | 6602.677 | 6428.370 | -2.64% | pass |
| damage_last_byte_each_band | 6482.984 | 6504.356 | +0.33% | pass |
| damage_one_cell | 6334.887 | 5953.662 | -6.02% | pass |
| damage_one_digit_with_ranges | 6409.320 | 3109.871 | -51.48% | pass |
| damage_popup_removal_with_ranges | 5385.526 | 2839.336 | -47.28% | pass |
| damage_stationary | 8182.434 | 6771.518 | -17.24% | pass |
| fill_contiguous/clipped_four_edges | 169769.173 | 82244.058 | -51.56% | pass |
| fill_contiguous/full | 168196.849 | 81852.355 | -51.34% | pass |
| fill_contiguous/narrow_1px | 987.659 | 550.866 | -44.23% | pass |
| fill_contiguous/odd_x_width | 22403.992 | 11427.448 | -48.99% | pass |
| fill_solid/clipped_four_edges | 23234.634 | 2237.005 | -90.37% | pass |
| fill_solid/full | 23180.561 | 2159.915 | -90.68% | pass |
| fill_solid/narrow_1px | 163.146 | 69.081 | -57.66% | pass |
| fill_solid/odd_x_width | 3205.304 | 403.758 | -87.40% | pass |
| frame_window_record_all_metrics | 89.079 | 90.753 | +1.88% | pass |
| frame_window_summarize_1500_frames | 2123.467 | 2262.708 | +6.56% | pass |
| framebuffer/clear_full | 2050.553 | 1994.230 | -2.75% | pass |
| input_coherent_mailbox_roundtrip | 54.443 | 56.978 | +4.66% | pass |
| input_legacy_atomic_roundtrip | 21.523 | 24.657 | +14.56% | pass |
| logging_full_page | 105761.027 | 104319.065 | -1.36% | pass |
| logging_insert_and_snapshot | 268.879 | 278.054 | +3.41% | pass |
| mini_graph_60_samples_constant | 3013.562 | 3165.625 | +5.05% | pass |
| mini_graph_60_samples_newest_impulse | 3783.655 | 3850.722 | +1.77% | pass |
| protocol_isotp_128_bytes | 719.895 | 753.118 | +4.62% | pass |
| protocol_rx12_record | 72.596 | 72.286 | -0.43% | pass |
| protocol_single_frame_obd | 57.394 | 53.731 | -6.38% | pass |
| replay_demo_model_to_full_eight_frames | 2316764.423 | 1287656.061 | -44.42% | pass |
| replay_demo_model_to_retained_eight_frames | 2319457.708 | 1269912.816 | -45.25% | pass |
| replay_slow_bursty_session_to_full_seventeen_frames | 1470754.269 | 1394316.250 | -5.20% | pass |
| replay_slow_bursty_session_to_retained_seventeen_frames | 453275.955 | 428855.591 | -5.39% | pass |
| scene_full_all_changed_eight_frames | 2251670.379 | 1259184.514 | -44.08% | pass |
| scene_full_dashboard | 252467.969 | 225446.930 | -10.70% | pass |
| scene_full_one_cell | 248640.317 | 222467.544 | -10.53% | pass |
| scene_retained_all_changed_eight_frames | 2378300.000 | 1268650.146 | -46.66% | pass |
| scene_retained_one_cell | 25640.511 | 21217.884 | -17.25% | pass |
| scene_retained_stationary | 4150.877 | 3926.321 | -5.41% | pass |
| scene_transition_cold_cache_one_frame | 261438.170 | 139934.505 | -46.48% | pass |
| scene_transition_digit_width_one_frame | 40592.220 | 22346.579 | -44.95% | pass |
| scene_transition_quality_transitions_three_frames | 47826.584 | 25346.771 | -47.00% | pass |
| scene_transition_same_rounded_text_one_frame | 4051.029 | 2600.948 | -35.80% | pass |
| scene_transition_unit_toggle_one_frame | 24044.345 | 14015.771 | -41.71% | pass |
| sensor_state_next_measurement_update | 6.378 | 6.018 | -5.64% | pass |
| sensor_state_no_new_sample_tick_snapshot | 55.605 | 60.720 | +9.20% | pass |
| session_coolant_transaction_and_snapshot | 921.024 | 819.080 | -11.07% | pass |
| session_diagnostic_distribution_export | 645.931 | 710.719 | +10.03% | pass |
| settings_load_full_journal | 24980.434 | 23814.568 | -4.67% | pass |
| settings_maintenance_transaction_model | 62.919 | 64.458 | +2.45% | pass |
| settings_rollover_transaction_model | 29013.932 | 31598.968 | +8.91% | pass |
| target_kernel_clear_eight_iterations | 31934.679 | 32704.615 | +2.41% | pass |
| target_kernel_clipped_eight_iterations | 4487.298 | 4708.671 | +4.93% | pass |
| target_kernel_graphwidget_eight_iterations | 158949.966 | 124013.743 | -21.98% | pass |
| target_kernel_history_eight_iterations | 9072.951 | 9303.831 | +2.54% | pass |
| uart_rx12_ring_copy | 20.815 | 17.889 | -14.06% | pass |
| vehicle_coolant_io_session_snapshot | 979.904 | 960.326 | -2.00% | pass |

## audit2_pinned_before to audit2_pinned_after

Both full captures have identical measurement coverage.

Counts: 49 pass, 7 review, 19 regression. Gate failed.

| Fixture | Before ns | After ns | Change | Result |
|---|---:|---:|---:|---|
| acquisition_batch_and_snapshot | 1168.261 | 647.299 | -44.59% | pass |
| acquisition_typed_sample_and_snapshot | 431.816 | 285.012 | -34.00% | pass |
| acquisition_watch_copy_roundtrip | 165.204 | 86.251 | -47.79% | pass |
| alarms_eight_channel_three_samples | 225.377 | 428.378 | +90.07% | regression |
| boost_cell_psi_peak | 11315.569 | 6451.568 | -42.99% | pass |
| cell_render_battery | 15789.265 | 16687.990 | +5.69% | pass |
| cell_render_boost | 12347.445 | 13470.066 | +9.09% | pass |
| cell_render_lambda | 22154.255 | 26744.120 | +20.72% | review |
| cell_render_temperature | 17452.158 | 18002.544 | +3.15% | pass |
| client_isotp_128_bytes | 678.645 | 845.451 | +24.58% | review |
| client_single_frame_obd | 80.839 | 74.113 | -8.32% | pass |
| client_terminal_malformed | 57.333 | 70.046 | +22.17% | review |
| client_terminal_negative | 77.058 | 99.953 | +29.71% | review |
| client_terminal_pendingthennegative | 112.346 | 155.046 | +38.01% | regression |
| client_terminal_timeout | 33.762 | 47.500 | +40.69% | regression |
| clipped_contiguous/left_8_px | 9285.331 | 6554.837 | -29.41% | pass |
| damage_dense | 637.307 | 289.529 | -54.57% | pass |
| damage_digit_width_with_ranges | 3078.900 | 4000.050 | +29.92% | regression |
| damage_disjoint_full_fallback_with_ranges | 3191.581 | 4007.222 | +25.56% | regression |
| damage_disjoint_sparse_cells_with_ranges | 3405.602 | 4832.678 | +41.90% | regression |
| damage_first_byte | 4658.778 | 3126.894 | -32.88% | pass |
| damage_last_byte | 3229.681 | 3219.931 | -0.30% | pass |
| damage_last_byte_each_band | 3323.468 | 3186.760 | -4.11% | pass |
| damage_one_cell | 3383.600 | 3197.367 | -5.50% | pass |
| damage_one_digit_with_ranges | 3209.529 | 4012.117 | +25.01% | regression |
| damage_popup_removal_with_ranges | 2824.475 | 3605.590 | +27.66% | regression |
| damage_stationary | 5911.640 | 3202.821 | -45.82% | pass |
| fill_contiguous/clipped_four_edges | 85427.179 | 111254.106 | +30.23% | regression |
| fill_contiguous/full | 122068.847 | 114111.978 | -6.52% | pass |
| fill_contiguous/narrow_1px | 579.994 | 775.529 | +33.71% | regression |
| fill_contiguous/odd_x_width | 11698.769 | 15848.020 | +35.47% | regression |
| fill_solid/clipped_four_edges | 14905.935 | 3236.132 | -78.29% | pass |
| fill_solid/full | 15008.589 | 2969.775 | -80.21% | pass |
| fill_solid/narrow_1px | 103.734 | 97.761 | -5.76% | pass |
| fill_solid/odd_x_width | 2895.739 | 555.568 | -80.81% | pass |
| frame_window_record_all_metrics | 46.937 | 53.776 | +14.57% | pass |
| frame_window_summarize_1500_frames | 1219.676 | 1584.031 | +29.87% | regression |
| framebuffer/clear_full | 2199.201 | 2705.078 | +23.00% | review |
| input_coherent_mailbox_roundtrip | 42.862 | 49.607 | +15.74% | pass |
| input_legacy_atomic_roundtrip | 17.084 | 21.867 | +28.00% | review |
| logging_full_page | 54922.393 | 77607.067 | +41.30% | regression |
| logging_insert_and_snapshot | 196.501 | 219.989 | +11.95% | pass |
| mini_graph_60_samples_constant | 2128.472 | 2407.645 | +13.12% | pass |
| mini_graph_60_samples_newest_impulse | 2727.700 | 2910.524 | +6.70% | pass |
| protocol_isotp_128_bytes | 411.510 | 398.002 | -3.28% | pass |
| protocol_rx12_record | 40.670 | 28.302 | -30.41% | pass |
| protocol_single_frame_obd | 46.630 | 39.528 | -15.23% | pass |
| replay_demo_model_to_full_eight_frames | 1881063.561 | 1766696.667 | -6.08% | pass |
| replay_demo_model_to_retained_eight_frames | 2003169.281 | 1765103.409 | -11.88% | pass |
| replay_slow_bursty_session_to_full_seventeen_frames | 1095280.114 | 1137224.561 | +3.83% | pass |
| replay_slow_bursty_session_to_retained_seventeen_frames | 317935.594 | 346962.582 | +9.13% | pass |
| scene_full_all_changed_eight_frames | 1579167.262 | 1799271.875 | +13.94% | pass |
| scene_full_dashboard | 162802.433 | 135500.485 | -16.77% | pass |
| scene_full_one_cell | 164227.239 | 135643.046 | -17.41% | pass |
| scene_retained_all_changed_eight_frames | 1601996.970 | 1735501.579 | +8.33% | pass |
| scene_retained_one_cell | 15571.078 | 12902.179 | -17.14% | pass |
| scene_retained_stationary | 2219.899 | 2124.586 | -4.29% | pass |
| scene_transition_cold_cache_one_frame | 158940.297 | 184790.163 | +16.26% | pass |
| scene_transition_digit_width_one_frame | 23617.871 | 30491.339 | +29.10% | regression |
| scene_transition_quality_transitions_three_frames | 33392.124 | 35778.951 | +7.15% | pass |
| scene_transition_same_rounded_text_one_frame | 2636.900 | 3395.742 | +28.78% | regression |
| scene_transition_unit_toggle_one_frame | 15246.264 | 18147.091 | +19.03% | pass |
| sensor_state_next_measurement_update | 6.197 | 4.160 | -32.87% | pass |
| sensor_state_no_new_sample_tick_snapshot | 32.879 | 44.397 | +35.03% | regression |
| session_coolant_transaction_and_snapshot | 631.273 | 586.942 | -7.02% | pass |
| session_diagnostic_distribution_export | 316.048 | 453.604 | +43.52% | regression |
| settings_load_full_journal | 22474.750 | 16400.508 | -27.03% | pass |
| settings_maintenance_transaction_model | 38.516 | 49.938 | +29.66% | regression |
| settings_rollover_transaction_model | 18541.626 | 25865.958 | +39.50% | regression |
| target_kernel_clear_eight_iterations | 30413.971 | 24542.720 | -19.30% | pass |
| target_kernel_clipped_eight_iterations | 3604.046 | 3408.857 | -5.42% | pass |
| target_kernel_graphwidget_eight_iterations | 186990.377 | 104495.321 | -44.12% | pass |
| target_kernel_history_eight_iterations | 7980.762 | 7814.585 | -2.08% | pass |
| uart_rx12_ring_copy | 13.395 | 10.791 | -19.44% | pass |
| vehicle_coolant_io_session_snapshot | 565.546 | 694.550 | +22.81% | review |

## audit2_pinned_after to audit2_pinned_repeat

Before measurement records are restricted in memory to the matching candidate keys; full metadata is validated.

Counts: 6 pass, 1 review, 1 regression. Gate failed.

| Fixture | Before ns | After ns | Change | Result |
|---|---:|---:|---:|---|
| alarms_eight_channel_three_samples | 428.378 | 329.410 | -23.10% | pass |
| client_isotp_128_bytes | 845.451 | 985.543 | +16.57% | pass |
| fill_solid/clipped_four_edges | 3236.132 | 3213.753 | -0.69% | pass |
| fill_solid/full | 2969.775 | 3333.408 | +12.24% | pass |
| fill_solid/narrow_1px | 97.761 | 98.395 | +0.65% | pass |
| fill_solid/odd_x_width | 555.568 | 629.683 | +13.34% | pass |
| framebuffer/clear_full | 2705.078 | 3368.486 | +24.52% | review |
| scene_retained_one_cell | 12902.179 | 17736.947 | +37.47% | regression |
