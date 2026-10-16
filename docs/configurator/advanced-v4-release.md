# Complete advanced v4 configuration

This historical v4 release delivered approved stages 2–7 in firmware and Studio: configurable
RGB patterns, per-widget units, current-value smoothing and reduced motion,
channel-shared history duration, peak/reference/band markers and advanced warnings.
Existing controls, eight views, 64 independent widgets, six styles and the
320×240/26-pixel-header/4×2 grid remain the contract. Organized editor workspaces,
full Debug/Logs browser pixels, JSON Apply/diff/resource tools and native Build &
Flash were delivered in later stages, documented in [Studio](studio.md).
The measurements below describe this release's frozen inputs only.

## Defaults and behavior

- Smoothing is Off; reduced motion is false. Enabled smoothing affects the current
  number and gauge, including AFR from the same prepared canonical lambda.
  Reduced motion snaps current values and decorative breathing. Raw warnings,
  history, extrema and peak acquisition remain unsmoothed.
- Every channel retains sixty timed history buckets, default 120 seconds,
  configurable from 10–900 seconds. Missing observations remain gaps.
- Peak/reference markers and bands default Off/empty. Gauge peak hold is
  100–60000 ms (default 750), with independent Maximum/Minimum choices.
  References use canonical Zero or an explicit Manual value. At most three
  finite, sorted, nonoverlapping bands are stored; hidden settings stay validated.
- Boost defaults to FollowBoost; fixed bar/PSI/kPa widgets ignore the toggle.
  Equal secondaries are omitted. Temperature C/F and lambda/estimated AFR/both
  convert only at presentation. Default equivalent-gasoline stoichiometry is 14.7;
  E5 does not establish actual composition or ECU target AFR.
- Warning defaults retain DemoOnly synthetic thresholds, Normal priority,
  no latch and no snooze. Window rules use qualified measurements and dwell.
  Latch requires acknowledgment plus qualified clear; stale data never clears
  an episode or restores a live value. Active acknowledged and snoozed states
  retain steady widget indications. Priority/new episodes can interrupt attention.
  An enabled latch/snooze requires onboard Dashboard AcknowledgeWarnings.
  Commands identify the last completed warning page and await a producer result.
- RGB defaults are Solid green normal and synchronized red Blink warnings,
  100% brightness, 400 ms/50% timing. Background switches default false.
  Independent Off/Solid/Blink/Breathe patterns use completed presentation state,
  bounded elapsed-time arithmetic and independent RGB PWM owners.

See [controls](control-mapping-design.md), [advanced controls](advanced-controls.md)
and [RGB policy](led-policy-design.md) for bounds, capability rules and repair.
The protected two-second Y recovery always returns to Dashboard at Full brightness.
Statistics reset and warning acknowledgment remain independent.

## Files, migration and startup

Ordinary JSON export is compact and strict; EAV uses the canonical v4 Postcard
payload in the unchanged EAV1/12-byte CRC envelope. Limits remain 16 KiB EAV and
128 KiB standalone JSON or saved-editor envelope. The saved envelope's own
editor_version remains 1; it is separate from dashboard schema version 4.
Frozen v1, v2 and v3 JSON/binary imports explicitly migrate to current defaults.
The CLI default/compile/inspect and editor save/export use the same validator
and codec. Firmware accepts only current v4 envelopes and falls back atomically
to the complete reviewed default, preserving the fault.

Checked-in default JSON/EAV are verified without regeneration. The default EAV
is 1157 bytes, SHA256 `bda31315cbab2c63d4672c925126dfb5d1262d64e97fac95a1defd13617480f1`.
The selected advanced maximum fixture is 13362 bytes, SHA256
`4f63015b3b853c225156eb2fe2dcd85aac8d50ed0497ae141ed0eeb70551b2f2`.
It is distinct from the 14007-byte structural/string stress fixture; neither
fixture name alone establishes every maximum. See [format](configuration-format.md).

## Software resource evidence

The admitted target layout is Config 14576, ConfigSlot 14600, acquisition owner 16056
and DashboardSnapshot 4144 bytes. Tests retain 4160/16384 snapshot/owner guards.
The complete ELF admission retains minimum Core 0 headroom 65536 bytes and
Core 0/Core 1 reserves 8192/4096 bytes; a larger type guard alone grants no memory.
Historical cache7, renderer-stage and original v3 figures remain historical.

Final ordinary release captures use nightly-2026-10-12, rustc 080cc2714/LLVM 23.1.3,
CPU 280/SPI 70 with frame/TE traces and normal defaults/features:

| Capture | Static RAM | Core 0 headroom | Boot | Core 1 construct/runtime | Dashboard |
| --- | ---: | ---: | ---: | ---: | ---: |
| advanced-release-task12-demo280-approved-traced, default |448312|75976|2824|12024 / 25088|27784|
| advanced-release-task12-vehicle280-approved-traced, advancedMax |458296|65992|2816|17464 / 17312|28816|

All values are bytes. Maximum Vehicle has 456 bytes above the unchanged minimum.
Demo ELF SHA256 is `59ffe68e73c6a8f87beccf5b2e2e2063eb5f6f0f81ddfb29f9613d882e068e20`;
Vehicle ELF is `6028f668a5f77bd797d1ab1938708959ff7ad0bcfa1f4e50f4d100b102d153e6`.
Source ZIP SHA256 values are respectively
`dfc29d636343f54ff320c047e6595bc1db4194488eef13e79314861ab84b2cf1` and
`a95b8032485ca9aa08e0586dd23f5f01d723dc2cee43bd203b012aa020e1325b`.
Each archive reconciles 233 source inputs plus its exact embedded configuration.
Artifacts are retained locally under `firmware/target/bench-captures/<name>`;
the hashes identify the measured sources even before the release commit is made.
Both pass linked RAM, revised named-path stack, reviewed flash-lockout/startup/
IRQ-vector checks and normal/optimized corrupted-artifact rejection tests.
The first restricted Demo attempt failed with linker permission denial and is
retained separately; it is not resource-admission evidence.

Release integration passes native shared/config/host/editor suites, current/legacy
codec and boot/fallback regressions, exact example pixel replay in both outlines,
Wasm checking, all four formatting roots and strict Clippy. Dependency deprecation/
future-incompatibility diagnostics remain visible and belong to maintenance.
No temporary config-size measurement cfg, alternative serializer or export gate
is part of the release.

## Qualification limits

The diagnostic/Longan deployment remains unqualified for the user's DNUC ECU.
Catalogue selection does not supply an absent MAP/wideband/manufacturer decoder,
qualify ECU scaling/context, or enable new commands. Demo is synthetic.
Linked stack checks cover named paths and manual call structure, not whole-program
high-water/IRQ overlap. Host timing is not Pico FPS. Physical stack high-water,
carrier/color/optical LED behavior, panel scanout, electrical stability, sustained
overclock operation and ECU support require device validation. No device was
flashed or contacted while integrating this release.
