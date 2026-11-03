# NGM collector offline validation, 3 November 2026

The separate `ngm-collector` image implements a locally confirmed, finite ECU
read collection. Paired implementation/review, regression checks and final
source-bound stock/280-70 ELF/UF2 admission pass with the original resource limits.
The exact artifacts and final software matrix are recorded below. Publication is
verified separately against the containing commit's CI run; these local receipts
are not device, ECU-support or physical-safety receipts.

This phase performed no device connection, USB enumeration, BLE/CAN exchange,
flash, reboot or capture-slot operation. It establishes neither exact DNUC support
nor proprietary adapter behavior, physical stack high-water, display scanout or
power-interruption recovery. Earlier [INFO-only validation](ngm-identity-audit-2026-11-01.md)
and the [full firmware audit](full-firmware-audit-2026-11-02.md) remain historical
evidence for their recorded source and scope.

## Scope and identities

The [collector contract](../vehicle/ngm-collector.md) specifies the complete
manifest, literal requests, controls, deadlines and qualification limits.
Three linked identities remain separate:

| Image | Tool discriminator | Permitted user operation |
| --- | --- | --- |
| `ble-probe` | 0 | Selected-peer standard GATT inventory and allowlisted reads |
| `ngm-identity` | 1 | Standard inventory, four scoped vendor reads, one INFO |
| `ngm-collector` | 2 | The same adapter-only Inspect, or separately confirmed fixed collection |

Compiling the collector feature does not change another binary's selected tool
kind. Radio dispatch and the collector owner both check the explicit action and
tool kind. Boot scans; stored data, USB export and ordinary Inspect cannot grant
collection authority. No utility enables pairing or bonding. The collector USB
product is `Pico NGM Collector Captures`; its exact name and permitted Windows
port suffix are covered by host device-selection regressions.

## Paired engineering walkthrough

The review used two engineering roles: the implementation owner traced the
intended sequence and ownership, while a separate reviewer challenged actual
callers, failure transitions and emitted bytes. Findings were returned to the
responsible owner and checked against production-path regressions. Some later
adversarial passes were by the original author of the core being reviewed;
those passes supplement cross-review and are not organizationally independent
certification. No claim of two physical hardware trials is made.

The principal trace covers [controls/model](../../crates/dashboard-core/src/discovery/model.rs),
[UI/presentation ownership](../../firmware/ble-probe/src/ui.rs),
[cross-core bridge](../../firmware/ble-probe/src/bridge.rs),
[radio integration](../../firmware/ble-probe/src/identity_radio.rs),
[portable collection flow](../../firmware/ble-probe/src/collection_flow.rs),
the [closed command owner](../../crates/dashboard-core/src/ble/canyonero/collection.rs),
the locked transport, and [capture admission](../../crates/dashboard-core/src/discovery/capture.rs).

| Boundary | Authority and outgoing traffic | Failure/evidence contract reviewed |
| --- | --- | --- |
| Prompt | Separate B hold opens a prompt for an exact peer/address type and request generation. It sends nothing. | A new confirmation input must follow this prompt's completed display-driver transfer. Changed context, backward time, stale advertisement or expiry revokes consent. |
| Expired prompt | No collection or Inspect fallback. | The expired screen remains visible; stale clicks are ignored until deliberate navigation or a new prompt. Transfer completion is not measured LCD scanout. |
| Fresh prelude | One selected connection performs full standard inventory, four scoped vendor reads and configured SPSM129 INFO `1F 11 00 00`. | Missing/ambiguous endpoints, limited inventory, incomplete vendor/INFO evidence or failed local submission prevent collection. Saved handles are never reused. |
| Capacity | Core 1 reserves complete bounded evidence for the same request generation before any collection command, including Voltage. | Admission consumes prerequisite updates before deciding, uses fresh monotonic time and rejects after its deadline. Rejection creates no collection send permit. |
| Setup | One Voltage `12`, raw CAN OPEN `30`, and arbitration `34`. | Exact response type, length, current handle and state are required. Remote input cannot choose a protocol, address, bitrate or command. |
| Session observation | First request is `22 F1 86`; only exact `62 F1 86 01` permits later reads. | A well-formed denial/nondefault result stops later reads and permits only the matching known-handle CLOSE. No session-control fallback is sent. |
| Read loop | One outstanding single-DID `22` request from the closed 28-entry manifest. Correlated response reception may authorize bounded ISO-TP CTS. | Strict envelope, service/DID, handle, full CAN ID, sequence, length and deadline checks precede progression. Already queued complete unexpected SDUs stop the next request or CTS. |
| Cancellation/uncertainty | Further permits are revoked; no retry or speculative CLOSE. | Attempted-but-unconfirmed local submission remains unknown. The same evidence generation survives cancellation until terminal publication and actual cleanup. |
| Completion | Matching CLOSE acknowledgment, completed reads and actual link cleanup are separate observations. | An early session stop can retain a valid terminal capture without becoming successful. Green additionally requires all 28 positive, complete rows, exact F186 result, no fault and complete terminal evidence. |
| Save/export | Explicit local storage/export only. | No BLE/CAN request follows from saving or reading a slot. Saved results never grant live green status, handle reuse or measurement qualification. |

### Closed egress contract

The fixed route is raw classical CAN protocol `00`, 500,000 bit/s, standard
physical request `7E0`, response `7E8`, mask `7FF`, with zero extensions.
It deliberately does not select adapter ISO-TP `01` or automatic-flow-control
raw mode `05`. Each ECU request is an eight-byte single frame
`03 22 DID_HI DID_LO 00 00 00 00` inside a fixed CANyonero SEND envelope.
The only response-derived outbound field is the fresh OPEN handle; it cannot
select a different command or route. Private affine permits and current-owner
checks guard the final local writer. There is no arbitrary packet console.

The [closed catalogue](../../crates/dashboard-core/src/ble/canyonero/reads.rs)
contains nine identity/session entries, seven widget-family entries and twelve
owner-confirmed OBDeleven channels, each exactly once. The owner's confirmation
that those twelve channels work in OBDeleven is accepted. It does not independently
establish their raw widths, signedness or scaling. Unknown schemas and unexpected
widths remain raw; optional exact-width family interpretations are unqualified
and never enter dashboard acquisition or alarms. DSG oil and unresolved EGT are
not replaced with speculative values. Absolute pressure is not gauge boost;
AFR requires a fuel assumption; sequential cylinder reads are not an atomic sample.

Positive UDS replies must echo `62` and the requested DID with 1–64 data bytes.
Negative replies are exactly `7F 22 NRC`. Up to three NRC `78` observations wait
within the original read deadline; they do not resend or extend it. A valid
nonpending negative after the session gate may be recorded before continuing
the finite manifest, without escalation or a numeric result.

Only an admitted, correlated multi-frame response can authorize CTS
`30 04 05 00 00 00 00 00`: block size four, advertised separation 5 ms. At most
three CTS frames are required by the admitted 67-byte UDS response bound.
Foreign, oversized, malformed or stale first frames cannot create CTS, including
overflow FC. Tester-present `3E` is a diagnostic keepalive and is prohibited;
CTS is an intentionally allowed transport response. A request SEND acknowledgment
is not accepted in place of the ECU response.

No UDS write/flash, clear-DTC, coding/adaptation, reset, security/authentication,
session change, communication control, actuator/routine, memory-read exploration,
periodic read or tester-present command can be constructed by this owner.
Adapter firmware update/reset, RPC, periodic jobs, abort-transfer, alternative
SPSM/protocol searches, functional/TCU routes and identifier scans are excluded.
The [literal policy table](../vehicle/ngm-collector.md#exact-outbound-application-bytes)
distinguishes adapter opcodes from similarly numbered UDS services.

### Timing and cancellation

Final INFO publication retains its original two-second reply deadline even after
the identity state enters Closing. A post-completion clock check rejects equality,
late readiness and backward time before handing off authority. Capacity admission
has its own two-second limit and post-completion check before the first permit.
Collection has an immutable 80-second total budget and two seconds per command/read;
ISO-TP inter-frame/CTS completion is also capped at one second. CTS and NRC `78`
do not renew the read deadline. Actual teardown has at most five seconds within
the collector's 90-second post-INFO allowance. Reliable terminal evidence may
wait after cleanup without application traffic; this is not a promise that the
UI always reaches Paused within 90 seconds.

The final writer validates the current owner before beginning submission. Local
completion means local submission, not physical CAN acknowledgment or ECU
processing. Cancellation, timeout or partial submission cannot be interpreted
as proof that no bytes left the device. First failure, latest submission state,
raw prefix and actual cleanup remain distinct.

## Findings closed through regressions

These were concrete implementation/integration defects found during review,
not merely hypothetical checklist items. The tests below exercise actual shared
model, flow, reducer, export or status code. Final whole-tree admission remains
separate from these focused checkpoints.

| Original defect | Correction and regression evidence |
| --- | --- |
| A queued click could confirm before the prompt was presented. | Prompt identity, completed-transfer acknowledgment and input observation time gate consent; same-batch/stale-context cases are covered by [controls tests](../../host/tests/probe_collection_controls.rs). |
| Expiry could remove the prompt and let a late click become Inspect. | Expired prompt remains visible with no click authority; `expired_visible_prompt_never_falls_through_to_inspect` reproduced the failure before correction. |
| Back changed generation and discarded terminal canceled evidence. | Stop intent retains the operation generation through cleanup/publication; [flow tests](../../host/tests/probe_collection_flow.rs) replay cancellation into a saveable capture before the next scan. |
| Admission could inspect stale prerequisites or a pre-drain timestamp. | Pending admission is retained while prerequisite events drain, with a fresh decision time and revision checks. Late/equal/reversed completion tests verify no first permit. |
| A complete unexpected queued SDU was discovered only after the next send. | A synchronous locked-transport dequeue checks the ready state before each permit, without polling/canceling an async receive. Separate-SDU duplicate and early-CF tests reject before the next request/CTS. |
| CTS authorization erased the FF/last CF needed after timeout. | The latest received envelope survives CTS authorization and failure. FF and later-block timeout tests check exact retained bytes and no extra permit. |
| Early F186 denial produced CLOSE completion rejected by capture validation. | Literal CLOSE acknowledgment is separate from all-read success; real-flow early-stop tests apply updates and encode/decode a valid capture with later rows unattempted. |
| A completed manifest with negative replies could be called successful by one layer. | Runtime and [production status tests](../../host/tests/probe_status.rs) require positive complete evidence; negative, early-stop, unknown-send, incomplete row and incomplete terminal outcomes never become green. |
| Final INFO publication/admission could cross a deadline during a ready poll. | Post-completion clock checks preserve publication effects but refuse new authority; [identity flow](../../host/tests/probe_identity_flow.rs) and collection-flow regressions cover pending, equal, late and reversed-clock cases. |
| Automatic export omitted the new USB product. | [Export-selection tests](../../host/tests/probe_capture_export.rs) cover exact product, valid COM suffix, lookalikes, wrong suffixes and ambiguity. |

## Capture, storage and resource-preserving changes

[Capture v6](../vehicle/ble-probe-captures.md#collection-evidence-v6) is emitted
only when collection evidence exists. Collection-absent output remains v5 and
frozen v1–v5 decoding remains supported. The extension retains exactly 28 rows,
including unattempted ones, with up to 67 raw UDS bytes per row, independent
attempt/submission/result timestamps and an optional terminal prefix up to 245
bytes. Unknown/absent is not silently converted to valid zero. Adapter supply
voltage is labeled separately from the candidate ECU module voltage.

The same serializers measure reservation before collection: maximal collection
data plus bounded later metadata growth must fit the unchanged 8,192-byte
payload. Insufficient capacity refuses collection; it does not discard existing
inventory or raw rows. The four-slot, two-copy journal, unknown-format protection,
explicit overwrite/Reset, affine byte leases and read-only USB recovery remain
required. Compact journal selection retains immutable copy metadata and checked
selection; payload loading revalidates the selected copy and checksum.

The pending NRC count accompanies a completed row header. Timeout/incomplete
rows may retain zero despite earlier pending replies. A terminal prefix is the
latest retained envelope or payload, not a full CAN/ISO-TP transcript. Unsaved
RAM evidence can be lost on reset or unconfirmed cleanup.

Resource refactoring uses a shared borrowed ISO-TP completion parser, resident
model/evidence/decoder owners, in-place row initialization, compact timestamp and
journal views, and shared bounded formatting. Nested GATT relationship lookups
borrow narrow fields only after the complete record-validation pass; public
records, checked framing, allowlists and cross-record validation remain intact.
The RP235x flash work preserves scoped SRAM boot2 copies and existing XIP-off
ownership; it does not execute directly from BOOTRAM or add mutable global boot2.
These source properties are not a stack or flash admission result. No slot,
framebuffer, headroom, stack-classification or image limit was relaxed.

## Executed software evidence and its limits

The final semantic batch precedes only documentation/format corrections and the
final measured inline-boundary annotations; the subsequent target checks and
source-bound artifacts include those corrections. The earlier focused tests
below record defect-specific evidence, not additional independent hardware trials.

| Check | Recorded execution/result |
| --- | --- |
| Paired core/flow/control/status review | Literal full traces, positive/negative/malformed responses, queue ordering, cancellation, late completion, actual cleanup and UI authority regressions were run as their fixes landed. Source tests are linked above; counts changed as regressions were added. |
| Latest borrowed capture lookups | Root `cargo test --locked -p dashboard-core --lib borrowed_lookup_tests`: 2 passed. Host `cargo test --locked --test probe_capture --test probe_capture_semantics --test probe_collection_captures --test probe_ngm_export --test probe_capture_identity`: 55 passed. Core lib/tests strict Clippy and owned formatting checks passed at that checkpoint. |
| Capture representation | Compact row timestamp tests exercise all eight presence combinations and zero/maximum values; frozen wire bytes and legacy/current semantic/export fixtures remain covered by [collection capture tests](../../host/tests/probe_collection_captures.rs) and codec tests. |
| Borrowed parser/lifetime | Owned/borrowed ISO-TP parity, error/abort/reset behavior and compile-fail lifetime coverage were executed. The existing owned API retains its original payload bound for sibling callers. |
| Final broad Rust batch | Root workspace: 543 tests; config JSON: 136; host default: 891; host no-default: 891. All passed, none failed or ignored. Root/host all-target strict Clippy, affected format checks and configurator Wasm strict Clippy passed. |
| Locked transport contexts | Native security/default-pool and discovery/no-security contexts recorded 337, 277 and 277 test executions. Queue ownership, empty queue and foreign-stack nondequeue regressions ran in all three. Security and no-security features were tested separately. |
| Final ARM source contexts | Strict Clippy and private rustdoc passed separately for standard, identity and collector binaries plus dashboard `ble-obd` and `demo` (ten commands). Two existing HAL dependency warnings at `time_driver.rs:46/51` concern ignored exported-function inline attributes; selected-package Clippy/rustdoc passed without relaxing flags. |
| Final Python/provenance | All 175 tracked Python sources passed hash-locked Ruff lint/format and strict Mypy. Forty-seven focused script tests passed normally and under `-O` (94 executions). Current HAL overlay, original archive hash, CYW43/Trouble/bt-hci sources, radio firmware/NVRAM and licenses passed provenance checks. |
| Final private rustdoc | Core all-features, host library/examples and the ARM contexts above passed strict private-item documentation. Source inventory: 0 uncovered of 24,004 Rust items across 748 files. Seventeen rendered pages/2,292 local HTML links and 359 Markdown references were checked; presence/rendering is not proof of semantic accuracy. |
| Synthetic gallery | Three production-rendered 320×240 confirmation/result/expired PNGs were visually inspected; PNG integrity and exact pixel parity with the Rust-rendered originals were checked. They establish neither physical display behavior nor ECU values. |

The [synthetic gallery](../vehicle/ngm-collector.md#synthetic-renderer-examples)
is reproducible through the
[host example](../../host/examples/ble_probe_gallery.rs).
The [capture handoff contract](../vehicle/ble-capture-studio-handoff.md) keeps
raw observations and unqualified interpretations separate; Studio import and
vehicle-profile qualification remain future work.

## Final linked image admission

Both collector wrappers completed with `STATIC_IMAGE_CHECKS_PASS`,
`source_bound=true`, `uf2_checked=true`, `hardware_qualified=false` and
`complete_instruction_proof=false`. Artifacts remain local under
`firmware/target/ble-probe-captures/` in the named directories below. Each retains
its manifest, source inventory, command log, clock metadata and image result.
The source digest binds the wrapper's inventory, not a complete compiler-consumed
input closure or a claim that the working tree was already committed.

| Evidence | Stock | Expert 280/70 |
| --- | --- | --- |
| Capture directory | `ngm-collector-20261103-stock-final` | `ngm-collector-20261103-280-final` |
| Generated CPU/SPI | 150/37.5 MHz | 280/70 MHz |
| Selected voltage | 1.10 V | 1.30 V |
| ELF SHA256 | `90e04da6dc1a8ca4e9e25e2bc3ac9a872f50bc8e005e32c5077f9b1f107f1769` | `b888c32146bc25d0e00b894723bd0a62a131677030749b0bbf18353756706008` |
| UF2 SHA256 | `54baca3cc6c2cd38546ea19955ae687572622f5584bfe3ea176f053436451679` | `e2746826cf8d552a158193a77acf0c76df11654b1ebe04a080e442ca7ba17d43` |
| Source digest | `f53316b475feb5d939b53623e50ba9640befc17740fa6dc49a8bcb47e7d3b1b2` | Same |

Both use `nightly-2026-10-18` (`rustc 8006091eb`, LLVM 23.1.4),
`thumbv8m.main-none-eabihf`, Cortex-M33, release optimization 3, strip none and
the existing binary-only LLVM inline threshold 100000. Stock requests 62.5 MHz
SPI; the configured divisor yields 37.5 MHz. Neither figure is an oscilloscope
measurement. The pinned picotool converter only produced files; no device was
opened. Rejected compiler/outline experiments were not retained as policy changes.

| Utility image | Aggregate / 20,480 B | Nonradio / 12,288 B | Largest / 8,192 B | Static RAM | Main-RAM headroom |
| --- | --- | --- | --- | --- | --- |
| Collector stock and 280/70 | 20,464 | 12,144 | 1,616 | 413,048 | 111,240 |
| Standard stock, collector feature compiled | 18,228 | 11,588 | 968 | 411,424 | 112,864 |
| Identity stock, collector feature compiled | 20,472 | 12,152 | 1,616 | 413,048 | 111,240 |

Aggregate admission margin is only **16 bytes** for the collector and **8 bytes**
for the identity feature-union check. These are sums of emitted frame metadata,
not physical stack high-water or remaining runtime stack. Any source/compiler
change requires renewed admission. Named conservative bounds are boot 17,120,
core 0 17,128, radio 24,648 and render 17,384 bytes. Both 153,600-byte framebuffers,
32,768-byte core 1 stack, core 0/core 1 reserves (8,192/4,096) and all limits remain.
Reviewed flash pause/JEDEC/write bytes, SRAM startup copy and IRQ25 vector passed.
Collector erase ranges end at `0x101D2000`, below the unchanged 98,304-byte capture
reservation beginning `0x101E6000`; ELF and UF2 exclude all four saved slots.

Sibling standard/identity checks above are **ELF-only**, with exact linked tool
discriminators, not source-bound UF2 or hardware receipts. Their preserved ELF
SHA256s are respectively `b655e7324c9eec78dbfd465c351e857666b224ecc2cf63de460e996e63f47e14`
and `b54db5ac8567cfaf978b2a8cf32f429b1c56a0a7b750391b92a3707ea4fc4acf`.
Strict selected-feature contexts were built separately from the pairing-enabled
dashboard.

Fresh dashboard practical captures also passed with source/options/configuration
binding and flash artifact mutation checks normally and under Python `-O`:

| Dashboard capture | Static RAM / headroom | ELF SHA256 |
| --- | --- | --- |
| `dash-ble-280-261103-final-a2` | 417,480 / 106,808 B | `3a3fbf6385baa579aa3f2831cbc95fc55c83a465c8dfcff96c63e42eb205b058` |
| `dash-demo-stock-261103-final-a1` | 401,640 / 122,648 B | `d79f651cbe0a5bda82cfbdb88044888bf04a6e446f57c05a14782c8ab56f94de` |

Dashboard captures produce checked ELF evidence, not UF2 files. Initial local
linker-permission failures were retained and rerun with approved offline compiler
access; no admission guard was bypassed. The two existing HAL warnings noted above
remain. Exact-commit publication/CI is separate from these local build receipts.

Physical work is unperformed, not scheduled for automatic execution. Before any
later authorized firmware change, export and byte-verify all four existing slots
and retain their build receipts. Follow the
[preparation workflow](../vehicle/ngm-collector.md#build-and-prepare-one-useful-visit)
and the existing explicit flashing process.

## Primary source basis and remaining trust

The source review compared the literal constructors and candidate definitions
against these pinned primary implementations; it did not run their flashing or
diagnostic clients:

- [CANyonero constructors at 78091bb](https://github.com/Automotive-Swift/Swift-CANyonero/blob/78091bb5599c4f5e3002fbd5700b826a625e8b04/Sources/libCANyonero/Protocol.cpp#L295-L339),
  [mode definitions](https://github.com/Automotive-Swift/Swift-CANyonero/blob/78091bb5599c4f5e3002fbd5700b826a625e8b04/Sources/libCANyonero/include/Protocol.hpp#L82-L100)
  and [raw client setup](https://github.com/Automotive-Swift/Swift-CANyonero/blob/78091bb5599c4f5e3002fbd5700b826a625e8b04/python/ecuconnect_tool/ecuconnect_tool/ecuconnect.py#L346-L382)
  support the selected byte layouts and raw/automatic-FC distinction. The relevant
  primitives were also compared with upstream revision
  `a02493cfc7ae8a1c34e63a882c94c96feedfec8f`; newer facilities are not admitted.
- [VW_Flash parameter table at 293f46d](https://github.com/bri3d/VW_Flash/blob/293f46d782bae2ced930497889b71eb782f122cb/logs/csv/parameters_22.csv),
  [byte interpretation](https://github.com/bri3d/VW_Flash/blob/293f46d782bae2ced930497889b71eb782f122cb/lib/simos_hsl.py)
  and [identity/route catalogue](https://github.com/bri3d/VW_Flash/blob/293f46d782bae2ced930497889b71eb782f122cb/lib/constants.py)
  support family-level candidates, not this ECU's confirmed default-session access.
- [SimosTools at f99771c](https://github.com/Switchleg1/SimosTools/blob/f99771ceb0442c863601a84a282c6e67e6b311a8/app/src/main/java/com/app/simostools/PIDs.kt)
  and [MQB variables at 747b50e](https://github.com/bri3d/MQBSimosLogVariables/blob/747b50e1af650c39cd9c5f6b4d58905db6debb45/exportedPIDs.csv)
  corroborate selected definitions without qualifying every owner channel.
- [AUTOSAR DCM R21-11](https://www.autosar.org/fileadmin/standards/R21-11/CP/AUTOSAR_SWS_DiagnosticCommunicationManager.pdf),
  SWS_Dcm_00433/00434/00435/00819, documents read access/session/security/mode
  gates. [Linux ISO-TP documentation](https://docs.kernel.org/networking/iso15765-2.html)
  explains physical addressing and flow control. These describe general contracts,
  not the exact NGM server or DNUC implementation; the Linux page is not a pinned
  implementation revision.

The enforceable policy is the finite set of **host-requested application bytes**.
Public clients cannot prove private adapter internals, prior autonomous periodic
jobs, controller ACK/error/retransmission behavior, all physical IDE/RTR flags,
or absence of stale internal frames. The queue guard observes already queued
complete SDUs, not incomplete assembly, lower-layer buffering or physical arrival.
Fresh handles and matching syntax do not authenticate an adapter; fabricated
replies could advance the finite allowlist.

Raw OPEN/CLOSE changes adapter state. Active reads can wake controllers and affect
communication timers or processing. F186 reports a session at one instant; it
does not lock the session or exclude another tester. Neither this review nor a
successful build proves zero state change, all available adapter information,
correct numeric decoding or physical vehicle safety.
