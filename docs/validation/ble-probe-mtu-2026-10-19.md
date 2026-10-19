# BLE probe MTU and HCI diagnostic validation

Date: 2026-10-19. Compiler: nightly-2026-10-18 (rustc 1.101.0-nightly,
8006091eb, LLVM 23.1.4). Scope: standalone BLE probe, shared evidence rendering,
reviewed Trouble 0.8.0 overlay, and affected dashboard BLE integration.

## Physical evidence and interpretation

The latest owner photographs show scanning followed by an MTU-stage disconnect
after 215 ms, with no available reason code and no discovered attributes. Global
scan diagnostics show 96 invalid-size batches and zero invalid-value batches;
the last failed batch had one declared report, failing at its first entry.
Earlier photographs include a complete three-service/twelve-characteristic
inventory and a separate partial result. These observations do not establish
one shared cause or attribute global scan errors to the selected device.

215 ms is below the application's 3-second ATT deadline. RSSI is not a parser
length, and its value alone cannot cause InvalidSize. The supplied photographs
contain no raw failed batch or retained local/terminal disconnect reason.

## Confirmed code defects and corrections

| Boundary | Reproduction and correction |
| --- | --- |
| Peer MTU request | Local response advertised the negotiated minimum. Actual emitted-byte tests now require local receive capacity, while effective MTU remains the minimum. |
| Below-default MTU | Values 1 through 22 became the effective MTU. All values below 23 now retain default 23 and complete the exchange. |
| Startup interleaving | An early valid-shaped notification caused UnexpectedGattResponse. Startup consumes early values until the MTU reply; ingress confirms indications once. No subscriber exists yet. |
| MTU refusal | Every correlated ATT error was treated as successful fallback. Only RequestNotSupported (0x06) permits default-MTU fallback; other codes propagate. |
| Event supervision | Connection events were not consumed until MTU construction finished. One event consumer now runs throughout startup and inventory; cancellation drops both paths before cleanup. |
| Lost reason | A full two-entry event queue could lose terminal reason permanently. Independent per-slot first-ingress and terminal reason fields survive queue drain and cleanup, then reset on safe slot reuse. |
| Failed disconnect completion | A nonzero HCI completion status falsely marked the connection terminal and invented reason 0x1F. Failed status now propagates to host recovery; successful status retains the raw reason. |

The real ingress/constructor tests failed before their repairs. These are
confirmed software defects, not proof that any one caused the photographed TV
failure. Explicit connection, sequential ATT, bounded queues, timeouts, terminal
cleanup, no-security policy and request-generation guards remain enforced.

## HCI source trace and added evidence

Reviewed the actual locked path: CYW43439 ring header/payload and padding removal,
BtDriver read/trailing-byte check, bt-hci 0.10.1 ExternalController's 259-byte
buffer and exact event read, Trouble LE dispatch, and the probe report adapter.
The radio ring publishes exact payload length; its queue-full path retains the
ring for retry. Outer truncation or trailing bytes fail transport admission.

The report decoder consumes event kind, address kind, six address bytes, data
length, data and RSSI. Its record size is `10 + Data_Length`; subevent and count
consume two of the maximum 255 event-parameter bytes. The probe now retains total
report bytes, bytes remaining at the failed record and its length byte when
present. No failed payload or address is retained. The callback remains borrowed
and synchronous, with a backward-compatible default for existing handlers.

No valid-length truncation bug was reproduced. The locked decoder also accepts
some out-of-spec data lengths, reserved RSSI encodings and trailing report bytes
after the stated count; tests characterize this permissiveness without changing
admission. Remaining length may include subsequent records in a batch. New
numeric evidence narrows the next observation; it does not establish controller,
bus, RF or peer fault. Other malformed fixed HCI envelopes can reach existing
host fault/panic paths; this work does not claim exhaustive HCI validation.

## Tests and review

- Root workspace: 348 tests passed. Shared core: 137 passed with defaults and
  137 without defaults. Core no_std ARM and Wasm checks passed.
- Full host tests passed with and without defaults. Added six production-session
  lifecycle tests, extended thirteen production-report adapter tests and seven
  transport-framing tests using actual `PacketToHost::read_hci` and LE decoders.
- Transport tests include 40,960 combinations of 32 data lengths, five report
  kinds and all 256 RSSI byte patterns (including reserved encodings). They cover
  maximum event batches, exact receive-buffer capacity, every single-record
  truncation, valid prefixes, outer length/padding errors and inner size/value
  errors. The RSSI sweep is not a claim of 256 legal RF values.
- Trouble serialized native suites: 78 without security, 116 with security, and
  78 using the probe's eight-packet/251-byte/four-notification configuration.
  Fifteen new actual-host tests cover both MTU directions, crossed/pre-acceptance
  exchanges, all ATT error codes, all failed HCI statuses and successful reasons,
  pool exhaustion, full event queues, late ACL, first-cause retention and reuse.
- Shared tests cover stale/canceled context, first-known fields, delayed terminal
  enrichment, new attempt/recovery, late completion and maximum-value formatting.
  Pixel comparisons verify diagnostics stay inside status/body bounds and leave
  telemetry and footer unchanged. Actual shared-renderer PNGs were inspected.
- Strict nightly Clippy passed for root/host and ARM probe280/dashboard ble-obd.
  Formatting and exact vendor provenance checks passed. CI additionally exercises
  Linux vendor tests, both probe profiles and the existing dashboard image matrix.
- Independent reviews covered vendor protocol/lifecycle and app/shared integration;
  the parent read the reports and source/diffs, then closed the two identified
  session-drop and pixel-composition coverage gaps.

Reproduction entry points:

```powershell
cargo test --locked --workspace
cargo test --locked -p dashboard-core --no-default-features
cargo test --locked --manifest-path host/Cargo.toml
cargo test --locked --manifest-path host/Cargo.toml --no-default-features
cargo test --locked --manifest-path firmware/vendor/trouble-host/Cargo.toml --target x86_64-pc-windows-msvc --lib --no-default-features --features central,gatt,scan,discovery-no-security,default-packet-pool-size-8,default-packet-pool-mtu-251,gatt-client-notification-queue-size-4 -- --test-threads=1
./firmware/scripts/Build-BleProbe.ps1 -Name a-fresh-capture-name -Profile 280-70 -Expert
```

## Checked image and limitations

Capture `probe-mtu-280-02` passed the unchanged practical image checks. The first
attempt failed at sandbox linker access; no image from that attempt was admitted.
Target `thumbv8m.main-none-eabihf`, `-C target-cpu=cortex-m33`, CPU280 MHz/SPI70 MHz,
1.30 V profile, bounded info logging. No hardware was accessed by the build.

| Measurement | Result |
| --- | ---: |
| Static RAM | 374632 bytes |
| Main-stack/headroom region | 149656 bytes |
| Framebuffers | Two at 153600 bytes each |
| Core-1 stack | 32768 bytes |
| Largest emitted function frame | 5432 bytes |
| Aggregate emitted frames / unchanged limit | 19344 / 20480 bytes |

ELF SHA256: `479709f420aa79e8149177c823fb9140bc1fd07721669986bd2b6e2774d55fdc`.
UF2 SHA256: `b636129792a67b370a73f4ac9dc779491ede5d84d05e82d430ef62ca69ecbf56`.
The frozen manifest binds compiler flags, clock metadata, source hashes and files.
Aggregate frames are a regression budget, not simultaneous stack use; named-path
budgets retain child/IRQ reserves. Static checks do not measure physical stack
high-water, electrical margin, radio interoperability or scanout. The prior 280/70
observation is not general overclock qualification.

New flashing requires current owner approval. Afterward, photograph the first
failure page and the final global scan-length block if a failure recurs. A new
HCI reason may describe deliberate cleanup; interpret it with original stage and
local ingress evidence. Physical resolution is pending.

## Primary protocol references

- [Bluetooth Core 6.2 ATT, sections 3.4.2 and 3.4.7](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/Core-62/out/en/host/attribute-protocol--att-.html): MTU capability/minimum and notification flow.
- [Bluetooth Core 6.2 GATT, section 4.3.1](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/Core-62/out/en/host/generic-attribute-profile--gatt-.html): once-per-connection exchange and RequestNotSupported fallback.
- [Bluetooth Core 5.4 HCI, sections 5.4.4, 7.7.5 and 7.7.65.2](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/Core-54/out/en/host-controller-interface/host-controller-interface-functional-specification.html): framing, successful versus failed termination, legacy reports. Full current HCI HTML exceeded retrieval limits; the disconnect clause was checked via official search extraction. Exact legacy field tables were also checked in the SIG-authored [Core 5.2 PDF, pages 2382–2383](https://community.renesas.com/cfs-file/__key/communityserver-discussions-components-files/297/Core_5F00_v5.2.pdf).
- [Trouble issue 619](https://github.com/embassy-rs/trouble/issues/619): relevant crossed-MTU context on another platform; not evidence of this TV's cause. The local locked source and [patch record](../../firmware/vendor/trouble-host/LOCAL_PATCHES.md) define this build.
