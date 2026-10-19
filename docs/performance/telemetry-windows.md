# Long-duration software telemetry

`frame-trace` enables one fixed-memory histogram owner in the display task.
It records successful HAL completions and publishes nonoverlapping windows of
at least 30 seconds. A one-second receive timeout also publishes empty windows.
No timeout surrounds DMA. Locked Embassy time 0.5.1 polls the inner receive
first; sync 0.8.0 removes an item only when returning `Ready`. A pending receive
holds no framebuffer, consistent with [Embassy cancellation semantics](https://docs.embassy.dev/embassy-futures/0.1.2/default/select/fn.select.html).

The initial window starts at boot zero. Exclude it from claims about operation
after warmup. Previous completion identity survives window resets, so an interval
after idle can exceed the current window duration. Invalid clocks, replayed
frame/input IDs, overlapping transfers and exhausted counters reject the entire
record before any histogram changes.

## Metrics and bounds

Frame metrics use microseconds. Bins are exact through 31, followed by sixteen
bins per power of two, covering all `u64`. p50/p95/p99 are inclusive nearest-rank
**bounds**, not interpolated points. Min/max are exact observations. Empty
statistics remain absent. One coalesced input batch contributes one sample.

| ID | Software observation |
| --- | --- |
| 0 | Time between accepted HAL completions |
| 1 | Render wall time, including cooperative suspension |
| 2 | Queued to first observed payload poll |
| 3 | First payload poll to HAL return and CS high |
| 4 | Render owner's blocked buffer-acquisition time |
| 5 | Oldest accepted snapshot sample to completion, if present |
| 6 | Earliest observation in an applied input batch to completion |
| 7 | Latest consumption in that batch to completion |
| 8 | Intersection of current render wall span and previous transfer wall span |

Metric 8 includes render yields and gaps within multi-window transfers. It does
not measure simultaneous useful CPU and DMA work. Metrics 0 and 8 need a preceding
completion. Failed HAL writes never contribute a sample. Input exclusions are
counted separately; see [input correlation](frame-traces.md).

Summaries contain accepted frames, payload bytes, render active-cycle sums and
rejected records. Seven input counters are lifetime snapshots of the render
owner's last publication: ignored, no-damage, obscured, invalid, maintenance gaps,
encoder dropped and encoder discarded. They are coherent copies but need not
have the timestamp of the final completed frame.

Nine histograms live in one flusher-owned static cell. Scans/resets occur outside
critical sections. A Watch copies the compact report; main exports outside the
transfer operation. Sequence gaps expose overwritten reports. The independent
64-record raw ring is not the source of these distributions.

### Exact counter packing

Histogram counters retain their full `u32` range. Each bin stores its low 24
bits in a `u16` and a `u8`; a 156-byte bitset stores the high bytes as unary
runs terminated by zero bits. Since all accepted bin counts together cannot
exceed `u32::MAX`, the sum of their high bytes is at most 255. Thus even 976
bin delimiters plus 255 one bits fit in the fixed 1248-bit bitset. Overflow
samples use the existing separate counter. This proof assumes no frame rate,
clock increment, maximum timing value or early window reset.

Ordinary recording updates the low counter directly. A 24-bit carry inserts
one bit with at most 39 word scans, 31 zero selections and 39 word shifts. At
most 255 such carries occur between clears. Quantiles decode at most 976 bins
and 255 high bits; the three summary percentiles share one forward scan.
All work remains outside locks, with no allocation or temporary histogram.
Count exhaustion, rejection saturation, exact extrema, inclusive bounds and
the versioned export remain unchanged, including idle gaps spanning `u64`.

Host regression checks bound `FrameWindow` to 29000 bytes; the current Windows
layout is 28440 bytes. Linked ARM images, stack allowances and the two fixed
framebuffers require their separate image checks. The seeded differential
tests retain the dense implementation from `168cb0d7` and cover every bin's
carry positions, full high capacity, counter exhaustion, compact overflow,
all percentile inputs and full-range boundary/random streams.

The 2026-10-18 matched policy-2 captures `ble_ram_before` and
`ble_ram_after_scan` use nightly-2026-10-16, unchanged workloads and 30 raw
samples per case on `ea888-workstation-01`. Median nine-metric recording cost
was 47.932 ns before and 54.300 ns after (+13.29%); summary/reset of 1500 frames
was 1305.789 ns before and 1088.874 ns after (-16.61%). Both pass the explicit
20% host comparison budget. The initial `ble_ram_after` capture retained three
scans and regressed summary cost to 1964.777 ns; that evidence is retained.
These are host observations, not target cycle bounds or physical FPS. The
finite carry path is covered by seeded correctness tests and the work bound
above; it is not exercised by these short benchmark workloads.

## Versioned export and collector

RTT writes are nonblocking and may lose data. The shared Rust encoder is also
used by the host fixture; Python tests parse that producer's actual output.

```text
FRAME2,seq,start_us,end_us,frames,payload_bytes,active_cycles,rejected
INPUT1,seq,ignored,no_damage,obscured,invalid,maintenance_gaps,dropped,discarded
METRIC1,kind,seq,metric,samples,overflow,rejected,min,max,p50lo,p50hi,p95lo,p95hi,p99lo,p99hi
END1,kind,seq
```

Kind 0 requires nine metrics and one input row. Kind 1 is a
[diagnostic snapshot](diagnostic-telemetry.md) with seven PID and seven metric
rows, using milliseconds. Groups may interleave. A fixed 320-byte encoder buffer
covers maximum-width numeric rows. Bound zero means absent only when samples=0.

`FRAME2` cycle sums are either an unsigned integer or the exact token `na`.
If any completed frame lacks a valid cycle observation, or their sum overflows,
the whole window's cycle sum is unavailable; its frame count, payload total and
timing distributions remain valid. An idle window has a known zero sum. The
collector maps `na` to JSON `null`, accepts it only in this field, and still reads
legacy numeric `FRAME1` groups. JSON retains `record_version` (1 or 2) so a legacy
zero, which cannot establish counter availability, remains distinguishable.
Other row versions and group identity rules remain unchanged. Old collectors
must be upgraded before reading `FRAME2`; they cannot silently treat `na` as zero.

Cycle availability is separate from timing admission: a frame can have valid
monotonic transfer timestamps while its DWT sample is unavailable. Its timing
distribution and frame/payload counts still contribute. A partially known cycle
sum must not be divided by the whole window to estimate CPU use. These are render
active cycles, not both-core work or all interrupt/scheduler cycles. They also
remain distinct from the standalone probe's elapsed task-poll work percentages.

Keep the original RTT text beside parsed JSON. `record_version` identifies the
producer row's availability semantics; it does not authenticate the image or
replace its ELF/source receipt. JSON `null`, numeric zero and a missing/rejected
group carry different meanings and must remain distinct in later analysis.

From `host`:

```powershell
python scripts/parse_telemetry.py capture.txt --output capture.json
python scripts/test_telemetry_export.py
```

The collector requires matching headers/end markers, complete unique rows,
legal counts/bounds, monotonic sequences/clocks and lifetime counters. Corrupt
identities conservatively invalidate open groups. It emits only validated groups
and retains errors; errors or no complete recognized group give a nonzero exit
status. `missing_or_incomplete` counts all sequences through the greatest
observed monotonic header that were not accepted, including trailing incomplete
groups. Loss after the last observed header cannot be inferred. Corrupt header
identities can also corrupt this reported span; inspect errors before using it.
Split captures after reboot; resets cannot silently join continuous data.

The JSON rate uses accepted frames over actual window duration. Inspect rejects
and gaps before comparison. These values do not prove physical FPS, scanout,
physical input latency or ECU validity. SPI/CS/TE and vehicle captures are pending.

## Host and build evidence

Tests include sorted-reference distributions over 1,500 serial/overlapping
frames, idle/boundary windows, zero/extreme clocks, count exhaustion, replay,
receive cancellation, maximum-width encoding and corrupt RTT groups. The Longan
both-trace release recorded during the original telemetry-window work used
438,456 bytes static RAM with 85,832 bytes
below the core-0 stack top. All four reviewed SRAM flash routines pass their byte
gates. These historical figures are linker bounds, not measured stack high-water
usage or measurements of the current BLE/capture firmware. Use the current
image's admission receipt for its allocation and stack estimates.

`stage08_windows` retains policy-2 provenance, 171 source inputs and 30 raw samples
per case on `ea888-workstation-01`. Median cost is 41.453 ns for recording all nine
metrics and 1.093 us for summarizing/resetting a fixed 1,500-frame fixture. Setup
is outside each measured operation. These establish host instrumentation cost;
no preexisting equivalent feature or target speedup is claimed.
