# Probe captures and dashboard diagnostics audit

Historical delivery. The [20 October whole-code audit](full-code-audit-2026-10-20.md)
and [detailed diagnostics re-audit](diagnostics-reaudit-2026-10-20.md)
record subsequent fixes and current software/image verification.

Review date: 2026-10-19. This delivery adds bounded offline evidence, corrects
shared BLE failure handling and audits diagnostics on both firmware routes.
Software checks and linked-image admission are separate from device validation.
No firmware was flashed during this audit.

Later [slot-1 hardware evidence](../hardware/cyw43-transport-follow-up.md)
records owner-observed power-off/reopen and an independently successful normal
USB export. Pending hardware statements below describe this earlier audit.

## Hardware evidence and scope

The supplied follow-up photographs show successful enumeration of the TV:
three primary services, twelve characteristics and zero omitted/rejected
records. They also show six global short advertising reports. The recorded
shape has 16 remaining bytes and a declared seven-byte payload, requiring 17
bytes for that report. It remains rejected; no byte is synthesized and no device
is blamed for the global counter. Neither the photographs nor the transport
fixtures identify where the short report originated.

An old advertisement prevents a new connection attempt, but does not invalidate
an already collected GATT table. The revised footer distinguishes these states.
The TV result establishes that particular enumeration only. It does not qualify
NGM, OBDeleven, USB recovery, persistent storage or DNUC ECU access.

## Findings, fixes and both firmware consumers

| Area | Correction | Consumer and regression evidence |
| --- | --- | --- |
| HCI receive dispatch | Malformed known events return typed errors instead of panicking; unknown LE subevents are ignored; controller Hardware Error retains its vendor byte | Shared Trouble source, consumed by dashboard and probe; real receive-loop fixtures and both runner error mappings |
| Connection failure | Retain the exact failed connection status until the requester consumes it; validate packet extent before interpreting undefined failure fields | Both firmware routes; public connect/extended-connect futures, cancellation and late-completion tests |
| Radio transport | Address-backed fixtures exercise actual ring reads/writes, wrapping and malformed inner reports | Shared CYW source; test coverage added, no claimed production CYW repair |
| Live diagnostics | First, invalid, future and expired observations remain unavailable; valid idle zero remains zero; core-1 work/SP/timestamp publish together | Dashboard producer and shared/probe consumers; independently expected values and pixel tests |
| Cycle telemetry | Missing or overflowing cycles do not discard valid frame timing; FRAME2 exports `na` with retained protocol version | Shared frame/window model, dashboard producer and host collector; old FRAME1 remains readable |
| RAM admission | Store the private trace cycle-presence flag without an additional per-entry Option allocation | Dashboard traced paths; all 64 entries and the full u64 domain retained; target layout, roundtrip and ring-overwrite regressions |
| Flash exclusion | Reuse the checked hardware guard with distinct bounded settings/capture regions | Dashboard settings and probe journal; existing exact SRAM/IRQ/lockout byte checks remain mandatory |
| Discovery inventory | Preserve descriptor ranges independently of service-adapter ordering | Probe inventory; exact calls for every service range, partial/failure/capacity tests |
| Capture admission | Reject contradictory stage/timeline/handle metadata even with valid CRCs; timestamp Save after admitted events | Shared evidence model, probe save and host decoder; CRC-valid semantic mutations and positive partial controls |
| Slot recovery | Readable older evidence stays openable when another copy has an unknown format; replacement requires explicit Reset | Probe storage/UI; both physical copies, unknown nested payload schema, generation exhaustion and reset isolation |
| USB lifecycle | Check the configuration epoch before every pending endpoint poll; reclaim storage leases before dropping stale replies | Probe CDC; reset/reconfigure versus pending reads/writes and failed-session tests |
| Controls and export | Debounce held-button release, suppress duplicate short actions, provide onboard confirmation, label address byte order accurately | Shared controls, probe UI and host exporter; raw bounce/hold tests, recovery transitions and non-palindromic address fixture |

Probe-specific findings are not duplicated into unrelated dashboard features.
The dashboard receives the common wire, flash guard, diagnostic and trace fixes.
It retains its pairing-enabled optional BLE prototype; the probe remains a
separate build with the no-pairing guard. No new adapter commands or ECU traffic
are enabled.

## Offline and UI result

Four explicit slots retain a selected attempt across power removal. Each logical
slot has two copies, CRCs, readback verification and a final commit page. Saving
requires paused ownership; confirmations expire, and navigation cannot cancel
an active flash write. There is no automatic overwrite or unknown-format erase.

Normal USB CDC exposes only LIST and GET. The Rust host tool exports the original
bounded binary plus JSON and text into a new directory, publishing COMPLETE
last. The documented route uses normal firmware USB, not BOOTSEL. Pending USB
timeouts require a bus reset/replug before retry. Physical USB and power-cut
tests remain outstanding.

Evidence includes independent raw advertisement/scan-response bytes, primary
services, characteristics, descriptor identities, allowlisted standard reads,
link/MTU context, stage outcomes, available raw error codes, timings and producer
identity. It does not contain vendor command traffic, descriptor values, a
pairing/authentication transcript or ECU responses. The bounded service API can
lose its locally collected service prefix if service discovery itself fails;
that stage remains visibly incomplete.

The UI has bounded body/footer regions, explicit advertisement/GATT/saved labels,
slot state/confirmation/progress/error pages and onboard alternatives to encoder
click. Forty-four synthetic gallery images come from the actual shared renderer.
All 44 remained byte-identical through the resource-focused renderer refactor;
independent glyph and pixel regressions also passed. These images verify rendered
software output, not LCD scanout or physical control timing.

See [controls and gallery](../vehicle/ble-probe.md),
[save/export workflow and format limits](../vehicle/ble-probe-captures.md),
[adapter research](../vehicle/adapter-research.md) and
[measurement definitions](../performance/diagnostic-measurements.md).

## Executed software verification

All Rust checks used locked dependencies and nightly-2026-10-18. Firmware retains
`thumbv8m.main-none-eabihf` and `-C target-cpu=cortex-m33`.

| Scope | Checks |
| --- | --- |
| Shared workspace | Workspace tests/strict all-target Clippy; configuration JSON; core no_std ARM and Wasm |
| Host | Default and no-default tests, strict all-target Clippy and benchmark smoke; capture codec/journal/UI/USB/export/inventory and diagnostic regressions |
| Studio | Native tests/strict Clippy and strict Wasm Clippy; shared diagnostic API compatibility |
| Windows helper | Full tests and strict Clippy outside the restricted sandbox after sandbox process-access failures |
| Firmware | Strict ARM Clippy for dashboard default, BLE, Longan, demo traces and BLE benchmark traces; probe stock and 280/70 |
| Vendor | 332 passing tests across five CYW/Trouble feature configurations, including pairing-enabled and no-security paths |
| Policy/provenance | UART ownership, backend/profile exclusion, logging policy, no-security rejection and complete reviewed HAL/radio overlays |
| Python | Hash-locked Ruff lint/format; configured strict Mypy boundary; image/stack/practical admission and telemetry collector regressions, including optimized-mode checks |

The journal tests inject interruption at storage-operation boundaries and torn
mutations, corrupt header/payload/commit bytes, silent write failures, ties and
counter exhaustion. Export tests cover fragmented/truncated/wrong-slot replies,
CRC errors, timeouts, unknown schemas and no-overwrite publication. These are
bounded modeled failures, not a claim that every physical failure is proved.

Standalone upstream vendor Clippy still has existing diagnostics outside the
changed logic. No lint allowances were introduced to conceal them. The strict
application firmware Clippy gates remain in CI. Fresh exact published-commit CI
is a separate delivery gate; earlier runs are not substituted.

## Linked-image evidence

All six final captures passed their practical gates. The dashboard captures
check ELF/program-map/source-context evidence; the probe wrapper additionally
converts and checks exact UF2 equality. No dashboard UF2 transfer is implied.
The original 65536-byte core-0 headroom floor, core stack reserves, fixed
framebuffers and trace capacities remain unchanged.

| Image | Static RAM | Core-0 headroom | ELF SHA-256 |
| --- | ---: | ---: | --- |
| Probe stock | 401632 B | 122656 B | `f729a62548651776f8abd087c3b143f3c771caf83646d1ec83a46b56cbdfa1da` |
| Probe 280/70 | 401632 B | 122656 B | `c22a7ca5ae811cdef485c32c7f3a43bbd5c15b4c1ca30ee02033f33446baf5a0` |
| Dashboard demo, traces | 441584 B | 82704 B | `2a867bfeaf21ce2aa8aa16e0e4972e22eb43b4fb5f9d7000717a28acde5adb33` |
| Dashboard BLE stock, traces | 458344 B | 65944 B | `52499fc71fff4223975051aabc78ad0815ebaac4ae22ff4afe69469c4b41f87e` |
| Dashboard BLE 280/70, traces | 458344 B | 65944 B | `90304f9f7069cffcd4986337a53478006798f87a9d3f5e056c9e14f9b0455c96` |
| Dashboard BLE benchmark, traces | 458344 B | 65944 B | `75b346b51912c865eff3fddde4b78676de60e7aea76bd8e850edfc0745a1c826` |

Both probe images have an 18428-byte aggregate of emitted frames, 11908-byte
non-radio aggregate and 1504-byte largest emitted frame. Their conservative
named bounds are boot 17128, core0 17240, radio 24648 and render 17272 bytes.
The 32 KiB core-1 stack and 8192/4096-byte core-0/core-1 reserves are unchanged.

The probe stock UF2 SHA-256 is
`609bab3f5bb65f1ce199f88801e1c03dabfc339aeb6b024ce8a212d75c2d364e`;
280/70 is
`f029e2b49580824cb135ab604cad5bd0a21061443e7fd2536c8d28c05a8b7708`.
Their common embedded source-inventory digest is
`8431fe09977d6ee592a9d760511c41a8f09ec40bcabc21b725c331fa9c04e9ce`.
The pinned compiler reports rustc 1.101.0-nightly `8006091eb`, LLVM 23.1.4.

The private trace representation recovered 560 linked bytes in the traced BLE
dashboard, leaving 408 bytes above the fixed headroom floor. This small margin
is an allocation constraint for future changes, not spare capacity to assume
away. The gate was not weakened. Demo named bounds are boot 3336, constructor
18808, dashboard 28200 and core1 runtime 15704 bytes. The three BLE variants use
3336, 18736, 28200 and 15632 respectively, plus radio 17352 bytes.

Receipts were produced from one frozen worktree before publication. Their source
hash maps were rechecked against the final files. They are artifact-specific;
another host, compiler or source inventory may produce different hashes. Local
receipt directories are `firmware/target/ble-probe-captures/offline-captures-20261019-{stock,280}-final`
and `firmware/target/bench-captures/offline-audit-20261019-{demo,ble-stock,ble280,bench}-final`.
Raw build directories and private tracking are deliberately not committed.

The probe's measured binary-only inlining choice and its borrowed radio poll
boundary are documented in the [dependency/admission review](../dependencies/probe-captures-2026-10-19.md).
All emitted frames remain counted. This is conservative named-path admission,
not a complete instruction proof or measured physical stack high-water.

## Sources and remaining validation

The audit checked the actual locked HAL, USB, CYW, Trouble and bt-hci source,
including cancellation, owner acknowledgments and error delivery. Primary
references and specific limitations are linked from the
[dependency review](../dependencies/probe-captures-2026-10-19.md),
[vendor patch notes](../../firmware/vendor/trouble-host/LOCAL_PATCHES.md),
[flash contract](../hardware/flash-persistence.md) and
[adapter research](../vehicle/adapter-research.md).

Remaining device work is explicit: normal USB enumeration and export/replug,
save/load retention and interrupted writes, physical button timing, watchdog and
radio/display stress, true stack high-water, electrical margins and adapter
interoperability. Use the [physical checklist](pico2-dnuc-runbook.md#standalone-probe-captures-and-diagnostics)
with an explicitly selected and separately authorized image. The public NGM
transport lead and OBDeleven Gen3 inventory do not establish proprietary protocol
access or exact ECU support.
