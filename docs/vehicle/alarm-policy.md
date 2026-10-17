# Alarm policy and presentation

Alarm decisions belong to the acquisition owner. Every accepted measurement and every recorded fault reaches the shared engine before latest-value Watch publication. The display can suppress an expired alarm; it cannot accumulate dwell or reconstruct skipped samples. Resetting statistics does not reset an active alarm.

## Enabled policies

The default Vehicle configuration enables **no qualified alarm rules**. Configured
VehicleAndDemo applicability admits a rule only when its actual source, quality,
freshness and operating context qualify. A plausible temperature or generic OBD
response does not establish a DNUC limit. Exact source/channel mapping and
threshold provenance remain required; the current Longan deployment is unqualified.
Engine oil and transmission oil are separate channels. Unknown, modeled or catalyst
EGT cannot produce a measured-manifold alarm.

Configured Demo rules use each widget's validated thresholds and synthetic
running/warm/high-load/non-fuel-cut context with target lambda1.0. Defaults are
DemoOnly UI fixtures, not driving recommendations, and retain separate oil/DSG
limits. `alarms::demo_rules` belongs to the explicit historical host adapter.
Runtime Demo generates canonical lambda without assuming fuel-dependent AFR.

## Timing and failure behavior

- Thresholds must be finite with separate activation/clear points; dwell is1..30000ms. Rules must match their slot and source and carry a nonempty reference.
- Contextual voltage requires an explicit running signal. Lambda requires running, warm, sufficient load, no fuel cut and a finite target; enrichment matching the target is normal.
- Context must match the measurement's source and sequence, be no older than500ms and be within250ms of its receipt time. These are provisional software bounds, not proven ECU rates.
- Dwell advances only on new measurements. Activation and clearing use hysteresis and separate dwell. A context or measurement gap breaks continuity. Joint dwell starts at the later first timestamp and ends at the earlier latest timestamp.
- Invalid/missing context, source mismatch, quality fault, stale values, regressed timestamps or clock failure clear the active decision to Unavailable. Reusing a rejected sample cannot restart or backdate dwell. Sequence gaps alone cannot prove missing data because diagnostic request generations span channels; single-owner evaluation provides the observation contract.
- The Demo batch helper retains its existing NaN omission convention for sparse fixtures. Omission is not a measured invalid value. Typed invalid samples and explicit faults invalidate the channel. Omitted data expires at its original timestamp; it is never refreshed by a different channel.
- Published alert deadlines preserve real time. A stopped producer or delayed display cannot keep an expired contextual alarm active. Unavailable is distinct from Normal and Disabled; current cells show their sample quality, while no unqualified alarm is presented as active.

## Rendering

Current warnings support High/Low and direct Inside/Outside window hysteresis,
priority paging, latch and snooze. Acknowledgment and qualified clear are distinct:
an active acknowledged condition remains indicated, and unavailable data never
clears a remembered episode. Raw warning eligibility does not provide a fresh
number. See [the complete warning presentation contract](../configurator/advanced-controls.md).

Widgets receive the evaluated state and no longer contain guessed coolant, shared oil/DSG, EGT or voltage thresholds. Old peak values do not imply a current contextual alarm. Warnings appear only on Dashboard. Brightness, unit, FPS and reset feedback takes precedence until its wall-clock deadline (1.5 seconds for Full/Dim or Debug brightness, 3 seconds for unit/FPS/reset), then any active warning resumes its current blink phase. Dashboard Off feedback lasts two seconds from its first completed presentation; after the popup-removal frame completes, the backlight switches off. Page changes or replacement feedback cancel a pending dashboard shutdown. Debug permits its percentage brightness feedback; Logs stays unobstructed. Page changes clear transient feedback without clearing alarm state. The RGB policy defaults to completed-warning red/off and otherwise green; configured normal/warning patterns and background switches follow the shared completion-aware policy. Green is a presentation state, not engine-health evidence. Current AFR derives from the same prepared canonical lambda with the declared fuel stoichiometry. Raw alarm inputs remain unsmoothed lambda. Eligible warnings sort Critical/High/Normal/Low then configured view/widget order; this user-selected priority is not calibrated severity. Explicit backlight off remains off.

The retained cache uses the effective overlay, including sensor and blink state. Removing or replacing a warning restores the entire underlay on each framebuffer. Host tests render both buffers and compare with complete redraws.

## Verification limits

Table-driven host tests cover cold/warm and unknown context, enrichment, fuel cut, load and target bounds, source/sequence/time mismatch, voltage/EGT context, chatter, exact dwell/freshness boundaries, stale gaps, rejected-sample recovery, Watch coalescing and overlay replacement. These verify shared production logic and pixel output. They do not validate ECU support, physical warning latency, watchdog behavior or panel FPS.
