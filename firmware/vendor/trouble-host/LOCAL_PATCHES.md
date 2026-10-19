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

Run library tests only. Upstream integration tests access physical controllers.
The tests share a global bounded packet pool, so serialize the test runner.

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
