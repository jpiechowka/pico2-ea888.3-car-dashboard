# NGM identity pre-flash audit, 1 November 2026

Baseline: `1170350785c7c076bf23e0893b71e32d397368a4`. Scope is the complete
`ngm-identity` image and its shared probe/dashboard dependencies. Five defects
were reproduced and corrected. This was an offline audit: no device access,
flashing, reboot, BLE connection, slot mutation or ECU traffic occurred.

Two independent senior embedded reviewers used an explicit explanation/challenge
cycle. Engineer A traced application state, cancellation and evidence. Engineer B
challenged driver, security, radio and protocol assumptions using actual source
and current primary references. For every repair, its author explained the old
path, replacement transitions, partial effects and tests; the other engineer
reread those paths and challenged the result. The integrating reviewer then read
the complete reports, production/test diffs and callers, and independently traced
the platform and persistence boundaries. A third reviewer checked capture/export.
The private work log retains the full exchanges and baseline-failure logs.

This record concerns correctness and bounded failure behavior. It is not a proof
of Bluetooth conformance, physical stack high-water or adapter interoperability.
The [utility contract](../vehicle/ngm-identity.md) retains the future garage runbook.

## Coverage and trust boundaries

| Boundary | Actual paths reviewed | Critical invariant |
| --- | --- | --- |
| Build to entrypoint | utility Cargo/build scripts, both binary entrypoints, `application.rs`, `tool.rs`, image checker | Fixed binary policy, tool/source markers, M33 target; standard binary stays standard even with identity feature |
| Boot to tasks | application, tasks, bridge, metrics/status/UI; shared watchdog and flash guard; locked HAL | One peripheral owner; core 1 only catalogue/render; retained DMA buffers; independent watchdog |
| Selection to link | model, radio, session, identity radio/flow; Trouble central/connection/GATT | Fresh address **and type**, request revision, scan-stop acknowledgment, one operation lease |
| ATT to INFO | inventory/errors, endpoint admission, CANyonero decoder/session, actual GATT/CoC host | Fresh scoped handles, four vendor reads, one configured channel, one affine INFO permit |
| Bytes to saved result | identity model/codec, complete capture admission, journal/UI/reply | Bounded raw evidence; parser validity, cleanup and live success are separate |
| Saved result to PC | storage/maintenance, USB/session, wire parser, exporter/CLI, Windows serial implementation | No radio side effects; exclusive byte leases; read-only protocol; bounded no-overwrite publication |
| Shared siblings | dashboard BLE task, standard inspector, shared vendor stack and image gates | Repair common rules once; preserve separate traffic/security policies |

Complete source was reviewed for the named utility modules and shared codec,
session, journal, export, flash and watchdog paths. The audit followed resolved
Embassy/CYW43/Trouble implementations, rather than inferring cancellation safety
from API names. It did not re-audit unrelated Studio/editor features or the entire
optional security algorithm implementation.

## Boot, concurrent owners and recovery

**A explains:** each binary includes the shared application and binds a fixed
inspector and `ToolKind`. The identity binary requires its feature and linked
discriminator 1; the standard binary uses 0 and the standard inspector. USB product
names, display titles and image admission agree with this identity. Capture mode
does not select live transport behavior. Utilities compile without security and
reject accidental unification with the dashboard's security feature context.

**B challenges:** a feature, saved record or late input must not upgrade standard
inspection into vendor traffic. The concrete entrypoint aliases and feature guard
meet this boundary; the test/build matrix includes the standard binary compiled
with identity support. No ECU command, automatic retry, pairing or CCCD write is
reachable through the identity utility's application policy.

Core 0 owns hardware; core 1 owns the catalogue/render loop. Two fixed 153600-byte
RGB565 buffers and a 32768-byte core 1 stack remain. Frame ownership crosses a
bounded queue and is returned only on valid transfer completion. A display timeout
quarantines its buffer, rather than cancelling DMA and reusing the bytes. UI work
is capped at 20 Hz, not treated as measured display scanout. Request/frame identity
and input queues are separate from the lossy 32-report scan queue; eight reliable
events and sixteen input slots remain bounded.

The W-board radio owns GP23/24/25/29, PIO0 SM0/IRQ0 and DMA1. Display SPI0/DMA0
uses GP18/19/16/17/20; PIM715 RGB uses GP26/27/28; buttons use 12–15 and encoder
I2C0 uses GP4/5. No VSYS ADC sampling takes the shared radio pin during this path.
These mappings were checked against the [Pico 2 W pin table][board] and actual
initialization, not non-W LED/VBUS assumptions.

The CYW bus runner is constructed once and joined with protocol work. An operation
timeout does not drop that permanent DMA-owning runner. The locked PIO driver waits
for DMA/status before releasing CS; a hardware stall is left to the watchdog.
The radio runner yields and checks its maintenance boundary, services receive after
each selected branch, and bounds an HCI drain to four packets. Ring pointers,
alignment, declared length, complete availability and queue capacity are checked
before publication. Queue-full transmit retains the packet; malformed receive
faults do not trigger an in-place ring repair.

The watchdog uses 15 s during startup and 2 s during normal supervision. The wrapper
corrects the HAL's inherited RP2040 reset mask to the RP2350 mask, uses a 1 MHz tick
and disables debug pause. Its 50 ms supervisor requires actual completed display
and UI/button/encoder/radio progress, with a 12 s radio-start allowance. First fatal
cause remains sticky and stops feeding. Retained scratch 0–3 uses checked commit
metadata; ROM scratch 4–7 is not repurposed. [SDK watchdog source][watchdog] confirms
that deliberate reboot also uses the watchdog and scratch markers. TIMER/normal
boot evidence remains ambiguous: it is neither proof of a missed feed nor grounds
to silently clear a reported fault. The observed post-flash red indication is not
claimed fixed by these transport changes.

## Live state machine and cancellation

The outer `Session`, pending publication and connection owner survive cancellation
of inner work. The exclusive operation gate is held through full cleanup; it
excludes flash mutation. A revision change cancels work and filters old model
events, but never substitutes for transport termination.

| Stage | Admission and bound | Failure/cancellation behavior |
| --- | --- | --- |
| Selected peer | Explicit Inspect; advertisement at most 30000 ms old; exact address/type | Stale/no selection cannot connect; saved handles never enter the flow |
| Connecting | Reliable Start plus connect within one 8 s deadline | Start remains pending if publication is cancelled; wrong peer or connection failure still cleans up |
| Discovering | 30 s total; GATT startup capped 3 s; 24 services/64 characteristics/128 descriptors/16 standard values | Syntax, order, scope or capacity failure keeps partial evidence and stops vendor admission |
| TransportParameters | Exactly one fresh FFF1 service and one readable 0081/0082/FFF2/FFF3 each; 15 s total, 3 s per read | Duplicate/missing/cross-service handles reject; failures clear unacknowledged bytes; terminal errors stop subsequent I/O |
| OpeningCoc | One configured SPSM 129; 5 s; local receive MTU 245/MPS 247/credit 1 | Refusal retains available status; no alternate SPSM or automatic retry |
| SendingInfo | One affine `1F110000` permit; 2 s | Partial or uncertain submission cannot reuse the permit; submission evidence is local, not peer acknowledgment |
| AwaitingInfo | One 2 s absolute budget across all owner-delivered SDUs | More bytes never renew time; empty/malformed/oversized/negative response faults preserve bounded raw evidence |
| Closing | 5 s, with inner GATT/channel owners dropped before outer disconnect | Full link idle, local ingress error and terminal reason are checked independently |
| Finished | Terminal outcome, successful or failed; success additionally needs clean identity/ordinary inspection and normal termination | First fault survives clean cleanup; saved evidence never produces live green |

**B challenges:** dropping a timeout wrapper is not proof that the borrowed
operation has been destroyed. The helper races a pinned borrowed future; callers
end the owning scope before conflicting work or disconnect. A regression verifies
this owner/drop distinction. Generation changes are polled every 10 ms, not claimed
instantaneous. Active work uses the current stage deadline; pending observations
can flush under cleanup and final-publication rules below. Read/channel/send
timestamps are captured before publication can block.

Vendor reads retain at most 64 bytes; a full buffer conservatively means truncated.
Protected and optional unavailable outcomes remain inspectable and prevent clean
success, even if INFO later parses. A timeout/error after a partial copy cannot
publish the unfinished buffer. Database Out Of Sync is now terminal (F1 below).
Offered transport values are separate from observed directional limits; absent
CID/credit observations remain absent, not zero or reconstructed offers.

Receive returns a pool-owned complete SDU. The decoder retains at most 245 bytes,
then drops the SDU before any reliable event await. Raw publication uses chunks of
at most 64 bytes and a retained next offset; cancellation cannot duplicate or skip
already accepted chunks. Start/read/channel/send/SDU/raw observations flush in
chronological order before terminal events.

After connected work exits, outer cleanup requests full link disconnect and waits
for `connections_idle`, which includes pending create/cancel and terminal link
state. `0x13`/`0x16` normal reasons alone do not qualify success if a local receive
fault exists. Confirmed idle with abnormal termination records failed cleanup and
returns a visible failed attempt. **Unconfirmed** cleanup enters a fatal parked
path and retains the operation lease; it cannot permit a journal write. Final
evidence publication can wait after disconnection without granting new I/O.
A global HCI-runner failure can prevent some unsaved local evidence from reaching
the model before watchdog reset; persistent success is not promised on that path.

## INFO framing and channel ownership

The pinned public [header][protocol-hpp], [implementation][protocol-cpp] and
[endpoint parser][endpoint] were fetched independently. INFO uses marker 1F,
request 11/reply 91, a four-byte header, big-endian body length, and five LF-separated
fields: vendor/model/hardware/serial/firmware. The local 245-byte frame, 241-byte
body and 64-byte field limits are deliberately stricter resource policies. Empty
fields mean unavailable; one final LF is accepted. UTF-8/control checks, exact
field count, remote error types, declared length and suffix checks run before
derived values become usable. First parser/session fault is sticky. Wrong epochs
do not mutate state; reversed time, overflow and equality at a deadline fail.

The live flow returns after its first complete frame. A later suffix passed to
the session API before cleanup revokes success, but the transport does not keep
draining through cleanup. The reviewers resolved this distinction against actual credit behavior:
dequeue returns without another await/grant, so a conforming second SDU cannot
queue behind the completed first under the one-credit window. Same-SDU suffixes
are checked. Illegal ingress before disconnect can create a local fault; later
in-flight traffic may be discarded. This is strict parsing of delivered bytes,
not a complete remote-traffic recording claim.

The linked utility does not enable optimized SDU reassembly. Per-link ACL/PDU
assembly and per-channel CoC/SDU assembly have separate ownership. A dynamic
channel reserves a packet before advertising one receive credit. A partial SDU
can obtain a newly backed continuation credit; a completed queued SDU blocks new
grants until consumed. Length/MTU/MPS/credit violations reject before copying
past capacity. Close releases reserved, partial and queued packets and wakes
read, credit, transport and lock waiters. Send serializes an entire SDU and
consumes one peer credit per K-frame; ambiguous partial submission terminates
the channel. Outstanding transmit reservations count toward credit-overflow checks.

Channel slot reuse requires Disconnected **and** zero references. Create futures,
credit grants and deferred disconnect tokens retain that identity. Foreign stack
or ACL identity cannot redirect those owners. The native tests exercise real
host/channel queues with controller doubles, including 245 bytes split into eleven
legal 23-byte K-frames, zero/one credit, exhausted pools, cancellation prefixes and
all close wakeups. These are software contracts, not RF timing measurements.

## Confirmed corrections

All five corrections were explained before implementation, checked against source
and specification by the second engineer, then rereviewed with actual failing and
passing regression evidence. The parent independently reviewed the final changes.

| Finding | Baseline behavior | Correction and meaningful regression |
| --- | --- | --- |
| F1: ATT database invalidation | `0x12` became optional Unavailable; later discovered handles and potentially INFO were still used | Shared utility classifier makes only 0x12 terminal Error. Standard inventory stops before the second read; identity stops before later vendor reads/open/send. Raw ATT 18 survives capture/export and clean cleanup never creates success. Exhaustive 256-code test preserves other classifications. |
| F2: disconnect endpoint rejection | Unknown destination emitted only reason 2, omitting both CIDs; wrong source on known destination also replied | Typed six-byte reason/local/remote body; known local plus wrong remote silently ignored. Literal wire, foreign ACL and assigned/unassigned closing-state tests. |
| F3: duplicate pending peer close | PeerDisconnecting was treated as unknown CID while its first reply remained deferred | Reply with received identifier while retaining original pending identifier/reference/state. Same/new identifier, full outbound queue and packet-release tests. |
| F4: unrelated connect reply | Unknown identifier closed every Connecting slot on that ACL | No mutation or wake for unmatched ID; own refusal/cancellation/deadline still terminates a create. Two pending owners, zero/wrong ID, foreign link, backing-packet retention and eventual matching completion tested. |
| F5: uncorrelated close reply | Header ID discarded; matching endpoints could close before the local request was assigned/sent | Disconnecting carries 0 (unassigned) or one assigned nonzero ID, retained through cancellation. Response requires ID/link/both CIDs; exact match retains runner reference until confirm/drop. Actual canceled/repeated sends reuse identical wire ID; local completion still adds no ACK wait. |

F2–F5 are shared Trouble fixes, inherited by both utilities and the BLE dashboard.
Standard probe/dashboard currently use ATT instead of this INFO CoC, but incoming
signaling still reaches the common host. F1 is shared by both utility inspectors;
the dashboard already propagates ATT failures, invalidates its connected epoch and
disconnects before its existing reconnect/discovery route. No duplicate dashboard
classifier was added. Neither shared repair enables new ECU traffic.

Reference checks: [GATT 2.5.2][gatt] covers invalidated attribute information;
[L2CAP 4, 4.1, 4.6/4.7][l2cap] covers response identifiers, duplicate requests and
reason-specific endpoint handling. The utility does not enable robust caching, so
F1 is defensive receipt handling, not a claim that a conforming server ordinarily
generates 0x12 here. These are transaction/conformance defects; no memory-corruption
finding or peer authentication claim follows from the fixes.

## Save, overwrite, reset and export

**A explains:** capture storage occupies 96 KiB at offsets `0x1E6000..0x1FE000`;
four slots each have two 12 KiB copies, with 8192-byte maximum payload. Save erases
only the older/inactive copy, verifies header/payload, writes a final commit page,
then reinspects. Header, physical location, generation, reserved bytes and CRCs
must agree. Unknown format in either copy blocks replacement. Generation never
wraps; equal generations are ambiguous.

**B challenges:** a failed save cannot imply rollback. Failure after commit can
leave the new generation valid. On a first save there is no old generation: the
only admissible outcomes are erased, corrupt/reset-required, or fully committed.
The new fault-injection sweep cuts every storage call with representative torn
prefixes, checks unchanged other-slot bytes, rejects ordinary repair of corruption,
and verifies explicit Reset then Save recovery. Existing replacement/Reset tests
remain; physical power-cut qualification is still open. Reset intentionally cannot
retain its selected slot atomically.

Overwrite confirmation binds slot/generation and live revision for ten seconds.
Four occupied slots need no fifth scratch slot. A 30 s UI operation timeout or Back
hides work but does not cancel the storage owner or release its pending fence.
Only its matching completion releases ownership and updates the original slot.
One affine reply envelope carries the UI 8192-byte lease; USB has a separate lease.
Wrong/stale replies still return their bytes without redirecting state.

Flash maintenance first inhibits frames, obtains core 1/UI quiet and completed
display idle, and parks encoder and CYW at actual transaction boundaries. Each
4 KiB erase or 256-byte program requires a fresh watchdog-entry ticket; resumed
producers must make fresh progress before another step. The application guard
checks core/thread/interrupt state, approved JEDEC/QMI settings, all sixteen DMA
channels including chains, peripheral/CS state and SRAM sources. HAL then parks
core 1 through its SRAM FIFO handler **before** entering the critical section and
disabling XIP. SRAM/ROM helpers restore XIP before returning. The [SDK RAM lockout
handler][multicore] and [flash API requirements][sdk-flash] corroborate these
ownership needs. HAL waits are not intrinsically timed; the armed watchdog is
the independent bound. Linked flash-helper placement/mutation checks remain gates.

USB exposes only LIST and GET 1–4, never save/reset/inspect/reboot. Epoch checks
occur before and after endpoint polls; reset invalidates stale operations while
storage ownership is still returned. Checked epoch exhaustion closes admission.
An aborted USB response waits for a bus change, not just reopening the COM port.
UI can select an older supported sibling where documented, whereas raw USB selects
the newest journal-valid copy; an unsupported payload can consequently be rejected
by the current exporter. Neither route rewrites it.

The host validates role/slot/generation/length/CRC and full capture semantics
before publication, with no framing resynchronization or automatic retry. One
deadline covers writes and reads; retryable I/O never resets it. New tests verify
exact unsent-suffix handling and positive progress after expiry stopping further
reads. Blocking calls are finite in the configured serial adapter, but the deadline
is checked between calls: it can be exceeded by one call, and a late terminal
read is not post-checked. No hard-real-time claim is made.

Artifacts use a fresh directory and individual [create_new][create-new] writes,
sync binary/JSON/text, then write a COMPLETE marker. This prevents individual-file
overwrite, not atomic multi-file publication or universal power-loss durability.
Names are bounded ASCII with a fixed prefix, address/type and content suffix;
INFO serial/model cannot rename files. Derived fields require all owner-observed
raw bytes and a valid frame. Legacy v1–v4 clears identity state; both utilities
retain v5 and unknown-format protection. Saved data is never live authorization.

## Diagnostics and presentation review

The shared probe/identity header reports current Pico measurements; capture bodies
hold frozen boot-relative observations. Static RAM comes from linked boundaries;
SP values are samples, not stack high-water. Work percentages count instrumented
poll/work wall time and exclude cooperative awaits; they are not total CPU-cycle
utilization. Unavailable/time-reversed observations remain distinct from zero.
The bounded publication and discarded-render paths retain coherent timing data.

Faults have atomic priority and remain latched until two-second Y recovery. Blue
means scanning. Green requires clean **live** inventory/identity plus terminal
cleanup; viewing a saved success is amber. Pending flash/storage work retains
its own indication and owner even after the page is hidden. Watchdog reset cause
ambiguity is preserved. Existing [synthetic gallery][gallery] remains current:
these changes alter error handling, not geometry or widget behavior, so no new
screenshots are presented as hardware evidence.

## Integrated verification and images

All checks below passed on the final audited sources with `nightly-2026-10-18`.
Local native execution used Windows/MSVC. Counts overlap between feature contexts;
they are not a total of unique tests. Prior implementation receipts are historical,
not admission for these changed sources.

| Verification | Result |
| --- | --- |
| Root `cargo test --locked --workspace` | 487 tests/doctests passed |
| Root configuration JSON context | 136 tests passed |
| Host default and no-default-feature suites | 833 tests each passed |
| Trouble utility, security, and security/legacy/optimized contexts | 244, 296, and 305 unit tests passed; the optimized context includes two isolated child-process diagnostic checks. An independent reviewer repeated the 244-test utility context. |
| Focused capture/journal/export matrix | 112 tests passed, including first-save cut/tear and bounded transport regressions; included in broader host coverage |
| Strict Clippy | Root workspace/all targets, host/all targets, vendor utility/security, both utilities at stock/280-70, standard binary with identity feature, and dashboard BLE/280-70 passed |
| Cross-target shared code | Core no-default-feature Wasm and Cortex-M33 checks passed |
| Rust documentation | All 22071 inventoried items across 699 files covered; strict private-item rustdoc passed for affected root/host/vendor and ARM utility/dashboard contexts. Broken intra-doc links are errors. |
| Python | All 175 sources passed hash-pinned Ruff lint/format and strict Mypy; image/provenance/feature-guard regressions passed normally and with `-O` |
| Flash helper artifact mutation tests | New NGM and dashboard positive ELFs passed normally and with `-O`; altered instruction/placement/vector fixtures rejected |
| Source and profile admission | HAL, radio, Trouble, firmware blobs and options verified; no-security feature-unification guard passed |

Global packet-pool tests were executed serially; an initial parallel vendor run
contended on the shared test fixture and was replaced with the serial context.
Initial integration also exposed six missing comments in new test helpers, which
were corrected before the final inventory and image builds. A provenance check
started before the overlay hashes were refreshed was rerun successfully. Windows
initially denied linker output creation; fresh offline capture directories under
normal tool access passed. The Python checks used the existing repository-local
quality environment after the system interpreter lacked Ruff. No product guard
was relaxed to resolve these tool/context failures. Strict rustdoc passed, while
Cargo still reports its existing same-name binary/library output warning and
the HAL dependency emits two existing exported-`inline` compiler warnings.

Utility captures used `firmware/scripts/build_ble_probe.py`, with `--expert` for
280/70, and the extra binary-only LLVM inline threshold required by that wrapper.
Every target retained `thumbv8m.main-none-eabihf` and
`-C target-cpu=cortex-m33`. All four ELF/UF2 pairs passed program-map equality,
tool identity, source/options, conservative stack and flash-helper admission.
Dashboard used `capture_target_bench.py --firmware --image-gate practical
--features ble-obd,cpu280-spi70-1v30 --expert`; its result is ELF admission, not a
new dashboard UF2 or device receipt.

| Image / local capture directory | Static RAM, bytes | Main-RAM headroom, bytes | ELF SHA-256 |
| --- | ---: | ---: | --- |
| NGM stock / `ngm-s-261101-a2` | 410880 | 113408 | `d29ed0d27150880fc038c2857ca4533c0a528314553694150a5af9cfb8c5fdfe` |
| NGM 280/70 / `ngm-280-261101-a2` | 410880 | 113408 | `f85e467d26be1f15bc5968fd0ea50d783bcc8d9dec7af5e087e8cdaf52c38d22` |
| Standard probe stock / `probe-s-261101-a2` | 410120 | 114168 | `b53c6828fd13b35dbb2f698c2b12d18a76ee03a0ec13112cbf9e317baca8cc8e` |
| Standard probe 280/70 / `probe-280-261101-a2` | 410120 | 114168 | `79ef97c6d7ab683e5ed8e1e0122717150b6dc9d5bfe1d8b970d31b4db134c6db` |
| Dashboard BLE 280/70 / `ngm-dash-280-261101` | 418624 | 105664 | `d585b880411f406d335ce501c27d807c245cc004ffc1527ddc3f15742b6cf031` |

The utility directories are under `firmware/target/ble-probe-captures/`; the
dashboard directory is under `firmware/target/bench-captures/`. These local
receipts and binaries remain ignored. Utility source digest:
`bd08d5f2343de147d041a081427cc686d91391726b2663be3bd6256a57775fde`.
Dashboard source-context digest:
`aa3dc499729b64bca99f6c3990a5878e1a9c7585bd9d6ddd5f75ca6c9eb4be7a`.
These bind the reviewed snapshot, not a complete compiler-input closure.

| Utility UF2 | SHA-256 |
| --- | --- |
| NGM stock | `7641f750cca375446c4abd44af70e6fc9e1370c687de10b3cd91fdffacb56f3f` |
| NGM 280/70 | `20efb80f30fe8e5b1933fe2f4d0d3b917ec65ec6eff7f808e62bc55570011c2b` |
| Standard probe stock | `343230bc25fa0321183cf3c8430ccfdca80defd382fe1065f2a918c23b2ceae5` |
| Standard probe 280/70 | `05de0f90dc2787fabd023adf91da2122c2a6f13c513a6393ffc063d642739c10` |

NGM emitted-frame totals are 20204 bytes overall and 12128 excluding the reviewed
radio frames; the largest frame is 1592. Standard probe totals are 18764/11968,
largest 1688. The unchanged ceilings are 20480/12288/8192 respectively: NGM has
276/160 bytes of aggregate allowance left, so future call-path/allocation changes
require fresh admission. These aggregate figures are conservative gate inputs,
not observed stack usage or a call graph proof. Named utility path bounds are
boot 17112, core 0 17208, radio 24648 and render 17416 bytes, under their existing
separate reserves. Main-RAM headroom is not additional core 1 stack space.

The two fixed framebuffers, 32 KiB core 1 stack, 64 KiB minimum main-RAM headroom,
conservative stack reserves and separate program/capture flash ranges remain.
Program erase/verification/UF2 ranges exclude all four saved slots. This verifies
image boundaries; byte-for-byte preservation on a particular device still needs
the separately authorized backup/flash/readback procedure. No device was accessed.

## Remaining limits

- Candidate SPSM 129 is the public endpoint parser's configured default. Dynamic
  SPSMs 0x0080–0x00FF ordinarily come from GATT on reconnect ([Table 4.15][l2cap]);
  the four opaque vendor values have no qualified SPSM decoder. This is a manual
  compatibility trial, not standards-conformant dynamic service discovery.
- The utility never subscribes/bonds for Service Changed and has no consumer
  invalidating handles on an unsolicited indication. F1 does not establish every
  conceivable database-change defense. No conforming subscribed path is enabled.
- The identity extension and outer capture assessment have independently checked
  scopes; not every outer/inner end-time and cleanup combination is correlated.
  An externally crafted capture can contain contradictory observations. Both
  scopes remain exported; saved success cannot turn the live LED green or enable
  commands. No live producer path to the contradiction was established.
- Host captures are not complete HCI/L2CAP transcripts. Lower-layer discarded
  traffic and unsupported/reserved refusal codes can be absent. A selected peer
  can exhaust bounded resources or stop responding; visible failure is preserved,
  not availability against a hostile peer.
- Actual NGM CoC acceptance/INFO, USB/RF timing, stack high-water, electrical
  margins, flash endurance and physical interruption recovery remain unverified.
  No vehicle measurement or ECU support follows from an identity response.

## Primary sources checked

Accessed online on 1 November 2026; exact upstream code pins below were retrieved
without falling back to a different revision. Locked dependency code and callers
were checked separately. External instructions were not treated as authorization.

- [Bluetooth GATT][gatt]: database invalidation receipt and discovery/read semantics.
- [Bluetooth L2CAP][l2cap]: signaling, dynamic SPSM allocation, directional limits,
  K-frame/SDU and credit behavior.
- [CANyonero Protocol.hpp][protocol-hpp], [Protocol.cpp][protocol-cpp] and
  [Python endpoint parser][endpoint]: framing/fields and candidate provenance.
- [Pico 2 W datasheet][board], [Pico SDK flash APIs][sdk-flash], pinned
  [multicore lockout][multicore] and [watchdog implementation][watchdog].
- Rust [create_new][create-new] and [sync_all][sync-all]; Microsoft
  [file-name rules][filenames] and [COMMTIMEOUTS][timeouts], compared with resolved
  serialport 4.10.1 Windows source and embassy-usb 0.6.0/locked RP2350 USB code.

[gatt]: https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/Core-61/out/en/host/generic-attribute-profile--gatt-.html
[l2cap]: https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/Core-62/out/en/host/logical-link-control-and-adaptation-protocol-specification.html
[protocol-hpp]: https://github.com/Automotive-Swift/Swift-CANyonero/blob/78091bb5599c4f5e3002fbd5700b826a625e8b04/Sources/libCANyonero/include/Protocol.hpp
[protocol-cpp]: https://github.com/Automotive-Swift/Swift-CANyonero/blob/78091bb5599c4f5e3002fbd5700b826a625e8b04/Sources/libCANyonero/Protocol.cpp
[endpoint]: https://github.com/Automotive-Swift/Swift-CANyonero/blob/78091bb5599c4f5e3002fbd5700b826a625e8b04/python/ecuconnect_tool/ecuconnect_tool/ecuconnect.py#L44
[board]: https://datasheets.raspberrypi.com/picow/pico-2-w-datasheet.pdf
[sdk-flash]: https://www.raspberrypi.com/documentation/pico-sdk/hardware.html#hardware_flash
[multicore]: https://github.com/raspberrypi/pico-sdk/blob/079c6f39023649b154152db30f1d781e884879bc/src/rp2_common/pico_multicore/multicore.c#L230
[watchdog]: https://github.com/raspberrypi/pico-sdk/blob/079c6f39023649b154152db30f1d781e884879bc/src/rp2_common/hardware_watchdog/watchdog.c
[create-new]: https://doc.rust-lang.org/std/fs/struct.OpenOptions.html#method.create_new
[sync-all]: https://doc.rust-lang.org/std/fs/struct.File.html#method.sync_all
[filenames]: https://learn.microsoft.com/en-us/windows/win32/fileio/naming-a-file
[timeouts]: https://learn.microsoft.com/en-us/windows/win32/api/winbase/ns-winbase-commtimeouts
[gallery]: ../vehicle/ngm-identity.md#synthetic-renderer-gallery
