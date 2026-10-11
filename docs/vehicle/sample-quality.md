# Measurement source and quality

The acquisition owner now accepts canonical `Quantity` values with an explicit
source, capture time and per-channel sequence. Publication sequence and reset
acknowledgement remain separate. Vehicle mode starts with every channel unknown.
The `demo` Cargo feature explicitly selects synthetic data; disconnects never
select it automatically. This checkpoint provides the model and presentation;
the UART/protocol owner is the next implementation slice.

Vehicle ingestion rejects future times, duplicate/regressed sequences or sample
times, source/PID mismatches, non-diagnostic response IDs and changes of responder
or source within a channel. Invalid numeric payloads invalidate quality without
altering last-good statistics. Faults preserve those statistics and their original
timestamp. Recovery requires a newer valid sample. Sequence exhaustion is terminal
and checked before mutation, including mixed typed and batch Demo ingestion.

Freshness is evaluated against the presentation clock, so a coalesced or stopped
producer cannot keep a stale value visible. Provisional age limits are 500 ms for
boost/lambda, 2 s for module voltage and 3 s for temperatures. These are software
presentation limits, not validated polling rates for this car. Unknown, stale,
unsupported, transport-faulted or invalid channels show `--` with a status label;
they do not show zero, old graphs or critical colors. A valid zero remains distinct.
Both retained framebuffer caches include the effective quality and source.

Lambda is the canonical mixture quantity. No unconditional lean/optimal alarm
is inferred from a fixed AFR threshold. `ECU V` identifies control-module
voltage, not a battery-terminal measurement. Vehicle alarm coloring is disabled
until source semantics and policies have been qualified. The remaining Demo
temperature/color thresholds are synthetic presentation behavior and are not
vehicle limits. Dedicated alarm policy and boot changes follow in Stage07.

An accepted typed source does not establish vehicle compatibility. The transport
must still enforce selected responder, request, payload width, capabilities,
scaling and epoch. In particular, PID4F can change MAP/wideband scaling; the
future decoder must not silently assume the default when scaling is unknown.
No DNU-specific DID, DSG oil or physical EGT source is enabled by this change.
See [vehicle evidence limits](canchecked-dnu-evidence.md).

## Lambda and derived AFR

The lambda cell includes a secondary gasoline-equivalent readout:
`AFR = lambda * 14.7`. Its visible `EQ 14.7` footer identifies the selected
display scale. Examples: lambda 0.800 gives 11.76, 1.000 gives 14.70, and 1.200
gives 17.64. Calculation uses the original value before rounding lambda to
three decimals or AFR to two. The graph, acquisition and alarm model continue
to use lambda; this adds no second sensor or independent AFR alarm.

The user's fuel is Shell V-Power Racing 100 labelled E5.
[E5 denotes up to 5% ethanol](https://www.gov.uk/guidance/e10-petrol-explained);
the fuel name and label do not supply the actual tank blend or its
stoichiometric mass ratio.
Consequently the 14.7 display scale is a convention, not a claim of exact E5 AFR.
The [AEM X-Series manual](https://documents.aemelectronics.com/7de5aac1eb25d0b33b15314fdaed2446291df20e.pdf)
explains fuel-dependent AFR conversion and uses 14.65 for its own gasoline
display. That product-specific scale is not assumed to identify this fuel.

Derived text and retained-frame keys share the same formatter, including AFR
changes that do not change the rounded lambda digits. It shows `AFR --` for
nonfinite, nonpositive or unrepresentable results outside the 0.01..999.99
display window. This is a formatting bound, not a validated sensor range.
The entire cell still obeys lambda's source, payload, timestamp and freshness
guards. Stale or unqualified lambda cannot retain an apparently live AFR.
Vehicle lambda remains unavailable until its actual source/scaling is qualified;
Demo continues to use synthetic lambda.

## Verification and cost

114 runtime host tests and two compile-fail ownership checks pass in both outline
modes. New tests cover absence versus zero, all channel stale boundaries,
timestamp/source/replay rejection, invalid/fault recovery, mixed Demo sequence
monotonicity and preflight exhaustion. Actual retained pixel replay covers both
buffers through valid, stale, link-fault and recovery states. The original numeric
shortening and timed trend tests retain their original meaning under these rules.
An independent review found the mixed Demo sequence defect; its regression test
failed before the correction and passes afterward.

Strict host and firmware Clippy pass. Stock Vehicle and Demo with both traces at
300 MHz cross-build and pass the ELF RAM boundary gate: stock static 374352 B,
headroom 149936 B; Demo/trace300 static 381136 B, headroom 143152 B. These figures
are linked capacity, not measured stack maxima.

The raw 30-sample Criterion run in `host/baselines/stage06_typed` records source
hashes, command, host and toolchain. Local medians:

| Fixture | Median |
| --- | ---: |
| Typed single-channel ingestion and snapshot | 277.5 ns |
| Eight-channel Demo batch and snapshot | 436.5 ns |
| Coherent Watch copy roundtrip | 77.7 ns |
| Full eight-cell scene | 105.480 us |
| Retained stationary scene | 1.955 us |
| Full scene with one changing cell | 104.313 us |
| Retained one changing cell | 11.217 us |

This establishes the new typed/canonical-lambda baseline. Earlier scene output
included AFR text and different labels, so cross-stage differences are not an
isolated optimization measurement. Same-contract paired render paths remain
comparable. Acquisition/copy costs are close to the earlier 443.2/76.5 ns run,
within the observed local variation. No physical FPS, ECU support, UART timing or
vehicle alarm validation is established by these host measurements.
