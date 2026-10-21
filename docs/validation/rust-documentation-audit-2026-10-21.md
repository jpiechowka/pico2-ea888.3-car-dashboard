# Rust documentation and source audit, 21 October 2026

## Scope

This pass started at `1c7da0b0f854531d9add4add1314398ac5190079` and covers
tracked Rust sources across the shared crates, host tools/tests, Studio, native
helper, both firmware applications and reviewed vendor trees. It also covers
private items, inactive `cfg` branches, build scripts, examples, test fixtures
and repository-owned macro definitions. This is a source and software audit;
no device was flashed, capture slot changed or BLE/vehicle traffic initiated.

The [documentation policy](rust-documentation.md) defines the inventory,
input/output contracts, private-page rendering, doctests and limitations of
external macro expansion. `AGENTS.md` now requires updating these contracts
alongside code. Type names and primitive types alone are insufficient: comments
describe units, limits, sentinels, ownership, mutation, cancellation and errors.

The automated inventory checks syntax and contract presence. Manual source and
caller review supplies meaning; neither establishes exhaustive correctness.
Original vendor hashes remain immutable. Reviewed overlay hashes include each
intentional source change and new helper; no dependency upgrade is part of this
pass.

## Confirmed corrections

### Shared configuration and Studio

| Path | Correction | Regression evidence |
| --- | --- | --- |
| Configuration tools | Sentinel read-limit arithmetic handles `usize::MAX` without overflow. Existing size rejection and errors remain. | Zero, exact, excess, maximum and directory cases. |
| Generated clock admission | Accept complete LF outer-doc lines before literal checking, preserving full source hashes and exact executable syntax/values. | Documented and legacy sources; duplicate, missing, changed or commented-out constants; extra code, unsupported comment/attribute framing and invalid UTF-8. |
| Studio preview | Missing source identities cannot compare equal and admit an unqualified preview. | Missing/stale identity and admitted-source cases. |
| Studio artwork maintenance | Abort the IndexedDB transaction on an early error so previously queued deletion does not partially commit. | Real-browser failure reproduction and successful atomic maintenance. |

Native editor tests do not substitute for browser event/transaction behavior.
The browser regressions use actual IndexedDB transactions. See the
[IndexedDB transaction lifecycle](https://w3c.github.io/IndexedDB/#transaction-lifetime).
The clock-source schema accepts only the generator's documented subset of
[Rust comment syntax](https://doc.rust-lang.org/reference/comments.html), not arbitrary Rust.

### HAL and radio

The [HAL patch ledger](../../firmware/vendor/embassy-rp/LOCAL_PATCHES.md) and
[radio patch ledger](../../firmware/vendor/cyw43/LOCAL_PATCHES.md) retain detailed
scope and earlier corrections. Additional fixes found while documenting:

| Area | Correction and boundary |
| --- | --- |
| Core 1 launch | Admit at least eight bookkeeping words before reset or pointer writes. This does not prove enough stack for an arbitrary closure. |
| Interrupt executor | Initialize its storage and publish readiness in one critical section before enabling the IRQ. Duplicate start preserves the first executor. |
| SPI divider | Use wide ceiling arithmetic; reject zero/unachievable low requests, preserve valid profile results and bound high requests to the fastest legal divider. |
| RP2040 flash identifier | Empty reads bypass hardware; nonempty reads include four dummy bytes within the 16-bit SSI count. |
| PIO batch and HD44780 | Last enable/disable request wins for each selected machine; relative IRQ0 reaches the correct HD44780 waiter on machines 0–3. |
| AON timer | Validate source-specific calibration widths before writes and reject elapsed-time conversion overflow. |
| ADC and I2C | Reject empty ADC channel sets, zero I2C frequency and empty async write iterators before the invalid operation. Constructor reset/pin effects are documented separately. |
| PWM duty trait | Full active output uses TOP+1 when representable. Maximum TOP returns an explicit unrepresentable-full-duty error. Direct compare configuration remains unchanged. |
| RTC and clock observations | Handle the RP2040 zero divider encoding, reject unrepresentable RTC rates, report cached RTC output and retain unavailable LPOSC frequency as zero. |
| UART line control | Derive the 15-bit wait with shared wide ceiling arithmetic, including low peripheral clocks. No wire-timing qualification follows from arithmetic tests. |
| RP2040 floats | Correct nonfinite signs/NaNs, preserve both `atan2` coordinates, truncate signed EABI conversions toward zero and correct the raw `float_to_fix64` return ABI. Finite ROM approximations are unchanged. |
| ROM and integer ABI | Publish RP2040 cached pointers atomically; remove incorrect 32-bit implementations of DI64 aliases; supply the missing fourth argument of RP235x `pick_ab_parition`. The historical public spelling remains. |
| PIO pin windows | Validate configuration before register writes, use the selected 32-pin window, include explicit JMP/side-set pins and exclude the optional side-set enable bit from physical pin count. RP235xB checks retain optional APIs without adding supported boards. |
| USB control lifecycle | Validate EP0 packet size; invalidate stale setup/address completion across new SETUP, reset and disable. Bounded endpoint abort quarantines buffers on timeout instead of reusing potentially owned storage. Bus-reset behavior is cross-checked against TinyUSB, not physically qualified. |
| CYW43 transport | Preserve logical SDIO lengths within padded storage, complete the scan-parameter tail and use modulo-256 credit-window semantics. |
| CYW43 initialization/scan | Validate firmware/NVRAM placement before bus writes; release host scan subscription/mask if startup is cancelled. This does not send an undocumented firmware scan-cancel command. |
| CYW43 diagnostics | Keep status widths intact, correct IRQ-name formatting and omit sensitive request payloads from automatic logs. |

Primary references include the
[RP2350 register documentation](https://datasheets.raspberrypi.com/rp2350/rp2350-datasheet.pdf),
[Pico SDK](https://github.com/raspberrypi/pico-sdk),
[Arm run-time ABI](https://github.com/ARM-software/abi-aa/blob/main/rtabi32/rtabi32.rst),
[GCC integer runtime contract](https://gcc.gnu.org/onlinedocs/gccint/Integer-library-routines.html),
and [Infineon driver headers](https://github.com/Infineon/wifi-host-driver).
The actual locked implementation determines which API paths exist locally.
RP2040-specific work preserves retained vendor APIs; the supported dashboard
board remains Pico 2 W / RP2350 Cortex-M33.

### BLE state, storage and protocol

| Area | Correction and boundary |
| --- | --- |
| Packet ownership | Separate allocator metadata from leased `UnsafeCell` byte arrays so another allocation does not invalidate a live packet's backing borrow. Actual production regressions run under Miri. |
| L2CAP fragments and credits | Reject impossible first fragments before publication; include the SDU's two-byte prefix in wide credit arithmetic; reject overflowing cancellation refunds by requesting channel cleanup. |
| Channel identities/references | Require the same manager/stack as well as numeric indices; transfer split/merge references without transient overflow; skip already claimed pending tokens. |
| ACL identity | Retain a local slot independently of application connection references across locks, grants and host-queued PDUs. Reused controller handle numbers cannot redirect that queue or credit/unlock a new peer. |
| Queue admission | Balance retained identities on full queues, cancellation, dequeue and manager drop. SMP's existing mutable table borrow uses the reviewed already-borrowed enqueue seam. |
| Pending transport | Check exact-slot liveness before each transport poll and wake on close. Already accepted bytes cannot be retracted; generic transports may have partial effects while Pending. Selected CYW publication follows its capacity wait. |
| Stack destruction | Drain/reset borrowed MaybeUninit-backed connection/channel records so queued packets and partial reassembly release pool capacity before HostResources reuse. No controller command is sent. |
| Listener ownership | Reject duplicate admission before changing state; retain the connection first so reference-count exhaustion cannot strand the listening flag. |
| Listener identity | Check the retained link before every wait poll and before Drop rejection. A closed listener cannot claim, accept or reject requests belonging to a new peer with the same numeric handle. Terminal cleanup wakes the wait; local close can wait for cleanup or another channel event. |
| Connection state | Clear per-peer OOB/legacy-key observations on reuse; update connection-rate observations before attempting event publication, including a full event queue. |
| Secure Connections | Reject equal public-key X coordinates in both backends, including opposite-Y points. Reject selected OOB authentication without peer material; preserve legitimate unilateral OOB selection. |
| Pairing fallback | Resume rejection falls back to a fresh request instead of unwrapping absent state. Bonding diagnostic labels match their unchanged wire flags. |
| ATT UUIDs | Expand UUID32 to Bluetooth-base UUID128 for requests and declarations; normalize matching at the protocol boundary without changing generic UUID equality. |
| ATT codecs/grouping | Encode the already-decodable Prepare Write, Execute Write and Read Multiple Variable requests; permit only primary/secondary service group types. |
| ATT rejection | Retain the exact denied handle for Read Multiple Variable and Prepare Write; preserve request opcode, default/overridden status and response-free Write Commands. |
| Client attributes | Match a retained bonded identity before an earlier vacant slot, preserving its CCCD values. Unbonded disconnect still clears values. |
| Automatic traces | Log peer addresses without retained identity keys; omit raw restored per-client attribute bytes. Explicit formatting traits are separate APIs. |

The [BLE patch ledger](../../firmware/vendor/trouble-host/LOCAL_PATCHES.md)
documents these shared dependency paths. Both firmware applications inherit
applicable transport fixes; the probe's no-pairing feature guard remains.
Server, peripheral and dynamic-channel fixes also cover optional retained APIs;
they are not evidence of the cause of a photographed client disconnect.

Protocol claims were checked against Bluetooth Core
[ATT](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/Core-61/out/en/host/attribute-protocol--att-.html),
[GATT](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/Core-61/out/en/host/generic-attribute-profile--gatt-.html),
[L2CAP](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/Core-61/out/en/host/logical-link-control-and-adaptation-protocol-specification.html)
and [Security Manager](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/Core-61/out/en/host/security-manager-specification.html).
Memory reasoning uses the Rust [UnsafeCell contract](https://doc.rust-lang.org/std/cell/struct.UnsafeCell.html).
[Miri](https://github.com/rust-lang/miri) validates its current experimental model,
not the whole stack or physical radio/DMA behavior.

## Verification status

The final source inventory covers **665 Rust files and 21,046 items**, with zero
uncovered item/input/output/safety contracts detected. Eight inventory regressions
cover parsing, nested items and path admission. Repository-owned macro templates
are reviewed; the external test-macro limitation is recorded in the policy.

| Check | Result and scope |
| --- | --- |
| Root crates | All-feature workspace tests/doctests and strict Clippy pass; private library and separate binary rustdoc pass. |
| Host | Default and no-default-feature suites, Clippy and private library/example docs pass. Pure production HAL helpers have 15 passing boundary tests. |
| Studio/helper | Native tests and strict native/Wasm Clippy/docs pass; actual IndexedDB rollback regression verified in a browser. Windows helper tests pass outside the sandbox after sandbox file-lock access failures. |
| HAL | RP235x and retained RP2040 native suites pass, including release arithmetic. Cortex-M33 and retained M0 documentation/compilation and the DI64 archive link fixture pass. Retained RP235xB is compile-only. |
| CYW43 | 40 native regressions and strict private documentation pass. |
| BLE | Final combined native feature configuration passes 278 library tests with `--test-threads=1`, required by its shared packet-pool fixtures. Other documented feature configurations and hardware-free integration tests also pass; physical tests remain opt-in. |
| Packet aliasing | Five actual packet-pool regressions pass under the pinned Miri interpreter. |
| Optional Cortex crypto | C/assembly backend and private documentation compile for Cortex-M33 using the installed Zig cross-compiler locally; CI uses GNU Arm. No backend execution or link qualification is claimed. |
| Python | All 175 files pass hash-locked Ruff lint/format and strict Mypy. Target-capture suite: 38 tests run, 37 pass, one platform-specific skip. |
| Source/render drift | All five workspace formatting checks, provenance checks and normalized non-doc source review pass. Representative private fields, tuples, generated props/intrinsics, async ownership contracts, navigation and search were inspected in a browser. Not every rendered page was visually inspected. |

Final linked image checks use the pinned nightly and Cortex-M33 target settings:

| Image | Options | Static RAM | Remaining main RAM |
| --- | --- | ---: | ---: |
| BLE probe | 280/70, 1.30 V, explicit probe inlining | 406,704 bytes | 117,584 bytes |
| Dashboard | 280/70, 1.30 V, BLE, frame/TE trace, simple outline | 458,584 bytes | 65,704 bytes |

Both practical image gates pass with conservative named stack paths and existing
reserves. Both fixed 153,600-byte framebuffers remain. The probe ELF/UF2 agrees
and excludes the 98,304-byte capture reservation from program/erase/verification
ranges. These checks neither read nor alter saved device bytes.

Checked ELF identities:

- Probe: `8c1f8b9796c7a916fdfbfe8ded104d3c9baa42010b3d9a45515c61f0438247ff`.
- Dashboard: `81bf01b68f4cfef3da455c49ea766acec9af5d5452867bcfac948cd932837c31`.

Completed software checks do not establish physical clock tolerance, stack
high-water, USB/radio interoperability, DMA ordering, flash persistence or the
cause of an observed watchdog reset. Remaining retained-API assumptions are
listed in the vendor ledgers. No flash or hardware-validation result is implied.
