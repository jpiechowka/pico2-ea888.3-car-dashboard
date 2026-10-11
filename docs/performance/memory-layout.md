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
headroom. `scripts/check_memory_layout.py` reads the actual ELF sections and
symbols, checks all allocated RAM against those boundaries, and confirms the
boot footer stays in FLASH. The check failed on the original diagnostic ELF
and passes after the fix. CI runs it on stock and 300 MHz builds.

Reproduce from `firmware` with `CARGO_PROFILE_RELEASE_STRIP=none`:

```text
cargo build --locked -p dashboard-pico2 --release
python scripts/check_memory_layout.py target/thumbv8m.main-none-eabihf/release/pico2
```

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
