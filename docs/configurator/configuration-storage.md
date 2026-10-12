# Bounded configuration storage

The shared configuration library offers `ConfigSlot` for loading one configuration into its final storage. A caller can initialize it with `ConfigSlot::new()` in constant static storage, obtain exclusive access during startup, and call `load_at_boot(bytes)`. The result is an immutable `&BootConfig` borrowing that slot. Rust prevents another load while a previously returned result is still in use.

The slot's initialized placeholder is private. Loading checks file size, magic, envelope version, exact length and checksum before parsing. The Postcard seed assigns fields individually and fills the bounded view and widget collections one element at a time. Complete input consumption, streaming comparison with canonical serialization and the shared semantic validator are required before a reference escapes. A failed load replaces every field with the reviewed default and publishes the original fault alongside it. The slot exposes no storage getter, mutable validated reference or unchecked conversion.

`ConfigSlot` accepts only current version 3 EAV files, including in host builds with the JSON feature enabled. Owned `decode`, `load_at_boot`, `validate` and `default_config` remain available. Host JSON import and owned binary import continue to upgrade the frozen version 1 and 2 formats. JSON uses derived serde; the explicit seed is private and supports the Postcard sequence representation only.

The wire version and serialized defaults remain unchanged. Capacity remains eight views and sixty-four widgets, with a 16 KiB EAV limit, 128 KiB JSON limit and existing 10,240-byte ARM object guards. The slot path uses no heap, configuration-sized decode temporary or configuration-sized canonicality buffer. Its defaults and semantic validation are shared with the owned APIs.

Creating a local slot still puts its storage on the caller's stack. Firmware uses one `ConstStaticCell<ConfigSlot>` initialized with `ConfigSlot::new()`. Core0 takes it once, loads the embedded bytes, then publishes `&'static BootConfig` before Core1 starts. The thin startup task spawns the dashboard task and returns before its large runtime frame is polled. That lifetime boundary must remain intact.

## Target admission, 13 October 2026

The current version 3 configuration passed software admission on both standard traced 280 MHz / 70 MHz builds, with default simple outline and parallel rendering. The selected eight-view / sixty-four-widget input was `firmware/target/gauge-audit/maximum-v2.eav`; the build compiler migrated it to current version 3. These measurements do not admit future widget-field growth.

From the repository root, the exact capture commands were:

```powershell
python firmware/scripts/capture_target_bench.py slot2-demo280-traced --firmware --features demo,cpu280-spi70-1v30,frame-trace,te-trace --dashboard-config D:/GitHub/pico2-ea888.3-car-dashboard/firmware/target/gauge-audit/maximum-v2.eav
python firmware/scripts/capture_target_bench.py slot2-vehicle280-traced --firmware --features longan-rx12,cpu280-spi70-1v30,frame-trace,te-trace --dashboard-config D:/GitHub/pico2-ea888.3-car-dashboard/firmware/target/gauge-audit/maximum-v2.eav
```

Each ignored capture directory under `firmware/target/bench-captures` retains `pico2.elf`, `manifest.json`, `source.zip`, the selected input and normalized embedded configuration. The workflow sets `CARGO_PROFILE_RELEASE_STRIP=none`, uses the checked-in `-Z emit-stack-sizes` flags, and records its exact `cargo build --locked` command and effective compiler/configuration. Both archives include the new `slot.rs` and `decode_in_place.rs` sources.

| Identity | Value |
| --- | --- |
| Toolchain | nightly-2026-10-12, rustc 1.101.0-nightly `080cc27146389dbb5c8772c476b1937be9c6110f`, LLVM 23.1.3 |
| Locked dependencies | static_cell 2.1.1, serde 1.0.229, postcard 1.1.3, heapless 0.9.3 |
| Firmware Cargo.lock SHA-256 | `448c2dfd4ac7d8138e654862ba0d5fb9686c223aec45ce41bab0ec4d56391465` |
| Selected maximum v2 SHA-256 | `1c997388c33605f05a24bc08f85d4a02d56b45ee3b34f0e5af74218beef165c1` |
| Embedded maximum v3 SHA-256 | `e8a02f13e60930e37d32b0c7762c2b2675d1ea132962c109c084d25f6bb89c38` |
| Demo ELF SHA-256 | `18bd1a8adfd5938efc7e79b1b10273dbdce5203f4bee17da7b25554d19f905ab` |
| Vehicle ELF SHA-256 | `f81420b4fed05c3168c112652881dfea012f2dadecca1a2396b7c908edccc973` |

The actual `static_cell` source initializes its `UnsafeCell<T>` in `const fn new`; `take` atomically claims and borrows that existing storage. Application code introduces no unsafe. The linked configuration cell is 9,692 bytes at `0x2000069c` in both images, including the cell's claim flag/padding around the 9,688-byte slot. Its legal enum-containing initial value resides in `.data`. This replaces the previous static allocation; it does not add another configuration.

| Software resource | Demo | Vehicle |
| --- | ---: | ---: |
| Static RAM, including alignment | 445,632 B | 455,616 B |
| Core0 headroom | 78,656 B | 68,672 B |
| Boot path admission | 2,356 B | 2,356 B |
| Core1 construction admission | 7,616 B | 25,456 B |
| Core1 runtime admission | 14,784 B | 13,592 B |
| Dashboard admission | 33,504 B | 34,464 B |
| Last flash LOAD end (exclusive) | `0x1006dde0` | `0x100785b8` |

Compared with `controls5-demo280-traced` and `controls5-vehicle280-traced`, total static RAM, Core0 headroom, Core1 admission and dashboard admission are unchanged. Boot admission decreases from 50,536 to 2,356 bytes, a 48,180-byte reduction. Initialized RAM grows by 9,688 bytes while `.bss` shrinks by 9,696 bytes; alignment leaves the total unchanged. The flash LOAD end increases by 9,128 bytes for Demo and 9,136 bytes for Vehicle, remaining below the settings journal at `0x101fe000`.

## Reviewed call paths

Both linked images have the same configuration frames. The initializer is 8 bytes and the slot loader is 768 bytes. Header/CRC checks, the top-level seed, both vector seed loops, clear operations and fallback construction are inlined into that loader. Legal constant embedded-header checks can be folded by LLVM. Collection clear reduces length; these element types need no drop calls. Disassembly shows per-field/per-element stores and copies into the static object, with no whole-Config return, assignment or local decode buffer. Owned `load_at_boot`, `decode` and `default_config` are absent from the linked firmware path.

| Path below the loader | Simultaneous compiler frames |
| --- | ---: |
| View element -> UTF-8 | 40 + 36 = 76 B |
| Widget -> warning -> bounded text -> UTF-8 | 296 + 144 + 48 + 36 = 524 B |
| Source array -> source -> color | 424 + 72 + 16 = 512 B |
| Controls -> screen bindings -> action | 120 + 64 + 16 = 200 B |
| Canonical comparison -> widget serialization -> warning colors | 56 + 56 + 40 = 152 B |
| Borrowed validator -> widget color validation | 200 + 132 = 332 B |

Other nested widget children (graph range, gauge, colors), scalar field decoders, serializer children and validator helpers are required by the named gate and included in their branch maxima. The inlined fallback runs after failed decode, canonicality or validation children return, then calls the same borrowed validator. Early envelope faults use that same refill path. The fallback's default-widget construction and catalogue work are inlined; any separately emitted seed or fallback frame is counted at its source call-tree position. An unknown decoder/serializer helper or missing required symbol/frame fails the gate for review.

The compiler-frame configuration maximum is `768 + 524 = 1,292` bytes. Boot admission adds the 32-byte startup poll, conservatively retains the initializer's 8-byte frame even though the normal path tail-calls the loader, and retains the existing 1,024-byte allowance for resident entry/executor, memory helpers and panic formatting: `32 + 8 + 1292 + 1024 = 2356`. Compiler builtins lack `.stack_sizes`: disassembly shows memcpy uses 72 bytes, the aligned memcpy wrapper uses at most 16 before a tail call, and memcmp uses 32. They fit that reviewed allowance. It is not a whole-program stack proof; changes to compiler, dependencies, source call structure or those helper allowances require renewed disassembly review.

The 8 KiB Core0 reserve, 4 KiB Core1 reserve and 32 KiB Core1 stack remain unchanged. Both artifacts pass memory/flash layout, exact flash-helper checks and the revised stack gate. Eleven stack regression tests pass normally and with Python `-O`; capture/policy tests, both configuration feature suites, host contracts, both traced firmware Clippy commands with `-D warnings`, and all four workspace formatter checks pass. Clippy still reports pre-existing dependency notices: deprecated `cortex_m_rt::pre_init` in vendored embassy-rp and future incompatibility E0365 in proc-macro-error2 2.0.1. They are outside this change and do not fail the pinned checks.

Device stack high-water, simultaneous interrupt/exception use, physical display/SPI timing and vehicle behavior remain unverified. No target code was run and no device was flashed during this admission. Future advanced fields must retain all established capacities and undergo a separate measured admission before changing object guards or storage representation.
