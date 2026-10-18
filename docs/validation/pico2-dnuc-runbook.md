# Pico 2 W and 2019 DNUC Cupra validation

Keep one evidence directory per image and power-on run. **The current firmware
does not enable live vehicle measurements.** `demo` is synthetic; the no-backend
build has no vehicle transport; `ble-obd` is the selected USB-powered prototype,
with disabled-by-default settings and adapter-only MC+/CX profiles.
`longan-rx12` retains the historical adapter bring-up with `Profile::UNQUALIFIED`.
BLE ONLY, TEST PEER and RAW ONLY do not qualify the ECU or enable live polling.

No device or vehicle test below has been performed by the software checks.

## Selected USB/BLE route

Use [current hardware](../../hardware/README.md) and the
[BLE acceptance checks](../vehicle/ble-runtime.md#verification-boundaries) for
the selected route. Retain the encoder. Bench testing starts with USB-powered
Pico/display/encoder and a separate synthetic peer, with the car disconnected.
Do not assemble the Longan/Pololu/covered-switch circuit for this prototype.

The numbered procedure below retains the earlier wired runbook: its Longan power,
UART, switch and regulator steps apply only to that historical alternative.
Display/control/recovery checks remain useful, but require an explicitly selected
image and separate authorization before physical flashing or any vehicle work.

## 1. Prepare and inspect, with the car disconnected

Equipment for the historical wired route: Pico 2 W (radio unused in this backend),
PIM715 Display Pack 2.8, optional Adafruit 5880
encoder, data-capable USB cable, multimeter, current-limited bench supply, and
the actual Longan module. For timing/recovery evidence also use a 3.3 V SWD
probe, suitable scope/logic analyser and a CAN interface that retains timestamps,
IDs, DLC and frame flags. Visual checks alone cannot close the timing gates.

1. Photograph board markings, both sides of Longan, solder options and wiring.
   Check against the [wiring record](../../hardware/wiring-diagram.txt).
2. Disconnect OBD completely. For USB programming disconnect the external 5 V
   distribution from Pico VSYS and disconnect Longan's UART signals. USB can
   otherwise backfeed the adapter/regulator even with the vehicle switch off.
   No simultaneous USB/vehicle supply circuit is qualified for this assembly.
3. Verify no short between each supply and ground. Keep Pololu EN disconnected
   from Pico GPIO: its pull-up is to VIN. OBD pin 16 must never feed Pico GPIO,
   Pico VSYS or the Longan module VCC directly.
4. The owner's Longan is V1.4 with a photographed SIT65HVD230 transceiver.
   Its regulator, rail and UART levels remain unmeasured; published V1.0–V1.3
   mappings do not establish this assembly's limits. Measure IO levels before
   connecting GP0/GP1. A 5 V supply label does not establish UART compatibility.
5. Start with Pico/display/encoder on USB, with the vehicle power circuit and
   adapter isolated. Separately verify the regulator's unloaded and loaded 5 V
   output on the bench before adding it. Log 5 V, 3.3 V, current and temperatures.
   The provisional fuse/TVS/PTC list is not an automotive transient qualification.
6. Keep stock clocks throughout initial acceptance: 150 MHz CPU and configured
   37.5 MHz SPI. All overclock features require a separate repeat of the tests.

These power and IO rules were rechecked against the
[Pico 2 W datasheet, sections 3.4–3.5](https://datasheets.raspberrypi.com/picow/pico-2-w-datasheet.pdf),
[Longan revision table](https://docs.longan-labs.cc/1030001/) and
[Pololu connections](https://www.pololu.com/product/3782).
The [W board/pin audit](../hardware/pico2w-board-audit.md) and
[protection proposal](../../hardware/protection-plan.md) distinguish the reviewed
wiring from remaining electrical acceptance. An external branch switch does not
cut USB power or a separately powered Bluetooth dongle.

## 2. Build, check and freeze the image

After the merge, update a clean checkout from `master`. Keep local evidence out
of commits. The shared crates, firmware, host, configurator and native helper use
the pinned `nightly-2026-10-16`; a newer
compiler requires rerunning the checks, including the exact flash-code gates.

Run this PowerShell block. It builds only, without contacting the Pico:

```powershell
Set-Location D:\GitHub\pico2-ea888.3-car-dashboard\firmware
$ErrorActionPreference = 'Stop'
if (git status --porcelain --untracked-files=no) { throw 'Tracked source is dirty' }
rustup show active-toolchain
if ($LASTEXITCODE -ne 0) { throw 'Toolchain check failed' }

# Use a NEW run name each time. Change these two values for the profiles below.
$validationRun = 'demo-stock-01'
$validationFeatures = 'demo,frame-trace,te-trace'
$validationDir = "target/hardware-validation/$validationRun"
New-Item -ItemType Directory -Path $validationDir -ErrorAction Stop | Out-Null
$validationOldStrip = $env:CARGO_PROFILE_RELEASE_STRIP
try {
    $env:CARGO_PROFILE_RELEASE_STRIP = 'none'
    cargo build --locked -p dashboard-pico2 --release --target thumbv8m.main-none-eabihf --target-dir "$validationDir/build" --features $validationFeatures
    if ($LASTEXITCODE -ne 0) { throw 'Build failed' }
} finally {
    $env:CARGO_PROFILE_RELEASE_STRIP = $validationOldStrip
}
$validationElf = "$validationDir/build/thumbv8m.main-none-eabihf/release/pico2"
python scripts/check_memory_layout.py $validationElf
if ($LASTEXITCODE -ne 0) { throw 'Memory/flash gate failed' }
python scripts/test_flash_lockout.py $validationElf
if ($LASTEXITCODE -ne 0) { throw 'Corrupted-artifact gate failed' }
Copy-Item -LiteralPath $validationElf -Destination "$validationDir/pico2.elf"
git rev-parse HEAD | Set-Content "$validationDir/commit.txt"
$validationFeatures | Set-Content "$validationDir/features.txt"
rustc -Vv | Set-Content "$validationDir/compiler.txt"
Get-FileHash "$validationDir/pico2.elf" -Algorithm SHA256 | Format-List | Out-File "$validationDir/sha256.txt"
```

| New run name | Feature string | Purpose |
|---|---|---|
| `demo-stock-01` | `demo,frame-trace,te-trace` | UI, controls and display timing |
| `vehicle-stock-01` | `longan-rx12,frame-trace,te-trace` | Unqualified adapter startup/recovery |
| `demo-serial-01` | `demo,frame-trace,te-trace,serial-display,full-frame` | Display comparison without overlap/partial updates |

Do not combine `demo` and `longan-rx12`. A gate failure means stop and retain its
output; changing expected instruction hashes is not a repair. Keep the frozen
ELF for flashing and decoding, even after building another profile. The scoped
strip override is restored so it cannot contaminate later benchmark captures.

### Flash over isolated USB

1. With the power/UART isolation from step 1 in place, hold Pico BOOTSEL, connect
   USB, then release BOOTSEL. Confirm the boot drive appears. This firmware does
   not provide a USB console or a running-firmware USB reboot service.
2. From the same PowerShell session, load the frozen ELF:

```powershell
.\tools\picotool.exe load -u -v -x "$validationDir/pico2.elf" -t elf
if ($LASTEXITCODE -ne 0) { throw 'Flash/verify failed' }
```

3. Require successful write verification and application startup. If Windows
   shows the drive but picotool cannot open it, fix BOOTSEL USB driver/access
   first; do not erase the whole flash as a troubleshooting shortcut.

The bundled picotool reports `v2.2.0-a4`; its local help and the
[official load documentation](https://github.com/raspberrypi/picotool#load)
confirm update, verify, execute and explicit ELF options. The reserved settings
sectors are outside the image; actual preservation is tested below.

## 3. Verify the Pico/display on the bench

For orientation-enabled images, complete the [four-mode display matrix](../hardware/display-orientation.md#physical-acceptance-matrix)
including different Boot info/Splash/view overrides, native stride, pending
transfers and physical controls. Source checks and host models do not establish
scanout, tearing, optical FPS or ECU operation.

With `demo-stock-01`, require an explicit DEMO identity and changing synthetic
values. There should be no old scripted 13-second welcome sequence. Record a
video from power-on through the first usable frame.

Use the captured configuration's accepted bindings. The matrix below describes
defaults; test remaps against [the action and fixed recovery contract](../configurator/control-mapping-design.md).
Run the [configuration-specific checks](../configurator/building.md#configuration-specific-device-checks)
for units, advanced warnings, RGB and completion ownership.

| Control | Default expected behavior |
|---|---|
| Y | Dashboard -> Debug -> Logs -> Dashboard |
| X, on Dashboard | FPS Off -> Instant -> Average -> Both -> Off |
| A, on Dashboard | BAR/PSI toggle changes FollowBoost; fixed readouts retain their units and equal secondaries are omitted |
| B, on Dashboard | Request statistics reset; successful reset follows acquisition acknowledgement |
| Encoder rotation, Dashboard | Previous/next configured view, including wraparound; no brightness change |
| Encoder press, Dashboard | Full (100% duty) -> Dim (50%) -> Off -> Full; named popups |
| Dashboard Off | Two seconds after first popup transfer completes, remove popup, complete that frame, then switch off |
| Encoder clockwise, Debug | Legacy brightness increases in 5% steps and clamps at 100% |
| Encoder press, Debug | Existing backlight toggle and 1.5-second popup |
| Encoder on Logs | Rotation scrolls; press has no action |
| Visible dashboard warning | RGB LED blinks red/off with the completed popup phase; green and blue remain off in both phases |
| No visible warning, including feedback, Debug/Logs, or backlight Off | Steady green; blue off |
| Fresh lambda | Main lambda plus derived AFR on the labelled EQ 14.7 gasoline scale; lambda 0.800 gives AFR 11.76 |
| Stale or unavailable lambda | No current lambda or AFR number |

Test default 800 ms A/B Dashboard view holds, Debug A/B/X brightness holds and
Logs A/B/X navigation holds. Verify release taps are suppressed after holds.
Hold Y for two seconds on every screen and while Off; Dashboard/Full recovery
must remain reachable with every accepted map. Press/release once at a time,
then test quick repeated presses,
rapid rotation, page changes during popups and unit changes. Leave the graph
running long enough to wrap. Require no trails, alternating stale digits,
clipping into neighbours or stale popup rectangles. Red/green/blue rendering
must agree with the [RGB565 contract](../hardware/display-color.md).

On Debug, record configured clocks, frame/transfer counters, reset cause and
settings status. The displayed completion FPS and sampled stack depth are
software observations. Neither proves panel FPS or peak stack use.

Test persistence at stock clocks:

1. Select Dim, a boost unit and FPS mode. After the last input wait
   until `SAVED` is actually visible. Debounce is 2 seconds and attempts are
   separated by at least 30 seconds; repeated changes can delay the next save.
2. Power off/on. Require Full brightness on every boot, and the saved boost unit
   and FPS mode to return. Brightness and statistics are runtime-only.
3. Reflash the same ELF without erase-all and check settings again.
4. `VOLATILE` or `FAILED` is a failed persistence gate, even if rendering works.
   Save the log and board flash markings. Release writes require the reviewed
   flash identity/profile; do not bypass that check.

Check that `BL: OFF` stays visible for two seconds, disappears before blackout,
and the next click restores Full without stale popup pixels. Try another click,
Y/page change, X/FPS and a reset acknowledgement while Off is pending: a click
restores Full; page changes or replacement feedback cancel the pending shutdown
and retain the previous lit level. Test this with a warning already blinking.
The configured 50% duty is not a claim of half perceived luminance or the lowest
usable brightness on this panel.

The PIM715 wiring was checked against its
[product/schematic](https://shop.pimoroni.com/products/pico-display-pack-2-8).
The PID5880 encoder uses address `0x36`, seesaw firmware product ID 4991 and
seesaw button GPIO24, as checked
against [Adafruit's guide](https://learn.adafruit.com/adafruit-i2c-qt-rotary-encoder?view=all)
and the [driver review](../hardware/encoder-recovery.md).

## 4. Capture timing and recovery evidence

### RTT over SWD

The USB boot connection does not provide the firmware's defmt log. Connect a
3.3 V debug probe's SC/SWCLK, SD/SWDIO and GND to the Pico's labelled SWD pads.
Make the ground reference first; do not add a second target supply through the
probe. Keep Longan UART GP0/GP1 separate. Follow the
[Raspberry Pi debug-probe wiring](https://www.raspberrypi.com/documentation/microcontrollers/debug-probe.html).

The locally checked probe-rs is 0.31.0 and lists the chip as `RP235x`. Its
[attach implementation](https://github.com/probe-rs/probe-rs/blob/v0.31.0/probe-rs-tools/src/bin/probe-rs/cmd/attach.rs)
attaches to a running image; `run` is a separate flash/reset operation. After
flashing the frozen image, use:

```powershell
probe-rs attach --chip RP235x --protocol swd --speed 1000 --no-catch-reset --no-catch-hardfault --no-location --no-timestamps --target-output-file "$validationDir/rtt-boot01.txt" "$validationDir/pico2.elf"
```

Keep the display on Dashboard. Capture at least 120 seconds: exclude the initial
boot window and retain at least three complete later 30-second windows. Use
separate captures for stationary controls, repeated input and the serial/full
comparison. End after a complete export, using Ctrl+C. A reboot starts a new
file; do not join boot-relative clocks into one run.

```powershell
python ../host/scripts/parse_telemetry.py "$validationDir/rtt-boot01.txt" --output "$validationDir/telemetry-boot01.json"
if ($LASTEXITCODE -ne 0) { throw 'Capture is incomplete or invalid; retain it and capture a new run' }
```

Require complete groups, inspect errors/rejections/sequence gaps and record the
actual window durations. Do not silently discard bad rows. Stopping mid-export
can invalidate the final group; preserve that file and repeat. Frame metrics
include p50/p95/p99 bounds for completion intervals, render, queue, transfer,
buffer waits and software input/sample ages. ECU samples/PID timings remain
absent in unqualified vehicle mode.

### Physical timing

At stock clocks capture GP17 CS, GP16 DC, GP18 SCK, GP19 MOSI and GP21 TE; measure
GP20 PWM separately if the analyser lacks channels. Use bandwidth/sample rate
suitable for 37.5 MHz SPI and 3.3 V inputs. Require approximately 1 kHz PWM and
compare observed SPI frequency with the configured rate. Decode window commands
and payloads: a full 320x240 or 240x320 RGB565 update contains 153600 pixel bytes, excluding
commands. Partial updates must touch only their selected windows.

Measure TE intervals physically, plus input-edge-to-visible-change with a
high-speed camera or photodiode. TE frequency is panel scan cadence, while
completed writes are update cadence; neither alone proves unique visible FPS.
Record tearing, colors and latency. Repeat the same actions in the serial/full
profile, with identical power, brightness and scene. This comparison changes
both overlap and partial updates, so it measures their combined effect. Add a
`demo,frame-trace,te-trace,full-frame` image to isolate those effects. Save waveforms and video
beside the ELF hash. See [TE limitations](../hardware/te-observations.md).

### Finite target kernels

Build/freeze a separate target experiment; it does not flash:

```powershell
python scripts/capture_target_bench.py device-kernels-01 --features target-bench,frame-trace,te-trace
if ($LASTEXITCODE -ne 0) { throw 'Target benchmark capture failed' }
```

Keep its `target/bench-captures/device-kernels-01/{manifest.json,source.zip,pico2.elf}`.
With an isolated bench supply and SWD, start logging from boot using the exact
ELF (this command flashes/resets the target):

```powershell
probe-rs run --chip RP235x --protocol swd --speed 1000 --verify --no-catch-reset --no-catch-hardfault --no-location --no-timestamps --target-output-file target/bench-captures/device-kernels-01/rtt-boot01.txt target/bench-captures/device-kernels-01/pico2.elf
```

Require complete build/clock/environment/XIP metadata, four ordered `BENCH1`
rows, 8 iterations each, checksums 3402950085 / 4004879533 / 3907113197 /
2271814903, zero row status and `BENCHEND1,0,4`. Missing early RTT records,
debugger stops or a nonzero status invalidate the run. Record the failure;
do not infer a timing from a partial run. These are finite software kernels,
not display FPS. [Full contract](../performance/target-benchmarks.md).

### Recovery and soak

On an isolated bench, power down before ordinary encoder disconnect/reconnect
tests. Then test powered disconnect only with a fixture that preserves ground
and prevents pin shorts/backfeed. Absence should leave the UI usable; a held
button at reconnect must not create a phantom press. A stuck I2C transaction
is intended to cause watchdog recovery rather than cancellation/reuse.

Watchdog/DMA/IRQ stall injection and flash power-cut sweeps need a controlled
bench fixture and a recorded test image. There is no production CLI that injects
those faults. Do not equate stopping both cores in a debugger with an isolated
task stall or short signal pins to force a fault. Test each owner independently;
require reset, both-core/peripheral restart and a matching reset reason. The
normal watchdog is 2 seconds, with an initial 15-second startup budget.

For journal fault testing, interrupt only the bench target's power, never the
car supply, and sweep cuts across data/commit programming and sector rollover.
After each cut require either the last committed settings or the completely
committed new settings, never mixed fields. Keep a BOOTSEL recovery path and
save the failed image/log. Manual random power cycling is only a smoke test.

Run an initial 60-minute stock bench soak with sparse input and a separate
60-minute active-input run, logging any reset, overflow or thermal change.
These are initial test durations, not lifetime or automotive qualification.
Long-duration, temperature and supply-transient testing remain separate gates.

## 5. Bring up Longan without the car

1. Confirm the module's firmware/revision and UART configuration independently.
   The application requires **115200, 8N1**. Longan's documented factory default
   is 9600. Provision on the isolated bench using its vendor procedure, then
   power-cycle and verify 115200 operation before attaching the Pico. The
   firmware does not change baud, CAN rate, masks or EEPROM on recovery.
2. With power off, connect Pico GP0 TX -> Longan RX and Longan TX -> Pico GP1 RX,
   through the already verified level interface, plus ground and the correct
   regulated module supply. Disconnect Pico USB before restoring a direct
   external VSYS feed. Use SWD without a second power feed for logs.
3. Flash `vehicle-stock-01` using the USB isolation procedure, then restore the
   bench connections. Capture UART electrically. Expected bootstrap bytes are:

   ```text
   Pico: +++                    (no line ending)
   Adapter: ENTER INTO SETTING MODE\r\n
   Pico: AT\r\n
   Adapter: OK\r\n
   Pico: AT+Q\r\n
   Adapter: OK\r\nENTER DATA MODE\r\n
   ```

4. Require `RAW ONLY` after that exact exchange. `STARTING`/`RETRY` means the
   handshake has not succeeded; `RESET REQ` is terminal quarantine. Record
   bytes/levels/baud rather than weakening matching. All gauge values must
   remain unavailable, and there must be **no 14-byte diagnostic CAN request**.
   No application CAN query does not mean the adapter is electrically silent:
   normal CAN controller mode can still acknowledge frames or signal errors.
5. Use a separate, correctly terminated two-node CAN bench. Send a known
   standard data frame, ID `0x7E8`, DLC 8, payload
   `03 41 05 8B 00 00 00 00`. The documented RX12 UART representation is
   `00 00 07 E8 03 41 05 8B 00 00 00 00`. Observe it on a UART analyser; it must
   not make the unqualified dashboard show 99 C. This is a synthetic fixture,
   not a frame to inject into the car.
6. Compare simultaneous CAN/UART captures for different IDs, DLC 0..8, extended
   frames, RTR, bursts, disconnects, reset and error recovery. Validate actual
   filter behavior with rejected-ID injections, not just configuration ACKs.
   Save module firmware provenance and raw traces.

**Qualification gate:** RX12 omits DLC and standard/extended/RTR metadata.
Neither a plausible 12-byte record nor a successful handshake recovers those
facts. Live polling needs a separately reviewed, evidenced deployment profile
and exact responder/filter contract, or a transport that preserves that
metadata. There is no command-line feature that bypasses this gate in this
release. Do not edit `PROFILE` just to get numbers on screen.

The protocol has been checked against [Longan's documentation](https://docs.longan-labs.cc/1030001/),
the vendor library/firmware and the exact application/HAL paths recorded in
[diagnostic transport](../vehicle/diagnostic-transport.md).

## 6. Collect DNUC vehicle evidence

Begin parked, outdoors for any engine-running stage, with parking brake applied
and a secure installation. Initial work requires no road test, wide-open throttle,
actuator test, coding, security access or ECU/TCU flashing.

1. **Keep this dashboard disconnected initially.** With a trusted VAG scan tool,
   save the complete Auto-Scan and original DTCs. Record address 01 Engine and
   02 Transmission identification: hardware/software part numbers, software
   versions, component, ASAM/ODX/ROD identity and calibration identifiers if
   exposed. Confirm the actual DNUC/Simos variant; do not infer it from model year
   or the name `U-DNU.TRX`. Keep VIN-bearing originals private.
2. With VCDS, use Auto-Scan -> Auto Detect (CAN Only) -> Start -> Save. Then
   Select -> 01 Engine -> Advanced Measuring Values -> select the exact named
   quantity -> Log -> Start, and Stop/Save at the end. Repeat separately for
   02 Transmission where needed. Retain selected channel names and units.
   Equivalent tools must export controller identity and timestamped readings.
   See [Auto-Scan](https://www.ross-tech.com/vcds/tour/autoscan.php),
   [measuring values](https://www.ross-tech.com/vcds/tour/adv-meas-blocks.html)
   and [logging](https://www.ross-tech.com/vcds/tour/logging.html).
3. Through a suitable breakout, use an independent CAN logger in verified
   listen-only mode alongside the scan tool. Capture the bus rate, direction,
   ID, standard/extended flag, RTR flag, actual DLC, payload and timestamps.
   Start before the diagnostic session; stop after it closes. Raw captures must
   include requests, responses, flow control, negatives and gaps. Scan-tool CSV
   alone cannot establish wire identifiers, scaling or framing.
4. Capture cold ignition-on/engine-off, normal warm-up and stable idle, plus
   ordinary ignition-off/sleep and wake. Record physical event times, ambient
   conditions and the scan tool's update interval. Do not manufacture overheating,
   low oil pressure or lean operation to test dashboard alarms.
5. For the four generic Mode 01 decoders, establish support via capability pages
   00/20/40 and retain the actual corresponding replies. Temperature PIDs 05
   (coolant), 0F (intake) and 5C (oil) use `A - 40` C; PID42 ECU voltage uses
   `(256*A + B)/1000` V. Match the exact response source and payload width.
   Compare same-source/same-time readings within documented source resolution
   and timing uncertainty. Battery-terminal voltage is a separate measurement,
   not automatically identical to ECU supply voltage.
6. Keep DSG temperature, boost, measured lambda and EGT unavailable until their
   exact source, DID/PID, units and scaling are established for this ECU/TCU.
   Do not substitute commanded lambda or catalyst temperature. Gauge boost
   additionally needs a qualified absolute pressure and barometric reference.
7. Only after the power/termination/baud and adapter gates pass, attach the
   **unqualified** dashboard for installation/recovery observation, with USB
   disconnected. Confirm no extra termination, correct OBD 6/14 mapping and
   switched regulated supply. The Longan kit and module documentation disagree
   on default termination; inspect the actual PCB. Never measure resistance
   on a powered bus or assume a sleeping vehicle has no voltage present.
8. Repeat parked startup/shutdown and normal wake/sleep, checking bus errors,
   reset/retry state and supply current. OBD power can remain present after
   ignition-off: the current hardware design does not establish automatic
   ignition-controlled shutdown. Verify actual switch-off current and bus sleep.
   Save an after-test scan and compare DTCs; preserve new faults for diagnosis.

The four generic formulas already have exhaustive host input coverage and three
upstream source checks in the [transport record](../vehicle/diagnostic-transport.md).
Those facts do not prove the car advertises them. The
[CANChecked evidence record](../vehicle/canchecked-dnu-evidence.md) explains why
the supplied opaque profile cannot enable manufacturer values by itself.

## 7. What closes acceptance

Keep `commit.txt`, feature/compiler record, ELF SHA-256, exact ELF, board photos,
power/clock measurements, complete RTT, parsed telemetry, CAN/UART traces,
private ECU scan, scan-tool CSV and observed failures together. Record every
step as PASS, FAIL or NOT TESTED; an unavailable instrument means NOT TESTED.

Bench passes qualify only the tested image, hardware and conditions. Actual
dashboard readings require the reviewed qualified profile described in step 5,
followed by captured request/reply and independent value comparisons. Until
then, the valid result on this release is a working UI with unavailable vehicle
values. Physical FPS, fault recovery, flash power-loss behavior and installed
ECU support remain separate results in the
[acceptance matrix](overhaul-2026-10-11.md).
