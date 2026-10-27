# Offline NGM identity implementation review

24 October 2026; final offline admission 27 October 2026. Scope: INFO codec/session, shared Trouble LE credit-based
channels, separate identity utility, capture/save/export, build admission and
affected documentation. No Pico, adapter, USB port or vehicle was accessed.

## Contract and source trace

- The owner's slot 2 is accepted as the NGM adapter. Its captured standard GATT
  inventory is evidence for fresh endpoint qualification, not a saved-handle
  connection configuration. See the [research record](../vehicle/adapter-research.md).
- The [pinned public CANyonero source](https://github.com/Automotive-Swift/Swift-CANyonero/blob/78091bb5599c4f5e3002fbd5700b826a625e8b04/Sources/libCANyonero/Protocol.cpp)
  defines the `1F`, message-type, big-endian body-length frame and five INFO
  fields. Local code exposes only the four-byte `1F 11 00 00` request. Bounds,
  UTF-8/control rejection, timeouts and SPSM 129 trial policy are local admission
  choices, not measured adapter limits.
- The [Bluetooth Core 6.2 L2CAP specification](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/Core-62/out/en/host/logical-link-control-and-adaptation-protocol-specification.html),
  sections 4.22/4.23, 7.3 and 10.1, was compared with actual vendored channel,
  signalling and receive/reassembly ownership. MTU bounds SDUs, MPS bounds each
  K-frame payload, and credits count K-frames. Neither ATT MTU nor advertised
  identity supplies the dynamic SPSM or proprietary protocol authorization.
- The actual pinned Embassy/Trouble implementations, permanent runner owners,
  cross-core events, journal admission, capture decoder and PC exporter were
  traced. Reports supplied review leads; parent source inspection and regressions
  established the changes described below.

## Implemented paths and corrections

| Path | Contract and tested failure handling |
| --- | --- |
| Codec | One frame, at most 245 bytes; five UTF-8 fields of at most 64 bytes; exact big-endian length; optional final LF; empty fields unavailable; first error sticky; malformed/duplicate/suffix/remote-error input retained and rejected |
| Session | Affine INFO request, explicit epoch, fixed stage deadlines, overflow/reversal handling, stale-event rejection, cancellation/parking, first fault separate from cleanup; clean reply alone cannot establish success |
| CoC TX | Per-K-frame credits allow progress with one credit; peer MPS is clamped to local scratch capacity; one SDU writer; ambiguous/partial cancellation closes the channel; overflow checks include reserved but unsent credits while ACL submission waits |
| CoC RX | Explicit zero credits remain zero; positive initial/refill credits are backed by packet ownership; partial-SDU progress can replenish without promising an extra buffer; queued complete SDUs retain capacity ownership |
| Channel lifetime | Refused/wrong-ID creates release reserved receive capacity immediately while the creator retains its slot reference; refusal codes remain available; credit/disconnect tokens retain the exact slot; connection close wakes waiters and releases owned packet state |
| GATT storage | Caller-owned startup and service output avoid large async return copies; occupied startup slots remain untouched; failed/canceled startup publishes no client; borrowers end before client drop and link cleanup; the existing owned APIs remain compatible |
| Runtime | Fresh unique readable FFF1 vendor endpoints, four bounded reads, one configured SPSM trial and one INFO; the initial Start publication shares the connect deadline; completed observations retained across publication backpressure/cancellation; truthful original timestamps flushed before later lifecycle events |
| Terminal failure | Host completion closes the atomic operation gate before the losing operation is dropped or terminal events can block; old lease drops cannot reopen admission; an already admitted flash mutation still owns restoration |
| Captures | Optional v5 identity evidence with explicit wire tags; legacy v1-v4 migration; 8192-byte envelope unchanged; no automatic slot rewrite; separate first fault/cleanup, raw bytes and unavailable observations |
| UI/export | Live clean-only green, saved evidence amber, red fault latch; complete retained bytes required for parsed fields; exact error domains/codes, directional limits and configured candidate remain inspectable; Windows friendly names accept only the same enumerated COM-port suffix |

The maximum synthetic inventory plus identity fixture is **8083 bytes** inside
the existing 8192-byte envelope. This is a format-bound regression, not a claim
that every real device fits the inventory limits. Omitted/truncated information
remains explicit. Saved handles and the future Studio handoff stay disabled.

## Memory and packaging review

Two explicit binaries include one shared application source at their crate root.
The ordinary entrypoint fixes `StandardProbe` even if identity support is built;
the identity entrypoint fixes `NgmIdentity`. Labels and the linked marker share
one private compile-time constant; each root fixes its inspector alias without
alternate async states. Feature validity is checked before hardware
owners or either core start. No runtime/USB input can switch it.

This layout preserves the existing binary-only LLVM option. As documented by
[Cargo rustc](https://doc.rust-lang.org/cargo/commands/cargo-rustc.html), extra
compiler arguments apply to the selected target; changing the owners into a
dependency library changed the optimization boundary. Reviewed SRAM helper
bytes, source markers and flash reservations remain separate checks.

The first integrated layouts failed flash or aggregate stack admission. Those
images were rejected. Lowering the inlining threshold also failed and was not
adopted. A borrowed complete-frame parser now shares validation with the stream
decoder while avoiding repeated 245-byte receive buffers in rendering/status.
Capture metadata is filled in unpublished final storage rather than copying a
second vendor payload. No memory ceiling, stack reserve or slot region was relaxed.

Direct event forwarding returns the existing lazy Embassy channel-send future;
it removes redundant async message owners without changing FIFO/backpressure or
acknowledgment. Discovery is pinned in a tight caller scope and lent to the same
timer-first deadline race. That owner ends before result interpretation, channel
creation or cleanup, including timeout and cancellation. The locked dependency
source and emitted assembly informed these changes; regressions check actual
queue behavior and pinned-owner address/drop ordering.

**Final linked-image admission passed on 27 October 2026.** All four utility
receipts bind source digest
`4b1d77e8bbe6383fd9dede82efcb4725df74b354e33a7f38d0e570514d9635f0`.
Each includes provenance, reviewed SRAM helpers, ELF/UF2 equality and exclusion
of the 98304-byte capture region `0x101E6000..0x101FE000`. The two 153600-byte
framebuffers, 32768-byte core-1 stack and 8192/4096-byte core-0/core-1 reserves
remain unchanged.

| Utility / profile | Static RAM bytes | Remaining RAM bytes | Aggregate / nonradio emitted-frame bytes | Local receipt directory |
| --- | ---: | ---: | ---: | --- |
| NGM identity / stock | 410880 | 113408 | 20212 / 12128 | `ngm-stock-20261027-final` |
| NGM identity / 280-70 | 410880 | 113408 | 20212 / 12128 | `ngm-280-20261027-final` |
| Standard probe / stock | 410120 | 114168 | 18772 / 11968 | `probe-stock-20261027-final` |
| Standard probe / 280-70 | 410120 | 114168 | 18772 / 11968 | `probe-280-20261027-final` |

Receipts are under `firmware/target/ble-probe-captures/`. The aggregate/nonradio
ceilings remain 20480/12288 bytes. Emitted-frame sums are conservative admission
inputs, not observed stack depth or physical high-water measurements.

| Artifact | ELF SHA-256 | UF2 SHA-256 |
| --- | --- | --- |
| NGM stock | `e3dc60b6e7a3a753ff5cd54cd0344c9574ce1f3aee8141541b874af5c514ec00` | `c826d5f63efb1a05ab0ed53f338eeabc1f2f2e44320e13240612485a16263e94` |
| NGM 280-70 | `5a64b1972493bfec4aeae1926a08b822b9e626a1b192b1b0c08b7a6f76ab39e6` | `d437ecd17bacc70c5ffa873db8295f1618fda82be19cc1fbcfd21573750416ff` |
| Probe stock | `f0e877e860d616478ec54186823a5335e9e4f4dc5d65146109f404dc277e4435` | `78e09b5ffc5698c0547254e41bc0b4cc21518b4074631288ee2e19bc487f8b69` |
| Probe 280-70 | `8d4d4820dc86b3d90436a82f7f13c3fd76d0739f62757e3c722cb3bff87518c5` | `16060a606624cc5a779b4ebb9abe561c5d0a130833abed933e4e1b5d415d7320` |

The dashboard `ble-obd,cpu280-spi70-1v30` capture
`firmware/target/bench-captures/ngm-dashboard-280-20261027-final` passed the
practical image and flash-helper mutation gates: static RAM 418496 bytes,
headroom 105792 bytes, ELF SHA-256
`c006d89af0b86bea3fe3ba75e9799845932510028b97df230b9b29d4f0b6bf73`.
Its named stack bounds are boot 3336, constructor 18744, dashboard 26720,
radio 19816 and runtime 15640 bytes. These are dated source/image receipts;
subsequent code/config changes require their own admission. No image was flashed.

## Offline verification

- Core codec/session: 26 integration tests plus cached-admission equivalence; identity evidence/capture: 9; bounded renderer
  regressions cover omitted raw suffixes, counters and unavailable SDU observations.
- Portable identity flow: 16 tests, including actual ready-result/deadline races,
  advancing timestamps, blocked publication, cancellation and clean teardown.
  Vendor reads target unpublished final storage; partial writes on timeout/error
  are cleared, cancellation cannot publish unfinished bytes, and empty/full or
  oversized reported lengths retain explicit outcomes.
- Atomic operation gate: 5; shared session/terminal publication: 10; full reliable
  queue and close-versus-drop races included.
- Actual bridge queue: lazy construction, full eight-event capacity, canceled
  sender, FIFO resume, exact event fields and subsequent reuse in one isolated
  regression executable. Borrowed deadline work also checks stable addresses,
  timer-first equality and exactly-once owner destruction.
- Vendored Trouble: 233 no-security, 280 security, 292 optimized/legacy-flow tests;
  strict Clippy in security and no-security contexts and private documentation.
- Root workspace tests and strict Clippy; host tests with default and no-default
  features and strict Clippy; core no-default checks on Wasm and Cortex-M33.
- Python: 175 sources under locked Ruff lint/format and strict Mypy. Image
  admission: 17 tests in normal and optimized Python. Security guard: 4 tests in
  both modes plus actual native Cargo rejection of the forbidden feature union.
- Vendor file provenance/hash inventory and strict private rustdoc passed in the
  checked contexts. Structural documentation coverage is checked separately from
  rendered links and semantic source review.
- Final documentation inventory: 687 Rust files, 21714 items, zero uncovered.
  Strict private ARM rustdoc and Clippy covered both utility entrypoints with
  identity/280-70 features; the ordinary stock binary was also linted with and
  without identity support. Browser visual rustdoc inspection was unavailable;
  successful compilation and link checks are not a visual-review claim.

The original slot 2 binary was decoded again with the current exporter and its
content hash remained unchanged. A private offline harness fed its recorded
service/characteristic rows through the production endpoint collector; the
recorded FFF1 layout qualified. The firmware still rediscovers endpoints on each
attempt and has no saved-handle input. This replay does not establish CoC acceptance
or an INFO response.

The shared-renderer gallery contains 226 synthetic pages; the two selected PNGs
and scenario contact sheets were visually inspected. The images in the
[identity guide](../vehicle/ngm-identity.md#synthetic-renderer-gallery) are actual
host renderer output, not hardware observations.

## Physical work remaining

The [identity runbook](../vehicle/ngm-identity.md#future-garage-runbook) prepares a
single useful visit with explicit selection, partial-result capture and normal-USB
export. CoC acceptance, actual INFO bytes, RF/USB timing, physical stack high-water,
power-cut recovery and electrical margins remain unverified. Protected reads or
authentication refusal are useful outcomes, not permission to add unlock traffic.
No automotive channel, ECU request, DID decoder or vehicle alarm was enabled.
