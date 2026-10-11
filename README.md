# Pico 2 EA888.3 dashboard

Rust firmware for a Raspberry Pi Pico 2, Pimoroni PIM715 320x240 display and
Adafruit 5880 I2C encoder, intended for a 2019 Seat Leon Cupra DNUC.
**Exact ECU support, electrical installation and physical display FPS are
unverified.** The default build shows unavailable vehicle channels. Demo data
requires `demo`; the optional Longan transport currently performs unqualified
adapter bring-up and sends no diagnostic CAN requests.

## Build and validation

Run firmware commands from `firmware/`:

```text
cargo build --locked -p dashboard-pico2 --release
cargo build --locked -p dashboard-pico2 --release --features demo
cargo build --locked -p dashboard-pico2 --release --features longan-rx12,frame-trace,te-trace
cargo clippy --locked -p dashboard-pico2 -- -D warnings
```

`demo` and `longan-rx12` are mutually exclusive. Both workspaces pin
`nightly-2026-10-11`, installed after `rustup update`; firmware also requests
`thumbv8m.main-none-eabihf`. The root `rustfmt.toml` applies to both workspaces.
The [dependency review](docs/dependencies/upgrade-2026-10-11.md) records current
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

Startup has no cosmetic delay. The header shows actual transport/sample state.
Valid saved preferences restore brightness, its on-value, boost units and FPS
mode; defaults apply when no valid journal exists. Persistence failures are
visible. See [startup status](docs/vehicle/startup-status.md) and
[settings](docs/performance/settings-journal.md).

| Button | Action |
|---|---|
| X | Dashboard FPS: Off, Instant, Average, Combined |
| Y | Dashboard, Debug, Logs |
| A | Dashboard boost unit: bar / PSI |
| B | Request an acknowledged statistics reset on Dashboard |

| Encoder input | Dashboard / Debug | Logs |
|---|---|---|
| Clockwise | Increase brightness by 5% | Newer entries |
| Counter-clockwise | Decrease brightness by 5%, down to off | Older entries |
| Press | Toggle backlight | No action |

The normalized direction follows the documented preassembled 5880 encoder;
physical direction remains an installation check. Press-to-off normally shows
a 1.5-second popup before switching off. Button feedback takes priority over dashboard warnings; warnings resume blinking after feedback expires and stay off Debug/Logs.
GP20 PWM is calculated for 1 kHz at every supported configured CPU rate. The
45%-100% visible duty mapping remains pending measured low-duty validation.

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

## Experimental clock profiles

Stock configuration is 150 MHz CPU and 37.5 MHz SPI. Optional mutually exclusive
profiles request 250/62.5, 280/70, 290/72.5 and 300/75 MHz CPU/SPI. Their feature
names are `cpu250-spi62-1v10`, `cpu280-spi70-1v30`, `cpu290-spi72-1v30` and
`cpu300-spi75-1v30`. These are experimental build settings; compilation proves
neither electrical margins nor reliable display operation. SPI limits depend
on `clk_peri`, divisors and the actual panel. See
[clock reporting](docs/performance/clock-reporting.md).

## Layout

```text
firmware/pico2/src/  Application, shared models, display, diagnostics and drivers
firmware/vendor/    Reviewed Embassy RP source and provenance manifest
firmware/scripts/   Build, ownership, logging, vendor and ELF gates
host/               Same pure Rust modules, replay/pixel tests and Criterion
host/baselines/     Historical raw samples, source archives and metadata
hardware/           Proposed wiring and physical inspection requirements
docs/               Architecture, source checks and validation records
```

## License

[MIT](LICENSE).
