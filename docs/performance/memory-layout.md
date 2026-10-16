# Linked RAM boundaries and sampled stack depth

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
and passes after the fix. CI runs it on twelve target matrix entries, covering stock, traced Demo/Vehicle,
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

Both gates require the explicit source context. Neither discovers a sidecar or
uses current checkout source as a fallback. Each source member is limited to
65536 UTF-8 bytes and the context to262144 bytes; memory results remain bounded
to65536 bytes. Recorded pairing and source-contract checks do not prove all
compiler-consumed inputs or complete machine ownership/effects. Practical
stack admission relies on reviewed named paths and fixed child allowances;
it is not whole-program stack or exception/reset-reentry safety proof, physical
stack high-water, clock/voltage qualification, or device validation. The full
experimental source/alias/diagnostic/loader graph work remains separate.

The trace plus 300 MHz diagnostic build used 373264 bytes of static RAM and
left 151024 bytes before the core 0 stack top. These numbers vary with features
and source changes. They are allocation evidence, not measured worst-case
stack use. The ELF check and linker assertions are the continuing gates.

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
