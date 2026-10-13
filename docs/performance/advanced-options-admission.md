# Advanced options: local resource and host evidence

This record admits the local v4 implementation to the existing compile and
resource gates. Release integration and independent review remain separate.
No device was flashed or measured. Host elapsed times establish neither Pico
FPS nor executor/input latency, ECU support or new overclock qualification.

## Frozen target artifacts

All builds use the ordinary serializer and traced production options,
`nightly-2026-10-12`, rustc commit
`080cc27146389dbb5c8772c476b1937be9c6110f` (LLVM 23.1.3), unchanged locks,
`frame-trace` and `te-trace`. There is no measurement cfg. Capture directories
are under ignored `firmware/target/bench-captures/`; their manifests include
233 source hashes, source ZIP, selected/normalized/embedded configuration,
compiler invocation, ELF and gate output.

| Capture | ELF SHA256 |
|---|---|
| `advanced11-demo280-traced` | `d7ebdda6e126ab7cd2d22876da25d10cec49bdcbb5c24cc6478d4ac68752bb5d` |
| `advanced11-vehicle280-traced2` | `5ea7aa6e2c15935104fb293295b89f8e8cd3d3135144abbccd3c39ef784b340d` |
| `advanced11-vehicle-stock-traced` | `720e280060b53d31193202e7b08ad227f50d91ea9d09e89f84a68b127c5811cf` |

Demo uses the default configuration; Vehicle uses the advanced maximum fixture
from `host/benches/config_perf.rs`: 8 views, 64 independently configured widgets,
all six styles, 192 bands, 64 distinct smoothing/peak times, warning priorities,
latch/snooze/inside/outside options, eight history windows and D2 LED globals.
Its EAV SHA256 is
`4f63015b3b853c225156eb2fe2dcd85aac8d50ed0497ae141ed0eeb70551b2f2`.
EAV is 13,362 bytes; compact JSON is 87,420 bytes; the saved editor envelope is
87,492 bytes. The separate structural/string stress fixture remains 14,007 EAV
bytes. Both fit the unchanged 16 KiB binary and 128 KiB JSON/envelope caps.
Escaped-string conservative bounds and legacy migration/canonicality tests also
pass; this advanced fixture does not claim to maximize every encoded byte.

## Whole-artifact admission

| Capture | Static RAM | Core0 headroom | Margin above 65,536 | Boot | Core1 construct | Core1 runtime | Dashboard |
|---|---:|---:|---:|---:|---:|---:|---:|
| Default Demo 280/70 | 448,312 | 75,976 | 10,440 | 2,824 | 12,024 | 25,088 | 27,784 |
| Maximum Vehicle 280/70 | 458,296 | 65,992 | 456 | 2,816 | 17,464 | 17,312 | 28,816 |
| Maximum Vehicle stock | 458,296 | 65,992 | 456 | 2,816 | 17,464 | 17,312 | 28,816 |

All numbers are bytes. The 8,192-byte Core0 reserve and 4,096-byte Core1 reserve
are unchanged; Core1 has 32,768 stack bytes. Memory layout, named stack paths,
flash lockout and their negative mutations pass. These are conservative named
path admissions, not whole-program stack high-water measurements. The remaining
456-byte static margin is small and must be remeasured after future growth.

ARM metadata and ELF reconciliation give Config 14,576; Widget 220;
ValidatedConfig and its Result 14,588; BootConfig/ConfigSlot 14,600;
SensorState 1,272; AcquisitionState 16,048 (borrowed owner 16,056);
RuleEngineState 4,368 (borrowed facade 4,376); Snapshot 4,144; RuleAlerts 680;
PeakHolds 776; PresentationState 200; DisplayReadings 32;
PresentedWidget 24; PresentedLed 264; PresentedScene 120; FrameRequest 272;
request channel 312; Snapshot Watch 4,176.

The raster key is 16 bytes, CellKey 168, CachedFrame 1,528 and DashboardCache
3,056. Exact shared quantization saves 128 static bytes versus the preceding
artifact without changing visible pixels. The configuration static is 14,604
including its cell wrapper. Each framebuffer static is 153,604; FrameWindow is
35,792 including synchronization; Core1 stack storage is 32,800. Embassy task
storage is 23,328 Vehicle / 16,256 Demo for acquisition, 10,848 / 10,816 for the
dashboard, 536 for flush and 336 for encoder. These are emitted storage sizes,
not additive estimates based on source locals.

Snapshot ownership is reconciled at both source and emitted copies. Persistent
owners are the Watch value and dashboard measurements. Publication has an
8,336-byte frame containing two full snapshot temporaries plus frame overhead;
the owner facade adds 8, private snapshot construction 4,568 and sensor snapshot
152, totaling 13,064. Eleven emitted 4,144-byte copy sites include startup,
publication and alternative Watch methods; they are not eleven simultaneously
live snapshots. Previous publication identity is compact metadata. Presentation
scratch is prepared for rendering and is not a second configuration owner.

Boot uses in-place seeded decode, then canonical comparison and borrowed
validation, or complete default refill. The largest decoded widget helper is
408 bytes, advanced-display helper 384, warning helper 176; the loader is
904 Vehicle / 912 Demo and canonical comparator 56. The reviewed nested config
path is 1,752 / 1,760, with sibling branches taking their maximum. No whole
14,576-byte automatic configuration is introduced. Disassembly retains 31
cooperative render checkpoint calls. Detailed layouts, all serializer helper
frames, copy sites and prologues are preserved in the ignored evidence directory.

## Host workload evidence

The registered suite has 139 cases and passes smoke checks with both outline
configurations. Configured-view timings include production prepare-once
presentation. The older raw-view captures omit that work and have changed
semantics; they are not a strict matched comparison.

The matched 38-case pair `advanced11_before` / `advanced11_after` uses identical
benchmark sources and options, CPU2 affinity, runner
`johnny-desktop-cpu2`, the pinned compiler/locks, 30 samples, one second warmup
and two seconds measurement per case, with default `simple-outline`. Medians derive from raw time/iteration
samples. The maintained strict 20% comparison passes all 38 cases. Raw archives
are under ignored `firmware/target/task11-evidence/` with the same capture names.

| Matched identical-pixel workload, both buffers | Before | After |
|---|---:|---:|
| Eight Circular smoothed three-band gauges | 272.532 us | 6.427 us |
| Eight Horizontal smoothed three-band gauges | 210.203 us | 6.113 us |

The change avoids repaint when the full formatted text, raw bounds, quantized
current/peak geometry, quality, warning/highlight, palette and other cache
dependencies are unchanged. Tests compare both buffers with complete redraws;
changed text at the same level still repaints. Actual moving pixels remain real
work: the separate `advanced11_supplement` measures 290.836 us Circular and
156.479 us Horizontal including preparation, both buffer draws and damage.
It has a different source inventory and is new-capability evidence, not another
matched speedup. Changes in unchanged code paths across runs show host timing
variability; no causal improvement is claimed for those paths.

| Bounded path | Host median |
|---|---:|
| 0 / 8 / 64 enabled warnings | 257.826 / 413.468 / 958.860 ns |
| 64 independent same-channel peaks | 336.735 ns |
| One-day history gap | 31.268 ns |
| Static graph retained / reconstructed preview | 3.121 / 182.227 us |
| Maximum owned decode / slot decode / late fallback | 82.605 / 82.911 / 64.774 us |
| Complete CycleOne / ListThree paging of all 64 warnings | 76.219 / 25.412 us |

Static graph and reconstructed preview deliberately perform different work.
Complete paging includes 64 or 22 deadline advances/page scans; acquisition is
outside the measured batch. These new cases have no fabricated pre-feature
baseline. Peak/smoothing preparation uses fixed bounded inputs without sleeps.

The preserved `power_user_before_a` / `_b` archives remain untouched. Their
12-case historical comparison is rejected by strict compatibility because
fixture/capture sources changed. For the accepted-sample sensor-state case,
the same input trace costs 4.328 / 4.391 ns historically versus 9.120 ns now:
+110.7% / +107.7%, about 4.8 ns more per sample. Old history sampled periodically;
the new required last-observation buckets perform ordinal, validity and revision
updates for accepted samples and represent gaps explicitly. This is disclosed
additional semantic work, not a strict old/new performance pass. Remaining
historical point estimates are within 10.4% increase or faster, with the same
comparability limitation. Target timing qualification remains outstanding.
