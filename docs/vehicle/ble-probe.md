# Standalone BLE discovery utility

`dashboard-ble-probe` is a separate, maintained firmware image for the Pico 2 W,
Pimoroni PIM715 and Adafruit 5880 encoder. It replaces the dashboard while installed
and runs without Studio, a phone or a computer. Its purpose is to collect
photographable adapter evidence before attempting protocol integration.

## Use it in the car

1. Power the Pico through USB. The display starts at full brightness and scans.
2. Close the phone app's adapter connection if the adapter stops advertising while
   connected. The utility cannot list a device that is not advertising.
3. Rotate the encoder to select a peer. The list keeps first-seen order so changing
   signal strength cannot move the selection. Four rows fit on each screen.
4. Press **X** for **Advert only**: pause and read a fresh snapshot of the selected
   advertisement without connecting. The heading is **ADVERTISEMENT**.
5. **Click the encoder** to connect to that selected address and inspect GATT.
   During inspection, rotate to read pages as records arrive. No peer is connected
   automatically. X on a detail page provides the same **Inspect GATT** action.
   The heading changes to **ADVERTISEMENT + GATT**. Without an encoder, press
   **X → wait for Paused → X again**.
6. Wait for **Paused**, then photograph the address/type, advertisement bytes,
   service UUIDs, characteristic handles/properties and Device Information pages.
   A partial/error result is useful evidence too. Include its error and page count.
7. Press **Y** to return to scanning. Hold Y for two seconds to clear the catalogue
   and restart. Power cycling also clears all observations.

An advertisement older than 30 seconds cannot start an inspection: return with Y
and let scanning refresh it. Private addresses can change; an address/name is not
authenticated device identity. RSSI is signal strength, not measured distance.
Names and firmware/serial strings are untrusted peer claims. Redact unrelated
devices and private identifiers before sharing photographs publicly.

## Controls and evidence lifetime

The footer describes the action available **on the current screen**. `Click`
means pressing the encoder knob; `Turn` means rotating it. `Advert` means the
advertisement and scan-response packets already collected during scanning.
`GATT` is the additional service/characteristic inventory obtained by connecting.
The footer occupies three fixed rows below the evidence; telemetry has its own
two rows at the top. Longer explanations are here so evidence stays readable.

| Control | Device list | Evidence pages |
| --- | --- | --- |
| Encoder rotation | Select the previous/next device, clamped at the ends; does not connect | Scroll previous/next evidence page, clamped at the ends; never changes the selected device |
| Encoder click | **Inspect GATT:** clear the old inspection, stop scanning, connect to the selected device, discover GATT and allowed standard information, then disconnect | **Inspect GATT / Repeat GATT:** start the same fresh inspection for this device, reset to the first page and replace previous GATT records |
| A / B tap | Select previous/next device; the onboard alternative to turning the encoder | Previous/next evidence page |
| X tap | **Advert only (fresh):** freeze the selected advertisement/scan response, open its first page and clear the old GATT result; no connection is requested | **Inspect GATT / Repeat GATT:** identical to encoder click, including replacing old GATT records |
| Y tap | Pause/resume scanning without clearing the device list | Cancel a pending inspection, finish radio cleanup and return to scanning; keeps the device list |
| Hold Y, 2 seconds | Clear the device list and GATT result, then restart scanning; a terminal fault requests reset | Same recovery action |

### Why the number of pages changes

**ADVERTISEMENT** contains the captured packets and decoded fields, followed by
**GATT not inspected** and a connection hint. This is an unrequested inspection,
not a failed one and not evidence that the device has zero services.
**ADVERTISEMENT + GATT** includes those packets plus the inspection's services,
characteristics and allowed Device Information values. Its status distinguishes
waiting for cleanup, connecting, discovery in progress, enumeration ended and
partial results. A timeout can leave zero or some records; it remains a partial
inspection with the error visible.

The owner's TV example used three advertisement pages and nine after finding
three services and twelve characteristics. Those counts are examples, not fixed
limits or adapter qualification. More records produce more pages as they arrive.

There is **one temporary GATT result, not a per-device archive**. Photograph all
needed pages before leaving them. Opening a fresh advertisement view with X from
the list, starting another inspection, holding Y or restarting clears/replaces
that result. After **Y → X**, fewer pages are therefore expected: X has opened a
new advertisement-only view. X never restores a previous inspection or connects
from the list. Use another X after Paused, or click the encoder, to inspect again.

Inspection clicks during stopping, connecting, discovering or disconnecting are
ignored; the footer says **GATT busy; please wait** and Y remains available.
Inspection is also unavailable during a terminal fault or with no selected peer.
If the selected advertisement is older than 30 seconds, the footer says stale:
return to the list with Y, resume scanning if paused, and wait for a fresh report
before inspecting. Reading existing evidence remains available.
A long Y hold includes the ordinary Y press before recovery. Ordinary recovery
clears the catalogue/GATT evidence, including `full` and per-device report counts.
It does not reset uptime FPS or the global `lost`/`HCIbad` counters; reboot clears
all RAM-only state.
Turning in details never changes the peer being inspected. A missing encoder is
shown explicitly; onboard controls remain usable. The utility has a fixed
**320×240 landscape layout**, full backlight and its own controls, independent of
dashboard configuration and brightness preferences.

The RGB LED is green while scanning, amber while paused, blue during startup and
inspection transitions, and red for a terminal fault. A terminal transport,
display or stalled-owner fault stops watchdog feeding: both cores/peripherals
restart after approximately two seconds. That reset loses the RAM-only evidence.
Ordinary connection/discovery timeouts retain partial records and wait for input.
The LED describes the tool's state, not vehicle health.

## Always-visible telemetry

Two fixed rows beneath the title remain visible while scanning, paused,
connecting, inspecting or showing faults. They do not replace device evidence or
change the controls. `K` means KiB, rounded up for these compact labels.

| Field | Meaning |
| --- | --- |
| FPS | Successful HAL display transfers divided by actual elapsed seconds in the latest window of at least one second; not an inverse render-time estimate |
| AVG | Total successful transfers divided by total elapsed time since probe UI startup; weighted by time, including pauses and idle windows |
| W C0 / C1 | Application task-poll work as a percentage of each core's actual wall-time window, to 0.1%; `W` means work, not whole-core utilization |
| Static | Linked static allocation / 512 KiB linker arena, including both framebuffers and the reserved 32 KiB core-1 stack |
| SP0 / SP1 | Greatest stack depth observed at task-poll boundaries / respective stack region; not a scan of stack memory or a physical high-water measurement |

FPS is **software completion, not LCD scanout**. Both rates initially show `--`
until the first complete window. Navigating, pausing scanning or clearing peers
does not reset the meter; reboot does. A paused radio still allows screen updates.
The existing 50 ms render cadence targets 20 updates/s; load can reduce that rate.
Failed transfers never add a frame, and their buffers remain quarantined.

Work uses the locked HAL's 1 MHz monotonic TIMER0 clock. Each top-level task is
wrapped once, including all radio child futures, and only time inside `poll` is
counted. Waiting on DMA, timers or another task is excluded. IRQ time inside a
poll is included; scheduler/IRQ work outside polls and the observer's recording
overhead are excluded. One-microsecond resolution limits tiny-poll accuracy.
Before a full window, after invalid timing, or more than two seconds after the
latest observation, work displays `--%`, never an invented idle value.

The two cores publish bounded observations under one short critical section;
snapshot time and the completion counter are captured together. No rendering,
hardware operation or await occurs inside that lock. The current UI poll is
visible only after it returns. Stack samples can miss deeper calls between poll
boundaries. Static memory is not total live usage: do not add the framebuffers or
core-1 stack again. The RP2350 has 520 KiB physically; this linker leaves its two
4 KiB scratch banks outside the displayed 512 KiB arena.

See [dashboard/probe measurement audit](../performance/diagnostic-measurements.md)
for differences between the two tools and remaining device checks.

## What it collects and sends

| Item | Bound / behavior |
| --- | --- |
| Catalogue | 48 address/type identities; no eviction; excess **reports** counted as `full` |
| Scan | Legacy 1M active scan, 100 ms interval / 50 ms window; duplicate reports enabled |
| Raw advertising | Latest 31-byte advertisement and separate 31-byte scan response per peer |
| Decoded advertising | Name, flags, advertised UUIDs, TX power, manufacturer ID; all other AD bytes remain in hex |
| Primary services | Up to 24; UUID and handle range |
| Characteristics | Up to 64 total; UUID, service, declaration/value handles, raw and decoded property flags |
| Standard information | Manufacturer, model, serial, hardware, firmware, software, System ID and PnP ID within Device Information service `180A` |
| Values | Up to 64 bytes each, readable-property required; ASCII plus complete retained hex; full buffer marked truncated conservatively |
| Deadlines | Scan start 3 s; stop/terminal disconnect 5 s; connect 8 s; each ATT operation 3 s; entire GATT inspection 30 s |

Active scanning sends BLE scan requests to obtain scan responses. Manual inspection
sends connection, MTU, primary-service/characteristic discovery and allowlisted
read requests, then disconnects. It **does not send OBD commands, characteristic
writes, subscription/CCCD writes, pairing, bonding or vendor authentication**.
Protected fields remain marked `Protected`; the tool does not try to unlock them.
The local `discovery-no-security` feature prevents accidental Cargo feature
unification from enabling pairing.

This is not an exhaustive Bluetooth inventory: Classic, extended-only advertising,
hidden/nonadvertising, already-connected and out-of-range devices can be absent.
Directed advertisements may target another central and fail to connect here.
Secondary services/descriptors and arbitrary proprietary values are not enumerated.
Some adapters expose their useful protocol only after authentication, so a successful
inspection does not establish NGM/OBDeleven interoperability or DNUC measurements.

`lost` counts dropped host report-queue entries; `full` counts reports for new
identities when the catalogue is full, not unique omitted devices. `HCIbad` counts
scan report batches rejected by the HCI iterator, once at its first bad entry.
Malformed AD fields are separately identified on their detail pages.
Limits and malformed discovery are never labelled complete.
If the underlying client's 24-service capacity is exceeded, it returns a capacity
error before publishing its service list. Valid characteristic rows already
published before a later error remain available. UUIDs use conventional displayed
order; packet hex retains wire order. Non-ASCII/control bytes display as `?` with
their original bytes retained in hex.

## Understanding failures

An inspection failure moves to the first evidence page and retains the first
cause through disconnect/cleanup. The header may show a later terminal cleanup
fault while the detailed first cause stays intact. A fresh inspection replaces
the old result; there is no automatic retry. Available fields are:

| Field | Meaning and limitation |
| --- | --- |
| GATT FAILURE / stage | Connect, MTU, services, characteristics, Device Information, receive task, connection events or whole inspection |
| Reason and `0xNN` | Exact ATT/HCI status when supplied by the library; absent codes are not invented |
| Stage elapsed | Monotonic operation duration when measured; excludes later publication of retained rows; absent for events without a measured start |
| Query range | The application's requested service/read range, not the remote ATT Error Response handle or internal pagination cursor |
| Last accepted decl/value | Last validated characteristic in that service, not a guess at the failed handle |

Each 3-second ATT deadline covers the **whole operation**, including all pages of
one service's characteristic query. The 30-second deadline covers the entire
inspection after connection. A timeout may follow some successful responses.
Disconnecting can originate locally or remotely; a code-less disconnect does not
identify which side caused it. Some malformed ingress is rejected by the lower
host and closes the connection before the app can see the original decode error.
Such failures may still appear as `disconnected (code unavailable)`.

`HCIbad` and the **GLOBAL HCI SCAN ERRORS / since boot** pages describe all scanned
devices. They retain invalid-size/value counts and the latest failed batch's scan
revision, entry position and age. The actual HCI iterator stops at its first bad
entry: later entries in that batch are not individually counted. Valid preceding
entries remain available. These counters do not diagnose RF quality and cannot
be attributed to the selected TV/adapter. Raw AD-field decoding and rejected GATT
records are separate checks. With no accepted peer, X opens **SCAN DIAGNOSTICS**;
rotation/A/B scroll, Y returns to scanning, and click/X cannot connect without a
peer. Otherwise the global block follows the peer evidence pages.

`InvalidPeer` after reading paused evidence usually means the advertisement is
over 30 seconds old. Y returns to scanning; wait for a new report before clicking.
This rejection sends no connection request. The header does not assume staleness
for every InvalidPeer case: an unexpected connected address is also rejected.

The owner's LG TV photographs establish an intermittent partial GATT result,
not its underlying cause. Low RSSI does not establish why a host parser rejected
a report. Tests reproduced separate malformed-packet panic, tuple-validation,
128-bit UUID parsing and mismatched-response defects; richer on-device evidence
is still needed to relate them to that result.

## Build from the command line

Windows prerequisites are the same pinned Rust, Python 3.11+, MSVC/SDK and
PowerShell 7 tools listed in the [README](../../README.md#requirements-and-first-run).
The checked wrapper uses picotool **2.3.1** from
`target/configurator-tools/bin/picotool.exe`; the existing explicit
`./configurator/Studio.ps1 -Setup` installs it. Serving/building Studio is unnecessary.

From the repository root:

```powershell
# Fresh name for each attempt. Stock CPU150 MHz / SPI37.5 MHz is the default.
./firmware/scripts/Build-BleProbe.ps1 -Name probe-stock-01

# Owner-requested experimental profile: CPU280 MHz / SPI70 MHz / 1.30 V.
./firmware/scripts/Build-BleProbe.ps1 -Name probe-280-01 -Profile 280-70 -Expert
```

`280-70` is the owner's reported working overclock, not a safe electrical rating.
See [clock limitations](../../README.md#experimental-clock-profiles). The radio
uses its own integer PIO divider: at 280 MHz its GSPI clock is about 46.67 MHz,
independent of display SPI70 MHz.

The wrapper prints the dated compiler and options, uses two Cargo jobs, clears
ambient compiler/profile/dashboard overrides in the child process, retains stack
symbols and explicitly passes **`-C target-cpu=cortex-m33`** for
**`thumbv8m.main-none-eabihf`**. It stores commands, sources, resolved clocks, ELF,
UF2 and a separate utility image receipt under
`firmware/target/ble-probe-captures/<name>/`. It checks source drift, radio/HAL
provenance, flash/settings boundaries, fixed buffers, conservative stack reserves
and exact UF2 payload equality. It never accesses or flashes a device. Failed
attempts remain for diagnosis; use a new name after repairing the cause.

For a developer compile only, from `firmware/`:

```text
cargo ble-probe
cargo ble-probe-280-70-1v30
cargo clippy --locked -p dashboard-ble-probe -- -D warnings
```

These aliases build without the capture/admission wrapper. They inherit the
firmware Cargo debug logging filter. Build the probe separately from the
pairing-enabled dashboard; a combined all-features workspace build is deliberately
rejected. Existing `cargo pico2` and default workspace builds still target the
dashboard. Linux compilation is covered by CI; the checked capture wrapper and
physical flashing instructions here target Windows.

## Explicit BOOTSEL flashing

1. Connect the confirmed genuine **Pico 2 W** while holding BOOTSEL. Capacity is
   owner-confirmed from the board specification; picotool's content-derived flash
   estimate does not independently measure it. The image retains the 2 MiB layout
   and leaves the final two settings sectors untouched.
2. Select the exact USB serial (Windows Device Manager hardware instance path,
   `USB\VID_2E8A&PID_000F\<serial>`). Check `picotool info -d --debug` and
   `picotool partition info` with that selector: RP2350 A2/A3/A4, ARM available,
   secure boot off, both debug enables on, no partition table and secure ARM UF2
   allowed. Stop on ambiguous devices, unexpected state or failed commands.
3. Recheck the frozen image against current source before programming:

```powershell
$capture = './firmware/target/ble-probe-captures/probe-280-01'
python ./firmware/scripts/ble_probe_image.py "$capture/ble-probe.elf" "$capture/ble-probe.uf2" "$capture/manifest.json" --result "$capture/preflash.json"
if ($LASTEXITCODE -ne 0) { throw 'Image checks failed' }
$tool = './target/configurator-tools/bin/picotool.exe'
$manifest = Get-Content "$capture/manifest.json" -Raw | ConvertFrom-Json
if ((Get-FileHash $tool -Algorithm SHA256).Hash.ToLowerInvariant() -ne $manifest.converter_sha256) {
    throw 'Picotool changed since the checked build'
}
$selector = @('--vid', '0x2e8a', '--pid', '0x000f', '--ser', '<your-serial>')
& $tool info -d --debug @selector
if ($LASTEXITCODE -ne 0) { throw 'Device inspection failed' }
& $tool partition info @selector
if ($LASTEXITCODE -ne 0) { throw 'Partition inspection failed' }
```

4. After the device checks above, execute these **separate stages**. The
   `--ignore-partitions` load option is appropriate only after confirming the
   supported unpartitioned device state. Recheck identity before verification
   and reboot if the USB device was disconnected or changed.

```powershell
& $tool load -u --ignore-partitions "$capture/ble-probe.elf" -t elf @selector
if ($LASTEXITCODE -ne 0) { throw 'Transfer failed; do not reboot or retry automatically' }
& $tool verify "$capture/ble-probe.elf" -t elf @selector
if ($LASTEXITCODE -ne 0) { throw 'Verification failed; do not reboot' }
& $tool reboot -a -c arm @selector
if ($LASTEXITCODE -ne 0) { throw 'Reboot outcome needs inspection' }
```

Check the command output as well as exit codes: transfer and every verification
range must complete. Interruption leaves an unknown write outcome and must not
automatically resume. BOOTSEL can restore the normal dashboard later using its
own checked firmware artifact. A successful flash/readback is separate from
physical radio, encoder, display and overclock qualification.

## Rendering gallery

Actual shared RGB565 renderer output, with **synthetic peers and values**:

| Scanning | Paused photo view | Advertisement bytes |
| --- | --- | --- |
| ![Synthetic scan list](../images/ble-probe/scan-synthetic.png) | ![Synthetic paused list](../images/ble-probe/paused-synthetic.png) | ![Synthetic advertisement](../images/ble-probe/details-0-synthetic.png) |

| Fresh advertisement view | No GATT inspection requested | Explicit inspection running |
| --- | --- | --- |
| ![X opens advertisement-only evidence](../images/ble-probe/advertisement-synthetic.png) | ![Not-inspected status and connection hint](../images/ble-probe/not-inspected-synthetic.png) | ![GATT discovery with busy hints and cancellation](../images/ble-probe/inspecting-synthetic.png) |

| Advertisement decoding | GATT inventory | Standard information |
| --- | --- | --- |
| ![Synthetic packet decoding](../images/ble-probe/details-1-synthetic.png) | ![Synthetic GATT inventory](../images/ble-probe/details-2-synthetic.png) | ![Synthetic Device Information](../images/ble-probe/details-3-synthetic.png) |

| Failure stage and retained context | Global scan errors without a peer |
| --- | --- |
| ![Synthetic ATT failure, not a TV diagnosis](../images/ble-probe/failure-synthetic.png) | ![Synthetic global HCI parser diagnostics](../images/ble-probe/scan-errors-synthetic.png) |

Regenerate the lossless RGB captures from `host/`:

```text
cargo run --locked --example ble_probe_gallery -- target/ble-probe-gallery
```

The generator writes PPM files. To update the PNG documentation copies, use Python
with Pillow installed, from the same `host/` directory:

```python
from pathlib import Path
from PIL import Image

for source in Path("target/ble-probe-gallery").glob("*.ppm"):
    with Image.open(source) as picture:
        picture.save(Path("../docs/images/ble-probe") / source.with_suffix(".png").name)
```

## Implementation and verification

Core 0 owns CYW43439/PIO0 SM0/DMA1, SPI0/DMA0, I2C0, four buttons, backlight,
RGB and watchdog. Core 1 owns the bounded catalogue, inspection state and renderer.
Only owned messages and exclusive framebuffer references cross cores through
critical-section channels. No runtime flash writes, saved peers, ADC/VSYS reads
or extra framebuffer exist. Rendering targets 20 software updates/s; this is
neither measured device FPS nor panel scanout.

The two 153600-byte buffers circulate through FREE/render/READY/transfer/FREE.
Transfer failure retains the buffer until watchdog reset; SPI/PIO/I2C futures
owning hardware are not timeout-cancelled. Separate input/status/report queues
and bounded drain counts prevent an advertising flood from starving controls.
Request generations reject late results; scan stop and terminal connection
cleanup precede any new operation. ATT timeout abandons the entire connection.
Host-runner failure retains the CYW owner while recovery resets the device.

The shared progress monitor checks input/encoder/render/transfer work; radio
liveness observes actual bus boundaries, with a separate host-start deadline.
The checked build retains nonblocking RTT stage/revision/timing logs and filters
upstream info/debug traffic. RTT needs a debug probe to collect; no USB serial
logger or on-device persistent log was added.

Tests cover malformed advertisement/GATT bytes, handle/offset overflow, standard
UUID aliases, capacities, stable selection, cancellation, stale results, missing
encoder progress, radio stalls, time-based animation, clipping and pagination.
Telemetry tests also cover suspended/cancelled polls, propagated errors, irregular
windows, counter wrap, clock regression, idle frames, stale work, invalid values
and pixel isolation from list/details/footer regions.
Failure regressions cover first-cause retention, late completion/cancellation,
repeat/recovery, valid partial rows, terminal handles, exact error-code mapping
and protected/unavailable values. Tests use the production report adapter with
the locked HCI decoder, including all 256 RSSI byte patterns, mixed valid/bad
batches and both parse-error kinds. Vendor wire tests exercise valid multi-page
discovery, normal end-of-list responses, malformed tuples and response matching.
A deterministic 66,560-input ATT decoder sweep checks every opcode at lengths
0 through 64 with four seeded byte patterns and exhausts accepted iterators.
Explicit valid request families, empty values and 16/128-bit UUID cases check
that short-packet guards preserve valid traffic. This is bounded regression
coverage, not exhaustive enumeration of every possible packet or handler.
Maximum diagnostic values and every failure label are checked for text bounds;
pixel tests keep evidence separate from telemetry and the control footer.
CI checks stock/280 builds, strict nightly Clippy, Python gates and security-feature
exclusion. Software tests and conservative stack estimates do not qualify RF
interoperability, physical stack high-water, electrical margins or car data.

The checked 2026-10-19 build (`nightly-2026-10-18`, CPU280/SPI70, bounded info
logging) uses **373296 bytes of static RAM** and leaves **150992 bytes for the
main stack and headroom**. Static RAM includes both 153600-byte framebuffers and
the 32768-byte core-1 stack. The largest emitted function frame is 4992 bytes;
the aggregate emitted frames total 18992 bytes against the unchanged 20480-byte
limit. The aggregate is a regression check, not simultaneous stack use;
the conservative named-path budgets include additional child and interrupt
reserves. These are linked-image/source-review results, not physical stack
high-water measurements. The frozen ELF, UF2, compiler options and source hashes
are recorded together by the checked wrapper.

The permanent telemetry added 440 static bytes against its preceding utility
image; the failure diagnostics add another 136 bytes against that telemetry build.
A concrete pinned poll wrapper keeps each task future in one allocation;
bounded integer formatting avoids adding general float-formatting stack frames.
Synchronous ATT packet preparation keeps its temporary encoder out of the request
future. The current 280/70 image passed the unchanged practical image checks;
CI also checks the stock profile.

References checked against actual locked sources:
[Pico 2 W datasheet](https://datasheets.raspberrypi.com/picow/pico-2-w-datasheet.pdf),
[PIM715](https://shop.pimoroni.com/products/pico-display-pack-2-8),
[Adafruit encoder pins](https://learn.adafruit.com/adafruit-i2c-qt-rotary-encoder/pinouts),
[board audit](../hardware/pico2w-board-audit.md),
[Trouble local corrections](../../firmware/vendor/trouble-host/LOCAL_PATCHES.md),
[utility ownership](../../firmware/ble-probe/src/tasks.rs),
[radio lifecycle](../../firmware/ble-probe/src/radio.rs).
