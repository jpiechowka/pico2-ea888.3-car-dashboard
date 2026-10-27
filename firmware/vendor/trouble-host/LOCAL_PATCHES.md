# Local Trouble host changes

Based on published trouble-host 0.8.0. ../trouble-host-provenance.json records the
archive and every original/modified source hash. Upstream files and license
metadata are retained; this is a local reviewed patch set.

- NotificationListener::next_checked reports pubsub lag. Stream users must
  invalidate their epoch and disconnect on any reported loss. The legacy next
  API remains available, but is unsuitable for a fragmented serial stream.
- Register a notification listener before enabling CCCD, so an immediate peer
  notification cannot be lost before the write response. Failed subscription
  drops the listener. Oversized or malformed notification values disconnect.
- Empty MTU exchange and task PDUs return errors. All ACL ingress processing
  errors except explicitly unsupported L2CAP channels request disconnect,
  including pool exhaustion, reassembly, malformed ATT and client queue overflow.
  Indication confirmation does not excuse losing its local delivery.
- A disconnect request stops GATT delivery immediately and wakes its reader;
  queued bytes are not delivered while waiting for DisconnectionComplete.
- Central create cancellation has a CancelPending phase. The cancel command's
  response does not admit a new attempt; the terminal LE connection event does.
  Late success and a success not yet accepted by a canceled caller are quarantined
  in DisconnectRequest. The control runner sends Disconnect while RX stays active.
  Failure to allocate a connection slot terminates the host instead of awaiting
  a command from the RX loop that must receive that command's response.
- Failed HCI encryption changes/key refreshes and explicit encryption-off clear
  the stored security level and request disconnect before further GATT admission.
  This also works without an active pairing state or an available event slot.
- Connection::is_disconnected means controller-confirmed terminal state.
  Stack::connections_idle additionally requires no pending central create/cancel
  and every slot terminal. Keep the host runner alive during bounded cleanup.
  Failed cancellation/controller errors require owner-controlled fault recovery.

The firmware excludes legacy-pairing. Published pairing code rejects non-SC
central pairing and encryption key lengths below 16; the decoder rejects lengths
above 16. AuthReq::new sets the SC bit. Bond records are bounded RAM state and no
flash persistence is added here. Secure Connections Just Works is encryption,
not authenticated MITM protection; physical peer behavior remains unverified.

## Reproducible host tests

Physical serial integration tests are explicitly ignored unless selected with
`--ignored` on suitable hardware. Native library and harness tests share a global
bounded packet pool, so serialize the test runner.

```powershell
cargo test --locked --manifest-path firmware/vendor/trouble-host/Cargo.toml --target x86_64-pc-windows-msvc --lib --no-default-features --features central,gatt,security,default-packet-pool -- --test-threads=1
```

New regressions exercise CCCD ordering, visible notification lag, malformed/empty
PDUs, pool and GATT queue exhaustion, valid delivery, pre-ATT reassembly errors,
queued-byte quarantine, cancellation ordering, and terminal connection/stack
status. Decoded HCI security failures additionally verify source-state revocation
and terminal cleanup. No test establishes physical radio performance, pairing interoperability
or actual flash lockout timing. Root firmware gates remain mandatory.

## Discovery input hardening (2026-10-19)

Primary service discovery rejects zero, reversed, overlapping, out-of-range and
nonprogressing ranges. ReadByType and FindInformation validate ordered handles
inside the requested range and use widened cursors so handle 0xffff finishes
without overflow. Descriptor iterator errors are propagated instead of ignored.
Read/ReadBlob reject payloads above the negotiated ATT MTU; long reads stop when
the destination fills and reject an offset that would wrap on the next request.

Nine raw-response regression tests exercise these public async paths, including
valid terminal handles and multiple pages. The fixtures drain outgoing packets
so the shared test pool remains available to other tests. The library suite
passed 46 tests without security and 84 with security, serialized on the native
Windows target. Eight new regressions failed against the prior implementation;
the valid terminal-service fixture already passed. This does not qualify radio
hardware or make a canceled ATT request safe to reuse: disconnect before retry.

The local `discovery-no-security` feature is a compile-time exclusion guard.
Enabling it together with `security` is an error, including Cargo feature
unification from another workspace package. The standalone discovery utility
requires this feature. Build it separately from the pairing-enabled dashboard;
a combined workspace all-features build is intentionally unsupported. Original
published Cargo.toml.orig remains untouched as upstream metadata; the normalized
Cargo.toml carries the local feature addition.

Guard verification: the 46-test native central/gatt/scan library suite passed
with discovery-no-security. Adding security to the same feature set was rejected
by the intended compile_error diagnostic, not an unrelated compiler failure.

## ATT tuple and error-response validation (2026-10-19 follow-up)

FindByTypeValue, FindInformation, ReadByType and ReadByGroupType responses must
contain a nonempty list of complete tuples before their iterators are exposed.
Minimum tuple lengths include their handles; valid zero-length attribute values
remain accepted. This prevents length-subtraction underflow and silently ignored
truncated tails. The entire malformed response is rejected; valid records from
earlier complete response packets remain available to the caller.

The common GATT request boundary compares an Error Response's Request Opcode in
Error with the actual encoded request opcode. Mismatches return
UnexpectedGattResponse instead of falsely ending discovery/read-blob operations.
Matching AttributeNotFound remains a normal end of enumeration. Legitimate
zero error handles remain accepted. The separate MTU constructor also checks
that an Error Response names Exchange MTU (0x02); mismatches disconnect and fail,
while a matching RequestNotSupported refusal permits the default-MTU fallback.

Error Responses require their exact four-byte payload before field decoding.
Request prefixes and write-command handles are checked before indexing; empty
variable values remain valid. The 128-bit ReadByGroupType request UUID now uses
the correct 16-byte slice. Short Error Responses, empty fixed request prefixes
and a valid 128-bit group request each reproduced a decoder panic before repair.

Ten decoder tests and seven asynchronous wire tests were added. Complete
serialized native suites now pass 63 tests with the probe's no-security feature
and 101 with security. A deterministic 66,560-input sweep covers all 256 opcodes,
lengths 0 through 64 and four seeded byte patterns, including iterator exhaustion.
Explicit valid requests, empty values and UUID widths guard against over-rejection.
Wire tests check emitted request bytes, mixed 16/128-bit pages,
declaration-handle-plus-one pagination, prior-page retention, normal termination,
the real MTU constructor and mismatched/malformed errors. The shared synchronous
Error Response shape/opcode check avoids full response-enum decoding inside
async transaction frames. Packet allocation/encoding is a synchronous helper;
requests directly await sending its owned packet, avoiding a nested send-future
copy. Commands/confirmations use the same encoder and existing async wrapper.
The linked image must still pass unchanged stack gates.
Two pre-existing snippets (compile-error wrapping and security imports) were
formatted because the new native test dependency brings this path crate under
the host's formatting gate; their behavior is unchanged. These independent
malformed-input corrections are not a proven diagnosis of a photographed TV
failure. Host ingress may disconnect on malformed ATT before the app receives
the original decode reason. Cancellation still requires terminal disconnect
before another transaction.

Protocol references: Bluetooth Core 6.2
[ATT 3.4.1.1 and list response formats](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/Core-62/out/en/host/attribute-protocol--att-.html),
[GATT 4.6.1 characteristic discovery](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/Core-62/out/en/host/generic-attribute-profile--gatt-.html).

## MTU startup and retained disconnect evidence (2026-10-19)

MTU request/response PDUs require exactly three bytes. Replies advertise local
receive capacity; negotiated MTU is the smaller valid capability, with default
23 retained when a capability is below 23. A matching RequestNotSupported (0x06)
completes the default-MTU fallback once per connection. Other ATT errors propagate
their exact code. Early valid-shaped notifications/indications are consumed while
waiting for the MTU reply; there is no subscriber before construction completes.
Ingress already confirms indications once. The original transaction deadline,
bounded queues and fail-closed ingress policy remain in effect.

Connection slots retain the first live ACL ingress error and the successful
controller DisconnectionComplete reason separately from the bounded event queue.
Read-only getters survive queue overflow/drain and terminal cleanup; slot reuse
clears both fields after outstanding references end. Late ACL during intentional
cleanup cannot create a new local cause. A failed DisconnectionComplete status
propagates an HCI error to host recovery without falsely marking the link terminal.
Successful events retain the raw reason byte rather than synthesizing 0x1F.

The additional `on_adv_report_data` callback borrows a complete legacy report
batch synchronously. Its default invokes the existing iterator callback, preserving
existing handlers. It enables numerical length diagnostics without changing the
decoder, filtering, admission, packet ownership or report-count rules.

Fifteen actual-host/constructor regressions cover MTU boundaries, crossed and
pre-acceptance exchange, early values/one indication confirmation, pool exhaustion,
malformed ingress, all 256 ATT codes, all 255 failed HCI completion statuses,
all 256 successful reason bytes, full event queues, first-cause retention and slot
reuse. Serialized native suites pass 78 tests without security, 116 with security
and 78 using the probe's actual eight-packet/251-byte pool and four-notification
queue. The existing pool-exhaustion fixture also works when the pool fills before
the receive queue. These tests do not identify the cause of a physical disconnect.

Protocol references: Bluetooth Core 6.2
[ATT 3.4.2 MTU exchange and 3.4.7 notifications](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/Core-62/out/en/host/attribute-protocol--att-.html),
[GATT 4.3.1 MTU fallback](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/Core-62/out/en/host/generic-attribute-profile--gatt-.html),
and [Core 5.4 HCI 7.7.5 completion status](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/Core-54/out/en/host-controller-interface/host-controller-interface-functional-specification.html).
The current full HCI HTML exceeded the retrieval limit; that specific status
rule was checked in the accessible official 5.4 text. Upstream
[issue 619](https://github.com/embassy-rs/trouble/issues/619) concerns another platform
and crossed MTU timing; it is context, not a diagnosis of this device.

## HCI receive dispatch and connection failures (2026-10-19 offline audit)

Known HCI event payloads now propagate decode errors through the host runner
instead of panicking at an unwrap. Unknown LE subevents are ignored; a subsequent
supported event still reaches its handler. ScanTimeout requires its exact empty
payload. An inner advertising-report iterator error still reaches the scan
handler and does not become a controller fault or acquire a synthetic RSSI.

HardwareError terminates the host runner with `Error::HardwareError(u8)`. This
byte is vendor-defined, separate from standardized `Error::Hci` statuses. Both
firmware owners must retain it and enter their bounded controller-fault path.

Legacy and enhanced central connection failures now retain the original HCI
status until the requesting future consumes or drops it. A new request cannot
overwrite an unread result. Canceled requests still require their terminal event;
late success still enters disconnect quarantine. Failed completion events require
their full fixed length but ignore undefined fields after Status. Successful
completion events retain the complete decoder's validation. Advertising Timeout
remains the advertising procedure's separate outcome.

Actual RxRunner tests cover malformed envelopes/fixed events, unknown subevents,
all 256 HardwareError codes, preserved short-report callback delivery, and complete
connection events arriving while real Central.connect/connect_ext futures wait.
Command-state tests cover first-failure retention, pending admission, caller drop,
and cancellation before/after the cancel command. Eight no-security wire tests
plus two command tests extend the previous suite to 88 tests with the probe's
8-packet/251-byte pool; the security suite passes 124 tests. These tests use only
in-memory controller I/O and do not qualify hardware or identify the physical
short-report origin.

References: Bluetooth Core HCI [5.4.4 event forward compatibility, 4.5/Table 4.1
failed completion fields, 7.7.16 Hardware Error and 7.7.65.1/10 connection events](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/Core-54/out/en/host-controller-interface/host-controller-interface-functional-specification.html).
The failed-completion field table was additionally cross-checked in the official
[Core 6.0 HCI text](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/Core-60/out/en/host-controller-interface/host-controller-interface-functional-specification.html).
The locked bt-hci 0.10.1 LE envelope decoder rejects only a missing or unknown
subevent byte; known event payload decoding remains separate.

## Discovery handle boundaries (2026-10-19 hardware follow-up)

Both primary-service queries now validate monotonic, nonzero, nonoverlapping
ranges before atomically adding the completed result to the bounded service
cache. All-service discovery advances from the last group end; a UUID-width
change ends a response rather than skipping matching groups for later gap
queries. Sparse handle numbering is legal.

Both characteristic APIs share exact declaration-size and handle-order checks.
The previous characteristic ends before the next declaration, including sparse
value handles. Terminal value handle 0xFFFF never wraps descriptor requests.
Bulk descriptor capacity exhaustion now returns InsufficientSpace instead of a
false complete result. Probe services() and dashboard services_by_uuid()/
characteristic_by_uuid() use the corrected shared client.

Thirteen added wire-level regressions reproduce invalid ranges, overlapping
definitions, crossed descriptor boundaries, terminal arithmetic and silent
descriptor truncation, then cover valid sparse handles, mixed UUID widths,
multi-page replies and service-cache atomicity, including merge overflow with an
already populated cache and duplicate entries not consuming capacity. The photographed TV rejection
still lacks its rejected tuple and is not attributed to these defects.

References: Bluetooth Core 6.1 [GATT 3.1, 3.3, 4.4, 4.6 and 4.7](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/Core-61/out/en/host/generic-attribute-profile--gatt-.html),
[ATT 3.2.2, 3.4.3.4 and 3.4.4.10](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/Core-61/out/en/host/attribute-protocol--att-.html).

## Full-code audit (2026-10-20)

- GATT client transactions hold an exclusive lease through response handling.
  Cancellation retains quarantine until terminal disconnect; malformed responses
  cannot release ownership to a new transaction.
- Server custom reads return their actual data, check blob offsets and bound
  response lengths. Write Commands never receive ATT error responses. Typed
  strings, Read Multiple, prepared writes, subscription bits and indication
  authorization/serialization have packet-level regressions.
- Dynamic L2CAP channels validate result encodings, peer CID/MTU/MPS and complete
  payloads before admission. Pending creates reserve their slot until completion
  or cancellation, including rejected responses and disconnect. Cleanup releases
  the attempt's own reference without admitting stale ownership.
- LE connection-parameter replies echo the request identifier, reject zero,
  preserve queue/error handling and use the appropriate LL/HCI response path.
  Encryption Change V2 follows the V1 security revocation path. Resolvable
  private address generation uses the specification's byte order and bounded
  random retries; generation failure propagates.
- Advertising validates entire batches and HCI lengths before writes. Supported
  empty data explicitly clears old bytes; unsupported directed/scannable data
  commands are skipped. The high-level API owns all advertising sets: it waits
  for prior disable cleanup, clears retained sets, then configures/enables them.
  Accepting one extended connection still disables remaining sets before reuse,
  while preserving that connection. Unmanaged raw advertising commands must not
  be mixed with this global owner.
- Cursor splitting respects the remaining span. Physical harnesses check both
  task join and operation errors and bound input draining; a GATT fixture now
  explicitly accepts its second write before expecting a changed value.

The current probe/dashboard use the shared client/central paths; server,
advertising and dynamic-channel corrections also protect optional upstream APIs
but do not establish that those paths caused a physical failure. See the
[full-code audit](../../../docs/validation/full-code-audit-2026-10-20.md) for
finite test coverage and remaining hardware limits.

## CI feature-isolation follow-up (2026-10-21)

The central L2CAP parameter regression names its connection-parameter type
directly instead of inheriting a production import gated by the unrelated
`connection-params-update` feature. CI and a local reduced-feature build
reproduced the missing-type compilation error. This changes only test source;
the production feature boundary and firmware behavior remain unchanged.

## Rust contract audit (2026-10-21)

The documentation review covers private state, retained optional features,
generated contracts and tests as well as the firmware's selected client path.
Additional corrections found by tracing those implementations:

- Packet allocation metadata is separate from each leased byte buffer. Allocating
  or freeing another packet must not borrow the bytes of a live packet. The
  metadata lock grants one affine lease per buffer; only its owner accesses those
  bytes. Focused Miri regressions reproduce the old invalidation and check live
  shared/mutable slices, exhaustion, reuse and zero-sized pools/buffers. Miri's
  experimental alias model is not a complete soundness or hardware proof.
- First L2CAP fragments must fit their declared SDU length and allocated capacity
  before reassembly state is published. Failure requests disconnect and wakes
  cleanup instead of retaining an impossible assembly.
- SDU credit calculation widens the two-byte length-prefix addition before
  ceiling division, including a 65,535-byte SDU. Split/merge transfers references
  without a transient count overflow and requires the same channel manager as
  well as slot index. Cancelled credit grants check refunds against peer grants
  received in the meantime; overflow requests disconnect instead of wrapping.
- Channel operations and connection command wrappers reject a stack from a
  different owner before data transfer/controller I/O. A listener constructor
  with no error return asserts this precondition. Equal numeric indices or
  controller handles do not establish ownership.
- Secure Connections rejects a peer with the same public-key X coordinate as
  the local key, including the valid opposite-Y point, in both crypto backends.
  Debug peers remain rejected. A retained bond rejected by an earlier encryption
  attempt falls back to fresh pairing rather than unwrapping an absent resume
  state. These pairing paths remain excluded from the probe image.
- Bonding diagnostic text now matches its wire flag. Decoder tests include byte
  255 and OOB selection tests cover both flag values; these changes do not alter
  their wire encodings.
- ACL locks, reserved credits and queued PDUs retain an exact local connection
  slot independently of application references. A reused numeric HCI handle
  cannot redirect an old host-queued PDU, release a new peer's lock or refund its
  credits. Queue failure/cancellation, dequeue and destruction balance leases;
  SMP uses an explicit already-borrowed enqueue path without reborrowing its
  active connection table.
- An asynchronous ACL write rechecks that slot before every transport poll and
  registers a close wakeup. A blocked sender stops after terminal/local close or
  final connection-owner drop, preserving only previously accepted fragments.
  Generic transports can have partial effects while Pending. The selected CYW
  transport waits for capacity before synchronous publication; bytes already
  accepted into its queue cannot be retracted. Acceptance is not peer delivery.
- Stack and unbuilt-builder destruction drain borrowed connection queues before
  channel storage, then reset initialized MaybeUninit-backed records. Retained
  inbound packets, partial reassembly and queued SDUs no longer leak packet-pool
  capacity when the same HostResources is reused. No controller I/O is performed.
- Channel listeners reject duplicate admission before mutating the shared flag;
  cloning the connection first also prevents a reference-limit panic from leaving
  a flag without an owner. Foreign-stack constructors retain the same admission
  rule. Pending-request lookup skips tokens already held by callers.
- Listener waits check the retained connection slot before every pending lookup;
  Drop rejects unclaimed requests only while that exact link remains connected.
  An old listener cannot consume or reject a new peer's request after controller
  handle reuse. Terminal channel teardown wakes the wait; a local close may wait
  for that teardown or another channel event.
- Reused connection slots clear old OOB and legacy-key observations. Connection
  rate changes update retained state before attempting bounded event delivery.
  Selected OOB pairing without required peer data fails before cryptographic use;
  legitimate unilateral OOB selection and legacy key lengths 7 through 16 retain
  explicit regression coverage. Legacy pairing remains excluded by firmware.
- ATT UUID32 requests, service filters and declarations expand to Bluetooth-base
  UUID128; matching canonicalizes only the ATT boundary. Generic UUID equality
  is unchanged. Group queries allow primary/secondary service types only. Prepare
  Write, Execute Write and Read Multiple Variable request encoders now match
  their existing decoders, including exact buffer limits and empty tails.
- Denied Read Multiple Variable and Prepare Write events retain their failing
  handle instead of unwrapping an unavailable generic request handle. Default
  and explicit errors keep the original opcode/handle; Write Commands remain
  response-free. Retained bonded CCCDs are found by identity before a vacant
  table entry can consume that peer's reconnect.
- Automatic connection/security/client-table diagnostics expose addresses rather
  than identity keys and omit raw restored attribute bytes. Actual logger and
  formatter regressions cover these sites. Explicit public key-formatting traits
  remain separate APIs; callers must not treat them as redacted log payloads.

Actual production-future regressions cover delayed transport admission, handle
reuse, disconnect wakeups, cancellation refunds, stack recreation, complete SMP
exchanges and emitted ATT bytes. Strict native Clippy and private rustdoc pass; physical
tests remain opt-in. See the [dated audit](../../../docs/validation/rust-documentation-audit-2026-10-21.md)
for target-image gates and review limits.

Remaining retained-API limits are explicit: controller completed-packet counts
are trusted and malformed duplicate reports can overcredit; per-link budgets do
not prove controller-global capacity. Custom pool/table domains still require
caller admission, and untyped connection-parameter tokens require their original
stack. These constraints are not new hardware qualifications.

References: Bluetooth Core [Security Manager 2.3.5.6.1](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/Core-61/out/en/host/security-manager-specification.html),
[L2CAP credit-based segmentation](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/Core-61/out/en/host/logical-link-control-and-adaptation-protocol-specification.html),
and [Miri's validation scope](https://github.com/rust-lang/miri).
These software regressions do not identify the cause of a photographed reset or
disconnect, establish pairing interoperability, or qualify live radio timing.

## Documentation CI follow-up (2026-10-21)

Two queue-saturation tests now use separate bounded packet fixtures sized to the
queue plus one waiting packet. The production eight-packet configuration remains
unchanged; the fixtures can reach queue-full rejection and cancellation without
first exhausting the shared pool. Local manager ownership drains packets during
unwinding. A regression repeats full-queue unwind and recreation, checking exact
capacity restoration. Default and eight-packet discovery suites both pass 202 tests.

Role-specific StackBuilder links are emitted only when their methods exist.
Connection-parameter event documentation links to the shared token and states
the feature requirement for acceptance. Strict private docs cover both roles,
central-only discovery and peripheral-only builds. These follow-ups change only
tests and documentation; no firmware behavior or packet limits change.

## Credit-based channel progress and ownership (2026-10-24)

Async SDU transmission reserves one credit per K-frame, so a peer granting one
credit at a time can make progress. The exclusive writer covers the entire SDU;
cancellation/error after possible submission closes the channel rather than
allowing a new SDU to interleave. `try_send` retains whole-call pre-admission.
Both paths bound peer MPS by local scratch capacity; a large advertised maximum
does not require sending a frame that large.

Each dynamic channel owns a receive packet reservation before offering credit.
Initial receive credits are capped at one; explicit zero remains zero. Positive
configured values are upper bounds, and flow-policy batching is overridden by
the backed single-credit window. Partial-SDU progress wakes the receive task to
replenish one backed credit; a queued complete SDU stops grants until consumed.
The existing eight-packet/251-byte pool is unchanged. This trades burst throughput
for bounded backing. Fixed ATT/security traffic keeps its prior allocation route.

An SDU dequeued for delivery returns without another await. Cancellation during
possibly submitted credit signaling closes the channel; idle receive cancellation
retains admitted partial bytes. Close wakes credit, transport, ACL-lock and receive
waiters and releases reserved/partial/queued packets. Credit and disconnect tokens
retain their original channel slot. Refused/wrong-identifier creates release
reservations immediately while retaining the old attempt's reference/error, so
full-pool retry cannot strand a packet or redirect a late completion.

Actual host/manager regressions cover a245-byte SDU over11 legal23-byte K-frames,
one-credit TX, large peer MPS, zero credits, empty SDUs, every exercised cancellation
prefix, simultaneous channels, pool exhaustion, queued-consumer backpressure,
HCI prefix splits, local/peer/link close and failed-create retry. Serialized native
suites pass223(no-security),270(security) and282(legacy/optimized-reassembly
feature set). Strict Clippy passes both normal contexts. These tests establish
software behavior only; shared dashboard/probe linked-image admission remains
required. See the [identity implementation record](../../../docs/validation/ngm-identity-2026-10-24.md).

Specification: Bluetooth Core6.2 L2CAP sections3.4,4.22,4.23,7.3 and10.1,
[credit-based framing and flow control](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/Core-62/out/en/host/logical-link-control-and-adaptation-protocol-specification.html).
The optimized-reassembly API's existing short-prefix restrictions remain; no
new CID/current-credit snapshot getter or physical peer acknowledgment is implied.

## Caller-buffer primary service discovery (2026-10-24)

`GattClient::services_into` discovers directly into the caller's bounded service
vector. The retained `services` API delegates to the same parser and returns its
owned vector as before. Ordering, ATT pagination, errors, atomic cache merge and
pending-request cancellation behavior are shared. The caller buffer is cleared
on first poll; failure or cancellation can leave an unpublished ordered prefix,
while the cache stays unchanged. Probe inventory publishes rows only after the
method succeeds, avoiding a temporary owned service-vector return without
changing its service capacity, packet pool or publication rules.

Three actual-client regressions compare both APIs' requests, mixed-width UUIDs,
rows and caches; cover malformed later pages, ATT refusal, output/cache capacity,
pending cancellation and empty completion; and verify stale output replacement.
Serialized native suites pass 226 (no-security), 273 (security) and 285
(legacy/optimized reassembly). Linked target stack savings are measured separately;
these host tests do not qualify hardware or relax image admission limits.

## Reserved-credit overflow admission (2026-10-24)

Incoming peer grants now include outstanding unconfirmed TX reservations when
checking the 65535-credit limit. Reserving behind an occupied ACL lock cannot hide
an overflow that would otherwise disappear after successful submission. Each
channel adds one u16 counter; confirmation/drop release their matching reservation,
and final close clears it while retained references prevent slot reuse. Channel
metrics retain their documented reservation-count semantics, including refunds.
No packet-pool, service-capacity, public API or image-admission limit changes.

A real public-channel regression failed before this correction and now verifies
immediate overflow closure before both resumed and canceled sends. Exact-limit
replenishment remains valid across completion, cancellation and link close. The
prior refund test now requires rejection before refund. Serialized native suites
pass 228 (no-security), 275 (security), 287 (legacy/optimized reassembly); strict
Clippy in both normal contexts and strict private no-security rustdoc pass.
Target layout/stack measurements and physical interoperability remain separate.

## Caller-owned GATT startup (2026-10-24)

`GattClient::new_into` publishes the fully initialized client into an empty caller
Option and returns a reference to that owner. An occupied slot returns Busy without
changing it. Failure or cancellation leaves an initially empty slot empty and
releases the client claim; canceling admitted ATT startup closes the bearer.
The original `new` remains an owned-result compatibility wrapper over the same
startup body. Once-only MTU exchange, response/error correlation, early notification
handling, the 30-second ATT transaction timeout and best-effort bonded encryption
are unchanged. Probe callers retain the slot inside inspection work, dropping all
borrowers and the owner before full-link cleanup. No field, capacity or gate changes.

Four actual-client regressions cover final-slot address identity, occupied and
competing owners, claim release, pool exhaustion, malformed/failed ATT startup,
cancellation, a real outer startup deadline, and owner/borrower drop order across
successful work, event failure and outer cancellation. The existing owned-new
MTU response/error matrix still exercises the shared startup body. The outer
1ms deadline regression does not claim to elapse the unchanged internal30s timer.
Serialized native suites pass 232 (no-security), 279 (security) and 291
(legacy/optimized reassembly); strict Clippy in both normal contexts and strict
private no-security rustdoc pass. The reference return avoids an owned async
result; actual target-frame savings and image admission are measured separately.

## Synchronous notification decoding (2026-10-24)

The private notification parser had no suspension point. It now runs synchronously
and fills the final Notification buffer directly, removing its async wrapper and
separate payload-array temporary. Client task scheduling, zero-filled unused tail,
notification/indication classification, immediate publication/lag handling and
malformed/oversized bearer quarantine remain unchanged. No payload, subscriber,
queue or packet limit changes.

One added characterization passes before and after: empty/max payload bytes and
indication flags publish exactly; absent handles/oversized values close the bearer
without publication. Existing early subscription, lag and host-ingress tests pass.
Full native suites pass 233/280/292 in the same no-security/security/optimized
contexts above; both strict Clippy contexts and strict private rustdoc pass.
Linked stack/image effects require separate target measurement.
