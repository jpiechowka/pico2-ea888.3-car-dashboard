# Configuration files and firmware builds

Complete Studio projects use `.eavp` to keep dashboard views, startup settings
and artwork together. Dashboard-only editing also supports strict JSON and the
bounded `.eav` format described in [configuration-format.md](configuration-format.md).
All dashboard imports use the same Rust validator. Files cannot qualify an
adapter, invent a decoder, or bypass ECU capability discovery.

From the repository root in PowerShell:

```powershell
cargo run --locked -p dashboard-config-tools -- default my-dashboard.json
cargo run --locked -p dashboard-config-tools -- compile my-dashboard.json my-dashboard.eav
cargo run --locked -p dashboard-config-tools -- inspect my-dashboard.eav
```

Compiler errors leave an existing output unchanged. Keep the JSON for editing;
the firmware build accepts a binary `.eav` file. The reviewed default is also
available in `configs/default.json` and `configs/default.eav`.

The editor supports numeric/history, circular, needle, horizontal, vertical and
segmented gauges, with independent RGB565 palettes. See [gauges and colours](gauges-and-colours.md)
for layout capabilities, automatic scale expansion and colour validation.
Historical v1, v2, v3 and v4 files are explicitly upgraded by the editor or build
compiler; exported and embedded files use [current v5](configuration-format.md#current-v5-contract). The capture keeps both
the original selected file and the canonical upgraded bytes.

## Build & Flash workspace

The **Computer & devices** panel connects explicitly to the foreground helper
started with `.\configurator\Studio.ps1 -Helper`. Serve Studio on
`http://127.0.0.1:8080` or `http://localhost:8080`; other origins are rejected.
The helper listens only on `127.0.0.1:8765`. Connection credentials stay in browser
memory and are excluded from URLs, exported logs and saved projects.

**Check prerequisites** and **Find BOOTSEL devices** start real native jobs with
stage status, bounded timestamped logs, cancellation and log download. Reconnect
after a page reload to resume a running job; reloading does not cancel it.
Cancellation completes only after native cleanup. A descriptor scan cannot prove
flash capacity, exact board identity or readiness to flash. No programming command
is issued by connection, tool checks or descriptor discovery.

Choose Demo or Vehicle, one of the eight shared clock profiles, or exact custom
CPU/SPI frequencies and a supported voltage setpoint. Expert mode is explicit
and resets on reload. The workspace shows requested and resolved clocks, SPI
divisors and neighboring rates, repository toolchain, compiler target and exact
normalized options. Installed tools are reported separately by the native check.

The transfer estimate covers a full 153,600-byte RGB565 payload only. It excludes
rendering, synchronization, command overhead and panel scanout, so it is not
physical FPS. A smaller SPI request can select a larger divisor and substantially
slow transfers; equal SPI rates still leave different CPU rendering budgets.

**Download build recipe** creates a PowerShell 7 script containing the current
accepted dashboard, startup profile, converted artwork and build options. Invalid/pending edits or
an active drag block export. Run the downloaded file with `-Repository` pointing
to this checkout. It checks each embedded input's SHA-256 and creates fresh
`.eav`/`.easp` inputs and optional `.easa` artwork under `target/studio-exports`.
It checks the pinned toolchain and invokes the build wrapper below. Original
PNG/GIF artwork stays out of firmware. Existing capture names are never overwritten. The recipe builds and
validates; it does not flash a device. Re-export after changing the toolchain.

## Reproducible PowerShell workflow

The wrapper's `-Config` parameter takes dashboard-only `.eav`. Use the exported
build recipe or Studio's native build for a complete project with custom startup.

```powershell
.\firmware\scripts\Build-Dashboard.ps1 -Name custom-demo-01 -Mode Demo -Profile 280-70 -Config .\my-dashboard.eav
```

Each name must be new. Outputs go into
`firmware/target/bench-captures/<name>/`: `pico2.elf`, `dashboard.eav`,
`manifest.json`, and `source.zip`. The manifest identifies the compiler, features,
source files, selected configuration and ELF with SHA-256 hashes. The capture
builds against the working checkout, archives the source bytes read before the
build, verifies the build script's embedded bytes, and rejects observed source
or selected-file changes afterward. The selected configuration is frozen before
compilation. This is change detection, not an isolated source-tree build; avoid
concurrent source edits. It also runs the
linked RAM, cumulative stack-frame, flash helper and corrupted-artifact checks.

Profiles come from the shared Rust catalogue: `stock` (default), `132-66`,
`140-70`, `150-15`, `250-62.5`, `280-70`, `290-72.5` and `300-75`.
Stock requests 150/62.5 MHz and resolves 150/37.5 MHz; legacy `290-72.5` requests
290/72.5 MHz and resolves 290.4/48.4 MHz, retaining the user's failed observation.
The established explicit `-Profile 280-70` invocation retains its Expert intent.
All other experimental profiles and custom clocks require `-Expert`; Stock and
the 150-15 timing reference do not. This is build admission, not electrical qualification.
`-CpuHz`, `-SpiHz`, `-VselMv` accept an all-present strict decimal tuple and
conflict with an explicitly supplied Profile. For example:

```powershell
.\firmware\scripts\Build-Dashboard.ps1 -Name low-clock-01 -CpuHz 50000000 -SpiHz 25000000 -VselMv 1100 -Expert -Optimization 2
```

`-Optimization` accepts 3/2/s/z. Traces default true; `-FrameTrace $false` and
`-TeTrace $false` really omit their features. `-FullFrame`, `-SerialDisplay` and
`-FullOutline` select the existing display paths. Commands explicitly suppress
Cargo defaults and pass the complete shared effective feature set.
`-Mode Vehicle` selects the existing **unqualified** adapter path. The script builds and verifies files;
it does not flash or reboot a device. The older scripts under ignored `target`
directories remain frozen records of earlier builds.

Capture adds `build-options.json`, `clock_profile.rs`, `clock_profile.json` and
Cargo message evidence. It matches the firmware package's generated output to
the exact requested/resolved clocks, actual optimization, complete features and
source/config/compiler identity; absent or mismatched generation fails admission.
The original `capture_target_bench.py --features ...` route remains available;
experimental/custom selections there require the explicit local `--expert`
switch. `target-bench` is a separate finite capture mode, outside ordinary
BuildOptions. Ambient custom-clock and compiler/profile overrides are rejected.
Raw requests omitting both demo and longan-rx12 retain the old unavailable
acquisition path, recorded as `legacy_unavailable_backend=true`; the capture
never silently adds vehicle transport. Typed Vehicle requests always include
longan-rx12 and cannot use that compatibility adapter.

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
3. With default bindings, click Full → Dim (50% PWM) → Off. Confirm the Off popup remains visible for
   two seconds before disappearing and the backlight switching off. The next
   click returns to Full. Repeat with a click or page change during the delay.
4. Visit Debug and Logs and exercise the exact configured encoder/onboard actions.
   Return to Dashboard and confirm the last view is restored. Reboot and confirm
   the configured boot view is restored instead. Statistics reset must preserve
   the view configuration. Test FollowBoost and fixed bar/PSI/kPa widgets together:
   the unit toggle changes only FollowBoost, and an equal secondary unit is omitted.
   Exercise tap/hold suppression and the fixed two-second Y recovery from Off.
5. In a Demo-only test configuration, enable multiple warnings with deliberately
   reachable illustrative thresholds. Check CycleOne and ListThree, all-view and
   current-view scope, message clipping, and elapsed-time page rotation. Exercise
   FPS/unit/reset feedback during a warning: feedback wins, then warnings resume.
   Debug/Logs must stay clear of warning popups. Exercise window hysteresis,
   priority paging, latch/qualified clear, completed-page acknowledgment and snooze
   expiry, including stale data and a new episode. Verify default red warning blink
   and green normal indication, then the selected RGB/background/reduced-motion
   policy. A queued frame must not change LED or acknowledgment identity.
6. Compare configured current AFR against the same prepared canonical lambda
   times the declared stoichiometry, including enabled smoothing and reduced
   motion. Raw warnings/history/extrema/peaks remain unsmoothed.
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
## Native prerequisite checks and explicit setup

Run from the repository root in PowerShell 7:

```powershell
.\configurator\Studio.ps1 -Check
.\configurator\Studio.ps1 -Check -Json
.\configurator\Studio.ps1 -Setup
```

`-Check` is read-only: it never compiles the checker, installs or downloads tools,
starts a build, or enumerates USB. If the checker is absent it exits 4 with an
explicit setup instruction. `-Setup` explicitly installs the repository's dated
Rust nightly, ARM/Wasm targets and LLVM component, installs Trunk 0.21.14 and
wasm-bindgen CLI 0.2.129 into `target/configurator-tools/bin`, compiles the checker
into `target/studio-helper/debug`, and fetches the editor's locked dependencies.
Setup may download and compile. It also verifies and installs Binaryen 133 and
official USB-enabled picotool 2.3.1 under `target/configurator-tools/bin` from
pinned release archives. It ends with Check; exit 4 means a listed manual
prerequisite remains. Python and Visual Studio Build Tools/Windows SDK require
manual installation. The legacy tracked `firmware/tools/picotool.exe` is
unchanged; the native helper uses the ignored tool directory. See
[explicit tool setup](setup-tools.md) for release hashes and installation checks.

Existing `Studio.ps1` (build) and `Studio.ps1 -Serve -Port 8080` remain the static
editor workflow. They now use offline mode and fail with setup instructions
instead of silently installing a compiler target or Trunk. Setup preserves the
repository pin; it does not select a newer compiler. The launcher scopes and
restores its PATH, NO_COLOR and Rust environment changes.

The standalone command is:

```powershell
.\target\studio-helper\debug\studio-helper.exe check --repo "$PWD" --json
```

The JSON schema is version 1 with `selected_toolchain`, `checks`, `ready`, and
`readiness_scope: installed_tool_versions_only`. Each check includes its stage,
status, evidence, whether it is required, and an actionable setup instruction.
Exit codes: 0 installed prerequisites pass; 2 invalid CLI usage; 4 missing,
incompatible or failed requirements; 70 process cleanup failed (no more probes
are started). Actual compiler/cargo versions and the selected sysroot's LLVM
executable are reported. LLVM registered ARM/Thumb targets are checked; the exact
Cortex-M33/FPU fixture remains the existing build validation adapter's check.
MSVC registration and SDK libraries are detected, while actual host linking,
Wasm pipeline validation and device access are explicitly `not_checked`.
`ready` therefore does not admit firmware, prove USB access, or enable flashing.
This command checks prerequisites only; the separate `serve` command provides
the HTTP/build/flash adapters.

Native checks require Windows 10/11 x64. Each fixed probe is assigned to a
non-inheritable Windows Job Object at process creation, with no breakaway flag,
a 15-second deadline and 64 KiB raw output limit per stream. Both pipes are
polled and drained through the existing log sanitizer; errors terminate the
owned tree, wait for zero active processes (up to five seconds), and reap the
child. These bounds apply to the check probes, not the legacy interactive
PowerShell Setup or static Trunk build/serve processes. The runner does not
claim an explicit Windows HANDLE_LIST policy for std's inherited standard IO.
No device operation is run by any check.

Using an already built static editor needs no local compiler or connected Pico.
PC CPU/RAM/disk minimums have not been qualified. Firmware validation is separate
from browser editing and can be part of a firmware build: a previous workload
sampled approximately 16.5 GB of private memory before optimization work. That
observation is not a minimum RAM requirement or a browser memory measurement.

### Connect the existing Studio to native prerequisite jobs

In a separate PowerShell terminal, run `./configurator/Studio.ps1 -Helper` after
explicit Setup, or run `studio-helper.exe serve --repo <absolute repository>`.
The helper binds only `127.0.0.1:8765`. An occupied port fails without terminating
any listener. Keep the static Studio at port 8080 and use its explicit Connect
button. The helper does not host a second editor or start work on connect.

Only `http://127.0.0.1:8080` and `http://localhost:8080` are trusted browser
origins. They are a deliberate local trust boundary: a page served by another
program at either origin has the same access. All requests require the exact
helper Host and an approved Origin. `POST /api/v1/connect` additionally requires
`X-Studio-Connect: 1` and an empty body; its no-store response contains one random
in-memory session/capability pair. The browser retains the capability only in
memory and sends `X-Studio-Capability` on later requests. No token is placed in a
URL, log, file, cookie or process environment. A restart replaces it. CORS is
limited to those two origins, GET/POST, and the three explicit request headers;
there is no wildcard or credential grant.

The helper exposes `GET /api/v1/session`, `GET /api/v1/prerequisites` (cached,
never runs a check), `POST /api/v1/prerequisites/check`, `GET /api/v1/jobs`,
`GET /api/v1/jobs/{id}/events?after=0&limit=128`, and
`POST /api/v1/jobs/{id}/cancel`. POST bodies are empty. A check returns 202 with
the shared `JobSummary`; event polling returns the shared `EventBatch`. Advance
`after` to the last received record's `sequence`, not the batch's `next` value.
Keep u64 values in Rust serde rather than parsing through JavaScript Number.
Only one job runs at once; a concurrent request gets 409. Cancellation interrupts
the current read-only probe, joins its owned processes, and then closes the job.
The latest check report includes detailed prerequisites and setup actions; job
logs contain the start marker and sanitized report records on completion.

There are at most 32 HTTP connection tasks, each with 10-second header/body and
30-second total connection deadlines, 16 KiB headers, 64 header fields, no HTTP
upgrades and no keepalive. Probe workers remain owned independently of client
disconnect. Ctrl+C stops accepts, cancels the check, and joins owned work.
Setup remains an explicit PowerShell command. Build, device inspection, Flash
and local-record cleanup routes share the authenticated job lifecycle below.

Focused native HTTP regression command after building the helper:
`python studio-helper/tests/test_server_http.py` (run from repository root).
It starts and stops only its own temporary helper process; port 8765 must be free.

### Read-only BOOTSEL descriptor discovery

`POST /api/v1/devices/refresh` (empty body, same authentication) starts a
`DeviceScan` job. `GET /api/v1/devices` returns the cached, bounded
`DeviceScanReport`; GET never enumerates USB. The report contains at most 16
candidates with BOOTSEL VID/PID `2e8a:000f`, fresh per-scan opaque IDs, optional
serial/product strings, descriptor status, reason, and explicit capacity state.
Missing/unsupported serials and duplicates stay unqualified. No candidate is
automatically selected. Refresh replaces the snapshot and all IDs.

Discovery uses nusb 0.2.8 Windows descriptors in a fixed helper child,
under the existing 15-second deadline/64 KiB output/owned-Job cleanup boundary.
It does not call picotool, claim/reset the PICOBOOT interface, read program or
OTP memory, or program anything. Every returned entry has `flash_ready:false`
and `capacity:{state:"unverified",bytes:null,provenance:null}`. The separate Flash
confirmation uses the shared `user_confirmed` state with manufacturer 4 MiB
provenance; this scanner never fabricates that assertion or presents it as a
measurement.

`complete:true` means the bounded descriptor producer finished. nusb can omit
inaccessible Windows descriptors and allocates its device list before returning;
this flag does not establish exhaustive USB absence, continuous attachment,
physical model, chip revision, security, partition state, capacity, driver
access or future flash eligibility. More than 16 candidates returns an
incomplete empty result rather than a misleading truncated selection list.
The descriptor child lifetime and returned data are bounded; upstream nusb/OS
allocation is not claimed to have the same 16-device bound.

The optional focused smoke check
`python studio-helper/tests/test_server_http.py --devices` explicitly enumerates
read-only descriptors. Without `--devices`, the HTTP test does not enumerate USB.

### Accepted project upload and native capture

`POST /api/v1/projects` takes `application/octet-stream` converted-only EAVP,
maximum 589824 bytes. It returns 201 `ProjectUploadResponse` with session ID,
fresh project ID and project/configuration/startup profile/startup asset SHA256s.
Original artwork is rejected. One immutable upload is retained per session;
replacement is refused while a worker runs, and every replacement gets a fresh
ID. A job takes its own accepted bytes, so replacement cannot change its input.

`POST /api/v1/builds` takes `application/json` `ProjectBuildRequest`, maximum
65536 bytes: editor revision, project ID/digest, typed options and optional retry
job ID. The caller must recheck its accepted EditorStamp and build selection
after upload before sending this request. The helper checks the session token,
digest, codec and options independently. Browser-supplied paths or commands are
not accepted. The shared job/event/cancel routes apply.

The native job copies bounded repository sources and EAV/EASP/EASA/project/options
into a fresh attempt, records hashes and runs the existing capture script from
that source copy. Source and accepted input hashes are checked before and after
capture. This is an exact-byte snapshot with change detection, not a guarantee
against a malicious process running as the same Windows user. Capture uses the
owned Job process supervisor, a 30-minute deadline, 8 MiB per output stream and
offline Cargo resolution. It performs the practical image checks when the
user explicitly requests a build; implementation tests do not run that pipeline.

After capture, Validation checks the final linked EAV/EASP/EASA arrays, practical
image evidence and ELF program/boot metadata. The exact
picotool 2.3.1 converter runs with the fixed RP2350 secure ARM family/platform
and no absolute block. A separate consumer verifies every UF2 block against the
ELF physical payload map before publishing an ArtifactId. Frozen source/input
files deny writes/deletes during the attempt; retained ancestor handles prevent
path replacement. Capture files remain locked during conversion/validation.

`GET /api/v1/artifacts` lists at most two admitted artifacts. Metadata is available
at `/api/v1/artifacts/{id}`; authenticated `/api/v1/artifacts/{id}/uf2` downloads
immutable retained bytes (maximum4177920 bytes). Older artifacts can be evicted;
restart discards the cache. Every summary binds the complete project, startup
inputs, normalized options, source, converter, ELF and UF2 identities. All have
`hardware_qualified:false` and `flash_ready:false`. These conditional software
artifacts do not qualify arbitrary boards, partition/security states, or A2/E10
mass-storage fallback. Explicit Flash admission requires a matching artifact,
fresh device inspection and the user's genuine-Pico-2 confirmation; artifact
metadata alone grants no write authority.

Compilation uses [practical image checks](../performance/memory-layout.md#practical-image-checks-and-experimental-proof): exact ELF/config/startup identities, memory limits, conservative stack bounds and reviewed flash contracts, followed by strict ELF-to-UF2 comparison. The separate complete instruction-level proof remains experimental and unfinished. Practical admission makes no hardware qualification claim.
Cargo's structured compiler diagnostics and sanitized stdout/stderr are streamed
into bounded job logs while the child runs. Compiler messages have explicit
line and total limits; failure cleanup retains a bounded `cargo-diagnostics.log`
beside the attempt summary, so errors remain available after scratch removal.
The raw Cargo message file remains subject to its separate capture limit.
Contended log ingress is dropped with explicit loss accounting, never blocking
pipe draining or cancellation. Terminal transition/error
diagnostics use a bounded best-effort queue and honor NO_COLOR/non-TTY output.
They contain no capabilities or raw project data.

### Explicit BOOTSEL information inspection

`POST /api/v1/devices/{id}/inspect` (empty body) inspects one explicitly selected
cached descriptor ID; the browser cannot supply a serial or tool arguments.
`GET /api/v1/devices/{id}/inspection` returns cached `DeviceInspection` only.
Refresh invalidates all inspection reports. The job shares the existing
operation permit and cancellation path. A single30-second inspection budget
covers the fixed subprocess sequence; owned cleanup has its separate watchdog.

The fixed commands are `info -d --debug` and `partition info`, with exact
VID/PID and the stored literal serial. They perform no persistent flash/OTP
programming or reboot. They do cause transient PICOBOOT interface reset,
exclusive access and XIP changes. Missing, duplicate or unsupported fields,
security/CPU policy, partition presence and permission failures reject the
inspection. Capacity remains unverified; reported flash size is a content-based
estimate and never a measured capacity or genuine-board assertion.

A bounded hash of native descriptor identity is compared before and after the
information pair. Matching snapshots do not prove continuous attachment or
prevent a device spoofing descriptors. Every inspection still has
`flash_ready:false`. No physical inspection was performed in feature tests.

### Explicit Flash and recovery

**Flash selected Pico 2** submits the accepted artifact, inspected device and
session-bound confirmation to `POST /api/v1/flashes`. The native worker repeats
identity and policy checks before fixed picotool commands. Transfer,
verification and reboot request are separate stages with bounded logs.
Verification compares device contents; a successful reboot request does not
establish application startup, electrical stability or ECU support.

Cancellation, disconnect or a helper restart never resumes a write. **Flash
history** shows retained outcomes, including incomplete attempts that need
inspection. Device selection and inspection are invalidated after an attempt.
Retry is explicit and needs current evidence again. See the
[flashing contract](flashing-design.md) for stage and recovery details.

### Local build storage

**Build storage** reports retained successful and failed attempts separately.
Cleanup requires an explicit selected record and confirmation. Active attempts,
retained artifacts and unknown or incomplete ownership records are protected;
cleanup does not affect a device or project stored in the browser.

Successful attempts are bounded to 16 records / 8 GiB, failed attempts to four
records / 1 GiB, and active scratch space to 4 GiB. Admission reserves space
before building; these caps are not a measured minimum disk requirement. The
[storage policy](../../studio-helper/BUILD_STORAGE.md) explains quarantine,
ownership and cleanup behavior. Flash history has its own cleanup controls.
