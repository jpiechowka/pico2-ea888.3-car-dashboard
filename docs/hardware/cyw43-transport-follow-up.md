# CYW43 transport and saved fault evidence

Review dates: 2026-10-19 to 2026-10-20. Applies to the Pico 2 W standalone BLE probe and the
dashboard's `ble-obd` radio owner. This follows the
[discovery review](../validation/probe-discovery-followup-2026-10-19.md).

## What the hardware check established

The owner reported that slot 1 could be saved, unplugged and reopened. A subsequent
read-only export through normal Windows USB CDC succeeded and produced binary,
JSON, text and a completion marker. The original payload and photographs remain
local evidence. No slot was rewritten, no device reset and no firmware flashed
during this follow-up. This closes one retention/retrieval observation, not the
power-cut, endurance or general USB qualification matrix.

The schema-v2 saved attempt contained:

- A received GAP service range `0001..0004` and an accepted characteristic
  declaration/value pair `0002/0003`.
- Rejected declaration `0004`, five bytes `02 05 00 01 2A`: properties `02`,
  value handle `0005`, UUID `2A01`. The value lies outside the received service
  range. Rejection is correct; changing the range to accommodate it would invent
  evidence. A synthetic regression exercises this rejection and the legal
  end-handle `0005` happy path.
- A completed connection with ATT MTU 247, followed by characteristic-stage
  failure. HCI `0x16` was local disconnect cleanup, not its initiating cause.
- 1814 global HCI `InvalidSize` batches. The last had 18 remaining bytes and
  declared data length 9, which requires 19 bytes including RSSI. This describes
  an inconsistent received record, not its originating device or corruption layer.
- No LED fault mask: v2 cannot explain which non-scan source caused the reported
  immediate steady red. A large scan count alone does not identify that source.

Service decoding, little-endian handle storage, characteristic enumeration,
ring-buffer padding, state revisions and cleanup were traced in the locked
source. No software off-by-one explaining this captured pair was established.
The TV, RF link, controller firmware and host bus cannot be distinguished by
these bytes alone. CPU/LCD and sampled work/stack values in photographs are not
measurements of bus timing or signal integrity.

## Shared radio mitigation

Both radio owners now use `dashboard_core::ble::runtime::pio_divider` with a
**2.5 MHz GSPI ceiling**. The integer divider is `ceil(clk_sys / 5 MHz)`; the
locked `cyw43-pio 0.10.0` shift loops use two PIO instructions per bus bit.

| CPU profile | Previous configured GSPI | New divider | New configured GSPI |
| --- | --- | --- | --- |
| Stock 150 MHz | 37.5 MHz | 30 | 2.5 MHz |
| Expert 280 MHz | about 46.667 MHz | 56 | 2.5 MHz |

Both select the dependency's low-speed program. Actual `clk_sys` is the input;
fractional dividers are avoided. LCD SPI, CPU voltage/profile, pin ownership,
PIO0 SM0/IRQ0 and DMA1 remain as documented in the
[board audit](pico2w-board-audit.md) and [radio contract](../vehicle/ble-runtime.md).
No ADC work is added on shared GP29. Strict ATT/HCI validation remains enabled.

This conservative setting follows **upstream observations**, not a proved local
root cause. Embassy issue 6313 reports cleared bits on CYW43 reads; a reporter
said lowering SPI frequency removed their symptom, and a later comment describes
fewer occurrences at lower frequencies with the mechanism unresolved.
[Original report](https://github.com/embassy-rs/embassy/issues/6313),
[slower-clock observation](https://github.com/embassy-rs/embassy/issues/6313#issuecomment-4881599356),
[remaining uncertainty](https://github.com/embassy-rs/trouble/issues/573#issuecomment-4905281625).

Infineon's CYW43439 datasheet lists GSPI up to 50 MHz and separate timing limits.
A maximum chip specification does not prove margin for this board/PIO sampling
implementation. The selected 2.5 MHz ceiling is an engineering mitigation, not a
manufacturer-required value or qualification of the Expert CPU profile.
[Infineon/Cypress 002-30348 Rev B, sections 4.2 and 18.3, distributor-hosted copy](https://www.mouser.com/datasheet/2/196/Infineon_CYW43439_DataSheet_v03_00_EN-3074791.pdf).

### Timing and ownership trace

The radio owns its PIO/DMA transactions through completion. A timeout never
cancels a DMA operation and reuses its memory. Flash maintenance still requires
the completed bus boundary and all existing acknowledgments before XIP is stopped.
Core 1 catalogue/rendering and the two fixed framebuffers are unchanged.

The current WLAN image is 231077 bytes and Bluetooth patch 6164 bytes; backplane
writes use 64-byte chunks. Rough raw serialization at 2.5 MHz is about 0.9 seconds
including chunk command overhead, before initialization waits and scheduling.
One 2048-byte transfer takes about 6.6 ms plus protocol overhead. These estimates
are not measured startup or worst-case owner latency. The 10-second initial
progress allowance, 12-second readiness timeout, 500-ms active-progress deadline
and 400-ms maintenance preparation limit were not widened. Startup, sustained
scan load, BLE notifications and maintenance need a physical retest at the new
speed. Lower bus throughput is a deliberate tradeoff and may expose queue loss;
those errors must remain visible.

## Fault evidence and compatibility

The single atomic status word now retains first-recorded source and all nine
stable fault bits. Activity updates preserve both. Recovery atomically clears
the old activity/latch; faults recorded after that clear survive. First means
atomic recording order, not timestamp order or causality. Header selection
prioritizes a steady-red source over scan-only causes; detail pages show all.
Without any peer, short X can still open global fault details and pause scanning;
Inspect remains unavailable. Slots continue to save selected-peer evidence.

| Stable source code | Mask bit | Source |
| --- | --- | --- |
| 1 | `001` | Probe / GATT |
| 2 | `002` | HCI scan report |
| 3 | `004` | Capacity / dropped evidence |
| 4 | `008` | Capture storage |
| 5 | `010` | USB export |
| 6 | `020` | Encoder I2C |
| 7 | `040` | Display completion timeline |
| 8 | `080` | Watchdog timer reset |
| 9 | `100` | Advertisement data |

Mask values above are hexadecimal. Empty evidence uses mask/code zero; legacy
unknown evidence is distinct from a known empty mask.

Payload **v3** appends the mask, first code and configured radio Hz. Save samples
the atomic latch immediately before encoding. No new framebuffer, capture buffer
or unbounded event log is introduced. Legacy v1/v2 fields decode as unavailable;
reusing the decoder cannot carry a newer capture's causes into an older one.
The journal/USB protocols and 8192-byte payload cap remain unchanged. Old tools
cannot decode newer versions; retain original exports and use the current CLI.
[Complete capture contract](../vehicle/ble-probe-captures.md).

## Verification and next hardware run

Focused regressions cover divisor ceilings, every fault source/mask admission,
duplicate/concurrent recording and recovery, no-peer detail access, malformed
v3 fields with valid CRC, frozen v1/v2 decoding, decoder reuse, export fields,
and the captured range contradiction versus a legal characteristic. Existing
loss, cancellation, time-reversal, journal power-cut and display-boundary tests
remain enabled. The synthetic gallery uses the actual shared renderer.

The local tracker retains commands, logs, original capture hashes and review
findings. Linked-image measurements are recorded below. Host
correctness and image gates do not prove physical stack high-water, electrical
margin, scanout FPS, adapter authentication or vehicle data.

### Local verification receipts

The checks below exited zero using `nightly-2026-10-18`; the order of the final
repair and checks is noted below. An independent embedded review identified the no-peer fault
page access gap; a regression failed before the repair and passed afterward.

| Check | Result |
| --- | --- |
| Root `cargo test --locked --workspace` | 366 passed |
| Host `cargo test --locked` / `--no-default-features` | 702 passed in each configuration after the export handoff additions |
| Final core suite after no-peer repair | 155 passed |
| Root workspace and host strict Clippy, all targets | Passed |
| Core Wasm check | Passed |
| Probe and dashboard `ble-obd` strict ARM Clippy | Passed |
| Root, host and firmware format checks | Passed |
| Actual saved v2 payload with updated offline decoder | Passed; latch/radio fields remain null, host-derived hints added, original bytes/hash unchanged |
| Shared-renderer gallery | 52 images generated; new fault/length pages visually inspected |
| Probe stock/280 and dashboard BLE stock/280 practical image gates | All passed |

Both final host configurations include the no-peer model repair and the
[host export handoff](../vehicle/ble-capture-studio-handoff.md). A final focused
pass strengthened the individual property-bit checks; all 19 export/handoff
tests, strict host Clippy and formatting passed afterward. The new host metadata
does not change either firmware source inventory or any linked image below.
Initial sandboxed ARM linking was denied; unchanged build commands were rerun
outside that sandbox. No image gate, stack reserve or validation rule was relaxed.

| Local checked image | Static RAM bytes | Ordinary RAM headroom bytes | ELF SHA256 |
| --- | ---: | ---: | --- |
| Probe `slot1-checked-stock` | 401744 | 122544 | `8279007f51ff73eab419001f9d8bbd7c5c41aebec0a650883914fcfec72a1222` |
| Probe `slot1-checked-280` | 401744 | 122544 | `a177b05cc855f158957519b942d1e5038ed35147962a7e0da01d3441f21c13d0` |
| Dashboard BLE `sc-stock` | 418248 | 106040 | `cb15e9a0d2cf77aee007d2d74d662c917036698a4b8ce417589ced0c8c3e6796` |
| Dashboard BLE `sc-280` | 418248 | 106040 | `52603dccfdc9aacf569ce7fa22695453d7ac0062bd45aaf2b53727486f974aeb` |

Probe source inventory digest:
`3d2e3bd99cf32ac824e80cae07622ff87450ae72fecb8f6107e4d349acad7da3`.
Expert probe UF2 SHA256:
`a652ac2f119a10741dd1008724a856e11a3dfac05698c29fef312803ba794cb1`.
The probe's static allocation grows 48 bytes from the preceding image. Both
probe profiles have largest emitted frame 1512 bytes, non-radio frame sum 12112
against the unchanged 12288-byte limit (176 bytes margin), and conservative
render bound 17280 bytes. Core 1 remains 32768 bytes with a 4096-byte reserve.
Dashboard conservative dashboard/radio bounds are 26720/17352 bytes. These are
the gates' source-reviewed assumptions, not observed high-water or proofs of all
possible instruction paths. Ordinary RAM headroom is not a measured free heap.

### Remaining physical checks

After separately authorized installation:

1. Keep the exported old slot intact. Record cold-boot startup time, configured
   GSPI, LED/header and first/all fault sources immediately if red appears.
2. Scan, then pause and inspect the same TV repeatedly. Record attempts, failures,
   queue losses and exact saved service/characteristic bytes, including successes.
   Avoid concluding that a single successful connection proves stability.
3. If red appears, read short-X fault details before holding Y. Confirm Y clears
   only the live latch and a subsequent real fault relatches it.
4. Save a new attempt in an explicitly selected empty slot; reopen after power
   off and export through normal USB. Compare v3 causes/radio Hz with its display.
5. Exercise USB export and local capture operations under scan load. Repeat
   dashboard BLE startup/notification/liveness checks with a synthetic peer.
   Keep adapter-only/ECU restrictions unchanged.

## Primary source references

- [Bluetooth Core 6.1 GATT](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/Core-61/out/en/host/generic-attribute-profile--gatt-.html), service declaration end-group handle and characteristic discovery within the service.
- [Bluetooth Core 6.1 ATT](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/Core-61/out/en/host/attribute-protocol--att-.html), Read By Type/Read By Group Type formats and discovery progression.
- [Pico 2 W datasheet](https://datasheets.raspberrypi.com/picow/pico-2-w-datasheet.pdf) and [W schematic](https://datasheets.raspberrypi.com/picow/pico-2-w-schematic.pdf), shared wireless pins and board identity.
- [Locked cyw43-pio 0.10.0 source](https://docs.rs/crate/cyw43-pio/0.10.0/source/src/lib.rs), read locally in full; program-selection thresholds, shift instructions and DMA completion.
- [Raspberry Pi CYW43 PIO source](https://github.com/raspberrypi/pico-sdk/blob/master/src/rp2_common/pico_cyw43_driver/cyw43_bus_pio_spi.pio), independent sampling/clock reference, not a replacement for the locked implementation.
