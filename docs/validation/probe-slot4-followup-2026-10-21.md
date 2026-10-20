# Slot 4, watchdog evidence and full capture slots

2026-10-21. Follow-up to the checked `280-70` image from master `e334dde` and
the [diagnostics re-audit](diagnostics-reaudit-2026-10-20.md). This record separates
the owner's photographs, a read-only normal-USB export, source-derived findings
and host fault injection. Device addresses, capture payloads and local receipts
remain private. No slot write, Reset, reboot, new BLE connection or flash was
performed during this follow-up.

## What slot 4 contains

The owner reported red after flashing, blue after two-second Y recovery, a TV
inspection, another red indication and a saved slot 4. The retrieved generation-1
capture is a valid 662-byte schema-v4 record for the LG OLED42C21LA. Its exact
binary, JSON, text and COMPLETE marker were retained; independent offline decode
reproduced the binary identity. Slots 1–3 still reported their prior generation,
length and CRC. This inspection did not rewrite any of them.

| Saved field | Observation |
| --- | --- |
| Inspection | All four discovery/read stages complete; no failure, omitted or malformed records |
| Inventory | 3 primary services, 12 characteristics, 6 descriptors, 2 successful standard reads |
| Timing | 1516 ms inspection, from boot-relative 110819 to 112335 ms |
| Link | ATT MTU 247; connection interval 30 ms, peripheral latency 0, supervision timeout 4 s |
| Cleanup | Acknowledged local termination `0x16`, no retained local receive error |
| Global scan diagnostics | All counters zero in this snapshot |
| Name | Complete scan-response name; an independent successful GAP Device Name read also populated the name cache |
| Live fault snapshot at save | Probe/GATT bit only, mask `0x0001`; first source Probe/GATT |
| Immutable boot evidence | Raw watchdog reason 1, ROM type 0, previous record unavailable |

The saved attempt completed cleanly. The global fault latch is historical and
can survive a successful later attempt until Y recovery; it is not a field of
that attempt's completion result. The immutable boot evidence likewise survives
Y acknowledgment without restoring the live watchdog latch. No preceding fault's
exact timing or identity can be reconstructed from this single-attempt snapshot.
Neither page count nor LED colour alone determines whether the saved attempt
succeeded. This inventory does not qualify any proprietary adapter protocol.

## Separate early-link failure

A different supplied photograph shows `Disconnected` while awaiting MTU exchange,
222 ms in that stage, with controller reason `0x3E`. Bluetooth Core 6.1 Vol 1,
Part F §2.59 assigns that reason to failure to establish an LE connection or
synchronize to a broadcast within the required time. In this initiating-connection
path it indicates establishment failure. It is not an ATT rejection of an MTU.
The successful saved MTU 247 also contradicts a claim of universal MTU incompatibility.
The captured advertising RSSI around -91/-92 dBm makes a controlled closer-range
comparison useful, but does not prove RF interference, distance or hardware damage.
[Controller error definitions](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/Core-61/out/en/architecture,-change-history,-and-conventions/controller-error-codes.html).

The local trace covered scan stop/acknowledgment, explicit connect, automatic MTU
exchange in the locked Trouble client, operation cancellation, discovery, reads,
disconnect/cleanup and terminal reason publication. Shared termination rules
preserve abnormal reasons in both images; a normal cleanup cannot erase an earlier
operation fault. Probe cancellation still requires owner acknowledgment before a
new operation. No timeout, payload validation or LED-fault rule was relaxed.

Comparison with other stacks found no demonstrated driver fix for this capture:

- [Zephyr's connection handling](https://github.com/zephyrproject-rtos/zephyr/blob/main/subsys/bluetooth/host/conn.c)
  explains that `BT_HCI_ERR_CONN_FAIL_TO_ESTAB` can occur before initial data/ack
  exchange even after an apparent controller connection completion. It warns
  that higher layers can otherwise misinterpret the failure. This supports the
  distinction between the host's waiting stage and the link failure, without
  proving why this particular CYW43439 attempt failed.
- [BTstack's GATT client](https://github.com/bluekitchen/btstack/blob/master/src/ble/gatt_client.c)
  also supports automatic MTU exchange. Different connection parameters or
  controller-to-host credit handling are not evidence that copying one constant
  or enabling one flow-control flag will repair this symptom. Credits require
  corresponding completed-packet accounting and lifecycle handling.
- The pinned CYW43439 Bluetooth 0065 firmware already includes the upstream
  [Data Length Extension correction](https://github.com/georgerobotics/cyw43-driver/commit/7f422fef4ea5bff7285fb78340d3f28f5461cff2).
  Its provenance is recorded in the [copied-input audit](../dependencies/full-code-audit-2026-10-20.md).

These comparisons were read on 2026-10-21. Moving upstream links are comparison
sources, not replacement dependencies or proof of this device's behavior.

## Post-flash watchdog evidence

The completed flash used picotool 2.3.1 `reboot -a -c arm`. Its
[exact source](https://github.com/raspberrypi/picotool/blob/2041936441b48a3cc53ae3da9e805229fe8f4e18/main.cpp#L10059-L10101)
requests NORMAL reboot with a 500 ms delay, parameter 0 set to the default
`BOOT_PARTITION_NONE` and parameter 1 zero. The
[SDK definition](https://github.com/raspberrypi/pico-sdk/blob/2.2.0/src/rp2_common/boot_bootrom_headers/include/boot/bootrom_constants.h#L91)
is -1, represented as `u32::MAX` in the command.

The connected board was identified as RP2350 A2. Its exact
[ROM reboot implementation](https://github.com/raspberrypi/pico-bootrom-rp2350/blob/fd6104450fa8f55c11c0c9b54dbc69a27537130f/src/main/arm/varm_apis.c#L47-L124)
writes those parameters to watchdog scratch 2/3, clears scratch 4 for NORMAL,
loads the delay and enables the timer. This overwrites the retained record's
elapsed/checksum words. The [boot path](https://github.com/raspberrypi/pico-bootrom-rp2350/blob/fd6104450fa8f55c11c0c9b54dbc69a27537130f/src/main/arm/varm_boot_path.c#L935-L1010)
leaves the default type NORMAL when no special reboot vector is valid.
Thus the controlled post-flash reboot itself can account for TIMER=1, type=0
and an unavailable checked prior record. These fields do not establish an
application stall. They also cannot prove every ambiguous NORMAL watchdog was
intentional: early failures or damaged retained state can lack a valid record.

Both images retain the conservative shared classifier. `WD TIMEOUT` describes
the timer indication; it does not assert a missed feed. The probe continues to
latch red for ambiguous watchdog evidence until Y acknowledgment. No new scratch
ownership protocol or fault suppression was introduced. A source-derived fixture
covers the exact NORMAL command's scratch overwrite for Booting, Supervised and
explicit-failure records. Earlier red reports after physical USB unplug/replug
remain a separate unresolved observation; this controlled-reboot trace does not
retroactively explain them.

## Full slots, replacement and Reset

All four slots can be occupied and remain independently replaceable. Each slot
already reserves two copies, so replacing a selected healthy slot needs no free
fifth slot or prior Reset. X asks for confirmation; click/two-second A confirms,
Y cancels, and ten seconds or changed evidence invalidates the confirmation.
Healthy slots offer replacement, while damaged, unsupported or protected slots
offer destructive Reset. The [capture procedure](../vehicle/ble-probe-captures.md)
documents the user-visible states.

The audit traced the actual UI, frozen-byte lease, admission, serialized storage
owner, maintenance barrier, flash guard, journal, completion and USB paths:

- Every save rechecks both copies. Unknown journal schemas and intact
  journal copies with unsupported/malformed payload headers block replacement,
  even when a supported sibling remains readable. Invalid slot/length, preflight
  read failures and exhausted generations fail before a save mutates storage.
- Replacement erases the inactive copy, verifies erased/programmed bytes, and
  writes/verifies a separate commit page last. Fault-injection tests retain a
  prior or newly committed generation. First-save interruption may leave a
  damaged slot; there is no invented rollback guarantee.
- Reset erases both copies of only the selected slot. Interruption can destroy
  either or both old generations. It is not atomic, does not auto-resume and
  does not save current evidence after completion. Other slots remain isolated.
- Back, recovery and timeout can hide an operation without cancelling the
  storage owner's work. Its matching completion updates the original slot,
  returns the affine byte lease and releases the pending fence. Wrong/stale or
  duplicate replies cannot redirect that completion to another selection.
- USB exposes LIST/GET only. It copies the admitted payload before transfer and
  releases journal ownership, so host backpressure does not retain flash access.
  Session epochs prevent old replies crossing a USB reset. Maintenance parks
  owners and checks DMA/peripherals before each flash window.

### Confirmed correction

Successful save completion previously always projected a nonzero generation as
replaceable. At `u64::MAX` this disagreed with directory refresh, which correctly
required Reset. The journal already rejected overflow before erase; the defect
was a misleading confirmation followed by a refused write, not data loss.
A single readable-slot projection now serves refresh and completion. It keeps
the final capture readable and immediately requires Reset for reuse.

Two regressions exercise all four occupied slots, ordinary/final generations,
readability, cancellation, selected-slot Reset and completion hidden by Back,
recovery or timeout. Both failed against the old projection and pass with the
fix. The dashboard's separate settings journal already checks generation
addition before mutation and has no corresponding capture-slot UI projection.
Its exhaustion, corruption and interrupted-write tests were rerun; no duplicate
dashboard correction was justified.

## Verification and remaining physical checks

The final focused host run passed 113 tests across capture UI/store/admission/
reply/recovery/wire, USB sessions, status, maintenance, flash probing, settings
journal and reset evidence. Existing storage tests include per-call failures,
torn writes, silent erase/program failure, unknown-format siblings, equal/final
generations, confirmation expiry and ownership after cancellation. An independent
review found no additional correction in the focused production diff.

Additional local verification passed:

| Check | Result |
| --- | --- |
| Root `cargo test --locked --workspace` | Passed |
| Host UI/reset tests with `--no-default-features` | 25 passed |
| Root/host strict nightly Clippy, all targets | Passed with `-D warnings` |
| Root/host/firmware formatting | Passed |
| Cortex-M33 ARM strict Clippy | Probe `cpu280-spi70-1v30` and dashboard `ble-obd,cpu280-spi70-1v30` passed separately |
| Checked probe wrapper | `slot4-audit-20261021-280-checked`, `280-70`, Expert, pinned nightly 2026-10-18 |
| Linked image | ELF/UF2 equality, source identity, flash reservation and reviewed SRAM lockout checks passed |
| Static RAM/headroom | 406432 / 117856 bytes, unchanged from the preceding checked image |
| Retained allocations | Two 153600-byte framebuffers and 32768-byte core-1 stack |
| Conservative named stack bounds | Boot 17128, core 0 17160, radio 24648, render 17248 bytes; existing reserves/gates unchanged |

The first isolated build failed at linking with sandbox `permission denied`.
A fresh attempt using the same wrapper outside that sandbox passed; the failed
attempt and both logs were retained. The successful artifact's source digest is
`80570e7c956aa663a5e129a8e06bbc5e3c36de782b267b37ac71ac993ca2dae5`;
ELF SHA256 `1b7d9b877f47864fc54275cd7a71b30873682aed7a135786c130ca6ef5ed1146`;
UF2 SHA256 `52c8fb0a126aa476f514d1b5b591a2c1762706bf438be468661473783a96ded6`.
It was built and checked, **not flashed**. No physical slot mutation was used
to validate the fix.

These software checks do not qualify physical flash endurance, arbitrary
brownouts, USB driver behavior, RF reliability, actual stack high-water or
electrical margins. All four physical captures were preserved.

For the next separately authorized hardware session:

1. Record cold power-on separately from picotool/ROM reboot. If red appears,
   preserve raw reason, ROM type, previous stage and all fault sources before Y.
2. After Y, explicitly inspect the same TV near the Pico. Save success and
   failure separately with their attempt context; do not infer the failure from
   page count. Repeated failures should be compared with a second scanner and
   the TV's current power/connection state.
3. Export wanted captures before confirming replacement. Ordinary replacement
   can be checked with all four slots full; destructive Reset/power-cut tests
   require disposable backed-up fixtures and explicit authorization. No such
   destructive test was performed on the owner's four captures here.
