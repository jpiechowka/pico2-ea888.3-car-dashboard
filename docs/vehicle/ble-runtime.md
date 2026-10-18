# USB-powered BLE prototype

Implementation contract, 2026-10-18. The supported board remains Raspberry Pi
Pico 2 W / RP2350 Cortex-M33 with PIM715 and the retained Adafruit 5880 encoder.
The selected [hardware arrangement](../../hardware/README.md) powers the dashboard
from USB and removes the proposed Longan/Pololu/protection-board assembly.

**No live DNUC measurements are enabled.** MC+ and CX profiles initialize and
identify an adapter only. They send no diagnostic ECU requests, capability scans,
security unlocks, writes or actuator commands. Adapter initialization is not an
electrical listen-only certification. No physical radio, vehicle, power, pairing,
sleep or display-timing test is established by this software prototype.

## Entry and profiles

`ble-obd` selects the firmware transport. Studio's Vehicle build route uses it;
`demo`, `ble-obd` and historical `longan-rx12` cannot be combined. A no-backend
build remains valid and displays unavailable vehicle channels. BLE settings
default to disabled, including migrated projects. Enabling requires an explicit
public or static-random peer address; rotating private peer addresses are not
supported. Matching a name is insufficient.

The shared schema bounds connect/discovery timeout to 5–30 s, response timeout
to 250–5000 ms and the inter-command gap to 10–200 ms. Defaults are 10 s, 1500 ms and 20 ms.
These are software budgets, not measurements of adapter or ECU latency.

| Profile | Service | Notify / write | Admitted traffic |
| --- | --- | --- | --- |
| vLinker MC+ | `18F0` | `2AF0` / `2AF1` | Allowlisted initialization and `ATI` |
| OBDLink CX | `FFF0` | `FFF1` / `FFF2` | Same adapter-only traffic, with encryption required |
| Test peer | Private 128-bit service below | Private RX / TX | Initialization, identity and synthetic coolant fixture |

The test service bytes in HCI little-endian order are
`01 00 C8 0E 88 EA 52 A8 2F 43 C2 AE 00 00 88 EA`.
The RX/TX characteristic UUIDs replace byte index 12 with 1/2. The firmware must
discover this service before any synthetic request; selecting MC+ or CX never
selects synthetic operation. A spoofed peer is not ECU qualification.

The shared [ELM session](ble-protocol.md) admits a fixed initialization sequence:
`ATE0 ATL0 ATS1 ATH1 ATD1 ATM0 ATSP6 ATCEA ATCAF0 ATCFC1 ATSH7E0 ATCRA7E8`.
Each command must complete with the expected response and prompt before the next.
There is no free-text AT or diagnostic command entry point. `ATI` identifies the
adapter; an `OK` or model string does not prove command fidelity or ECU support.

After initialization, real profiles repeat identity after a 2 s gap. TestPeer uses
synthetic coolant requests after at least 100 ms, also respecting the configured
gap. The separate shared fair scheduler supports future qualified channels, but
is not a live DNUC polling plan. Neither path publishes `VehicleData`: firmware
acquisition remains unknown, and the header reports **BLE ONLY** or **TEST PEER**.
The Studio transport simulator is synthetic diagnostics, separate from its
synthetic dashboard sample controls. No connection failure switches to demo data.

## Resources and ownership

| Resource | Owner and bound |
| --- | --- |
| CYW power/data/CS/clock | Core 0 radio task, GP23/24/25/29 |
| Radio SPI engine | PIO0 SM0 / PIO IRQ0, DMA1; display remains SPI0/DMA0 |
| Trouble host, GATT and ELM session | Core 0; one connection, one transaction in flight |
| Default ATT packet pool | Eight 251-byte packet buffers |
| Client notifications | Four 244-byte values; observable overflow/loss |
| CYW HCI staging | Four 272-byte RX and four 272-byte TX buffers in Bluetooth-only mode |
| ELM parsing | 64-byte line, 64-byte diagnostic/identity result, 1024-byte transaction input |
| Cross-core diagnostics | Compact mutex-protected snapshot; no credentials or payload log |
| Display | Existing two 153600-byte RGB565 framebuffers; no third buffer |

Sizes in this table are payload bounds, not complete allocation totals. Task
futures, packet metadata, security state and call stacks must be included in
[linked image admission](../performance/memory-layout.md). Host `size_of` and
Criterion results in [protocol evidence](ble-protocol.md) do not establish ARM
stack high-water, radio throughput or panel FPS. Stock and 280 MHz images require
their own exact source/configuration-bound checks.

The integer PIO divider is `ceil(clk_sys / 100 MHz)` because the GSPI clock uses
two PIO cycles. This bounds the clock to 50 MHz without short fractional-divider
cycles. Actual divider values use the selected clock tree, not the profile label.
No ADC task samples shared GP29 during radio use. The onboard W LED and SMPS/VBUS
WL_GPIO functions remain separate from PIM715's GP26/27/28 RGB LED.

Pinned integration uses vendored CYW43 0.7.0 with the reviewed HCI compatibility
changes and Trouble 0.8.0. The firmware lockfile, vendor provenance manifests and
radio-binary manifest identify exact inputs. Helper and target-capture source
identity include the complete `firmware/vendor/` subtree, including radio binaries
and nested files. This does not rely on an untracked local firmware download.
The [radio dependency review](../dependencies/radio-review-2026-10-18.md) records
the retained buffer bounds, compatibility constraints and reviewed repairs.

## Stream, deadlines and connection lifetime

GATT writes use at most 20 bytes, covering the minimum ATT MTU 23. Discovery
requires Notify and prefers Write with response. Only MC+ may use Write without
response when that is its advertised write capability; CX and TestPeer require
acknowledged writes. A failed write never triggers a write-type fallback. Both
modes still require the expected ELM reply and prompt within the same deadline.
Notifications can arrive before the write response; they remain in a
bounded queue until `sent(token)` acknowledges the complete command. A notification
is an arbitrary fragment, never an assumed complete reply. CR/LF, echo, CAN header,
DLC, ISO-TP length/sequence and final prompt are parsed explicitly.

The session checks connection epoch, notification sequence and command generation.
Overflow, lost bytes, malformed/foreign replies, timeout or clock regression
invalidate the session. It cannot time out a command then accept a late reply as
that command's success. NRC pending never extends the original deadline. Idle
notifications are consumed; unsolicited non-CR/LF bytes fail the stream. A bounded
queue drain precedes each new command. Loss before the ELM parser must also fail
closed at the HCI/ACL/GATT boundary.

Each connection attempt has a checked new epoch. Reconnection requires a fresh
GATT client, subscription and initialization. The lifecycle contract requires
actual controller disconnection and settled create/cancel procedures before a
new attempt. A request to disconnect is not completion. Canceled create attempts
must quarantine any late successful connection, and the host runner must remain
alive to process cancellation while terminal status is reported. Cleanup has a 5 s
bound; a stalled host/controller must request watchdog recovery even when the
radio bus still makes progress. Runtime and vendor regression gates must cover
these races together before physical acceptance.

Automatic reconnect starts at 1 s and doubles to a 10 s cap; successful adapter
readiness resets it. Disabled automatic reconnect ends in Fault after cleanup.
Link state is bounded: Disabled, Starting, Connecting, Securing, Discovering,
Identifying, AdapterReady, SyntheticReady, Backoff, Maintenance or Fault. Snapshot
counts, epoch and last software transaction latency describe transport activity,
not vehicle freshness. The ELM wire has no transaction identifier: local tokens
cannot prove that an identical response is new or flush an adapter's hidden FIFO.

## Security and bonds

CX requires an encrypted link before GATT adapter traffic. Encryption is checked
again before each write chunk and received notification. The linked stack uses
LE Secure Connections; legacy pairing is not enabled as a fallback. Controller
`LeRand` seeds the host security RNG during startup. Pairing failure, lost bond,
unsupported interactive pairing or encryption loss terminates the session.

The host has NoInputNoOutput capabilities and one volatile bond slot. Just Works
encryption does not authenticate against an active MITM. Bond secrets are absent
from portable projects and logs and are not persisted in settings flash. Pico
restart loses them, so CX rebonding may require its power-on pairing window.
Physical CX compatibility, current firmware pairing behavior and repeated restart
remain test gates. The MC+ profile does not require encryption; its current-unit
access control is unqualified. A Classic Bluetooth PIN is not evidence of BLE
security.

## Flash, cancellation and watchdog

The radio task retains the bus owner rather than canceling a DMA operation.
Its maintenance hook runs between completed CYW bus operations. Park requires
CS high, DMA1 idle, a self chain target and the required interrupt context. The
settings driver independently checks radio idle and all DMA channels before a
synchronous flash operation. Radio is an additional participant in the existing
acquisition/input/display flash coordination; both framebuffers retain their
ownership rules.

Park changes a checked generation so connections and sessions spanning the pause
are invalidated, including a rejected save. The bus stays parked through the
maintenance command and acknowledges resume before new bus work. This is a bus
barrier, not a claim that the dongle or controller FIFO was flushed. Tests must
cover pending connects, writes, notifications, security and teardown at the pause.

Even disabled BLE retains power/CS ownership and participates in the barrier.
Enabled startup gets a 10 s software progress budget; after startup, 500 ms without
a completed radio boundary latches failure. The existing maintenance watchdog
grant bounds the pause and withholds ordinary feeds. A stalled DMA owner is
recovered by hardware reset, not by canceling the future and reusing its channel.
Warm-reset electrical states, actual flash pauses and watchdog reset behavior
remain physical checks.

## Verification boundaries

- Shared host tests cover fragmented replies, every frame split, loss, overflow,
  echo/prompt ordering, negative replies, epochs, time reversal, exhaustion,
  request lengths/units and bounded scheduling. These are synthetic fixtures.
- Vendor and runtime gates must cover create cancellation on both sides of the
  controller event, disconnected-state semantics, full ingress queues, notification
  arrival before CCCD/write responses, security transitions and terminal cleanup.
  Source inspection alone does not prove the composed async execution.
- Native/Wasm Studio checks and actual browser interactions establish editor and
  simulator behavior. ARM compile/link/image checks establish software/resource
  admission for the exact artifact. Neither executes a radio or ECU.
- Future authorized no-car bench tests need a real synthetic BLE peer, USB-powered
  Pico/display/encoder, observed negotiated MTU/interval/security, forced loss,
  reconnect, flash pause and reset cases. Do not substitute host timings for them.
- No adapter purchase, physical flashing or vehicle communication was performed.
  DNUC request/address/decoder and
  alarm qualification remains governed by [sample quality](sample-quality.md).

Automatic adapter sleep and vehicle-off behavior remain unqualified. The current
2 s identity loop is ongoing adapter traffic and may prevent inactivity sleep.
Turning off dashboard USB does not remove dongle power. Read-only commands do
not certify vehicle-port fault safety. Retain explicit physical disconnection for
long parking. Actual MC+ characteristic properties and either adapter's timing
remain physical qualification gates; the referenced MC+ integration uses Write
without response, while CX explicitly documents both write types.

## Source record

Sources reopened 2026-10-18, plus the exact repository/locked sources:

- [Pico 2 W datasheet](https://datasheets.raspberrypi.com/picow/pico-2-w-datasheet.pdf)
  and [board audit](../hardware/pico2w-board-audit.md): W pins and power boundaries.
- [OBDLink CX developer notes](https://support.obdlink.com/support/solutions/articles/43000746707-obdlink-cx-adapter-notes):
  UUIDs, encryption, bonding window, MTU 247, write responses and no QueuedWrites.
- [MC+ embedded configuration](https://raw.githubusercontent.com/eigger/espcomponents/master/packages/display/colorado/colorado-ble-elm327.yaml)
  and [component source](https://raw.githubusercontent.com/eigger/espcomponents/master/components/ble_elm327/ble_elm327.cpp):
  profile UUID evidence, not DNUC or exact adapter revision qualification.
- [Firmware owner](../../firmware/pico2/src/tasks/ble.rs),
  [maintenance gate](../../firmware/pico2/src/tasks/maintenance.rs),
  [recovery supervisor](../../firmware/pico2/src/tasks/recovery.rs),
  [shared session](../../crates/dashboard-core/src/ble/session.rs) and
  [configuration bounds](../../crates/dashboard-config/src/ble.rs): executable contracts.
- [CYW source provenance](../../firmware/vendor/cyw43-provenance.json) and
  [Trouble source provenance](../../firmware/vendor/trouble-host-provenance.json):
  upstream archive identity and reviewed local changes.
