# Configuration format and portable contracts

`dashboard-config` is a `no_std`, allocation-free device library. Its optional
`json` feature provides the editable host/browser format; JSON is never linked
into the device parser. The model, catalogue, validator and binary codec are
shared by all consumers. See the current v7 contract below, [schema v2](schema-v2.md) for wire history, and
[design](design.md) for the deployment and hardware limits.

## Bounds and validation

- 1 to 8 named views, 64 widget instances total, at most 8 occupied cells per
  view on its effective 4-column, 2-row landscape or 2-column, 4-row portrait
  grid. Supported spans are1x1,2x1,2x2;2x1 remains horizontal.
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
paths; [release evidence](advanced-v4-release.md) records the historical v4 admission.

### Historical v1 baseline

The following test and timing record describes the initial v1 implementation,
before gauges and palettes. The later v4 verification is recorded in the release guide.
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

## Initial dependency evidence

New direct dependencies are Postcard1.1.3, Serde1.0.229 and optional
serde_json1.0.154; heapless0.9.3 gains its Serde feature. Postcard defaults are
disabled, avoiding its separate heapless compatibility edge. Serde defaults
are disabled on the device. The initial four lockfiles agreed on Syn3.0.8 for
derive macros. The [16 October refresh](../dependencies/upgrade-2026-10-16.md)
records later versions; historical parser measurements retain their original
dependencies.

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

## Current v7 contract

Version7 adds `WidgetChannel::IgnitionRetard` after the eight scalar widget
channels. Scalar channel tags, the eight source settings and channel histories
keep their existing identities. The group has four independent signed-degree
observations, not four additional scalar channels. Its readouts are
`IgnitionNumeric`, `IgnitionStatus` and `IgnitionStatusAndNumber`; the default
shows status and number together at one decimal. `advanced.ignition_warning`
is appended after `reference_mode`, with `Global` and `CylinderLines` choices.
Scalar widgets require `Global`.

The group supports every widget span but requires Numeric style, no graph,
average, extrema, trend, smoothing, bands, peak or reference marker, and no
secondary unit/AFR. Degree values have a finite representation range of
−327.68 through 327.67; this is not an engine limit. Its warning must remain
DemoOnly and Low, with zero load gate and `−327.68 <= trigger <= clear <= 0`.
Activation uses strictly `value < trigger`; clearing uses `value >= clear`.
Defaults are enabled, zero/zero thresholds, 1000/500 ms dwell and the message
`Ignition retarded`. The stored message still has the 20-byte single-line limit;
cylinder prefixes and line breaks are generated by the popup renderer. See
[advanced controls](advanced-controls.md) for group clearing and acknowledgment.
No vehicle decoder or source is added.

Version5 adds `Config.orientation` and `View.orientation: Option<Orientation>` to
the end of their respective wire structures. The four values are Landscape,
Portrait, LandscapeInverted and PortraitInverted. View `null` means inherit the
device default; current JSON requires the field explicitly. Placement validation
uses each view's effective geometry. See [screen orientation](orientation.md).

Host migration uses frozen v1–v6 structures and their original enum vocabularies.
Versions1–4 yield Landscape plus inherited views; v5/v6 retain orientation.
Versions1–5 disable BLE; v6 preserves its BLE settings. Migrated scalar widgets
use `Global` warning presentation and retain their existing display/rule settings.
Migration never reinterprets an old positional payload as the current type.
Firmware and ConfigSlot accept only canonical v7 envelopes. EASP2 independently
stores Boot info/Splash overrides; those settings are outside dashboard-only EAV
and JSON. Complete EAVP2 projects carry all these contracts together.

The host configuration CLI and firmware build script run migration/admission on
a bounded 2 MiB worker stack, avoiding the Windows main-thread stack limit for
explicit inputs. This host allocation does not change Pico stack reserves or
linked-image admission.

The 16 KiB EAV and 128 KiB JSON limits remain. The historical size/benchmark records
below describe their named versions, not measurements of the orientation release.

## Advanced fields introduced in v4

Version4 introduced the complete advanced contract shared by ordinary editor save/export,
JSON/EAV codecs, host migration, configured acquisition, rendering, warning
acknowledgment and RGB policy. See [advanced controls](advanced-controls.md) for
actual defaults and capabilities and [release evidence](advanced-v4-release.md)
for historical software resource gates and physical-validation limits. Organized
editor workspaces, all-screen browser pixels and native Build & Flash adapters
are implemented; see [Studio](studio.md) and [building](building.md) for their
current workflows and verification boundaries.

Config appends channel-indexed `history_seconds[8]` (10–900, default120),
`reduced_motion` (defaultfalse), and `led`. Widget appends `advanced`; warnings
append flat `trigger_high`, `clear_high`, `snooze_s`, `priority`, `latch` fields
in that order, after `message`. Binary declaration order is fixed and uses
normal Postcard Serde. No memory representation is cast onto the wire.

The v4 widget advanced fields are `bands`, `reference`, `smoothing_ms`, `peak_hold_ms`,
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

Version4 standalone JSON was compact; field names, strictness and human colour
strings are preserved. Frozen v1/v2/v3/v4 import schemas retain their original
vocabularies and add current defaults during host migration. Firmware and
ConfigSlot accept only current envelopes. Structurally unrepresentable input
(fourth band, oversized label, unknown enum) reports Json with line/column;
representable semantic errors report Field/ErrorCode. Contextual band edits
report BandCount or BandLabel(index) without changing state on rejection.

The historical v4 synthetic64-widget capacity proof includes three stored bands per widget,
maximum escaped labels, colour varints and timing values. It measures14007B
EAV and94117B compact JSON. Independent structural/counting serializer bounds
are14135B EAV and119797B saved JSON, below the16KiB/128KiB caps. Wider legend
fixtures verify24-byte labels; total text capacity follows occupied grid cells.

## BLE selection introduced in v6

Schema v6 appends a required `ble` object after orientation. Its bounded Rust
storage is exactly 16 bytes; the v6 default binary grew from 1159 to
1174 bytes. Existing widget, orientation and startup data are preserved.
Migration from v1-v5 starts with BLE disabled; migration from v6 retains it.
Current JSON requires the entire BLE object. Pico ConfigSlot accepts only v7
and resets every field, including BLE, on failed admission.

`enabled` defaults false, `profile` defaults `VlinkerMcPlus`, `peer` contains
six display-order address bytes plus `Public` or `RandomStatic`, and
`auto_reconnect` defaults true. The disabled placeholder is the all-zero peer.
An enabled radio requires an explicit nonzero, nonbroadcast peer; random static
addresses require top bits 11 and a random part containing both zero and one.
Rotating private addresses are unsupported. No PIN or bond keys are stored.

Connect timeout is 5000-30000 ms (default 10000), response timeout 250-5000 ms
(default 1500), and minimum command gap 10-200 ms (default 20). All values remain
validated while disabled. These settings do not qualify a sensor, ECU or route.
MC+ and CX initialize/identify the adapter only; CX requires encryption and
volatile bonds require pairing again after reboot. `TestPeer` is explicitly
synthetic and uses a separate GATT service with no vehicle request admission.

Config, ValidatedConfig and BootConfig/ConfigSlot compile-time target bounds
increased in v6 by exactly 16 bytes to 14596, 14608 and 14620 respectively; v7
retains those ceilings. Linked-image
headroom and stack gates are unchanged and remain a separate verification step.
