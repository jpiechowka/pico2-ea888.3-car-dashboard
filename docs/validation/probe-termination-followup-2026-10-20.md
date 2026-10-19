# Completed inventory and abnormal termination

Review date: 2026-10-20. Applies to the standalone BLE probe, the dashboard's
BLE maintenance boundary, and offline capture reports. It follows the
[slot-1 transport review](../hardware/cyw43-transport-follow-up.md).

## Hardware evidence and limits

The owner saved slot 2 after inspecting a NEEWER TL21C light, reconnected normal
USB, and reported that the TV remained absent. The photographs show blue
scanning, an amber slot directory and green live inspection details. The owner
also reported red on the first boot with `HCIbad:0`; restart cleared red but
**did not restore the TV**. No first-boot fault ledger was captured, so its cause
is unknown. `HCIbad` counts malformed scan batches, not all possible LED faults.

A read-only slot export verified the payload integrity and expected build/profile
identity. The saved v3 attempt contains four primary services, eight
characteristics, one descriptor and three successful standard reads. All four
inventory stages ended, with no omissions/rejections, no global scan errors and
no recorded LED fault bits. The firmware saved both completion and cleanup
acknowledgment as true, but the terminal reason is **34 decimal, `0x22`**.

This is a proven false-clean reporting path. It does not prove the physical
cause of the timeout or that the earlier GSPI mitigation cured scan corruption.
The raw payload and original JSON/text remain private, unchanged evidence.
Offline decoding into a separate directory preserves the binary hash and raw
flags while adding `termination_assessment.status = abnormal_reason`.
Slots 1 and 2 were not modified and no firmware was flashed in this follow-up.

## Source and specification checks

The [Bluetooth Core 6.1 controller error codes](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/Core-61/out/en/architecture%2C-change-history%2C-and-conventions/controller-error-codes.html),
Vol 1 Part F sections 2.19, 2.22 and 2.34, define remote-user termination `0x13`,
local-host termination `0x16`, and LL response timeout `0x22`. The probe policy
accepts the first two only alongside otherwise complete work; the code alone
never proves successful inspection. A raw zero is not a normal termination.

The locked Trouble HCI handler checks event status before applying disconnect
state. Its connection manager retains terminal reason independently of event
queue delivery, and retains the first local ingress failure until connection
slot reuse. A held connection prevents reuse while cleanup reads that evidence.
The capture codec and JSON exporter preserve the reason byte without numeric
conversion loss. The official HCI functional HTML exceeded the browser tool's
size limit in this pass; no full online read of that document is claimed.

The scan path was traced through active legacy 1M scanning, a 100 ms interval,
50 ms window, disabled duplicate filtering and empty accept list to the bounded
catalogue. There is no name, TV or RSSI filter. Connectable/directed reports and
matched scan responses supply the catalogue. These source facts cannot tell us
whether the absent TV was advertising or whether packets were lost before it.

## Corrections in both firmware paths

| Boundary | Correction |
| --- | --- |
| Shared termination policy | Only `0x13`/`0x16` are normal reasons. Missing, reserved, zero and other codes cannot establish clean teardown. |
| Probe cleanup | Snapshot retained local ingress and terminal reason after the cleanup wait, including when inventory succeeded. Publish context before the end event. Acknowledgment stays separate from termination quality. |
| Probe model | Late receive/terminal failures become first-class failure evidence even without an earlier operation failure. Preserve the first cause, accepted inventory and generation isolation. |
| Probe LED | Green additionally requires a consistent normal reason and no local ingress failure. Abnormal/missing connected termination fails; red stays latched until Y recovery. |
| Dashboard maintenance | Normal intentional park can suppress generic admission cancellation only without ingress failure. Specific operation faults and abnormal/missing terminal evidence survive park. Active disconnects remain faults. |
| Dashboard logs | A bounded once-per-connection END record includes epoch, raw reason/unknown and ingress indicator; ring eviction remains possible. |
| Saved/live pages | Label the reason and show a separate warning for acknowledged abnormal/missing connected termination. Old completion flags remain historical. |
| Offline export | Add versioned termination assessment without changing binary schemas or overwriting original completion, errors, reason or LED fields. |

No pairing, vendor reads/writes, subscriptions, reconnect policy, radio clock,
DMA ownership, timeout, flash allocation or capture capacity was changed.
No extra retained Pico buffers or fields were added. Failed/canceled connects
without a link do not invent missing-disconnect faults. Unacknowledged cleanup
retains the existing bounded fatal recovery path.

The [Studio handoff](../vehicle/ble-capture-studio-handoff.md) still separates
inventory completeness from overall outcome and proprietary protocol support.
Useful endpoint evidence survives a terminal failure; it cannot automatically
enable an adapter profile or justify reusing saved handles.

## Software verification

New regressions failed before the repair: abnormal reason did not prevent green,
late local ingress was ignored, and the exporter lacked the derived assessment.
An older test that required discarding context without a prior failure was
updated, preserving its first-cause and stale-generation checks.

| Check | Result |
| --- | --- |
| Root workspace tests | 368 passed |
| Host default / no-default tests | 712 / 712 passed |
| Focused probe/session/handoff/termination/logging tests | 55 passed |
| Independent read-only review | No required findings; 52 relevant tests independently rerun and passed |
| Root/host and separate probe/dashboard ARM strict Clippy | Passed with `-D warnings` |
| Shared core no_std ARM / Wasm checks | Passed |
| Actual saved binary offline reassessment | Same binary hash and raw flags; derived abnormal reason `0x22` |

Coverage includes every terminal reason byte plus absence, all dashboard fault
classes, deliberate park versus active disconnect, late ingress, prior-failure
precedence, contradictory context, canceled/stale revisions, Y acknowledgment,
failed connect, bounded log formatting, legacy raw completion and normal paths.
The production cleanup helper is exercised with evidence becoming available
only after its wait resolves. Rendering tests preserve cleanup acknowledgment,
reason labels, text bounds and pagination. Gallery output is synthetic shared
renderer evidence, not a reconstruction of the light or TV.

### Exact linked images

All four final images passed their unchanged practical gates. The initial
sandbox attempts failed at linking; fresh attempts with linker access passed.
Both contexts use `nightly-2026-10-18`, target `thumbv8m.main-none-eabihf`,
`target-cpu=cortex-m33`, release optimization 3 and stack metadata. Probe
retains its reviewed inline threshold 100000. Dashboard images enable
`ble-obd,frame-trace,te-trace,simple-outline`; the 280/70 variant additionally
selects `cpu280-spi70-1v30` with explicit Expert admission.

| Image | Static RAM / core 0 headroom (bytes) | ELF SHA256 |
| --- | --- | --- |
| Probe stock | 401744 / 122544 | `b4c90120af8f7a3a600c9d22ad399c5479c90a7770a19dffc579f1e303dcc93b` |
| Probe 280/70 | 401744 / 122544 | `9d055addf29b3b9603a507d9fbb322e7af6356a2dc972cd53cddaaae9d35d8e5` |
| Traced BLE dashboard stock | 458360 / 65928 | `619b9681206cdf9fe172403c01e7a150d5056ce687f8e109ca3ee34782dd8406` |
| Traced BLE dashboard 280/70 | 458360 / 65928 | `0af77f666e5090371b65f9cc16d049abf17dae26c9d2a3fc02487ef67e8bdbac` |

The probe source digest is
`573dab432184be947c1ff295d569928cea8ef15f9a999865967667da7566e455`.
Stock and 280/70 UF2 hashes are respectively
`6b52d42a1fa438bb3e9e5bdbb66d46e715d1d2f03ec573387685e22b3f302223`
and `46f4656e554e7d1034db183794849dcb80befc2a9fd29d60e5d2474a3b3666d1`.
The image maps exclude the 98304-byte capture region and the separate settings
region. This preserves the allocation contract; no physical flash or byte
preservation test was performed in this wave.

Probe named bounds are boot 17128, core 0 17248, radio 24648 and render 17280
bytes. Its emitted nonradio total is 12136 bytes, **152 bytes below** the unchanged
12288-byte regression gate; the largest emitted frame is 1512 bytes. Dashboard
named bounds are boot 3336, construction 18736, runtime 15632, dashboard 28200
and radio 17352 bytes. Dashboard headroom is **392 bytes above** the unchanged
65536-byte floor. Both retain two 153600-byte buffers, a 32768-byte core 1 stack
and 8192/4096-byte core 0/core 1 reserves. These tight software bounds remain
separate from physical stack high-water and whole-program instruction proof.

These checks do not measure RF behavior, electrical margin or panel scanout.

## Next physical evidence

After a separately authorized checked-image flash, repeat inspection of a known
advertising peer and save the full result, including link/termination pages.
Normal completed work may be green; `0x22` must remain red and retain inventory.
If first-boot red recurs with `HCIbad:0`, capture the LED fault ledger before Y or
reboot, and save to an empty slot when a selected peer is available. Preserve
slots 1 and 2. For the missing TV, compare a separate scanner and record its
power state and other app connections. The
[bench runbook](pico2-dnuc-runbook.md#standalone-probe-captures-and-diagnostics)
keeps these physical checks separate from software verification.
