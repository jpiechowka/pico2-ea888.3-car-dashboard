# Full code audit, 20 October 2026

## Status and scope

This records the audit begun at
`137989b99619133aa8695de610866ffc982045ec` on `master`. Confirmed defects have
regression tests and fixes. Source review, implementation, integrated verification
and publication are separate stages. The table below records local evidence;
publication checks bind CI to the exact pushed commit.

The initial inventory contained **852 source/configuration files and 305,390
lines**. Review covered shared crates, both firmware applications, vendored
HAL/radio/BLE source, Studio, the Windows helper, host tools/tests, Python,
PowerShell and build/CI entry points. Tests and historical executable research
were included. Binary firmware, artwork and archived measurements were checked
through their format, provenance and relevant consumers; they are not counted
as source lines read.

| Review domain | Recorded breadth | Qualification |
| --- | --- | --- |
| Shared Rust | All 204 baseline files, 49,408 lines; new history tests reviewed separately | Configuration/codecs, acquisition/state, rendering, controls, startup, captures, helper protocol and provenance. |
| Host Rust | All 123 baseline files, 30,761 lines | Examples, replay/pixel tests, capture recovery/export and benchmark workloads. |
| Configurator | All 114 assigned Rust/HTML/CSS/manifest files, 40,482 baseline lines | Browser adapters, state/drafts/history, storage, workers, preview and tests. Scripts have separate coverage. |
| Firmware/vendor partition | All 198 baseline paths, 73,689 lines | 60 first-party paths and 138 vendor paths. The initial ledger's 98 pending paths exactly match the completed remainder review. |
| Python | All 162 original paths retained; all 175 current paths have attributed full reads and matching reviewed hashes at Python closure | Includes tests, the vendor generator, historical comparison tooling and unfinished instruction research. |
| Helper, entry points and integration | Parent review ledger plus independent changed-path reviews | HTTP/jobs/process ownership, snapshot/build/artifact boundaries, PowerShell, manifests, CI and final semantic changes. These overlap other domains. |

These rows must not be summed: some files appear in more than one review domain.
Per-path records distinguish full reads, contextual reads, baseline hashes,
changed-source hashes and later integration changes. Search results, compiler
success and automated annotations were not counted as full source reads. Final
reconciliation covers **872 current source/configuration paths**: all 852
baseline paths plus 20 new ones. Of these, 831 have current complete reads and
41 have prior complete reads plus completely reviewed subsequent deltas.
All current hashes match that ledger; four matches differ only in CRLF/LF
representation, with both exact raw hashes retained. There are no unaccounted
source paths. Partial dependency contexts are not promoted into complete reads.

This was a finite engineering review of happy paths, error paths and state
transitions. It is not an exhaustive proof of all executions, a hardware
certification or a claim that every possible defect has been removed.

## Important corrections

### Shared state, Studio and helper

| Defect | Correction and regression | Consumer scope |
| --- | --- | --- |
| A valid delayed measurement cleared earlier graph history after a later presentation tick | History selects the event's surviving bucket independently of advancing the monotonic window. Three regressions cover public acquisition, gaps/faults, expired ring aliases and genuine time reversal. | Shared core, including simulator injection. Current demo/transport callers usually timestamp current-step ingestion; no physical vehicle occurrence was established. |
| Configured host preview always rendered/exported landscape | Resolve the explicitly selected view's effective orientation for both shared rendering and the PPM header. Pixel comparison covers inherited portrait and both override directions, retaining valid landscape output and invalid-view rejection. | Host preview example; Studio already used resolved geometry. |
| Oversized complete-project imports reported the smaller dashboard JSON limit | Carry the shared format limit through both browser size guards and the typed event/visible notice. The regression checks project and JSON bounds. | Studio diagnostics; this was not an admission bypass or loss of accepted project data. |
| Frozen Studio build input omitted the probe workspace member | Include the sibling through the existing bounded, hashed and locked snapshot process. A real Cargo metadata regression loads both members while retaining dashboard-only defaults. | Windows helper builds and matching Python capture source identity. Including source does not select or flash the probe. |
| Prerequisites supported `python3.exe`, while build/artifact consumers searched only for `python.exe` | Share one absolute-PATH resolver, preserving preferred-name ordering and explicit missing-executable errors. Tests cover fallback, relative entries, directories and priority. | Helper prerequisites, capture and validation, including their test setup. |

The wider review traced accepted versus invalid drafts, stale import/conversion
completion, selected view versus published canvas targets, IndexedDB transaction
completion, original artwork retention, undo/redo, bounded diagnostics and helper
capability/session ownership. Rejection still leaves values inspectable and
repairable. No new handwritten JavaScript implementation was introduced.

### Fresh dashboard and probe diagnostics re-audit

The owner requested a separate detailed re-audit after the initial whole-source
review. The [field-by-field report](diagnostics-reaudit-2026-10-20.md) follows
the real producers, timebases, cross-core copies, arithmetic, units, validity,
rendering, logs and exports. It also records source and hardware limitations.

Corrections include coherent cycle/work/time publication after discarded renders,
checked optional SPI metadata, required benchmark clock/regulator metadata,
one frozen standard-read duration before queue publication, capture-time
admission for global scan errors, unavailable future-relative scan ages and
unavailable FPS in Studio's synthetic preview. Shared validation/render helpers
serve both firmware consumers where applicable.

Independent review read the complete changed dashboard paths and main-loop
maintenance/continuation context. The original publication mismatch was
reproduced with source-extracted assignments; portable regression tests cover
the observation contract. Neither is described as a physical maintenance test.
Probe queue timing uses an actually suspended host publisher. Pixel tests use
independently specified glyphs in both orientations. No photographed watchdog
reset or missing TV was attributed to these findings.

### Python boundaries and complete typing scope

The former seven-file strict gate now discovers every tracked or nonignored
Python source through [`check_python.py`](../../host/scripts/check_python.py).
Its bounded Git inventory checks canonical relative paths, repository containment
and actual files, then supplies absolute filenames to the tools. Tests cover
malformed/escaping/option-like paths, duplicates and inventory bounds. CI uses
the same entry point, so adding a directory does not silently omit it.

All **175** current files pass strict Mypy 2.4.0 with normal import following
under both Windows and Linux platform settings. Precise records, protocols,
literal alternatives and immutable ownership structures replace untyped call
chains. Raw external inputs remain `object` until checked; annotations and
fixture casts do not substitute for runtime admission. No blanket `Any`, ignored
imports or per-file type exclusions were used to claim this coverage.

Confirmed runtime corrections include:

- Probe manifests now have bounded reads, object/required-field checks and exact
  integer schema admission; `true`, arrays and null cannot impersonate a schema.
- Cargo/artifact/clock readers validate nested records and scalar types. The
  affected capture boundaries reject duplicate keys, duplicate environment
  identities, numeric success values and boolean/integer aliases.
- LLVM, benchmark and other tool wrappers reuse the bounded process supervisor.
  Timeout cleanup owns descendants, not only the direct child. LLVM context
  schema 2 binds the supervisor source as well as the adapter/tool identity.
- Benchmark JSON shape errors become controlled validation errors. The dated
  comparison helper retains compatibility/runner checks under `python -O` and
  binds shared helper identity.
- HTTP/browser test harnesses use bounded I/O, explicit validation and owned
  process cleanup. Importing the HTTP harness no longer starts a helper.

The large historical configuration suite and authored instruction tables were
also read and typed. A stale snapshot test compared invocation-local identity
graphs across different raw/cached providers. Its correction preserves full
value parity and checks alias-sensitive rejection separately within each
provider; production caching and graph admission were not relaxed. Typed fixture
views preserve the original exact runtime classes and mutable identities.

These changes do not complete the instruction-level proof. Practical image
admission remains the production route, with explicit source/configuration,
ELF/UF2, memory and conservative stack checks. Historical grammars require fresh
review when compiler, source or ABI changes.

### BLE host and controller transport

| Area | Correction | Exposure and remaining limit |
| --- | --- | --- |
| ATT client ownership | A single client lease and Idle/Pending/Received state admit a response only for an outstanding matching opcode. Cancelling an unfinished request closes the bearer; idle/duplicate responses cannot be retained for a later read. | Shared by probe/dashboard clients. ATT has no request identifier: a same-opcode duplicate arriving after a new request starts is inherently indistinguishable on the wire. |
| ATT server/codec | Return actual custom-read bytes, reject excess blob offsets, bound tuple lengths, validate fixed PDU lengths, correlate error opcodes, reject malformed optional Read Multiple requests and avoid recursive formatting of unknown error codes. Write Commands/confirmations remain silent on rejection. | Server APIs are dormant in the two current images. Typed-string capacity failures now return errors. All 256 error-code values have bounded-format coverage. |
| Server writes/indications | Check permissions before prepared-write admission; reject overflowing offsets/capacity/invalid Execute flags without corrupting retained state. Shared unsolicited-send logic enforces permissions and indication ownership/confirmation cleanup. | Optional server behavior; earlier peers may receive a bulk notification before a later peer fails. No atomic multicast promise. |
| L2CAP ingress | Validate all 65,536 result encodings before unsafe enum construction. Check short PDUs, peer CID/MTU/MPS and owning-link routing. | Both images reach signaling decode even without opening dynamic channels. Exhaustive result-field testing is not exhaustive protocol testing. |
| Dynamic-channel lifetime | Retain slot ownership from allocation through initial send and response wait. Rejection, malformed response, link loss, cancellation and send failure cannot let an old future touch a reused slot. | Current images do not open dynamic channels. Actual public-future regressions cover polling, cancellation and immediate retry. |
| Connection parameters | Echo the L2CAP request identifier, handle queue/controller failures, and send the HCI remote-parameter reply without an unrelated local update. | Probe reject and dashboard accept/reject routes share the corrected dependency. |
| Advertising | Bound AD/HCI lengths, clear supported empty data, set directed properties, and bind cancellation to the admitted advertiser. Extended restart clears retained sets after prior disable; accepting one connection cleans up remaining sets before reuse. | Optional peripheral API. High-level advertising owns all sets; unmanaged raw advertising commands must not be mixed with it. Controller updates across commands are not atomic. |
| Security | Dispatch Encryption Change V2 consistently; correct resolvable-address byte order using the official vector; bound invalid-random retries and propagate RNG failure. | Preserves the probe's no-pairing configuration. Native tests do not qualify pairing or physical encryption. |
| CYW43 event ingress | Bound event-mask indexing and preserve full-width wire classification before narrowing. Unknown events/statuses cannot alias a valid event or success. Correct little-endian SDIO 16-bit access. | F2 event handling remains active with Bluetooth-only operation; SDIO is dormant on the selected SPI route. No reset cause was established. |

The cursor split helper now starts at the remaining bytes and preserves an
already-written prefix. Existing production callers split at position zero.
Physical BLE integration tests now observe nested task errors, bound serial
draining and explicitly require selection rather than running as ordinary native
tests. Their compilation is distinct from operating two physical adapters.

### HAL, linker maps and radio firmware freshness

The local HAL retains latest published stable **Embassy RP 0.10.0**. Corrections
cover per-instance PIO state, RP235x fractional clocks, GPIO cancellation, PWM
disable/split ownership, calendar/AON bounds, partition metadata, I2C slave
buffers, TRNG cancellation, ROM ABI, UART and optional PIO/PSRAM helpers. The
full scope, official references, checked standalone lock alignment and native
model limits are in the
[HAL/linker dependency note](../dependencies/full-code-audit-2026-10-20.md).

The ROM buffer-validation binding now carries pointer, size, RCP access mode and
output-status arguments with a pointer return. PWM teardown clears a nonzero
enable mask, and split transfers ownership without stopping the slice. Hardware
UART flush waits for ring/FIFO/shifter completion; break cancellation clears the
break bit. PIO UART uses checked fractional baud conversion and waits for idle
plus a complete stop-bit interval, retaining the high idle level. These claims
were rechecked against the changed source for this report.

Both linker scripts adopt actual `.text` alignment from official Embassy commit
[`1948c4146aacd005459989ea4d2d24f8a5e32188`](https://github.com/embassy-rs/embassy/commit/1948c4146aacd005459989ea4d2d24f8a5e32188).
The 8 KiB settings reservation, probe's additional 96 KiB capture reservation,
fixed framebuffers and RAM/stack contracts remain intact. Six real ARM linker
fixture cases cover 16/32/256-byte alignment across the two maps. The additional
scratch banks now use upstream names SRAM8/SRAM9 with unchanged addresses and
4 KiB lengths. Four negative links reject a one-byte overflow in either bank.
Complete upstream comparison accounts for all missing applicable XIP fixes;
the optional whole-program RAM example is a different execution mode.

The Bluetooth blob advances from patch 0031 to **0065**, extracted as exactly
6,970 bytes from pinned official `cyw43-driver` source. The upstream change
addresses LE Data Length Extension. Wi-Fi and NVRAM bytes remain unchanged.
The new compact-format parser validates the complete immutable record stream
before the public runner touches hardware and preserves address modes and
neighboring alignment bytes during upload. Independent decoding matches every
address/payload in both old and new images; all 13,134 proper truncations and
512 count-byte variations were exercised. This verifies parsing/provenance,
not physical boot or radio interoperability. Exact revision, license and hashes
are in the [firmware provenance notes](../../firmware/vendor/cyw43-firmware/README.md).

## Verification checkpoint

Results below are observed local executions, including inspected final summaries,
not promises about the eventual published commit. Runs use the repository's
nightly pin and Windows native tools unless stated otherwise. Counts across
variants overlap and must not be summed into a unique-test total.

| Gate | Observed result | Scope / outstanding work |
| --- | --- | --- |
| Root Rust tests and strict Clippy | Fresh post-diagnostics workspace run: 374 passes, zero failures; strict Clippy and formatting pass | Earlier integrated workspace plus configuration-JSON runs totalled 491 executions. Counts describe their selected commands, not a unique-test total. |
| Host Rust | Fresh default and no-default runs each pass 759 tests; strict Clippy/format pass; earlier benchmark test run also passed | Repeated variants: 1,518 executions, not 1,518 unique tests. |
| Configurator | Fresh 418 native passes; strict native/Wasm Clippy and formatting pass | Includes 35 preview and 31 event tests. Real browser interactions remain separate. |
| Windows helper | 92 passes and completed strict check | Native process/file tests; no USB enumeration or flashing qualification. |
| Actual Trouble vendor manifest | 215 passes, three physical tests ignored; derive fixture passes | Additional advertising/channel feature subsets pass, as do final ARM consumers. Feature-off/on checks also cover the final conditional-import cleanup. |
| Actual CYW43 vendor manifest | 26 passes in each of Bluetooth and Bluetooth-only runs | Mock transport/real parser; no device execution. |
| Actual HAL vendor manifest | 54 native passes; optimized PIO UART selection 7/7 | Single-thread MMIO storage/mock-time models. Nine existing native compiler warnings remain in this run. |
| Python strict typing | 175/175 pass for Windows and Linux Mypy platform settings, with normal imports | Static platform checking on Windows; not Linux runtime execution. |
| Python syntax | 175/175 parse with Python 3.11 grammar | Local runtime tests use Python 3.15; grammar parsing is not a Python 3.11 runtime test. |
| Python runtime | Focused boundary/failure suites pass normally and under `-O`; the original 6,297,574-node source integration passes in both modes with retained counts/rejections | No single fresh all-482 historical-stack-suite claim; focused suites overlap. One capture test skips unavailable distinct Windows 8.3 alias behavior. |
| Python Ruff lint/format | All 175 current files pass lint and format checks | The earlier import-order/formatting failures were corrected; final logs and source hashes retained. |
| Linker regression | Six ARM map/alignment cases and four expected bank-overflow rejections pass normally and under optimized Python | Synthetic linked objects; full application images are a separate gate. |
| Browser assets and failure matrix | Release application/worker build passes; all ten browser stages pass in Edge at 1440×1000 and 760×1000 | Both harnesses returned zero and cleaned up their owned process trees. The final cached build returned zero with unchanged served artifact bytes. |
| Interactive synthetic diagnostics | Actual browser controls cycle Off → Instant → Average → Combined → Off; screenshots show `-- FPS`, `-- AVG`, `--/-- AVG`, then no rate | The appearance preview remains intact. Native independent-pixel tests cover all four modes in both orientations; the interactive check used landscape. |
| Firmware ARM/features | Fresh strict Clippy passes dashboard default, BLE+frame/TE, demo+target-bench+frame/TE at 280/70, legacy Longan, and probe stock/280 | `thumbv8m.main-none-eabihf` and Cortex-M33 retained. Actual image admission is recorded separately below. |
| Final provenance and source reconciliation | Root provenance tests pass; all 872 source/configuration paths have matching reviewed hashes | Commit/push verification and CI use the exact publication SHA. Prior CI is not evidence for these changes. |
| Physical validation | Not performed | No devices, flash or vehicle traffic used by this audit. |

## Final linked images

The final source refresh completed on 21 October 2026, after the diagnostics
changes and final comment correction. All four builds report
`STATIC_IMAGE_CHECKS_PASS` with the dated nightly, Cortex-M33 target and unchanged
admission rules. Dashboard builds select `ble-obd`, `frame-trace`, `te-trace`,
`simple-outline` and speed optimization 3; probe builds are separate standalone
images. Each pair covers stock clocks and the explicit 280/70 MHz, 1.30 V profile.

| Image | Static allocation | Core-0 headroom | Local artifact admission |
| --- | ---: | ---: | --- |
| BLE dashboard, stock | 458,520 B | 65,768 B | ELF, configuration/source context, program ranges, memory and conservative stack paths |
| BLE dashboard, 280/70 | 458,520 B | 65,768 B | Same gates, independently built profile |
| BLE probe, stock | 406,432 B | 117,856 B | ELF plus source-bound UF2 equality, program/erase ranges, memory and conservative stack paths |
| BLE probe, 280/70 | 406,432 B | 117,856 B | Same gates, independently built profile |

The traced dashboard has **232 bytes above the unchanged 65,536-byte headroom
floor**. This is the margin above a conservative admission requirement, not
232 bytes of total unused RAM or a physical stack measurement. Future changes
must pass the same gates; no reserve was reduced to admit this audit.

Conservative named-path stack bounds are unchanged between clock profiles:
dashboard boot 3,336 B, constructor 18,744 B, dashboard 28,200 B, radio 18,192 B
and runtime 15,640 B; probe boot 17,128 B, core 0 17,160 B, radio 24,648 B and
render 17,248 B. These are path estimates, not values to sum or measured deepest
usage. Both routes retain the 8,192-byte core-0 reserve, 32,768-byte core-1
allocation, 4,096-byte core-1 reserve and two 153,600-byte framebuffers.

Both probe UF2s contain 7,333 admitted pages. Their erase ranges end at
`0x101CB000`, before the protected capture region
`[0x101E6000, 0x101FE000)`. The separate 8 KiB settings reservation follows it.
These local dashboard runs checked ELF artifacts; they do not claim a local
dashboard UF2 transfer or device readback.

Exact local artifact SHA-256 identities:

| Artifact | SHA-256 |
| --- | --- |
| Dashboard stock ELF | `4345370a87572fe959892ca4ac0969f9bfb3977faa11124c06b86d8bdbdb27e6` |
| Dashboard 280/70 ELF | `1734f0f2d1c17abaa256e6c79ab50a77c674e24f17bd3d4e5397cd05eb987a52` |
| Probe stock ELF | `0c403741aec5a7679ea9837efedb2656b57c4b0f0ad3067896eaa273b5dc004f` |
| Probe 280/70 ELF | `da632e38a2e619a40aa95bb731b4516813a014f7c958355f0cad97b882975f85` |
| Probe stock UF2 | `44f1611d5c2cd1b07e19432ca7c7274fc21c10e855c977b7887bd1373964b85c` |
| Probe 280/70 UF2 | `740c41ff71dcf4a4058d45ac1404998479cc800b27e16e04f81cdc54f1a8ea52` |

All image results explicitly retain `hardware_qualified: false` and
`complete_instruction_proof: false`. No device was accessed and no saved slot
was read, overwritten or erased in this audit.

## Preserved boundaries and remaining limitations

- Two 153,600-byte RGB565 buffers remain affine. Delayed completion retains the
  original identity/orientation; timeout quarantine does not permit buffer reuse.
  SPI completion/CS high does not establish LCD scanout or absence of tearing.
- Probe Inspect remains explicit and read-only within the standard allowlist.
  Generation cancellation, stop/disconnect acknowledgments, fault-priority RGB,
  unknown-format journal protection and read-only USB recovery remain required.
  Saved data never establishes live success or authorizes handle reuse.
- MC+/CX paths still initialize/identify only. Unqualified ECU requests and alarms
  remain disabled; synthetic data does not establish exact DNUC ECU support.
- Reviewed optional library code has wider scope than the selected board. RP2040
  ROM/math and RP235x high-bank behavior outside the supported Pico 2 W pin range
  retain recorded concerns, not a compatibility guarantee. Dormant Wi-Fi/SDIO
  caller-error/panic behavior and unbounded hardware-progress waits require
  separate consumer-specific qualification before adoption.
- Software tests cannot establish electrical margins, physical UART/PIO timing,
  cache/IRQ races, power-cut persistence, USB transfer success, radio interoperability
  or continuous stack high-water. Retained watchdog stage/time is an observation,
  not a diagnosis. Hardware validation follows the
  [Pico 2/DNUC runbook](pico2-dnuc-runbook.md).

## Specification basis

Source claims were checked against actual resolved dependencies as well as
official references. Important references include:

- [Bluetooth ATT](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/Core-61/out/en/host/attribute-protocol--att-.html): request/command behavior, blob offsets, response framing and transaction rules.
- [Bluetooth L2CAP](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/Core-61/out/en/host/logical-link-control-and-adaptation-protocol-specification.html): LE credit signaling, parameters and link-local channels.
- [Bluetooth HCI](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/Core-60/out/en/host-controller-interface/host-controller-interface-functional-specification.html): extended advertising commands, retained sets/data and controller events.
- [Rust validity rules](https://doc.rust-lang.org/reference/behavior-considered-undefined.html): invalid enum representations must be rejected before construction.
- [Cargo workspaces](https://doc.rust-lang.org/cargo/reference/workspaces.html) and [structured messages](https://doc.rust-lang.org/cargo/reference/external-tools.html#json-messages): frozen member resolution and tool boundary handling.
- [Python typing](https://docs.python.org/3/library/typing.html), [JSON](https://docs.python.org/3/library/json.html) and [optimized execution](https://docs.python.org/3/using/cmdline.html#cmdoption-O): annotations do not validate input; checks must survive optimization.
- [IndexedDB](https://www.w3.org/TR/IndexedDB-3/) and [Web Locks](https://www.w3.org/TR/web-locks/): transaction completion and asynchronous lock ownership. These are working-draft references, supplemented by actual adapter review and browser gates.
- [PNG](https://www.w3.org/TR/png-3/), [GIF89a](https://www.w3.org/Graphics/GIF/spec-gif89a.txt) and [PPM](https://netpbm.sourceforge.net/doc/ppm.html): bounded image conversion and native-geometry export.
- The [HAL/linker note](../dependencies/full-code-audit-2026-10-20.md) and [radio firmware provenance](../../firmware/vendor/cyw43-firmware/README.md) retain manufacturer, SDK, upstream commit and exact package/byte identities.
