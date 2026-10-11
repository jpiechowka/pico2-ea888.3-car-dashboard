# Alarm policy and presentation

Alarm decisions belong to the acquisition owner. Every accepted measurement and every recorded fault reaches the shared engine before latest-value Watch publication. The display can suppress an expired alarm; it cannot accumulate dwell or reconstruct skipped samples. Resetting statistics does not reset an active alarm.

## Enabled policies

Vehicle mode currently enables **no alarm rules**. A plausible temperature or a valid generic OBD response is insufficient evidence for a DNUC limit. Future enablement requires an exact source/channel mapping, reviewed threshold provenance and operating context. Engine oil and transmission oil are separate channels. Unknown, modeled or catalyst EGT cannot produce a measured-manifold alarm.

Demo rules in `alarms::demo_rules` are synthetic UI fixtures, not driving recommendations. Each carries that provenance. The fixture synthesizes running/warm/high-load/non-fuel-cut context and target lambda1.0. It deliberately uses separate oil/DSG limits. The runtime demo generates lambda directly, without assuming a fuel-dependent AFR conversion.

## Timing and failure behavior

- Thresholds must be finite with separate activation/clear points; dwell is1..30000ms. Rules must match their slot and source and carry a nonempty reference.
- Contextual voltage requires an explicit running signal. Lambda requires running, warm, sufficient load, no fuel cut and a finite target; enrichment matching the target is normal.
- Context must match the measurement's source and sequence, be no older than500ms and be within250ms of its receipt time. These are provisional software bounds, not proven ECU rates.
- Dwell advances only on new measurements. Activation and clearing use hysteresis and separate dwell. A context or measurement gap breaks continuity. Joint dwell starts at the later first timestamp and ends at the earlier latest timestamp.
- Invalid/missing context, source mismatch, quality fault, stale values, regressed timestamps or clock failure clear the active decision to Unavailable. Reusing a rejected sample cannot restart or backdate dwell. Sequence gaps alone cannot prove missing data because diagnostic request generations span channels; single-owner evaluation provides the observation contract.
- The Demo batch helper retains its existing NaN omission convention for sparse fixtures. Omission is not a measured invalid value. Typed invalid samples and explicit faults invalidate the channel. Omitted data expires at its original timestamp; it is never refreshed by a different channel.
- Published alert deadlines preserve real time. A stopped producer or delayed display cannot keep an expired contextual alarm active. Unavailable is distinct from Normal and Disabled; current cells show their sample quality, while no unqualified alarm is presented as active.

## Rendering

Widgets receive the evaluated state and no longer contain guessed coolant, shared oil/DSG, EGT or voltage thresholds. Old peak values do not imply a current contextual alarm. Warnings appear only on Dashboard. Brightness, unit, FPS and reset feedback takes precedence until its wall-clock deadline (1.5 seconds for brightness, 3 seconds otherwise), then any active warning resumes its current blink phase. Debug permits brightness feedback; Logs stays unobstructed. Page changes clear transient feedback without clearing alarm state. Multiple active channels use deterministic order: coolant, oil, DSG, EGT, lambda, voltage, boost, IAT. This ordering is a UI policy, not a calibrated severity ranking. Explicit backlight off remains off.

The retained cache uses the effective overlay, including sensor and blink state. Removing or replacing a warning restores the entire underlay on each framebuffer. Host tests render both buffers and compare with complete redraws.

## Verification limits

Table-driven host tests cover cold/warm and unknown context, enrichment, fuel cut, load and target bounds, source/sequence/time mismatch, voltage/EGT context, chatter, exact dwell/freshness boundaries, stale gaps, rejected-sample recovery, Watch coalescing and overlay replacement. These verify shared production logic and pixel output. They do not validate ECU support, physical warning latency, watchdog behavior or panel FPS.
