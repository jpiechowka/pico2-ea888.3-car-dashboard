# Discovery boundaries and scan-error LED follow-up

Review date: 2026-10-19. This follows the
[LED delivery](probe-led-status-2026-10-19.md) and subsequent TV photographs.
It repairs reproduced software defects and improves failure evidence. It does
not identify the TV's rejected tuple or claim physical interoperability.

Subsequent [slot-1 USB evidence and transport follow-up](../hardware/cyw43-transport-follow-up.md)
now retain a contradictory service/characteristic pair and add global fault
evidence in schema v3. The observations below describe this earlier delivery.

## Hardware observations and limits

The first photograph shows scanning, a red RGB LED, `Error:None` and `HCIbad:5`.
The second shows a characteristic failure after 119 ms, query range `0001–0006`,
last accepted declaration/value `0004/0005`, and HCI disconnect `0x16`.
There is no rejected handle/value in those images. Several malformed responses
could produce that guard failure; none is established by the photographs.

`HCIbad` is a global scan counter and cannot be attributed to the selected TV.
The [SIG controller error definition](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/Core_v6.3/out/en/architecture%2C-change-history%2C-and-conventions/controller-error-codes.html)
defines `0x16` as local-host termination. The probe requests disconnection after
an inventory failure, so that code is compatible with cleanup and does not
explain the original rejection. The red channel is visibly illuminated; timing,
all colour states, latching and RF compatibility still need physical checks.

## Reproduced defects and fixes

| Boundary | Reproduced problem | Correction and regression |
| --- | --- | --- |
| All primary services | Backward searches into legal gaps | Monotonic pagination after each group end; mixed 16/128/16-bit UUID pages and sparse handles |
| Services by UUID | Zero, reversed, overlapping or backward ranges accepted | Reject before publication; same-page and cross-page cases, positive pagination and terminal `FFFF` |
| Service cache | A failed enumeration could leave new partial cache entries | Preflight capacity and publish only the complete enumeration; over-capacity and malformed-response cases preserve the previous cache |
| Both characteristic APIs | Invalid value/declaration relationships could be accepted | One admission helper validates exact 5/19-byte declarations, service bounds and nonoverlap; legal sparse handles remain valid |
| Descriptor endpoints | Sparse values produced the wrong end; targeted lookup could include the next declaration | Stop at next declaration minus one; wire tests assert the actual CCCD request range |
| Terminal characteristic | Descriptor arithmetic could wrap beyond `FFFF` | Empty range returns empty/NotFound without sending a request |
| Bulk descriptors | Capacity overflow silently returned a partial success | Explicit `InsufficientSpace`; a second record into capacity one reproduces the failure |
| Failure evidence | Application rejection omitted the rejected tuple | Retain first rejected handle, original length and 19-byte maximum prefix, preserving preceding valid rows and stopping later stages |

The service/characteristic/descriptor fixes are in the locked shared
`trouble-host` client. The dashboard's real and synthetic adapter routes consume
the filtered service and targeted characteristic APIs; the probe consumes all
services and raw bounded inventory callbacks. No extra adapter command, pairing
path, subscription or ECU request was added to the probe.

The earlier offline audit's reference to mixed UUID widths causing nonmonotonic
service order was an explanation of the old client's traversal, not a Bluetooth
requirement. That wording is corrected. The portable inventory still preserves
descriptor ownership regardless of its adapter's service ordering.

## End-to-end trace and retained protections

The reviewed path runs from explicit selected-peer input through generation
cancellation, scan-stop acknowledgment, connection, MTU setup, bounded inventory,
first-failure publication, disconnect and settled-controller acknowledgment.
Dropping the inventory/client futures precedes cleanup; the runner remains alive
for terminal events. Cleanup still has its five-second bound and requests fatal
recovery on failure. Partial records do not become a clean completion.

The dashboard uses a fresh client per connection, its existing encryption checks,
subscription and adapter initialization only after discovery succeeds. Discovery
failure reaches existing link-fault handling, disconnect/idle acknowledgment and
bounded reconnect policy. The shared request layer checks the matching ATT error
opcode. No guard was relaxed for the TV. Server-only behavior outside these client
routes was not modified by this follow-up.

The probe's guard rejects a tuple only after the ATT layer has delivered it.
Lower-layer decoding errors can therefore have no rejected tuple. The value-handle
field is shown as unvalidated, or absent below three bytes. Long malformed values
retain only their first 19 bytes and their original length, not a full transcript.

Capture payload **v2** stores this optional evidence. A frozen v1 failure fixture
and the existing independent v1 passive fixture remain readable. Reusing a decoder
for v2 then v1 clears the optional field. JSON identifies the input schema.
Journal/USB protocol versions, unknown-format protections and the 8192-byte bound
are unchanged. Maximum record counts plus the rejection prefix fit that bound.

## LED and presentation

Scan-report-only errors now blink red at 1 Hz. Any probe, capacity, storage, USB,
encoder, timing or watchdog error takes priority as steady red. Terminal faults
retain the 4 Hz red pattern. All error states override healthy activity and remain
latched until the two-second Y recovery. Historical counters do not relatch after
acknowledgment; a newly recorded error does, including a late storage completion.

The same atomic latch supplies the supervisor and the next UI snapshot. A new
concurrent fault may reach the LED before the next display frame; display timing
is not the latch's time source. The live header explains slow-red or steady-red
history when no current radio error needs that row. Rejection bytes and recovery
guidance paginate above the fixed footer. Saved captures do not replay the live
LED latch or current telemetry.

Actual shared-renderer [synthetic gallery images](../vehicle/ble-probe.md#rendering-gallery)
were visually checked, alongside pixel tests preserving telemetry and footer
bounds. These are generated fixtures, not reconstructions of the TV response.

## Primary references checked

- [Bluetooth Core 6.1 GATT](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/Core-61/out/en/host/generic-attribute-profile--gatt-.html), Vol 3 Part G, sections 3.1, 3.3, 4.4, 4.6 and 4.7: service groups, characteristic definitions, discovery continuation and descriptor ranges.
- [Bluetooth Core 6.1 ATT](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/Core-61/out/en/host/attribute-protocol--att-.html), Vol 3 Part F, sections 3.4.3.4, 3.4.4.1–2 and 3.4.4.9–10: ordered response records, uniform response entry widths and paginated requests. Read-by-type values are at most 253 bytes; the evidence length field is wider for robust decoding and synthetic boundary tests.
- Pimoroni's exact [Pico Display 2.8 pin header](https://github.com/pimoroni/pimoroni-pico/blob/main/libraries/pico_display_28/pico_display_28.hpp) and [RGB implementation](https://github.com/pimoroni/pimoroni-pico/blob/main/drivers/rgbled/rgbled.cpp), with its [active-low default](https://github.com/pimoroni/pimoroni-pico/blob/main/drivers/rgbled/rgbled.hpp), agree with GP26/27/28 and the existing active-low policy. The linked schematic PDF could not be retrieved in this pass; no new schematic verification is claimed.
- The actual locked Embassy GPIO implementation and Trouble client/iterators/callers were read, including output initialization, request/error matching and cleanup. The reviewed overlay and hashes are recorded in `firmware/vendor/trouble-host/LOCAL_PATCHES.md` and its provenance manifest.

## Verification

| Check | Result |
| --- | --- |
| Root workspace tests | 362 passed |
| Host default / no-default tests | 685 / 685 passed |
| Shared ATT client, three BLE feature configurations | 137 / 101 / 101 passed; 30 discovery tests per configuration, including 13 new wire regressions |
| Root and host strict Clippy | Passed with `-D warnings` |
| Probe and BLE dashboard ARM strict Clippy | Passed separately with `-D warnings` |
| Shared core no_std ARM / Wasm checks | Passed |
| Affected workspace formatting, diff whitespace and vendor provenance | Passed |
| Six final linked images | Passed unchanged practical image and artifact gates |
| Independent review | Full affected client/caller, failure-evidence, capture and LED paths reviewed; populated-cache overflow regression added and re-reviewed |

The first six wire regressions were observed failing before the shared-client
repair. Further cases cover malformed and empty responses, UUID-width changes,
terminal arithmetic, cache merge overflow and duplicate cache entries. Inventory,
capture and LED regressions likewise reproduced their missing behavior first.
Capture tests include all truncated prefixes, v1/v2 decoder reuse and maximum
record capacity with failure evidence. Pixel tests and inspected renderer output
cover the status row, rejected bytes, pagination and footer separation.

### Exact linked images

All builds use `nightly-2026-10-18`, Rust `1.101.0-nightly (8006091eb 2026-10-17)`,
`thumbv8m.main-none-eabihf`, `target-cpu=cortex-m33`, release optimization 3 and
unstripped stack metadata. Probe builds retain the reviewed inline threshold
100000. Dashboard builds include `frame-trace,te-trace,simple-outline` alongside
the named demo or BLE feature. Stock resolves to CPU 150 MHz / SPI 37.5 MHz;
Expert 280/70 resolves to CPU 280 MHz / SPI 70 MHz at the selected 1.30 V setting.
These are generated clock settings, not measured electrical qualification.

| Final artifact | Static RAM / core 0 headroom (bytes) | ELF SHA-256 |
| --- | --- | --- |
| Probe stock | 401696 / 122592 | `1edde82f56507b65a0df318cacbe667ab9f6bdf891d76ed521026e99a5470300` |
| Probe 280/70 | 401696 / 122592 | `b379d500a5d565427e8418dc518941ac99173d3984478bd187b6f062bfa3968f` |
| Traced BLE dashboard stock | 458360 / 65928 | `69cbc4c62ca43dd94f8c90cd2b2baf15f1529f1acf336354d47edc95ba1b6645` |
| Traced BLE dashboard 280/70 | 458360 / 65928 | `19d6d88804e50f4c543c6f5e7b029b5745cce8eba1b56fe418ca006a8a20f6bf` |
| Traced demo stock | 441584 / 82704 | `fc00362532ecfeda4bd1f6002e9dd10a12f4f2c0d3e1fe100708726894ddb89b` |
| Traced demo 280/70 | 441584 / 82704 | `6f6e157f9512ad714e0d1736846cd85d6dcf0f421a55ea385e2121b5fa6213d4` |

Probe UF2 hashes are respectively
`5e4c3ab93c754cc66c2c0f1dda322ecfd803cd3d354ee9573b4cf86e881d40b5`
and `5ea431f44b79e23d07b8f56d48e3d0927691dd3fa84e0061d6825ef15a53c431`.
Both carry source digest
`f45ca6f9d39ee76419e74b5f49be5eae7663fa600b7e88a8f6931bff95969ee0`.
The final 641-file probe and 527-file dashboard source maps were compared against
the checkout after building; no mapped input drifted. Artifact hashes were read
from the actual files and matched their receipts. This source snapshot is not a
proof of every compiler-consumed input.

Probe named stack bounds are boot 17128, core 0 17240, radio 24648 and render
17272 bytes. Its emitted nonradio total is 12080 bytes, 208 below the 12288-byte
gate; the largest emitted frame is 1504 bytes. BLE dashboard named bounds are
boot 3336, construction 18736, runtime 15632, dashboard 28200 and radio 17352
bytes. Demo construction/runtime are 18808/15704 bytes, with no radio bound.
All retain two 153600-byte buffers, a 32768-byte core 1 stack and 8192/4096-byte
core 0/core 1 reserves. Traced BLE headroom is only 392 bytes above the unchanged
65536-byte geometry floor. These are conservative software admission bounds,
not physical stack high-water measurements or complete instruction proofs.

Physical checks remain in the
[bench runbook](pico2-dnuc-runbook.md#standalone-probe-captures-and-diagnostics).
No new firmware was flashed during this follow-up.
