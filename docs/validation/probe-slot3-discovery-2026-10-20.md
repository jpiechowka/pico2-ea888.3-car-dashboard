# Slot 3, device names and watchdog evidence

## Hardware evidence and scope

The owner reported intermittent `Red: watchdog reset` with `HCIbad:0` after USB
unplug/replug. Y recovery cleared red; restarting did not restore the TV to the
scan list. These observations establish neither the reset cause nor why the TV
was absent. The HCI counter counts rejected scan batches, not every fault source.

Slot 3 was recovered read-only through normal USB and decoded: NEEWER-TL21C,
four primary services, eight characteristics, one descriptor and three successful
standard reads. Inventory stages and cleanup completed; controller reason was
`0x16`, local host termination. No faults, omissions, rejected records or HCI
errors were retained in this schema-v3 capture. It does not contain boot reset
metadata and cannot diagnose an earlier boot. Slots 1 and 2 remained present;
slot 4 was empty. No slot was written or reset during this follow-up.

The export already has a name/address/content-hash filename. The generic name
in the photograph was the on-device slot directory, which is now updated too.
Private raw captures and local logs remain outside Git. The Pico can be unplugged
after the completed export; further software checks do not need it connected.

## Changes and end-to-end checks

### Names and slots

- The catalogue keeps one bounded name observation per exact address/type. A
  nameless later packet retains that observation while replacing the raw packet
  normally. Empty shortened names no longer hide another usable name.
- Explicit Inspect already reads allowed GAP Device Name values. Only an admitted
  successful, nonempty, non-truncated value can update the cache at inspection end.
  Service/characteristic/read association, request generation and time guards
  remain enforced. No new automatic connection, pairing or arbitrary read exists.
- A complete current packet name has priority. Captured successful GAP reads and
  complete cached observations precede shortened names as specified by the
  [shared selection contract](../vehicle/ble-probe-captures.md#recover-captures-on-a-pc).
  Names are untrusted observations and are not matched across changing addresses.
  Y recovery clears the live catalogue and cache.
- Listing slots borrows the existing UI payload lease, loads and validates the
  selected supported journal copy, then copies only its bounded identity. It
  does not add a framebuffer or 8192-byte buffer. Names, address type/address and
  generation appear together. Unknown sibling formats still block replacement.
  Reset and failed/uncertain operations clear labels; stale replies cannot label
  another slot. Back/recovery/timeout keep the operation fence until completion.
- Shared name selection drives both on-device labels and host export identity.
  Source truncation is retained even when the displayed name fits in 31 bytes.
  Raw packets, values and original saved bytes remain unchanged.
- The expanded directory initially exceeded the unchanged aggregate stack gate.
  UI state now initializes once in its owner's static storage; completion flows
  by borrow and an exclusive reply-reference handoff. One-time boot/supervisor
  resource channels are populated before their tasks start. Task constructors
  directly return the existing measured futures, avoiding duplicate future
  construction. No byte/framebuffer, watchdog period, stack allowance or reserve
  was added or relaxed. The sole storage owner waits for the returned reply;
  the UI takes bytes before admitting completion and returns the reply before
  rendering. Maintenance has resumed both cores before publication.

Bluetooth Local Name is optional; shortened names are prefixes, and GATT Device
Name is read after connection. These specification rules do not require a peer
to advertise continuously or expose its name in a scan response. The probe cannot
force the missing TV to appear. [Bluetooth CSS Part A §1.2](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/CSS_v11/out/en/supplement-to-the-bluetooth-core-specification/data-types-specification.html)

### Both firmware watchdog paths

The same `RecoveryWatchdog` boundary serves the dashboard and probe. It retains
raw watchdog reason and checked ROM boot type, then writes a small checked
previous-stage record. Both supervisors record their first explicit failure
before logging or waiting for reset. Normal feeds update the supervised stage.
The probe publishes immutable boot evidence to core 1 and renders it even without
a selected peer. The dashboard retains the same evidence and logs its classified
reset label; it does not yet display the probe's full boot evidence page.

The classifier follows the SDK distinction between a normal boot following a
watchdog and known intentional ROM reboot modes. Missing/unknown ROM data or
reserved reason bits conservatively preserve the warning. Raw zero is `OTHER`,
not a promise of cold power-on. The GS reply must have exactly five words and
the requested supported flag before its boot-type byte is used.
[SDK watchdog classifier](https://raw.githubusercontent.com/raspberrypi/pico-sdk/master/src/rp2_common/hardware_watchdog/watchdog.c),
[ROM system information contract](https://raw.githubusercontent.com/raspberrypi/pico-sdk/master/src/rp2_common/pico_bootrom/include/pico/bootrom.h),
[ROM constants](https://raw.githubusercontent.com/raspberrypi/pico-sdk/master/src/rp2_common/boot_bootrom_headers/include/boot/bootrom_constants.h)

The retained record uses scratch 0–3: magic, version/stage, saturated `u32`
observation milliseconds, and CRC32. The sole core-0 writer invalidates magic,
writes the payload/checksum, then commits magic with peripheral ordering barriers.
Only a valid record on a watchdog-fault boot is adopted. An interrupted write or
ROM parameter overwrite can make the previous stage unavailable.

Scratch 2/3 are **not universally application-owned**: RP2350 ROM reboot uses
them as parameters behind a valid scratch 4–7 vector. The ROM consumes that
vector before application entry. This runtime never writes 4–7 and has no
concurrent ROM reboot caller. Future ROM reboot integration must first stop the
watchdog-record writer. Its parameter overwrite may invalidate our record; it
must not be mistaken for persistent history.
[ROM reboot API](https://raw.githubusercontent.com/raspberrypi/pico-bootrom-rp2350/master/src/main/arm/varm_apis.c),
[ROM vector validation and consumption](https://raw.githubusercontent.com/raspberrypi/pico-bootrom-rp2350/master/src/main/arm/varm_boot_path.c)

The actual locked `firmware/vendor/embassy-rp` watchdog and ROM lookup source
was read alongside those references. The existing RP2350 WDSEL correction,
1 MHz tick checks, 15-second startup period and two-second supervised period
remain. The GS lookup rejects absent/non-Thumb entries before using the locked
secure-Arm ABI. No feed, timeout, stack reserve or radio-rate guard was relaxed.

Retained stage/time is the **last observation**, not the exact reset instant,
proven electrical/software cause or producer-image identity. It saturates after
about 49.7 days. Power removal may erase scratch. A hard hang before recording a
failure can leave only Booting or Supervised. Y recovery clears the live LED
latch, while boot evidence remains inspectable. Hardware scratch/reset semantics
require physical confirmation. [RP2350 watchdog register definitions](https://raw.githubusercontent.com/raspberrypi/pico-sdk/master/src/rp2350/hardware_regs/include/hardware/regs/watchdog.h)

### Capture v4 and export

V4 appends optional cached-name and boot evidence after v3 metadata. The journal,
USB protocol and 8192-byte bound are unchanged. Frozen v1/v2/v3 fixtures remain
readable without rewriting bytes; absent fields stay null. Additive JSON fields
include `cached_name`, `boot`, and selected-name cache/time provenance. These
fields do not enable an adapter profile or establish protocol compatibility.
The [format](../vehicle/ble-probe-captures.md) and
[Studio handoff](../vehicle/ble-capture-studio-handoff.md) document the contract.

## Verification

Initial failing regressions reproduced cache loss after nameless packets,
empty-short masking, directory identity/state gaps and reset-record boundaries.
The parent additionally reproduced the lost source-truncation flag. The broader
suite exposed an old test using schema 4 as a future version; it now uses a
CRC-valid unsupported version. No rejection rule was weakened to pass it.

| Check | Result |
| --- | --- |
| Root workspace tests | 369 passed |
| Shared core without default features | 158 passed |
| Host tests, default / no default features | 753 passed in each configuration |
| Configurator native tests | 417 passed |
| Root / host strict Clippy, all targets | Passed |
| Configurator native / Wasm strict Clippy | Passed |
| Probe / dashboard default / dashboard BLE ARM strict Clippy | Passed, Cortex-M33 target retained |
| Root / host / firmware format checks | Passed |
| Independent Astra review | Complete relevant source/diff review; 60 focused tests passed; no actionable regression |
| Shared-renderer screenshots | 57 generated; name/address rows, error states, paginated reset/name and saved evidence visually inspected |
| Independent reply ownership review | No actionable defect; recommended partial-decoder-reuse regression added |

Regression coverage includes exact address/type and time association, failed/
empty/truncated/foreign/stale GAP reads, name precedence/provenance, all capacity
records with the longest cached name and previous-reset metadata, valid-checksum
invalid metadata, decoder reuse to all legacy versions, both journal copies,
read-failure injection, no-write listing, UI delayed/failed/stale completions,
all raw reason bits/boot bytes, corrupt scratch, every interrupted publication
prefix, ROM parameter overwrite and first-failure retention.
The actual firmware reply mailbox also has regressions for delayed acknowledgment,
exclusive record/byte identity, repeated reuse and failed/stale completions after
Back, recovery and timeout. Reusing a decoder after each CRC-valid malformed v4
metadata case clears partial cache/boot data before a legacy load and admits the
next valid v4 normally.

### Checked images

Fresh builds used `nightly-2026-10-18`, `thumbv8m.main-none-eabihf` and
`-C target-cpu=cortex-m33`. Both probe builds passed complete source/ELF/UF2,
capture-region, reviewed flash helper, memory and conservative stack admission.
The dashboard Vehicle 280/70 build used `ble-obd`, both traces, simple outline,
optimization 3 and no default features, and passed its practical image gate and
modified-artifact rejection fixtures. These commands compiled and checked files
without accessing a device. The initial sandboxed link permission failure was
resolved with elevated tool permission; build and image gates were unchanged.

| Image | Static RAM / remaining core-0 region | ELF SHA256 |
| --- | --- | --- |
| Probe stock | 406296 / 117992 B | `8bc75a4233520d857272c6f2cc494f32797dc6a8f17087368665a479bdbb6de0` |
| Probe 280/70 | 406296 / 117992 B | `6f18354897b8b418a06e63d2080d72803e2967e6bb037de3cb861ae3ea552aa9` |
| Dashboard BLE 280/70 | 458432 / 65856 B | `3eada0bd794de657c4967f00eb032569c04a6e17530f1c27d2c111a675885654` |

Probe emitted frame totals are 19136 B overall and 12248 B excluding radio-owned
symbols, below unchanged 20480/12288 B regression ceilings. These aggregates are
not simultaneous stack usage. The largest emitted frame is 1512 B; UI poll is
832 B, versus 1928 B in the rejected initial candidate. Its conservative named
bounds are boot 17128, core0 17160, radio 24648 and rendering 17248 B, with the
existing 8192-byte core0 and 4096-byte core1 reserves. Core1 remains 32768 bytes.
Both 153600-byte framebuffers and both existing 8192-byte capture leases remain.
The small aggregate-ceiling margin requires continued checks on future changes;
it is not the physical stack headroom.

Dashboard bounds are boot 3336, constructor 18736, dashboard 28200, radio 17352
and runtime 15632 B with unchanged reserves. None is a measured physical peak.
The probe source digest is
`9d347244b65e6777d6a82f08331c7664803025031e1d2d29be6b4b6be4d662dc`.
Local receipts are `slot3-final-stock`, `slot3-final-280` and
`slot3-dashboard-02` in their documented build-capture directories.

## Remaining physical checks

No firmware was flashed during this follow-up. With a separately authorized
checked image:

1. Preserve/export slots 1–3. Confirm their labels and bytes after installation;
   never erase-all. Older payloads remain unchanged and readable.
2. On a red cold boot, open short-X details before recovery. Save or photograph
   raw reset bits, ROM boot type, previous stage/time, first/all fault sources and
   HCI count. Repeat USB power cycles and record startup timing.
3. Confirm blue scan, clean successful explicit probe/green, amber saved browsing,
   and Y acknowledgment/re-latching. A saved successful capture cannot create
   green live status. The existing error policy remains in force.
4. Confirm observed names survive nameless packets and explicit GAP names survive
   Back to scanning; recovery clears the live cache. An unnamed peer need not
   become named. Compare the saved/source-labelled name with the export.
5. Record the TV separately if it advertises again. One successful NEEWER attempt
   neither diagnoses the TV nor validates NGM/OBDEleven or vehicle traffic.

Host results and image admission do not establish physical reset retention,
startup reliability, RF discovery, flash endurance/power cuts, panel scanout,
stack high-water, electrical overclock margins or ECU compatibility.
