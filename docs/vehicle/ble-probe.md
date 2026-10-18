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
4. Press **X** to pause and read the selected advertisement without connecting.
5. **Click the encoder** to connect to that selected address and inspect GATT.
   During inspection, rotate to read pages as records arrive. No peer is connected
   automatically. X on a detail page provides the same Inspect action.
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

| Control | Device list | Detail pages |
| --- | --- | --- |
| Encoder rotation | Previous/next device, clamped at ends | Previous/next page |
| Encoder click | Inspect selected device | Inspect again when idle |
| A / B | Previous/next device | Previous/next page |
| X | Pause and open advertisement evidence | Inspect selected device |
| Y | Pause/resume scan | Cancel inspection and return to scan |
| Hold Y, 2 seconds | Clear observations and restart scan | Clear observations and restart scan |

Clicks during stopping, connecting, discovering or disconnecting are ignored;
Y remains available. A long Y hold includes the ordinary Y press before recovery.
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
identities when the catalogue is full, not unique omitted devices. `bad` counts
malformed HCI advertisement reports; malformed AD fields are also identified on
their detail pages. Limits and malformed discovery are never labelled complete.
If the underlying client's 24-service capacity is exceeded, it returns a capacity
error before publishing its service list. Valid characteristic rows already
published before a later error remain available. UUIDs use conventional displayed
order; packet hex retains wire order. Non-ASCII/control bytes display as `?` with
their original bytes retained in hex.

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

| Advertisement decoding | GATT inventory | Standard information |
| --- | --- | --- |
| ![Synthetic packet decoding](../images/ble-probe/details-1-synthetic.png) | ![Synthetic GATT inventory](../images/ble-probe/details-2-synthetic.png) | ![Synthetic Device Information](../images/ble-probe/details-3-synthetic.png) |

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
CI checks stock/280 builds, strict nightly Clippy, Python gates and security-feature
exclusion. Software tests and conservative stack estimates do not qualify RF
interoperability, physical stack high-water, electrical margins or car data.

The checked 2026-10-19 build (`nightly-2026-10-18`, CPU280/SPI70, bounded info
logging) uses **372720 bytes of static RAM** and leaves **151568 bytes for the
main stack and headroom**. Static RAM includes both 153600-byte framebuffers and
the 32768-byte core-1 stack. The largest emitted function frame is 4992 bytes;
the conservative named-path budgets include additional child and interrupt
reserves. These are linked-image/source-review results, not physical stack
high-water measurements. The frozen ELF, UF2, compiler options and source hashes
are recorded together by the checked wrapper.

References checked against actual locked sources:
[Pico 2 W datasheet](https://datasheets.raspberrypi.com/picow/pico-2-w-datasheet.pdf),
[PIM715](https://shop.pimoroni.com/products/pico-display-pack-2-8),
[Adafruit encoder pins](https://learn.adafruit.com/adafruit-i2c-qt-rotary-encoder/pinouts),
[board audit](../hardware/pico2w-board-audit.md),
[Trouble local corrections](../../firmware/vendor/trouble-host/LOCAL_PATCHES.md),
[utility ownership](../../firmware/ble-probe/src/tasks.rs),
[radio lifecycle](../../firmware/ble-probe/src/radio.rs).
