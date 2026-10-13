# Configuration format and portable contracts

`dashboard-config` is a `no_std`, allocation-free device library. Its optional
`json` feature provides the editable host/browser format; JSON is never linked
into the device parser. The model, catalogue, validator and binary codec are
shared by all consumers. See the current v4 contract below, [schema v2](schema-v2.md) for wire history, and
[design](design.md) for the deployment and hardware limits.

## Bounds and validation

- 1 to 8 named views, 64 widget instances total, at most 8 occupied cells per
  view on the 4-column, 2-row grid. Supported spans are1x1,2x1,2x2.
- Printable ASCII: view names14 bytes, one-column labels12, two-column labels24,
  warning messages20. No controls, newlines, blank-only strings or truncation.
- Whole-document validation rejects overlapping/out-of-bounds geometry,
  duplicate/zero IDs, dangling views, invalid precision, nonfinite numbers,
  invalid graph ranges, incompatible AFR/rules and reversed hysteresis.
- Source access, decoder and evidence must match the versioned closed catalogue.
  Candidate selection cannot add a live decoder or qualify the transport.
- All rule fields remain validated when a rule is disabled. DemoOnly defaults
  preserve synthetic examples without enabling vehicle warnings.
- Fuel conversion distinguishes equivalent gasoline, a supplied stoichiometric
  ratio, and an explicit mass/component-volume blend estimate. E5 never silently
  becomes an assumed5% measurement.

Binary files are limited to16KiB including the12-byte CRC envelope. Editable
JSON is limited to128KiB. Unknown/duplicate JSON fields, extra documents,
over-capacity collections and unknown enum variants are rejected. `encode` and
JSON export require an immutable `ValidatedConfig`; arbitrary editable structs
cannot bypass validation at export.

Postcard1.1.3 can accept nonminimal usize varints on a64-bit host that its32-bit
decoder rejects. After decoding, a streaming serialization comparison requires
the canonical byte sequence. It adds no payload-sized scratch buffer. Empty
Postcard remainder is also required. The checksum is corruption detection,
not authentication or evidence of valid ECU metadata.

## Historical v2 storage and verification

With nightly2026-10-11, the actual v2 Thumb target LLVM type layout is:

| Type | Bytes |
| --- | ---: |
| Config | 9,640 |
| ValidatedConfig | 9,644 |
| Widget | 144 |
| BootConfig | 9,656 |
| StaticCell<BootConfig> | 9,660 |

Compile-time 32-bit guards bound Config and ValidatedConfig at 10 KiB. The
default v2 binary is 912 bytes; the exercised maximum document is 8,430 bytes.
Host layouts must not be substituted for device layout. Static type size does
not establish parser stack peak; the mandatory linked-image gate checks
static/task/stack budgets separately. See [gauges and colours](gauges-and-colours.md)
for that historical stage's resource measurements and corrected construction/boot
paths; [release evidence](advanced-v4-release.md) gives the current v4 admission.

### Historical v1 baseline

The following test and timing record describes the initial v1 implementation,
before gauges and palettes. Current v4 verification is recorded in the release guide.
Its Config/ValidatedConfig/Widget target sizes were 6,820/6,824/100 bytes.

The initial verification has18 runtime tests plus2 compile-fail contracts with
JSON enabled (16 plus2 without). Those tests include196,608 placement/span
combinations,10,000 deterministic random bodies both raw and validly checksummed,
2,000 generated valid layout round trips, every default-file truncation and
single-byte corruption, collection overflow, all source/channel combinations,
fuel endpoints/monotonicity and disabled-rule validation. A manually constructed
89-byte golden envelope uses an independent uvarint builder and Python zlib CRC.
Malformed semantic payloads get valid checksums so tests reach the validator.
These bounded tests are not a proof about all possible inputs.

Native tests and strict Clippy pass. The crate builds for Thumb with no JSON,
and Wasm with JSON. Existing host contracts pass for both outline modes and
the full84-case benchmark smoke suite passes. Firmware integration and physical
validation follow in later stages.

## Initial parser benchmark

Raw capture: `host/baselines/configurator_codec_before_a`,9 cases with30 samples
each. Criterion ran on logicalCPU0; both shell affinity and actual benchmark
process affinity were recorded as mask1. Source bytes were archived before the
run and checked unchanged afterward. The maximum fixture contains8 views and
64 widgets, full-length labels/messages, fixed graph bounds and a blend profile.

| Host elapsed median | Default | Maximum fixture |
| --- | ---: | ---: |
| Validate | 1.813us | 3.747us |
| Decode, CRC and canonical check | 3.432us | 29.185us |
| Encode and CRC | 1.848us | 20.180us |
| Reject bad CRC | 2.014us | 19.843us |

Oversize rejection median is43.685ns. Input construction is outside the measured
validator operation; decode and encode include the complete byte operation.
This is a first baseline, not a claimed gain, MCU execution time or physical
FPS measurement. The parser runs at startup, not in the frame loop.

## Dependency evidence

New direct dependencies are Postcard1.1.3, Serde1.0.229 and optional
serde_json1.0.154; heapless0.9.3 gains its Serde feature. Postcard defaults are
disabled, avoiding its separate heapless compatibility edge. Serde defaults
are disabled on the device. The four lockfiles agree on Syn3.0.8 for current
derive macros; unrelated reviewed versions are retained.

The implementation was checked against the resolved crate source, including
[Postcard's flavor interface](https://docs.rs/postcard/1.1.3/postcard/ser_flavors/trait.Flavor.html)
and [take_from_bytes](https://docs.rs/postcard/1.1.3/postcard/fn.take_from_bytes.html),
heapless bounded visitors, Serde struct/enum visitors, and serde_json's end-of-input
and nonfinite-value behavior. Syn3.0.8 changes expression parsing around`<-`
and`->`; [its release](https://github.com/dtolnay/syn/releases/tag/3.0.8) retains
the prior feature/MSRV contract. Tests exercise the actual resolved versions.

The catalogue exposes pinned request mapping, numeric conversion and byte-order
references together. These documentary sources do not establish support on the
user's DNUC ECU or DQ381 transmission.

## Current v4 contract

Version4 is the complete advanced contract shared by ordinary editor save/export,
JSON/EAV codecs, host migration, configured acquisition, rendering, warning
acknowledgment and RGB policy. See [advanced controls](advanced-controls.md) for
actual defaults and capabilities and [release evidence](advanced-v4-release.md)
for the software resource gates and physical-validation limits. Organized editor
workspaces, all-screen browser pixels and native Build & Flash remain planned.

Config appends channel-indexed `history_seconds[8]` (10–900, default120),
`reduced_motion` (defaultfalse), and `led`. Widget appends `advanced`; warnings
append flat `trigger_high`, `clear_high`, `snooze_s`, `priority`, `latch` fields
in that order, after `message`. Binary declaration order is fixed and uses
normal Postcard Serde. No memory representation is cast onto the wire.

Widget advanced fields are `bands`, `reference`, `smoothing_ms`, `peak_hold_ms`,
`readout`, `secondary`, `peak_mode`, `reference_mode`, in that order. Bands are
an active sequence of zero through three `{min,max,color,label}` objects.
Labels are strings of zero through five printable ASCII bytes, non-space if
nonempty. Count bytes, unused slots and padding never serialize. Intervals
are finite, sorted, nonoverlapping canonical values; touching is allowed.
Band colours must contrast with the existing track role, including when hidden.
Widget::capabilities supplies the shared size/style availability rules.

Readout supports Celsius/Fahrenheit, FollowBoost/Bar/Psi/Kpa, Volts, and
Lambda/Afr/LambdaAndAfr according to channel. Secondary is None/Bar/Psi/Kpa
and is allowed only for boost. The retained `display.afr` must equal whether
readout includes AFR; conflicting imports reject. Smoothing is0 or50–5000ms,
peak hold100–60000ms (stored default750), and selectionOff/Maximum/Minimum.
Reference modeOff/Zero/Manual stores a finite value; Manual additionally
requires the channel domain. Off/Zero retain a finite inactive value, including
Lambda's defaultzero. Hidden settings remain stored and validated.

Warnings retain their old canonical thresholds, qualification and positive
bounded dwell. OutsideWindow requires `trigger < clear <= clear_high <
trigger_high`; InsideWindow requires `clear < trigger <= trigger_high <
clear_high`. Lambda windows reject. In other modes high endpoints must be
canonical positive zero. Priorities are Low/Normal/High/Critical (defaultNormal),
latch defaultsfalse, snooze is0 or5–900 seconds. An enabled latching/snoozing
rule requires AcknowledgeWarnings on a Dashboard onboard tap/hold. The existing
page graph and protected Y recovery remain mandatory; an encoder alone is
insufficient.

D2 LED settings contain independent normal/warning `kind`, `color`,
`brightness_percent`, `period_ms`, `on_percent` patterns and
`sync_warning_blink`, `during_feedback`, `on_debug`, `on_logs`, `backlight_off`
switches. Kind isOff/Solid/Blink/Breathe; brightness0–100, period200–10000ms,
on-time10–90, including hidden timing. Default normal isSolid green100%,
default warningBlink red100% with400ms/50%; sync defaults true, background
switches false. Both patterns store400ms/50% timing by default. LED colours
have no contrast requirement; Off, black and zero brightness remain valid.

Version4 standalone JSON is compact; field names, strictness and human colour
strings are preserved. Frozen v1/v2/v3 import schemas retain their original
vocabularies and add current defaults during host migration. Firmware and
ConfigSlot accept only current envelopes. Structurally unrepresentable input
(fourth band, oversized label, unknown enum) reports Json with line/column;
representable semantic errors report Field/ErrorCode. Contextual band edits
report BandCount or BandLabel(index) without changing state on rejection.

The synthetic64-widget capacity proof includes three stored bands per widget,
maximum escaped labels, colour varints and timing values. It measures14007B
EAV and94117B compact JSON. Independent structural/counting serializer bounds
are14135B EAV and119797B saved JSON, below the16KiB/128KiB caps. Wider legend
fixtures verify24-byte labels; total text capacity follows occupied grid cells.
