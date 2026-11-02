# HCI declaration corrections, 2 November 2026

This review applies to the locally patched `bt-hci` 0.10.1 in
`firmware/vendor/bt-hci`. It covers declaration and record-decoding changes;
the vendor patch ledger records the separate controller and framing repairs.
No command was sent to a controller during this review.

The preserved upstream package README contains stale relative links to `src/uuid/`
and `update_uuids/`, neither present in the published 0.10.1 package. Its actual
`src/lib.rs` re-exports the separate `btuuid` dependency as `uuid`. Consult that
dependency and the locked feature declarations for UUID definitions; the old
README generator instructions do not describe this package. Original upstream
bytes remain intact for provenance verification.

The normative source is [Bluetooth Core 6.2, Volume 4, Part E](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/Core-62/out/en/host-controller-interface/host-controller-interface-functional-specification.html).
Section 5.2 requires repeated parameters to be interleaved unless explicitly
specified otherwise. A table showing each field as an array does not override
that rule.

## Wire and parser corrections

- Sections 7.8.28 and 7.8.29: transmitter V1, transmitter V3 and receiver V3 use
  OCFs `001E`, `0050` and `004F`. V3 PHY is a selection, not a bitmask: receiver
  values are 1 through 3; transmitter additionally accepts 4 for coded S=2.
- Section 7.8.2: the V2 ISO packet count is one byte. The successful return body,
  excluding status, is six bytes.
- Section 7.3.8: Read Stored Link Key returns two little-endian two-byte counts.
  Its typed return no longer duplicates the Command Complete status or truncates
  either count to one byte. This retained BR/EDR command has no selected caller.
- `PhyOptions` and `AdvPhyOptions` are naturally two-byte-aligned enums. Their
  validity checks now inspect both discriminant bytes. The false byte-alignment
  promise and borrowed-reference decoder were removed; owned decoding still
  accepts unaligned wire input.
- Extended advertisement decoding returns a size error when its secondary-PHY
  byte is absent instead of indexing an empty slice. Legacy and extended report
  iterators pre-parse at most their declared one-byte count so their exact length
  includes a valid prefix and, when present, one terminal decoding error. This
  does not change batch count/tail policy or the error yielded by iteration.
- Sections 7.7.65.37, 7.7.65.44 and 7.7.65.45: periodic response and channel
  sounding records are interleaved. Their containers validate all declared
  records before exposing an entry, retain the admitted raw prefix and preserve
  trailing bytes. No typed enum slice is built from unchecked bytes. The shared
  entry codec emits each data length once and rejects a locally constructed
  length/payload disagreement before writing.
- Sections 7.8.97 through 7.8.99: normal CIG parameters contain maximum transport
  latencies after SCA/packing/framing; the test variant retains its FT/ISO fields
  and adds the missing per-CIS NSE. Create CIS contains a count and CIS/ACL pairs,
  without a CIG identifier prefix.
- Sections 7.8.104 and 7.8.106: Create BIG Test ends at the broadcast code and has
  no handle list. BIG Create Sync contains a one-byte BIG identifier and a
  separate two-byte periodic synchronization handle. Its explicit BIS count
  must match the supplied indices.

These repairs preserve command names but necessarily change invalid retained
parameter types and constructors. Create BIG Test no longer needs a lifetime or
handle slice; Create CIS no longer takes CIG ID; normal CIG takes latency fields;
CIG test records take NSE; BIG Create Sync takes the synchronization handle;
RF V3 takes `PhyKind`. Callers using RX `LeCodedS2` receive a pre-write panic,
because that selection is only valid for the transmitter. Generic writer error
types cannot represent a local parameter error. Normal writer failure or
cancellation can still preserve a valid partial prefix.

## Bounded variable returns

CIG and CIG-test returns accept the declared 0 through 31 handles instead of
requiring 31 on every response. Both retain 31-element owned storage and a
64-byte maximum wire return buffer. The minimum connection interval response
(Section 7.8.156) accepts zero or more six-byte groups instead of requiring 255.
Its owned storage is limited to 41 groups: the 255-byte event parameter envelope
must also hold three Command Complete header bytes, status and two response
prefix bytes. The resulting maximum return buffer is 248 bytes, down from 1532.
Only array entries below the public count represent controller observations;
unused entries are zero-initialized storage, not valid measurements.

These returns implement variable-length decoding and streaming reads, not
`FixedSizeValue` or raw struct-byte conversion. Slice decoding preserves the
unused tail and complete decoding rejects it. Streaming reads need two scratch
bytes before consuming input, then validate count before payload reads. A later
scratch-size failure consumes the two-byte header only; I/O failure/cancellation
can consume a partial payload. A caller must restore stream framing before
retrying. Public count values larger than storage panic before serialization.

The real `ExternalController` accepts these returns without requiring
`FixedSizeValue`: response storage is bounded by `ReadHci::MAX_LEN`, and typed
decoding validates the copied prefix. An actual command/completion regression
covers zero and nonzero CIG/group counts, truncation, surplus bytes, excessive
counts and failed status. Its baseline failed to compile under the unnecessary
fixed-layout bound.

The shared one-byte length guard also covers custom ISO codec configuration,
RF switching patterns and periodic subevent lists, including nested payloads.
Size calculation and serialization reject unrepresentable local lengths before
publishing that serializer's bytes. Command-wide length admission remains a
separate framing check.

## Reachability and evidence

No selected dashboard, standard probe or NGM path calls CIG/BIG/RF test commands
or queries the minimum connection interval. Trouble exposes the latter through
a generic public wrapper whose signature remains usable; the returned array
capacity changes as described above. Selected initialization uses Read Buffer
Size V1. Advertisement parsing is used by selected scanning paths. The bounded
iterator pre-parse adds one pass over at most the declared count, without
allocation or payload copying. Periodic/channel-sounding indexed access scans
preceding validated records; it remains bounded by the one-byte count.

The original native declaration suite has 22 tests with independent normative bytes,
malformed/truncated prefixes, maximum and zero counts, both serializers,
partial writer/read errors, exact iterator sizes, and pre-write panic checks.
Safe baseline tests demonstrated the previous failures without constructing
invalid enums. Restoring the old normal-CIG field layout makes its normative
fixture fail; the corrected layout passes. The integrated native library suite
passed 134 tests at the review checkpoint. Default native strict Clippy and
rustdoc passed. Two later Read Stored Link Key regressions cover exact typed
length and real controller completion, including zero/full-width counts and
failed status. Final strict Cortex-M33 private documentation passed with
`serde`, `uuid` and `embassy-time`, separately selecting `defmt` and `log`.
Enabling both logging backends together is deliberately rejected by the crate;
`--all-features` is therefore not a valid documentation configuration. Selected
consumer/target checks are recorded separately by the integration audit.

These are host software results, not controller feature support, RF qualification,
RP2350 timing or linked-image memory admission. Malformed controller input has
not been shown to be directly injectable by a radio peer.
