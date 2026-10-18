# BLE prototype software validation

2026-10-18. USB-powered Pico 2 W, PIM715 and Adafruit 5880 encoder.
Implementation source: `394ac1f3b5c6c925fcd3465be989f5b360520438`.
The selected route replaces the proposed wired protection assembly with BLE.
Both framebuffers, dashboard features, startup artwork, orientations, controls,
LED policy, protected recovery and Studio workflows remain available.

**This is software evidence. No radio, adapter, ECU or physical display was
executed.** MC+/CX initialize and identify an adapter only. Their completion
does not enable live DNUC measurements or alarms. The synthetic test profile uses
a separate service and never publishes vehicle samples. Instruction-level proof
remains deferred.

## Changes and audit repairs

- Configuration v6 adds a bounded 16-byte BLE setting. Frozen v1–v5 projects
  migrate with BLE disabled; startup artwork and originals remain preserved.
- Shared parsing, one-command ownership, immutable deadlines, checked epochs,
  loss detection and fair scheduling serve firmware and Studio diagnostics.
- Core 0 owns radio pins, PIO0/DMA1, HCI and GATT. Flash coordination requires
  a completed bus boundary, current park acknowledgments and independent idle
  checks. Timeouts never release an active DMA buffer.
- Vendor review repaired concurrent HCI writes, starvation/loss, early
  notifications, canceled connection races and stale encryption state. Missing
  security or controller cleanup causes failure rather than a new admitted link.
- MC+ accepts its advertised Write Command capability when Write Request is
  absent. CX and TestPeer require acknowledged writes; no failed write is retried
  with another type. Every command still needs its ELM reply and final prompt.
- Windows debug CLI migration overflowed the default 1 MiB main stack. A named
  host worker with a fixed 2 MiB reserve passes full legacy/current/max-capacity
  tests. Firmware stack limits were unchanged.
- Studio has peer/timing controls, invalid-state repair, synthetic failure
  actions, direct diagnostics focus and BLE build selection. Actual browser
  checks repaired raw Option timing text and the diagnostics shortcut.

See the [runtime contract](../vehicle/ble-runtime.md),
[dependency review](../dependencies/radio-review-2026-10-18.md),
[protocol evidence](../vehicle/ble-protocol.md) and
[Studio workflow](../configurator/ble-prototype.md).

## Verification matrix

| Layer | Executed checks | Result |
| --- | --- | --- |
| Shared workspace | Locked workspace tests; configuration JSON tests; strict all-target Clippy; formatting | Pass |
| Host renderer/state | Default and full-outline tests; strict Clippy; benchmark smoke; formatting | Pass |
| Studio | Native tests/Clippy; Wasm Clippy; optimized production build; formatting | Pass |
| Browser | Full release failure suite in Edge at 1440×1000 and 760×1000; final shortcut/latency regression at 760×1000 | Pass |
| Final production UI | Independent actual browser at 1440×1000: peer/profile edits, CX help, direct focus, fragmented reply, old epoch, notification loss, BLE build choice | Pass |
| Local helper | Native tests/strict Clippy/formatting; Windows wrapper/setup contracts; Python helper and browser-process cleanup contracts | Pass |
| Radio dependencies | CYW 12 tests in each Bluetooth mode; Trouble 75 security-enabled tests; complete provenance checks and normal/optimized verifier regressions | Pass |
| ARM | Strict Clippy for unavailable, BLE, legacy Longan, traced demo and traced BLE target-bench paths; formatting | Pass |
| Artifact tooling | Capture, practical-image, clocks, named-stack and validation/logging policy regressions; selected Python `-O` repeats | Pass |
| Linked firmware | Exact practical normal, stock/280 traced and maximum config/artwork traced captures; ELF-to-UF2 conversion and independent byte/range/boot checks | Pass |

Browser fixture response times are virtual milliseconds. Neither native tests
nor browser tests negotiate a physical BLE connection. Failed sandbox linker and
helper-access attempts were repeated with the required local process permissions;
they were not counted as passes. The final build used the repository-installed
Trunk/wasm-bindgen tools as `Studio.ps1` configures them.

The first published CI run exposed a cold-cache setup gap: the all-feature,
offline logging-policy check could not download optional `aes 0.8.4`, which the
preceding default-backend Clippy build did not need. CI now fetches the locked
firmware dependencies before those offline checks. The policy and strict Clippy
requirements are unchanged; this repair changes no firmware source or allocation.

## Exact linked images

All four captures used nightly-2026-10-16, `thumbv8m.main-none-eabihf`, explicit
`-C target-cpu=cortex-m33`, optimization 3, `ble-obd` and unstripped ELF metadata.
The traced captures add `frame-trace,te-trace`; the 280/70 capture also uses its
named Expert profile. All retain the default `simple-outline` feature. Their
491-file production source hash maps are identical.
Default configuration SHA-256:
`4d255ed4d1e9546a608caea2c618861d19f35a493b00e7d676ce41826b168b81`.

| Capture | ELF SHA-256 | Static RAM | Core 0 headroom |
| --- | --- | ---: | ---: |
| `ble-final-stock` | `631fcea8af3a27cda631f2f69643e1a68bd92d95ad33a7dc57db2e0714c3e22c` | 458168 B | 66120 B |
| `ble-final-280` | `c37d7b624debe2ecde96a053e1fb350077f8dc13355cd16b45556eaba0f6ae4d` | 458168 B | 66120 B |
| `ble-final-normal-studio` | `195987d0937b65ea7b8115f6ba7d2b6c16277d3874fdeff0022942432ca3dfc0` | 418056 B | 106232 B |
| `ble-final-max-art-studio` | `7c39d5507ce338031a0e3474574779892ff56f95145f5cd9f9ba317c9e6fb368` | 458168 B | 66120 B |

The mandatory geometry minimum remains 65536 B, with 584 B additional margin
in the traced builds. Normal firmware leaves 106232 B before the core 0 stack top.
Named bounds are boot 3336, core1 construction 18736, core1 runtime 15632,
dashboard 28200 and radio 16640 bytes for the default traced image. Normal
dashboard bound is 26728 B; maximum-artwork boot bound is 3360 B. Core0 reserve remains 8192 bytes; core1
stack/reserve remain 32768/4096 bytes. These are source-reviewed conservative
named-path bounds, not a complete activation graph or measured high-water.

The maximum case contains eight views, 64 widgets and enabled TestPeer settings.
Its 14041-byte EAV SHA-256 is
`f5c4087f3c0212fb38269222c0e4041a0f027ed6a50ec9f932b5796e627a2910`.
The shared converter produced an exactly 524288-byte EASA from a generated PNG
sprite sheet: three raw frames, one row-RLE frame, four 100 ms durations, no padding.
All 307200 decoded pixels were compared with the source. Asset SHA-256:
`6e8cc35a33e7000ec3ccacf8ecbe31a830d1dffd70353879c39ebd80b4420e0b`.
The linked asset occupies 524289 B including the existing sentinel. This image
also passes the 2 MiB program layout and exact UF2 comparison. An initial fixture
generator had an RGB565 rounding mismatch; it was corrected before admission,
and the failed attempt remains in local evidence.

Reproduce from `firmware/`, with fresh capture names:

```text
python scripts/capture_target_bench.py ble-stock-review --firmware --image-gate practical --features ble-obd,frame-trace,te-trace
python scripts/capture_target_bench.py ble-280-review --firmware --image-gate practical --features ble-obd,cpu280-spi70-1v30,frame-trace,te-trace --expert
python scripts/capture_target_bench.py ble-normal-review --firmware --image-gate practical --features ble-obd
```

Captures retain compiler identity, source archive, configuration, options,
ELF, clock evidence, practical receipt and altered-artifact rejection results.
The pinned picotool converts ELF to `rp2350-arm-s` UF2 without device access;
`check_program_image.py <elf> --uf2 <uf2>` checks the exact emitted payload.
Image hashes vary with source/build identity. The
[memory contract](../performance/memory-layout.md#ble-prototype-integration-evidence-2026-10-18)
retains failed initial images and the later compact-buffer evidence.

## Performance and visual evidence

Exact compact histograms save 7344 bytes across nine traced distributions.
The matched host benchmark found recording 13.29% slower and summary/reset
16.61% faster; the rejected first implementation and raw samples are retained.
See [histogram evidence](../performance/telemetry-windows.md).
The Bluetooth-only payload changes save 8916 bytes before alignment/metadata,
retaining four TX/four RX HCI slots, ACL251, ATT247 and four notifications.
No physical polling latency or FPS gain is claimed.

The final main Wasm is 5281938 bytes. Actual settings, diagnostics and build
screenshots are linked in [README](../../README.md#bluetooth-prototype-in-studio)
and the [gallery](../configurator/gallery.md#configure-and-exercise-the-ble-prototype),
with source/Wasm/image hashes in the
[capture manifest](../images/gallery/workflows/ble-capture.json).

## Remaining device qualification

No-purchase testing still needs a physical synthetic BLE peer and Pico trial:
connection/security, negotiated interval/MTU, disconnect/loss, radio/display
coexistence, controls/LEDs, flash park/resume, watchdog and physical stack peaks.
MC+/CX pairing, actual characteristic properties, dongle sleep and DNUC diagnostic
requests/decoders remain separate checks. USB off leaves the dongle powered, and
the prototype identity loop may prevent inactivity sleep. No adapter purchase
recommendation follows from these software results alone.
