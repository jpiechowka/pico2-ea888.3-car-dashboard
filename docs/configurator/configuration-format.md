# Configuration format and portable contracts

`dashboard-config` is a `no_std`, allocation-free device library. Its optional
`json` feature provides the editable host/browser format; JSON is never linked
into the device parser. The model, catalogue, validator and binary codec are
shared by all consumers. See [schema v1](schema-v1.md) for the wire contract and
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

## Measured storage and verification

With nightly2026-10-11, the actual Thumb target LLVM type layout is:

| Type | Bytes |
| --- | ---: |
| Config | 6,820 |
| ValidatedConfig | 6,824 |
| Widget | 100 |

Compile-time32-bit guards bound Config and ValidatedConfig at8KiB. The64-bit
host layouts are8,176 and8,184 bytes and must not be substituted for device
layout. Static type size does not establish parser stack peak; firmware boot
integration must check its linked static/task/stack budgets separately.

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
are disabled on the device. The three lockfiles agree on Syn3.0.8 for current
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
