# Pico 2 EA888.3 dashboard

Rust firmware for a Raspberry Pi Pico 2, Pimoroni PIM715 320x240 display and
Adafruit 5880 I2C encoder, intended for a 2019 Seat Leon Cupra DNUC.
**Exact ECU support, electrical installation and physical display FPS are
unverified.** The default build shows unavailable vehicle channels. Demo data
requires `demo`; the optional Longan transport currently performs unqualified
adapter bring-up and sends no diagnostic CAN requests.

Browse the offline **ECU Reference** workspace while configuring views: searchable source/scaling evidence, family limitations and unresolved definitions, with the shared preview retained. [Studio usage](docs/configurator/ecu-reference.md) · [Technical ECU library](docs/vehicle/ecu-reference/README.md).

## Configure dashboard views

Launch the Rust/Wasm editor from the repository root:

```powershell
./configurator/Studio.ps1 -Setup
./configurator/Studio.ps1 -Check
./configurator/Studio.ps1 -Serve
```

Open `http://127.0.0.1:8080`. Drag widgets onto a 4×2 grid, configure up to eight
named views, and export a complete `.eavp` project. Dashboard-only JSON and compact
`.eav` exports remain available. The preview
uses the firmware renderer. See the [View Studio guide](docs/configurator/studio.md)
for warnings, fuel/AFR, ECU source evidence, local saving and import/export.
Choose numeric/history, circular, needle, horizontal, vertical or segmented
gauges. [Gauge and colour controls](docs/configurator/gauges-and-colours.md) include
fixed or expanding scales, per-widget palettes and separate combined warnings.

Embed the exported file in a checked build using the working profile:

```powershell
./firmware/scripts/Build-Dashboard.ps1 -Name my-views -Profile 280-70 -Mode Demo -Config ./dashboard.eav -Expert
```

This builds and validates an artifact without flashing. See the
[configuration build guide](docs/configurator/building.md) for runtime parsing,
fallback and hardware validation. Vehicle source qualification remains required.
The [configurator validation record](docs/configurator/validation.md) maps the
completed implementation, final review corrections, measurements and remaining
device checks.

### Build & Flash from Studio

In a second terminal, run `./configurator/Studio.ps1 -Helper`. Open **Build & Flash**,
connect the helper and check prerequisites. Choose Demo or Vehicle, a named
profile or validated custom clocks, and inspect the actual frequencies and
compiler options before building the accepted project. Invalid drafts block a
new build; changes after a build mark its firmware as different.

After a build passes its artifact checks, Studio offers a checked UF2 download
and explicit flashing. Hold BOOTSEL while connecting the isolated board, scan,
inspect the intended device and confirm it is a genuine Pico 2. Its 4 MB capacity
is manufacturer specified and user confirmed, not measured by picotool.
The job shows **transfer → verification → reboot request** separately. A reboot
request does not establish that firmware is running correctly. Interrupted jobs
never resume a write automatically; retained history explains the last known
outcome. The downloadable PowerShell recipe is a build-only fallback.

The helper currently runs on **Windows 10/11 x64**. Setup needs rustup, MSVC Build
Tools with a Windows SDK, Python, and Internet access for the pinned Rust/tool
downloads. Check reports missing tools before a build. Browser editing of an
already served Studio build does not require a Rust toolchain on the browser's
computer. No minimum PC RAM specification has been qualified. A full development
firmware-image validation previously sampled up to **16.5 GB of process private
memory**; this is a heavy validation workload, not browser editing or Pico RAM.
See [setup details](docs/configurator/setup-tools.md) and the
[flashing contract and clock references](docs/configurator/flashing-design.md).

## Firmware build and validation

Run firmware commands from `firmware/`:

```text
cargo build --locked -p dashboard-pico2 --release
cargo build --locked -p dashboard-pico2 --release --features demo
cargo build --locked -p dashboard-pico2 --release --features longan-rx12,frame-trace,te-trace
cargo clippy --locked -p dashboard-pico2 -- -D warnings
```

`demo` and `longan-rx12` are mutually exclusive. All Rust workspaces pin
`nightly-2026-10-16`, installed after `rustup update`; firmware also requests
`thumbv8m.main-none-eabihf` and `target-cpu=cortex-m33`. The root `rustfmt.toml` applies to the workspaces.
The [dependency review](docs/dependencies/upgrade-2026-10-16.md) records current
releases, compatibility constraints and reviewed local HAL corrections.

Run shared correctness checks from `host/`:

```text
cargo test --locked
cargo test --locked --no-default-features
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --benches
python scripts/test_benchmark_comparison.py
python scripts/test_telemetry_export.py
```

The [benchmark policy](docs/performance/benchmark-policy.md) describes frozen
sources, raw Rust Criterion samples and controlled comparisons. CI also checks
feature conflicts, UART ownership, nonblocking logging, vendor hashes, linked
RAM/flash boundaries and corrupted artifact rejection. Host checks and target
builds do not execute the hardware.

The [overhaul acceptance record](docs/validation/overhaul-2026-10-11.md) maps
the implemented stages and review corrections to local test/build evidence and
the remaining hardware gates. The [fixed workload matrix](docs/performance/benchmark-coverage.md)
states exactly what each group of host benchmarks measures.

For hands-on testing, follow the [Pico 2 and DNUC validation runbook](docs/validation/pico2-dnuc-runbook.md)
for isolated bench flashing, controls, RTT/timing capture, Longan qualification
and parked vehicle evidence collection.

For USB programming, first follow the power-domain isolation in the
[wiring and inspection record](hardware/wiring-diagram.txt): direct VSYS wiring
can backfeed the adapter/regulator from USB. With the assembly isolated and
BOOTSEL active, `cargo run --locked -p dashboard-pico2 --release` uses the bundled
`firmware/tools/picotool.exe` runner to load and execute the image. No hardware
was flashed during the software overhaul.

## Architecture

- **Core0:** cooperative rendering, buttons, I2C encoder, PWM backlight, DMA
  display owner, recovery supervisor and optional TE observations.
- **Core1:** acquisition and per-measurement history before publication; optional
  UART parsing, bounded request scheduler, transport recovery and diagnostics.
  Static UART buffers, handles and IRQ remain on this core.
- **Handoff:** coherent typed samples carry source, sequence, timestamp and
  quality. Embassy Watch may coalesce display snapshots; it does not discard
  samples before acquisition statistics. Resets require producer acknowledgement.
- **Display:** two aligned 153600-byte framebuffers move through affine ownership
  and checked transfer tokens. Completion follows HAL return and CS high. A
  timed-out transfer is quarantined without cancelling DMA or reusing its buffer.
  Retained rendering and bounded damage regions preserve exact panel contents;
  `full-frame` and `serial-display` retain diagnostic fallbacks.
- **Persistence:** bounded two-sector settings journal, debounced writes and a
  coordinated display/I2C/UART pause. Reviewed flash-critical code executes in
  SRAM. Physical erase, power-cut and watchdog recovery tests remain pending.
- **Logs:** nonblocking ordinary RTT writes and bounded on-screen history with a
  protected latest fault. Cross-core locks cover bounded copies and updates.

Detailed contracts: [display ownership](docs/performance/display-pipeline.md),
[partial updates](docs/performance/partial-updates.md),
[acquisition](docs/performance/acquisition-handoff.md),
[typed quality](docs/vehicle/sample-quality.md),
[transport and qualification](docs/vehicle/diagnostic-transport.md),
[flash coordination](docs/hardware/flash-persistence.md).

The supplied CANChecked U-DNU profile is opaque evidence. It does not establish
DIDs, scaling or exact ECU compatibility. See the
[DNU evidence record](docs/vehicle/canchecked-dnu-evidence.md). Unsupported boost,
lambda, DSG and EGT remain unavailable in Vehicle mode. Vehicle alarms also
remain disabled until source-specific policies are qualified.

## Controls and startup

Startup presentation is configurable: a brief status screen, then the selected
animation. Acquisition continues throughout; the dashboard header shows the
actual transport/sample state. See [startup settings](docs/configurator/startup.md).
Valid saved preferences restore boost units and FPS mode; every boot starts at
Full brightness. Defaults apply when no valid journal exists. Persistence failures are
visible. See [startup status](docs/vehicle/startup-status.md) and
[settings](docs/performance/settings-journal.md).

The following tables show defaults. Accepted per-screen tap/hold and encoder
remaps follow [the controls and recovery contract](docs/configurator/control-mapping-design.md).
Ordinary assigned holds default to 800 ms and suppress the release tap. Dashboard
A/B holds select previous/next view; Debug A/B/X holds lower/raise/toggle brightness;
Logs A/B/X holds scroll older/newer/return live. A fixed two-second Y hold always
recovers Dashboard, the selected view and Full brightness, even from Off.
The [v4 release](docs/configurator/advanced-v4-release.md) describes advanced
units/history/markers/warnings/RGB. Studio also includes organized workspaces,
the ECU reference, startup artwork and local Build & Flash.

| Button | Default tap action |
|---|---|
| X | Dashboard FPS: Off, Instant, Average, Combined |
| Y | Dashboard, Debug, Logs |
| A | Dashboard FollowBoost unit: bar / PSI; fixed-unit widgets retain their unit |
| B | Request an acknowledged statistics reset on Dashboard |

| Encoder input | Dashboard | Debug | Logs |
|---|---|---|---|
| Clockwise | Next configured view | Increase legacy brightness by 5% | Newer entries |
| Counter-clockwise | Previous configured view | Decrease legacy brightness by 5%, down to off | Older entries |
| Press | Full -> Dim -> Off -> Full | Toggle backlight | No action |

The normalized direction follows the documented preassembled 5880 encoder;
physical direction remains an installation check. Every boot starts at Full
(100% PWM duty); Dim is exactly 50%, Off is 0%. Dashboard popups read `BL: FULL`,
`BL: DIM` and `BL: OFF`. Off remains visible for two seconds after its first
completed transfer, then a frame removing it completes before the backlight
switches off. Another click returns to Full; changing pages or replacing the
pending Off feedback cancels shutdown. The default configuration has one view;
rotation switches configured views and displays their names. Returning from
Debug/Logs preserves the selected view; reboot selects the configured boot view.

Debug keeps its legacy 5% control scale and 1.5-second toggle popup. Button
feedback takes priority over dashboard warnings; warnings resume blinking after
feedback expires and stay off Debug/Logs. The RGB LED blinks red with the last
completed warning frame, suppressing green during both warning phases. Otherwise
the LED stays green; blue remains off. Green means no visible warning, not
verified vehicle health. GP20 PWM aims near 1 kHz from the actual configured clock
(132 MHz gives an integer 1015 Hz diagnostic; legacy 290.4 MHz gives 1001 Hz). Brightness is
runtime-only; unit and FPS choices remain persistent. See
[backlight and indicator controls](docs/hardware/backlight-controls.md).

The default lambda cell also shows derived AFR and an `EQ 14.7` scale label. This is
gasoline-equivalent AFR (`lambda * 14.7`), not a measured fuel composition.
The configurator can hide AFR or select an explicit fuel conversion with declared
assumptions. Lambda remains the primary value and graph; invalid or stale lambda hides both
readings. An E5 label does not establish an exact fuel-specific conversion. See
[measurement source and quality](docs/vehicle/sample-quality.md#lambda-and-derived-afr).

The FPS modes count successful software transfers: Instant is the recent
one-second-or-longer window; Average is since the last page change. An unchanged
scene can send no frames. These counters do not measure panel scanout. Optional
[frame/input/diagnostic windows](docs/performance/telemetry-windows.md) and
[TE observations](docs/hardware/te-observations.md) preserve separate metrics.
The default-off [target benchmark](docs/performance/target-benchmarks.md) records
finite CPU kernels with counter/timebase guards and reproducible ELF identity.

Logs retain 128 entries of up to 40 UTF-8 bytes, with truncation/overwrite
counters and a separately protected latest fault. Scrolled views use stable
sequence anchors. See [logging contracts](docs/performance/logging.md).

Studio **Diagnostics** provides filtered browser events and console output.
Its optional terminal relay forwards new warning/error context to the connected
helper: severity, approved event code, workspace, sequence, timing and accepted
revision. Raw messages, configuration and artwork remain in the browser. Terminal
output uses colours when supported; bounded queues may drop context under load
without blocking builds or flashing. The UI reports those drops. The ordinary
Trunk terminal cannot receive browser events by itself.

## Experimental clock profiles

Stock configuration is 150 MHz CPU and 37.5 MHz SPI. Optional mutually exclusive
profiles also include 132/66, 140/70 and 150/15 MHz, with their `cpu132-spi66-1v10`,
`cpu140-spi70-1v10`, `cpu150-spi15-1v10` features. Explicit Stock is
`cpu150-spi62-1v10`; absence of a clock selector still means Stock.
Existing profiles request 250/62.5, 280/70, 290/72.5 and 300/75 MHz CPU/SPI. Their feature
names are `cpu250-spi62-1v10`, `cpu280-spi70-1v30`, `cpu290-spi72-1v30` and
`cpu300-spi75-1v30`. These are experimental build settings; compilation proves
neither electrical margins nor reliable display operation. SPI limits depend
on `clk_peri`, divisors and the actual panel. Legacy290's actual 290.4/48.4 MHz
and user-reported failure remain distinct from exact custom 290/72.5 MHz.
See [the shared catalogue and build options](docs/configurator/building.md) and
[clock reporting](docs/performance/clock-reporting.md).

## Layout

```text
crates/dashboard-core/  Shared no_std rendering, models and diagnostic contracts
crates/dashboard-config/  Bounded schema, catalogue, fuel model and binary codec
crates/dashboard-config-tools/  Host validation and build snapshot tools
crates/dashboard-startup-tools/ Rust artwork conversion and worker contracts
configurator/       Rust/Wasm view editor, shared pixel preview and local saving
studio-helper/      Native prerequisites, builds, device inspection and flashing
configs/            Reviewed default JSON and compact runtime configuration
firmware/pico2/src/  Hardware application, tasks, profiling and drivers
firmware/vendor/    Reviewed Embassy RP source and provenance manifest
firmware/scripts/   Build, ownership, logging, vendor and ELF gates
host/               Same pure Rust modules, replay/pixel tests and Criterion
host/baselines/     Historical raw samples, source archives and metadata
hardware/           Proposed wiring and physical inspection requirements
docs/               Architecture, source checks and validation records
```

## License

[MIT](LICENSE).

## Startup presentation

Studio **Device → Startup** offers five shared Rust animations, factual status,
timing, looping, warning interruption and warning LED controls. Defaults are a
600 ms Brief status then 2500 ms AEZAKMI. Zero splash duration leaves status
independent; loop off holds the final frame. Warnings normally wait for Dashboard
while the LED may indicate them, and hidden warnings cannot be acknowledged.
Export a complete `.eavp` project with original artwork by default, or an explicit smaller converted-only project. Startup shares Undo/Redo and atomic browser storage; legacy `.eav` and JSON are clearly dashboard-only.
See [startup behavior](docs/configurator/startup.md) and [complete projects, source retention and storage](docs/configurator/projects.md).
