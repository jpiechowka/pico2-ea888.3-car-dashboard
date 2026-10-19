# BLE probe RGB status review

Date: 2026-10-19. Scope: the standalone probe's PIM715 status LED. The dashboard's
configurable warning LED, BLE requests, capture format, display geometry, clocks,
watchdog thresholds and flash maintenance protocol are unchanged.

The later [termination follow-up](probe-termination-followup-2026-10-20.md)
repairs a missing green guard found in saved hardware evidence: cleanup
acknowledgment alone did not qualify its terminal reason or late receive faults.
The measurements below describe this earlier delivery.

## Behavior and findings

The old supervisor used radio phase alone: scanning was green, paused was amber,
and recoverable failures did not affect RGB. The new
[probe policy](../../firmware/ble-probe/src/status.rs) implements the approved
[colour and recovery contract](../vehicle/ble-probe.md#rgb-status-and-recovery).
Green requires current live inspection, all four completed stages, negotiated
MTU/link evidence, ordered terminal timestamps, cleanup acknowledgment, no
omitted/rejected/truncated records and successful valid-length permitted reads.
It does not require optional fields to exist or imply a still-open connection.

The final review also reproduced a false-green case where ATT returned success
with an invalid-length standard field. The policy now reuses the shared
`info_length_valid` validator. A 325-case regression checks lengths 0 through 64
for Appearance, Preferred Parameters, System ID, PnP ID and Database Hash. Raw
bytes remain retained; the LED does not rewrite the original ATT result.

Errors stay red through successful retries, navigation and storage operations.
Two-second Y recovery clears the latch and live observations atomically. Saved
slots and lifetime scan counters remain. A new failure can immediately relatch;
an already pending storage operation retains ownership and may still fail after
recovery. Terminal faults override the clear. Cold/non-watchdog restart loses
volatile history; watchdog timer reset records a new error at boot. There is no
flash write or scratch-register persistence for LED history.

## End-to-end ownership review

| Path | Reviewed behavior |
| --- | --- |
| Radio/report ingress to core 1 | Current-generation events pass through the status wrapper before later input can replace their error. Stale generations are ignored. Malformed AD framing and truncated reports are observed before a later packet can replace them. |
| Core-0 scan and queue errors | HCI rejection and report-queue overflow record errors at their producers, independently of saturating diagnostic counters. Old lifetime counts do not themselves relatch after Y. |
| Inventory to cleanup | Connection alone and intermediate stage completion cannot create green. Read protection/unavailability is an error outcome; normal completion/intentional disconnect is not. Complete enumeration can coexist with a failed or malformed-length read and remains red. |
| Capture UI and storage owner | Local pending work shows purple if no error has priority. Timeouts, hidden failed completions, damaged/unsupported slots, internal lease/queue failures and storage-owner errors record red. Empty slots and normal admission guards are neutral. Stale unmatched replies are ignored by the UI; owner-side failures still record at their source. |
| USB | A malformed command, current-session overflow or failed/timed-out response records red. Idle input timeout and session reset/unplug cancellation do not. Session invalidation, read-only protocol and byte-lease ownership are unchanged. |
| Encoder | Startup without the optional encoder is neutral. Failure after established communication records red, while onboard controls remain usable. No I2C future is timeout-cancelled. |
| Supervisor and GPIO | Core 0 retains exclusive GP26/27/28 outputs. A single atomic word holds activity and errors; read-modify-write publication preserves concurrent faults. Relaxed ordering is sufficient for this scalar-only communication; it publishes no borrowed/non-atomic data. |
| Recovery and fatal progress | Y's atomic clear removes old green activity as well as errors. A fault linearized after that clear survives subsequent publication. Existing display/radio/progress fatal flags still stop feeding; the existing RP2350 watchdog mask/tick boundary remains in use. |
| Timing and maintenance | Blink phase uses monotonic milliseconds, with no frame-count catch-up or overflow-prone deadline addition. The 50 ms supervisor samples 500 ms blue/125 ms red half-periods. Existing flash parking can defer an LED update. A stalled core cannot guarantee a visible blink before reset. |

The latch's scope is recorded errors, not proof that every possible hardware fault
is detected. Protected reads can be expected access restrictions. Global scan
errors cannot be blamed on the selected adapter. Browsing saved evidence is
amber and never supplies live success. No pairing, retry, write, subscription or
ECU request was added. The existing two fixed framebuffers and peripheral owners
remain in place.

## Manufacturer, specification and locked-source checks

- The manufacturer's [PIM715 product](https://shop.pimoroni.com/products/pico-display-pack-2-8)
  and exact [2.8-inch library pin definitions](https://raw.githubusercontent.com/pimoroni/pimoroni-pico/main/libraries/pico_display_28/pico_display_28.hpp)
  identify GP26/27/28. The similarly named 2.0-inch library has different pins and
  was not used as this board's pin evidence. Manufacturer
  [RGB defaults](https://raw.githubusercontent.com/pimoroni/pimoroni-pico/main/drivers/rgbled/rgbled.hpp)
  and [implementation](https://raw.githubusercontent.com/pimoroni/pimoroni-pico/main/drivers/rgbled/rgbled.cpp)
  corroborate active-low drive. The schematic PDF fetch was unavailable in this
  pass; this is source corroboration, not a claimed new electrical measurement.
- The actual locked [Embassy RP GPIO implementation](../../firmware/vendor/embassy-rp/src/gpio.rs)
  initializes the output level before enabling output and implements `set_level`
  with SIO. [Embassy's API reference](https://docs.embassy.dev/embassy-rp/git/rp235xa/gpio/struct.Output.html)
  was cross-checked; the local 0.10.0 source is authoritative. All three outputs
  start high/off; the existing supervisor owns subsequent updates. No PWM, DMA,
  interrupt or W-reserved pin was repurposed.
- The locked [watchdog](../../firmware/vendor/embassy-rp/src/watchdog.rs), shared
  [RP2350 recovery wrapper](../../firmware/pico2/src/drivers/watchdog.rs), and
  official [RP2350 register definitions](https://raw.githubusercontent.com/raspberrypi/pico-sdk/master/src/rp2350/hardware_regs/include/hardware/regs/watchdog.h)
  were traced. The timeout test uses TIMER without FORCE, matching the HAL's
  force-first precedence. The existing wrapper corrects the HAL's RP2040 reset
  mask and verifies tick/control setup. This LED change does not alter that fix.
  The full RP2350 PDF exceeded the web fetch limit; register-source evidence was
  used instead of claiming a fresh complete datasheet read.
- [Bluetooth ATT](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/Core-61/out/en/host/attribute-protocol--att-.html)
  distinguishes access errors from successful transactions. The
  [GAP definitions, sections 12.2/12.3](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/Core-61/out/en/host/generic-access-profile.html)
  require two/eight octets for Appearance/Preferred Parameters;
  [GATT section 7.3](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/Core-61/out/en/host/generic-attribute-profile--gatt-.html)
  defines the 128-bit Database Hash. [DIS 1.2](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/DIS_v1.2/out/en/index-en.html)
  and the [2026-09-09 GATT Supplement](https://btprodspecificationrefs.blob.core.windows.net/gatt-specification-supplement/GATT_Specification_Supplement.pdf)
  define the information fields (PnP ID has one 1-byte and three 2-byte fields;
  System ID is 64 bits in section 3.234's prose and `uint64` type). That section's
  table 3.360 lists 64 under octets, an apparent unit error inconsistent with its
  prose/type; the existing eight-byte validator follows the latter. The policy
  reuses existing length validation, with
  tests covering all five fixed-length fields. LED colours are application policy,
  not Bluetooth-standard connection states.

## Verification

The native tests include the production status module and actual bridge channel.
Twenty tests cover lifecycle/cleanup, failed and invalid-length reads, capacity,
all probe error categories, stale generations, cancellation, counter saturation,
concurrent fault/publication, optional hardware, USB error classification,
elapsed-time boundaries, capture activity/timeouts/hidden completion and recovery.
Initial reproductions failed for green scanning and malformed-length false green
before their fixes. The complete host suite passed 679 tests across 94 test
executables/doc-test groups, with zero failures or ignored tests; strict host
Clippy passes. Strict Cortex-M33 ARM Clippy passes for stock and 280/70. Formatting is checked in the
affected host and firmware workspaces. Existing session/report/capture/USB tests
remain part of the host suite.

Both source-bound images passed the unchanged practical image, UF2 equality,
capture-reservation and reviewed flash-lockout checks. The initial sandboxed
links failed with `rust-lld: permission denied`; they were not admitted or flashed.
Final builds used fresh directories and the same original gates.

| Measurement | Stock CPU150/SPI37.5 | Expert CPU280/SPI70 |
| --- | ---: | ---: |
| Static SRAM, bytes | 401632 | 401632 |
| Main stack/headroom, bytes | 122656 | 122656 |
| Largest emitted function frame, bytes | 1504 | 1504 |
| Aggregate emitted frames, bytes | 18576 | 18576 |
| Non-radio aggregate frames, bytes | 12056 | 12056 |

Static allocation/headroom match the preceding capture build. Aggregate and
non-radio frames each increased by 148 bytes. The unchanged limits are 20480
total, 12288 non-radio and 8192 per frame. Aggregate frames are a regression
guard, not simultaneous stack use. Named-path bounds remain below their reserves;
the two 153600-byte buffers and 32768-byte core-1 stack retain their exact sizes.

The final wrapper attempts are `led-status-final-stock` and `led-status-final-280`,
using `nightly-2026-10-18`, Cortex-M33, `thumbv8m.main-none-eabihf`, the established
binary-only inlining option and picotool 2.3.1. Both bind source inventory digest
`ba74ddd601e21fcd27163f89829c3e7e96b1fff835d9eba67c1c22535148d917`.

| Profile | ELF SHA-256 | UF2 SHA-256 |
| --- | --- | --- |
| Stock | `6a94b2fb6670829cd756cdb41f3d06a8fab00c85d01cf064aaef471ace8638f4` | `e0ff82d807b2fd05f23141179cbcac26394d07f0387f6cbab658e78245de1902` |
| 280/70 | `555bb6b97095d2d0215c8c458312ed8b0fd1839b2a38295cba684cfbb7dd0e4f` | `5fb6f3b01f685f744dfd6fb6c0ce90573c53c8bc4cb75968c9eabd1568375b85` |

Full compiler/options/source manifests and gate results remain with each ignored
build capture. Exact published-commit CI and authorized device transfer/readback/
reboot are separate delivery stages, recorded in the local progress/flash receipt.

Physical colour/polarity, visible blink timing, peer behavior, storage retention,
USB interruption and induced watchdog recovery remain separate checks in the
[bench runbook](pico2-dnuc-runbook.md#standalone-probe-captures-and-diagnostics).
Neither host tests nor flash/readback establish those observations or physical
stack high-water, overclock electrical margin or NGM/OBDeleven interoperability.
