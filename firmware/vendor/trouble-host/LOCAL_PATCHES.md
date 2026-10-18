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
status. No test establishes physical radio performance, pairing interoperability
or actual flash lockout timing. Root firmware gates remain mandatory.
