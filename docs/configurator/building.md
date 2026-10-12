# Configuration files and firmware builds

The editable document is strict JSON. The runtime file uses the bounded `.eav`
format described in [configuration-format.md](configuration-format.md). Both
imports use the same Rust validator. Files cannot qualify an adapter, invent a
decoder, or bypass ECU capability discovery.

From the repository root in PowerShell:

```powershell
cargo run --locked -p dashboard-config-tools -- default my-dashboard.json
cargo run --locked -p dashboard-config-tools -- compile my-dashboard.json my-dashboard.eav
cargo run --locked -p dashboard-config-tools -- inspect my-dashboard.eav
```

Compiler errors leave an existing output unchanged. Keep the JSON for editing;
the firmware build accepts a binary `.eav` file. The reviewed default is also
available in `configs/default.json` and `configs/default.eav`.

## Reproducible PowerShell workflow

```powershell
.\firmware\scripts\Build-Dashboard.ps1 -Name custom-demo-01 -Mode Demo -Profile 280-70 -Config .\my-dashboard.eav
```

Each name must be new. Outputs go into
`firmware/target/bench-captures/<name>/`: `pico2.elf`, `dashboard.eav`,
`manifest.json`, and `source.zip`. The manifest identifies the compiler, features,
source files, selected configuration and ELF with SHA-256 hashes. The capture
builds against a frozen copy, verifies the build script's embedded bytes, and
rejects source or selected-file changes during the build. It also runs the
linked RAM, flash helper and corrupted-artifact checks.

Available profiles are `stock` (default) and `280-70`. `-Mode Vehicle` selects the
existing **unqualified** adapter path. Optional `-SerialDisplay` and `-FullOutline`
retain their existing diagnostic meanings. The script builds and verifies files;
it does not flash or reboot a device. The older scripts under ignored `target`
directories remain frozen records of earlier builds.

280/70 is the user's observed working overclock on this device. It is outside
the manufacturer's stock operating specification. Do not use 290/72.5 on this
device; the user observed display corruption. See the existing
[hardware validation runbook](../validation/pico2-dnuc-runbook.md) for flashing
and physical checks. No new configuration firmware has been physically validated.

## Configuration-specific device checks

These checks require the physical Pico and display. They have not been executed
for this configurable build. Follow the power/wiring and flashing procedure in
the linked runbook, then use a **Demo** image for layout and controls:

1. Export a test document with a compact view, a pair of large widgets and a
   wide-widget view. Include the longest accepted labels and a lambda/AFR cell.
   Build with the wrapper above and retain the capture's `.eav`, ELF and manifest.
2. After flashing, verify Full brightness and the configured boot-view name.
   Rotate through every view in both directions, including wraparound. Check
   borders, empty cells, text, graphs and popup removal for stale pixels.
3. Click Full → Dim (50% PWM) → Off. Confirm the Off popup remains visible for
   two seconds before disappearing and the backlight switching off. The next
   click returns to Full. Repeat with a click or page change during the delay.
4. Visit Debug and Logs and confirm their encoder controls are preserved.
   Return to Dashboard and confirm the last view is restored. Reboot and confirm
   the configured boot view is restored instead. Statistics reset must preserve
   the view configuration; bar/PSI must update every boost presentation.
5. In a Demo-only test configuration, enable multiple warnings with deliberately
   reachable illustrative thresholds. Check CycleOne and ListThree, all-view and
   current-view scope, message clipping, and elapsed-time page rotation. Exercise
   FPS/unit/reset feedback during a warning: feedback wins, then warnings resume.
   Debug/Logs must stay clear of warning popups. Verify red warning blinking and
   continuous green without a visible warning.
6. Compare configured AFR against lambda times the declared stoichiometry.
   E5 alone is not an exact ethanol measurement. Confirm disabling AFR removes
   its secondary row, and stale/unavailable lambda hides derived values.
7. Record the exact ELF hash, profile, configuration, controls and observations.
   Software transfer FPS, panel scanout, input latency and electrical/thermal
   stability are different measurements. Perform the runbook's physical timing
   and extended soak checks before treating the configuration as qualified.

Vehicle validation is a separate step. Candidate source selection does not
enable an unimplemented decoder or qualify the Longan adapter/ECU. Obtain ECU
identity and timestamped captures before establishing DNUC support; use the
runbook's parked-vehicle procedure. Do not substitute demo warnings or catalogue
documentation for vehicle evidence.

## Direct Cargo selection

`DASHBOARD_CONFIG` selects a binary file. Relative paths are resolved against
`firmware/pico2`; prefer an absolute path. An unset variable selects the reviewed
default. An explicitly empty variable, missing file, invalid payload, bad checksum
or unsupported schema causes the build to fail. Both the environment selector
and selected file are Cargo rebuild dependencies. Build scripts copy the exact
validated bytes into `OUT_DIR` and the firmware includes that copy in flash.

```powershell
$previousConfig = $env:DASHBOARD_CONFIG
$previousStrip = $env:CARGO_PROFILE_RELEASE_STRIP
$selectedConfig = (Resolve-Path -LiteralPath .\my-dashboard.eav).Path
Push-Location -LiteralPath .\firmware
try {
    $env:DASHBOARD_CONFIG = $selectedConfig
    $env:CARGO_PROFILE_RELEASE_STRIP = 'none'
    cargo build --locked -p dashboard-pico2 --target thumbv8m.main-none-eabihf --release --features demo,cpu280-spi70-1v30,frame-trace,te-trace
    if ($LASTEXITCODE -ne 0) { throw 'Firmware build failed' }
} finally {
    $env:DASHBOARD_CONFIG = $previousConfig
    $env:CARGO_PROFILE_RELEASE_STRIP = $previousStrip
    Pop-Location
}
```

Use the PowerShell wrapper for a captured, gated image. The direct example enters
`firmware` so Cargo uses its checked-in linker flags, and restores the environment
and working directory afterward.

## Startup and ownership

Core 0 parses once into a statically allocated `BootConfig` before spawning core
1. Only immutable references reach the acquisition owner, diagnostic session and
renderer. Runtime corruption falls back atomically to the complete reviewed
default and retains a configuration fault. No partial fields are applied.
Statistics resets, reconnects and settings maintenance do not replace the
configuration. The settings journal format is unchanged.

Selected source routes, poll periods and tighter freshness limits apply before
sample publication and alarm expiry. Unsupported or unresolved catalogue entries
remain unavailable. Eight channel histories remain shared across all widgets.

The runtime crate is `no_std` and allocation-free. JSON and file I/O are host-only
build dependencies; Cargo's resolver separates their features from the ARM crate.
The linked image must leave at least 64 KiB for core 0 stack space. This is a
software acceptance bound; it is not a measured hardware stack high-water mark.

Relevant upstream contracts: [StaticCell initialization](https://docs.rs/static_cell/2.1.1/static_cell/struct.StaticCell.html),
[Embassy multicore](https://docs.embassy.dev/embassy-rp/git/rp235xa/multicore/fn.spawn_core1.html),
and [RP2350 specifications](https://www.raspberrypi.com/products/rp2350/).
The audit also checks the exact installed/vendored source and linked instructions;
the `init_with` API alone does not prove that LLVM eliminated stack temporaries.

### Embedding-stage measurements

Pinned nightly-2026-10-11 release artifacts, before the variable-view renderer:

| Image | Static RAM | Core 0 headroom |
|---|---:|---:|
| Stock, default config | 388,032 B | 136,256 B |
| Serial/full-frame, default config | 388,032 B | 136,256 B |
| Demo 280/70 with traces, default or maximum config | 435,504 B | 88,784 B |

The default file is 466 B. A capacity fixture with 8 views, 64 widgets, maximum
text lengths, fixed graph ranges and a volume-blend model is 5,287 B. Linked
disassembly gives a conservative non-panic boot call-chain sum of 48,072 B for
the default and 48,200 B for that fixture, before interrupt/startup residuals.
`init_with` still produces a stack temporary and copy in these artifacts.
Recheck the linked call chain when later renderer or task-frame changes grow the
stack. These numbers do not measure a physical stack peak.

Actual Cargo rebuild checks cover default selection, explicit default equality,
missing/empty/corrupt rejection, replacement of the selected file, and returning
to default after unsetting the selector. The file helper and CLI tests cover
bounded reads, same-byte snapshot copying, fingerprints and failed-import output
preservation. Host owner tests cover tighter freshness and dwell gaps, configured
poll/retry intervals, transport epochs, maintenance, reset and unqualified fallback.
