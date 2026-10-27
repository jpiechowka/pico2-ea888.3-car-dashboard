# BLE radio dependency and source review

Reviewed 2026-10-18 for the USB-powered Pico 2 W prototype. This records the
resolved sources, local changes and software evidence. No adapter purchase,
flashing, physical BLE pairing, vehicle traffic or electrical qualification was
performed. The [runtime contract](../vehicle/ble-runtime.md) and
[protocol contract](../vehicle/ble-protocol.md) retain adapter-only MC+/CX traffic
and a separate synthetic service; no exact DNUC source is enabled.

The later [slot-1 transport review](../hardware/cyw43-transport-follow-up.md)
adds a shared 2.5 MHz radio GSPI mitigation using the same locked dependencies.
It records the upstream reports, source timing trace and remaining hardware limits.

## Locked sources and provenance

| Dependency | Resolved version | Role |
| --- | --- | --- |
| CYW43 | 0.7.0, local overlay | CYW43439 bus, firmware and HCI transport |
| Trouble host | 0.8.0, local overlay | Central, GATT and Secure Connections |
| cyw43-pio | 0.10.0 | PIO SPI and DMA completion |
| bt-hci / bt-hci-transport | 0.10.1 / 0.1.0 | Host/controller packets and transport traits |
| embassy-net-driver-channel | 0.4.0 | Retained bounded CYW network channel |
| embassy-sync / embassy-rp | 0.8.0 / 0.10.0 | Queues/locks and existing reviewed HAL |

The [firmware lockfile](../../firmware/Cargo.lock) is authoritative. Published
[CYW43 archive](https://static.crates.io/crates/cyw43/cyw43-0.7.0.crate) SHA-256 is
`033e856bf6f38c96fee3e3f398cceff0ca2c2fae5db255bcd23495216c7047da`;
[Trouble archive](https://static.crates.io/crates/trouble-host/trouble-host-0.8.0.crate)
SHA-256 is `19a566cc37ceaff5cc2a6ff8cc02debd1ee6b95d59ced8830d005da8b080cbe9`.
These are reviewed local overlays, not claims that upstream ships the repairs.
CYW's transport implementation adapts the older driver to the locked HCI 0.10
traits; unrelated dependency versions were not upgraded for this prototype.

Original member hashes and modified/additional member hashes are retained in
[CYW provenance](../../firmware/vendor/cyw43-provenance.json) and
[Trouble provenance](../../firmware/vendor/trouble-host-provenance.json).
The radio firmware/NVRAM and binary license come from Embassy revision
`84444a19eb57d978e3c09fcdc8e60cdd7278eb03`, recorded in the
[binary manifest](../../firmware/vendor/cyw43-firmware-provenance.json).
The complete source/binary trees are checked by
[`check_radio_vendor.py`](../../firmware/scripts/check_radio_vendor.py).
The checker passed on the reviewed trees, including the security-state repair.
Build source identity includes the complete vendor subtree, not only top-level
Rust files. Generated Cargo `target` output is excluded from source provenance.

## Resource bounds

Bluetooth-only CYW retains four RX and four TX slots, each 272 bytes. Commands
need at most 258 bytes, events 257, and the configured ACL payload 251 plus its
four-byte header. Rounded payload 260 plus the packet indicator fits 272; the
four-byte GSPI ring header is separate. Oversize input faults instead of being
truncated. The general Bluetooth configuration retains 1024-byte slots.

The inert Wi-Fi channel uses one 64-byte slot per direction. Zero slots are invalid
in the locked channel constructor. Wi-Fi initialization is prohibited in this
mode; general mode retains 1514-byte MTU and four slots per direction. These two
payload changes save 8916 bytes before metadata/alignment/task-future differences.
Trouble retains one connection, two channels, one volatile bond, eight 251-byte
ACL buffers, ATT 247 and four 244-byte notification values. No third framebuffer
or new heap was introduced. See [linked memory evidence](../performance/memory-layout.md#ble-prototype-integration-evidence-2026-10-18)
for measured artifact scope and the unchanged memory guards.

## Reviewed repairs and ownership

- HCI writes serialize through an async mutex rather than retaining a mutable
  RefCell borrow across a suspended queue write. Faults wake blocked writers and
  the single reader; at most four queued TX slots are discarded.
- Every radio turn services Bluetooth RX, yields, and bounds F2/RX draining to
  four packets. A continuously ready TX queue cannot keep resetting the only RX
  timer. Malformed rings, lengths, offsets, ATT/ACL data and queue/pool exhaustion
  fail closed. Firmware-record padding has a 260-byte staging bound.
- Notification subscription reserves its listener before CCCD write; an early
  peer notification remains observable. `next_checked` reports pubsub loss.
  Local disconnect immediately stops GATT delivery and wakes the reader, while
  actual terminal state still waits for the controller event.
- Canceled Create Connection stays pending until a terminal LE event. Late
  success and success not accepted by a canceled caller are quarantined. The
  next epoch requires both controller-confirmed disconnect and settled create/
  cancel state. Control sends Disconnect while RX remains able to receive it.
- Failed encryption refresh/change or explicit encryption-off clears stored
  security state and requests disconnect, even without active pairing. Runtime
  checks CX encryption before each write chunk and before accepting received
  bytes, so a stale encrypted flag cannot keep the stream admitted.

The source-backed details and reproducible commands are in
[CYW local patches](../../firmware/vendor/cyw43/LOCAL_PATCHES.md) and
[Trouble local patches](../../firmware/vendor/trouble-host/LOCAL_PATCHES.md).
Focused vendor verification reported 12 CYW tests in each Bluetooth configuration
and 75 security-enabled Trouble library tests, serialized because the fixture pool
is shared. These exercise software state and an in-memory bus. Standalone vendor
Clippy still reports inherited diagnostics; it was not represented as clean and
no blanket suppression was added. Strict application firmware gates remain.

The locked PIO driver finishes DMA phases and releases CS on normal return.
Dropping that future can abort DMA before normal CS release. The runtime therefore
retains the runner and initialization, parks only between complete bus operations,
and uses watchdog recovery for stalled hardware. Flash admission also rechecks
radio CS, DMA1 and every DMA channel. Initialization contains upstream waits;
bounded queue work does not prove bounded hardware wall time.

## Security and adapter scope

Legacy pairing is not enabled. The reviewed central path requires Secure
Connections and 16-byte keys. Controller `LeRand` seeds the host before its initialized
state is exposed. Bonds remain bounded RAM state and disappear at reset; no key
flash persistence was added. NoInputNoOutput Just Works supplies encryption but
does not authenticate against an active MITM. Unsupported interactive pairing
and security loss terminate the link.

[OBDLink's CX developer notes](https://support.obdlink.com/support/solutions/articles/43000746707-obdlink-cx-adapter-notes),
reopened 2026-10-18, document encrypted communications, a power-on bonding window,
FFF0/FFF1/FFF2, both write properties, MTU 247, response ordering and no QueuedWrites.
The runtime requires Notify and selects Write Request whenever advertised.
Only MC+ may select advertised Write Without Response if Write Request is absent,
matching the [embedded component's implementation](https://raw.githubusercontent.com/eigger/espcomponents/master/components/ble_elm327/ble_elm327.cpp).
This selection occurs during discovery; a failed write never triggers a fallback
resend. CX and the synthetic peer require acknowledged writes. The shared property
matrix is tested; actual adapter properties, radio delivery and timing remain
unqualified. A Write Command's local completion is not peer acknowledgment.

This review does not establish vehicle compatibility, reliable radio operation,
physical DMA/flash timing, stack high-water, power/thermal margins or dongle sleep.
The current two-second adapter identity loop may prevent inactivity sleep.
Physical tests require separate authorization; instruction-level proof remains
separate unfinished research.

## 2026-10-24 CoC follow-up

The shared Trouble host now uses backed single-credit receive admission and
per-K-frame TX progress, with cancellation/close/failed-create ownership fixes.
This serves the separate INFO-only identity utility and protects the shared
vendor dependency used by the dashboard. No ECU traffic is enabled. Current
contracts, source tests and remaining physical limits are in
[the identity implementation record](../validation/ngm-identity-2026-10-24.md)
and [local patch notes](../../firmware/vendor/trouble-host/LOCAL_PATCHES.md).
Earlier counts and image figures above remain dated evidence for those sources.
