# Linked RAM boundaries and sampled stack depth

The [20 October diagnostics re-audit](../validation/diagnostics-reaudit-2026-10-20.md)
records current measurement semantics and checked images. Dated tables below
remain evidence for their exact earlier artifacts.

## Current linker conventions

Both active ARM maps align `_stext` to the actual `.text` section alignment,
including over-aligned sections. The scratch regions use upstream RP2350 names
`SRAM8` and `SRAM9`, at `0x20080000` and `0x20081000`, each 4096 bytes.
This naming correction changes no address, allocation or diagnostic denominator:
the main arena remains 512 KiB. The inactive copied RISC-V map uses the same
names without implying supported RISC-V firmware.

Real ARM link regressions cover 16/32/256-byte text alignment in both active
maps, exact scratch-bank origins and sizes, and independent one-byte overflow
rejection in each bank. The [complete upstream comparison](../dependencies/full-code-audit-2026-10-20.md)
accounts for the optional whole-program RAM execution mode and preserves this
project's XIP, boot metadata, settings and capture reservations.

## Original linked-boundary repair

The extended review found a linker integration regression after upgrading to
`cortex-m-rt` 0.7.7. Its default `__sheap` and `_stack_end` definitions use the
location counter after `.uninit`. Our RP2350 `.end_block` insertion switches
that counter to FLASH. An unstripped optimized build and its linker map showed:

| Boundary | Before fix | After fix |
| --- | --- | --- |
| `__euninit` | `0x2005b210` | `0x2005b210` |
| `__sheap` | `0x1002fdc0` | `0x2005b210` |
| `_stack_end` | `0x1002fdc0` | `0x2005b210` |

The new profiling implementation's range assertion would panic when opening
the profiling page. Earlier committed profiling used an estimate and did not
reference this symbol; no physical panic was reproduced. The application does
not currently install a core 0 stack guard using `_stack_end`.

`memory.x` now anchors both boundaries to the eight-byte-aligned `__euninit`,
asserts that this is inside RAM, and reserves at least 32 KiB of core 0 stack
headroom. The Python admission is stricter: at least 65536 bytes, with
unchanged Core0/Core1 stack reserves of 8192/4096 bytes. Both guards apply;
the linker assertion itself remains 32 KiB. `scripts/check_memory_layout.py` reads the actual ELF sections and
symbols, checks all allocated RAM against those boundaries, and confirms the
boot footer stays in FLASH. The check failed on the original diagnostic ELF
and passes after the fix. CI runs it on fourteen target matrix entries, covering stock, traced Demo/Vehicle,
280/70, legacy overclocks and serial/full-outline variants; the exact matrix
is in `.github/workflows/ci.yml`.

## Practical image checks and experimental proof

Normal captures and native builds use the practical gate. It checks the exact
ELF program map and boot metadata, linked configuration envelope and identity,
RAM geometry, fixed stack reserves and conservative named stack paths, and
reviewed flash lockout/SRAM-copy/vector contracts. Capture and native admission
also bind startup inputs, frozen source/options and the converted UF2 bytes.
No stack budget or reserve is increased by selecting this gate.

From `firmware`, create a new capture (without flashing):

```text
python scripts/capture_target_bench.py my-static-check --firmware --features demo --image-gate practical
```

Captures freeze an explicit `source-context.json` before the gate. It contains
the two captured Boot/private Config source members, bound to the frozen ELF,
capture attempt and exact build-options bytes. Recheck an existing capture with:

```text
python scripts/check_memory_layout.py <capture>/pico2.elf --gate practical --source-context <capture>/source-context.json --result-json <new-result.json>
```

The practical result uses schema3, gate `practical_image` and status
`STATIC_IMAGE_CHECKS_PASS`. It includes exact identities, conservative bounds,
fixed assumptions and limitations; `hardware_qualified` and
`complete_instruction_proof` are both false. It contains no execution-proof
receipt. Readers rerun the checks and reject altered bounds, assumptions,
identities or a result from another gate. Native admission requires this exact
practical scope; old captures without an explicit gate cannot be substituted.

The independent complete instruction-level proof remains experimental and
fail-closed. Select `--image-gate experimental-full` during capture, or
`--gate experimental-full` in the memory checker. Its original schema2
`full_memory_layout` contract is preserved. Its complete internal producer is
unfinished, so current actual captures in that mode fail without publishing a
successful result. There is no automatic downgrade to practical checks.

The experimental CLI reports the unfinished producer explicitly when its child
returns practical named-frame bounds. Those bounds cannot become an execution
proof by changing the result schema. The retained Source/Tail composition stops
at the original configuration Visitor return for its historical SizeZ family; loader
finalization, validation, diagnostics and complete resource composition remain
open. Its authored v4 machine/layout contracts also require a separately reviewed
port for current v6 images. A successful historical component replay would close
that component only, not the full current-image gate.

### Experimental validator allocation repair (2026-10-17)

The raw Tail handoff now compares plain model records without constructing a
second complete snapshot solely for equality. It still walks the whole graph,
checks values, types, container aliases and cycles, and retains all six Source/Tail
checkpoints. Custom getters, metadata, metaclasses and mapping proxies use the
unchanged snapshot writer, preserving callback and error order. Retained-state
revalidation and supplied-input paths keep their original checks.

A matched Windows/Python 3.15.0 pair used the same historical machine fixture with
a synthetic 128-byte payload extent, current analyzer inputs and default garbage
collection. The baseline was `a3de82a`; the repair was `9d512c1`. This is a host
component measurement, not the complete historical ELF replay or a firmware
performance measurement.

| Measure | Before | After |
| --- | ---: | ---: |
| Producer wall time | 103.559 s | 104.938 s |
| Producer CPU time | 77.500 s | 89.766 s |
| Maximum sampled process private memory | 1,352,937,472 B | 1,245,863,936 B |
| Complete snapshot constructions | 137 | 136 |
| Retained checkpoint node slots | 5,119,725 | 5,119,725 |

This pair traded more CPU work for fewer allocations: one temporary 974,371-slot
graph disappeared and the sampled private-memory maximum fell 7.91%. Ten-second
sampling can miss peaks; single-run timing varies with host load. No speedup or
full-image memory bound is claimed. The public result remained byte-identical
(40,946 bytes); a separate run reconstructed all six complete retained checkpoints
and matched their values and alias encodings with the unchanged writer.

Run the focused regressions from `firmware/scripts`:

```text
python -B -m unittest test_config_sources.SourcePhaseSnapshot test_phase_comparison
python -B -O -m unittest test_config_sources.SourcePhaseSnapshot test_phase_comparison
```

CI runs both modes. These checks cover alias/type distinctions, malformed
baselines, late traversal failures, observable callbacks, cleanup and a bounded
allocation regression. The larger `test_config_tail.TailClosedProducerRelations`
tests exercise handoff mutation rejection and supplied-input equivalence; those
expensive component tests remain separate from normal production image checks.

The corrected historical fixture preflight accepts missing stack metadata only
for an already reviewed complete builtin body with the exact role, extent and
frame. It rejects changed bodies, conflicting frames and missing ordinary-function
metadata. The deferred actual-image replay previously sampled about 16.5 GB of
private memory; the small-fixture reduction above does not establish that it fits
less memory. That replay and the unfinished current-image producer are separate
remaining work, and neither is a prerequisite for Build & Flash.

### Scope of image admission

Both gates require the explicit source context. Neither discovers a sidecar or
uses current checkout source as a fallback. Each source member is limited to
65536 UTF-8 bytes and the context to262144 bytes; memory results remain bounded
to65536 bytes. Recorded pairing and source-contract checks do not prove all
compiler-consumed inputs or complete machine ownership/effects. Practical
stack admission relies on reviewed named paths and fixed child allowances;
it is not whole-program stack or exception/reset-reentry safety proof, physical
stack high-water, clock/voltage qualification, or device validation. The full
experimental source/alias/diagnostic/loader graph work remains separate.

The historical trace plus 300 MHz diagnostic build used 373264 bytes of static RAM and
left 151024 bytes before the core 0 stack top. These numbers vary with features
and source changes. They are allocation evidence, not measured worst-case
stack use. The ELF check and linker assertions are the continuing gates.

### Current audit images (2026-10-20)

The full-code/diagnostics audit's stock and 280/70 traced BLE dashboard images
allocate 458520 bytes and leave 65768 bytes before the core-0 stack top. This
passes the unchanged 65536-byte geometry floor by **232 bytes**. The floor is
not a physical stack measurement or an additional free-allocation budget.
The corresponding standalone probe images allocate 406432 bytes and leave
117856 bytes. Both retain two 153600-byte buffers and the 32768-byte core-1
stack. Source, compiler, features, named stack bounds and exact artifact hashes
are recorded in the [current image evidence](../validation/full-code-audit-2026-10-20.md#final-linked-images).

### BLE prototype integration evidence (2026-10-18)

The radio adds static futures, HCI/ACL/GATT pools and security state. The selected
Bluetooth-only CYW configuration retains four RX and four TX HCI slots of 272
bytes, eight 251-byte ACL buffers and four notification values. Its inert Wi-Fi
channel retains one 64-byte slot per direction because a zero-capacity channel
panics during construction. Wi-Fi initialization is excluded. These payload
bounds are explained in the [radio dependency review](../dependencies/radio-review-2026-10-18.md);
they do not replace linked allocation measurements.

| Traced BLE artifact | Static RAM | Core 0 headroom | 65536 B geometry gate |
| --- | ---: | ---: | --- |
| Initial integration | 467040 B | 57248 B | Rejected |
| Compact buffers | 458144 B | 66144 B | Pass |
| Reviewed security/lifecycle image | 458168 B | 66120 B | Pass |

The last row is the review-stage unstripped ELF with SHA-256
`05b9b3e414e405ad890654025f4552f6ef779ea2921f6f56c7bf38ef6f47f37b`.
It used nightly-2026-10-16, the Cortex-M33 target, default features plus
`ble-obd,frame-trace,te-trace`, and `CARGO_PROFILE_RELEASE_STRIP=none`.
It leaves 584 B above the minimum headroom gate. These offline geometry and named
frame checks describe that historical artifact. Final practical captures at
`394ac1f` subsequently passed with the following geometry:

| Final capture | Static RAM | Core 0 headroom |
| --- | ---: | ---: |
| Normal BLE | 418056 B | 106232 B |
| Stock traced BLE | 458168 B | 66120 B |
| 280/70 traced BLE | 458168 B | 66120 B |
| Traced BLE, 64 widgets and 524288-byte custom artwork | 458168 B | 66120 B |

All four retained identical production source hash maps and passed independent
ELF/UF2 payload checks. Their hashes, input sizes and named-bound differences are
in the [final software validation record](../validation/ble-prototype-2026-10-18.md#exact-linked-images).
These remain artifact checks, not measured physical stack high-water.

The reviewed named bounds are boot 3336 B, core1 construction 18736 B, core1
runtime 15632 B, dashboard 28200 B and radio 16640 B. Radio adds an optional `radio`
bound to the existing frame record; no-BLE records keep their previous shape.
The radio path must fit actual core0 headroom with the unchanged 8192 B reserve.
Core1 remains 32768 B with 4096 B reserve. Both 153600-byte framebuffer payloads
remain unchanged.

Radio admission counts emitted initialization and poll frames, secure pairing
and P-256 helpers, taking the maximum of sibling paths. Static future storage
is already counted in RAM. A fixed 4096 B child allowance covers reviewed smaller
crypto, ATT/GATT/ELM, bus, executor, formatting and memory helpers. Exact named
boundaries and metadata are required; aliases count once. The mutex refinement
inlined host selection into an emitted `Join::poll`, which has its own reviewed
fixture. Missing boundaries cannot silently remove radio admission.

The v6 configuration path retains both BleSettings helpers and their folded
Color decoder/serializer children. Frame growth and malformed/missing metadata,
radio initialization, sibling maxima, old captures and altered receipt fields
have regressions. Dependency or compiler outlining changes require renewed
source/disassembly review of the allowances. This remains a practical named-path
check, with no complete indirect-call, recursion, exception or hardware high-water
proof. Later source or configuration changes need their own exact image admission.

The runtime script was checked at the exact published
[cortex-m-rt revision](https://github.com/rust-embedded/cortex-m/blob/f259e379457008329659c3b1207f61316c35b6f0/cortex-m-rt/link.x.in),
against the locked crate's script and the linked symbols. Core 1's separate
32 KiB stack bounds come from the actual HAL `Stack.mem` allocation; its
MSPLIM initialization was checked in locked Embassy RP 0.10.0 `multicore.rs`.

Core 0 reports the current SP depth. Core 1 reports the greatest depth observed
at sampling points. Neither measures the deepest unobserved call frame. The
old sentinel scan of live stack words was removed: initialization and aliasing
requirements were not established, and untouched bytes in deep frames could
understate use. No stack memory is read by the replacement. Both measurements
still require target stress tests; neither proves a safe worst-case margin.

Debug rounds observed stack byte depths upward to KiB on both cores; an invalid
current core 0 SP renders unavailable rather than zero. Its `Static` row counts
linked allocations in the 512 KiB main RAM arena, including framebuffers and the
reserved core 1 stack. It is neither total live usage nor all 520 KiB of physical
SRAM: the two extra 4 KiB banks lie outside this arena.

The standalone probe samples SP around task polls and retains the largest accepted
depth for each core. The dashboard keeps a current core-0 observation and the
largest core-1 acquisition observation. These scopes differ. Both routes report
unavailable until a valid observation exists; neither scans or paints live stack
memory. Core-1 work and stack fields cross cores in one timestamped publication,
while the historical stack maximum is retained independently of work freshness.

Compact `K` labels mean KiB rounded upward for both numerator and capacity, after
checking exact byte bounds. Rounded stack capacities must not be added to the
rounded `Static` value to reconstruct RAM: the core-1 stack is already a static
allocation, and the core-0 capacity describes the remaining linked arena. Capture
buffers, USB buffers and task futures belong in the new image's static count even
when their contents are idle or empty. A reserved flash capture region is not RAM.
Keep each image's exact-byte linked receipt as the memory admission evidence;
screen photographs and synthetic gallery values cannot replace it.

### Optional cycle evidence without growing the trace ring (2026-10-19)

Preserving missing cycle observations initially grew each stored trace by eight
bytes. The traced BLE dashboard then had 65384 bytes of core-0 headroom and was
correctly rejected by the unchanged 65536-byte gate. Private `FrameTrace` storage
now places an independent cycle-presence flag in existing padding, while its
public `RenderTrace` API retains `Option<u64>`. Missing, zero and `u64::MAX` remain
distinct; there is no sentinel, narrowing, packed representation or unsafe cast.

The pinned ARM compiler reports 152 bytes per stored trace, restoring the
64-entry ring's linked allocation to 9752 bytes including its wrapper. Final
traced BLE stock and 280/70 images use 458344 static bytes and leave 65944 bytes,
408 above the admission floor. The total reduction from the rejected image is
560 bytes across the ring and other stored copies. This does not change stack
reserves or establish physical worst-case depth. Exact image identities and
verification scope are in the [delivery record](../validation/probe-offline-audit-2026-10-19.md).

### Discovery boundary and rejected-evidence follow-up (2026-10-19)

The subsequent checked stock and 280/70 probe images use 401696 static bytes and
leave 122592 bytes of core-0 headroom. The bounded rejected-declaration prefix and
capture v2 preserve the existing buffers, 32768-byte core-1 stack and stack
reserves. The probe's nonradio emitted-frame total is 12080 bytes, 208 below its
unchanged 12288-byte gate; its largest emitted frame is 1504 bytes. Named bounds
are boot 17128, core 0 17240, radio 24648 and render 17272 bytes.

The corresponding traced BLE dashboard images use 458360 static bytes and leave
65928 bytes, 392 above the unchanged 65536-byte geometry floor. The shared client
repair changes its radio named bound to 17352 bytes; boot/construction/runtime/
dashboard bounds remain 3336/18736/15632/28200 bytes. Traced demo images use
441584 static bytes and leave 82704 bytes. Their construction/runtime bounds are
18808/15704 bytes, with no radio bound. Both clock profiles passed the same
practical checks for all three image types.

Exact hashes, compiler/options and the source-drift comparison are in the
[discovery follow-up record](../validation/probe-discovery-followup-2026-10-19.md#exact-linked-images).
The narrow margins remain visible; no gate or reserve was weakened. These figures
describe the linked artifacts and reviewed paths, not observed worst-case stack
use, total live RAM, CPU utilization or physical hardware qualification.
