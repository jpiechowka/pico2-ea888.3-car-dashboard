# Probe confirmation expiry follow-up, 2026-10-22

## Device evidence and limits

The owner reported red immediately after the prior checked 280/70 flash/reboot,
successful Y recovery and a clean NEEWER inspection. They accepted that boot
recovery behavior. Slot 1 was subsequently replaced; a photograph also showed
`Timed out; owner must finish before retry` with the old slot identity.

Read-only normal-USB recovery admitted slot 1 as generation 2, 609 bytes, CRC32
`062756FA`: complete inspection and cleanup, four services, eight characteristics,
one descriptor, three successful allowed standard reads, normal local termination
`0x16`, zero malformed/omitted records and zero global scan errors. Slots 2–4
reported their previous generations, lengths and CRCs. Raw binary/JSON/text and
previous backups remain local and unmodified; no save, Reset, reboot or flash
was performed during this follow-up.

The frozen global latch contains Storage. Firmware copies that latch **before**
encoding and queuing the save, so this bit predates generation 2's storage I/O.
It does not timestamp the fault or establish its cause. The photo's normal
footer also means no operation was pending at that rendered frame. Confirmation
expiry explains these observations, but a prior timed-out directory refresh
followed by late completion is another possible sequence. Neither the photos
nor the capture measure flash-write latency.

## Confirmed source defects and correction

| Path | Previous behavior | Corrected behavior |
| --- | --- | --- |
| Overwrite/Reset intent reaches 10 seconds | Generic I/O Timeout and Storage latch despite no I/O | Distinct ConfirmationExpired; retained slots, no new fault, Y back and fresh confirmation |
| Confirmation time reverses | Conflated with ordinary expiry | Clock fault; no mutation authorized |
| Real operation reaches 30 seconds | Fence retained; timeout shown | Same fence, deadline and red latch |
| Matching late completion releases ownership | Text still says the owner must finish | Historical timeout directs the operator back to review slots; no success inference or automatic latch clear |
| Error feedback with no pending operation | Advertised ignored slot navigation/save controls | Shows Y back before retry; hold-X refresh and protected Y recovery remain available |

The change is confined to shared capture controls/rendering and the standalone
probe's fault classification. No journal, capture schema, USB format, flash
reservation, clock, watchdog or byte-lease contract changed. The dashboard's
settings policy has no capture-confirmation path: its 30-second interval limits
save attempts and its Saving status follows an outstanding immutable ticket.
It does not share this defect.

## Execution and ownership trace

Core 1 consumes owner replies before controls and timeout evaluation. An
overwrite/Reset prompt allocates neither a request ID nor a byte lease. A click
first checks the prompt deadline, then validates slot/revision/readiness before
allocating an operation. Expiry at the exact boundary cannot write. Elapsed time
uses [checked subtraction](https://doc.rust-lang.org/std/primitive.u64.html#method.checked_sub),
including near `u64::MAX`; time reversal is distinct from elapsed expiry.

Only an accepted Save freezes the latch, encodes the bounded payload and transfers
the lease through the owner queue. The locked
[Embassy channel](https://docs.embassy.dev/embassy-sync/0.8.0/default/channel/struct.Channel.html#method.try_send)
returns immediately if full; the caller recovers an unsent lease and completes
the failed request. Core 0 admission, journal verification and finite flash
maintenance finish before publishing the affine reply. Core 1 always takes any
returned lease and returns the reply record synchronously. Wrong identities or
incompatible outcomes cannot release the UI fence. Back/recovery/timeout hides
work without cancelling it; matching late completion updates its original slot
without taking over the current screen.

## Verification

New regressions failed against the previous code for false Storage latching,
conflated expiry/clock reversal and stale owner-running text. After correction:

- 152 dashboard-core unit/integration tests, 11 doctests and 76 focused host tests passed.
- Coverage includes overwrite/Reset at 9,999/10,000/later milliseconds, maximum
  timestamps, repeated expired inputs, fresh confirmation, preserved slot identities,
  prior fault preservation, all four operation timeout/completion paths and stale replies.
- Shared-renderer pixel checks verify changed timeout guidance after matching
  completion and error-footer routing. Five affected/new 320x240 synthetic scenes
  were rendered and visually inspected.
- Strict core/host Clippy, probe and BLE-dashboard ARM Clippy, Wasm core admission,
  native core and ARM probe rustdoc passed. Existing dependency warnings for two
  ignored `inline` attributes in Embassy's exported time functions remain unchanged.
- Rustdoc source inventory reports zero uncovered items. Four affected rendered
  pages contain the updated contracts and valid existing local anchor targets;
  all 228 local links in the changed Markdown resolve. Formatting and diff checks passed.
- A fresh Expert 280/70/1.30 V image passed source/provenance, conservative named
  stack paths, flash-maintenance, ELF/UF2 equality and capture-reservation checks.
  Static RAM remains 406,704 bytes with 117,584 bytes of main-RAM headroom;
  both 153,600-byte framebuffers and the 32,768-byte core-1 stack are unchanged.
  These are linked-image facts, not physical high-water or electrical qualification.
- Independent source, regression-evidence and screenshot review found no further
  actionable defect. Artifact identities and publication results remain in the local tracker.

Software tests do not establish physical flash timing, power-cut endurance or
the exact earlier input sequence. The new behavior requires a later authorized
flash and device check. The prior first-boot watchdog indication remains visible;
this change does not reinterpret it as healthy or prove its underlying cause.
