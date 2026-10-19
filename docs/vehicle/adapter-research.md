# NGM and OBDeleven evidence boundaries

Reviewed 2026-10-19 against primary public material and the pinned source below.
This is a research guide for [saved probe evidence](ble-probe-captures.md), not an
enabled adapter profile or a claim of working autonomous access. The probe sends
no proprietary commands, pairing/authentication sequence or ECU request.

## What a capture lets us start offline

A selected-peer capture preserves raw advertisements, the public GATT structure,
descriptor identities, allowed standard reads, exact available failures and link
context. That supports comparison with public implementations, bounded codec and
parser work, simulated session tests, and identification of the next missing
evidence. It does not guarantee enough information to implement a working client.

| Evidence | Useful conclusion | Still unknown |
| --- | --- | --- |
| Advertised names/UUIDs and standard strings | Candidate product/family and claimed versions | Authenticated hardware/firmware identity |
| Service/characteristic/descriptor map | Candidate endpoint structure and advertised properties | Payload semantics and security permissions |
| Successful or protected standard reads | Outcome of those specific operations, including raw ATT status when available | Whether vendor reads/writes/notifications are permitted |
| Connection parameters and ATT MTU | Observed link settings and ATT receive constraint | CoC PSM, MTU/MPS/credits or application throughput |
| Partial result, timeout or disconnect code | Reproducible failure context for this attempt | A universal root cause or proof the adapter is incompatible |
| Source digest and full exported bytes | Reproducible comparison against the producer receipt | Proof of physical safety or exact ECU support |

Bluetooth characteristic properties do not encode access permissions. ATT
permissions are not generally discoverable; descriptor discovery returns
handle/UUID identities. A complete table is not a proprietary protocol trace.
See the Bluetooth SIG [GATT specification](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/Core-61/out/en/host/generic-attribute-profile--gatt-.html)
and [ATT specification](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/Core-61/out/en/host/attribute-protocol--att-.html).
These specification references explain the distinction; they do not qualify the
controller as a Core 6.1 implementation.

## NGM / ECUconnect family

The owned adapter's prior intake identified **ECUconnect V2.0B**, app-reported
`0.8.0.422`, `S3/V2` and NGM branding. These are prior owner/app observations,
not a new independent firmware read. Keep the label and app version alongside
the exported capture; do not substitute a name string for verified identity.

NGM advertises BLE/Wi-Fi and an ESP32 platform, and explicitly does not market the
adapter as a universal third-party diagnostic interface. This is a product
support boundary, not a description of the wire enforcement mechanism.
[NGM product documentation](https://ngmapp.com/products/ngm-obd2-bluetooth-adapter).

The public ECUconnect family implementation was reviewed at immutable
Swift-CANyonero revision `f476f522d5b1074aec03fc073e15f800e9dec19e`:

- [CLI source](https://github.com/Automotive-Swift/Swift-CANyonero/blob/f476f522d5b1074aec03fc073e15f800e9dec19e/Sources/ecuconnect-tool/ECUconnect.swift)
  uses the default URL `ecuconnect-l2cap://FFF1:129`.
- [Linux L2CAP source](https://github.com/Automotive-Swift/Swift-CANyonero/blob/f476f522d5b1074aec03fc073e15f800e9dec19e/python/ecuconnect_tool/ecuconnect_tool/linux_l2cap.py)
  uses a supplied PSM and low Bluetooth security. That path does not derive its
  PSM from this probe's descriptor table.
- [Protocol header](https://github.com/Automotive-Swift/Swift-CANyonero/blob/f476f522d5b1074aec03fc073e15f800e9dec19e/Sources/libCANyonero/include/Protocol.hpp)
  provides a framing/command-family lead. It was read, not executed against the
  owned adapter; acceptance by NGM firmware is unproved.

Decimal 129 is `0x0081`, in the dynamic SPSM range. The SIG specifies discovery of
dynamic SPSM through GATT on reconnection. The public default is a family-specific
lead, not a universally assigned ECUconnect port or a measured NGM value.
[L2CAP specification, Table 4.15](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/Core-61/out/en/host/logical-link-control-and-adaptation-protocol-specification.html).
Finding `FFF1` therefore does not authorize assuming PSM 129 works. The probe
does not open a CoC channel, test PSMs or send identity/channel commands.

Future qualification would need accepted transport establishment, actual channel
limits, identity response, framing/reassembly, reconnect behavior and any access
control. Exact DNUC request definitions and scaling remain separate from adapter
transport. See the [ECU reference](ecu-reference/README.md).

## OBDeleven 3

Keep Gen3 distinct from FirstGen and NextGen. The official Gen3 manual describes
an app/account setup flow, a six-digit PIN and BLE. It does not identify that PIN
as an SMP passkey or specify the proprietary authentication exchange.
[OBDeleven 3 manual, pages 4 and 6](https://obdeleven.com/sylius_user_manual/obdeleven3_manual/OBDeleven%203%20manual.pdf).
The instruction to select Gen3 inside the app does not establish the absence of
link security or application authentication.
[Official connection support](https://support.obdeleven.com/en/articles/5605970-why-can-t-my-phone-tablet-find-the-obdeleven-device-via-bluetooth).

Do not import legacy `0000`/`1234` assumptions, ELM framing or an older generation's
service/characteristic pair. A Gen3 inventory can identify candidate surfaces;
it cannot reveal login/cloud dependencies, keys, encrypted payload semantics,
notification framing, accepted commands or exact ECU definitions.

## Next evidence, only when a concrete gap requires it

Keep separate captures for each adapter and repeat/failure attempt, with manually
recorded physical label, app/device version, ignition state, whether a phone app
was connected, and what action was taken. The firmware cannot infer that context.
Keep originals private; addresses, serial strings and standard IDs may identify
devices. Redact a sharing copy rather than altering the original evidence.

If public sources and these captures leave a specific protocol gap, a separately
scoped app-side HCI trace may help. Android documents supported HCI logging and
debugging routes; actual phone availability and redaction need checking then.
[AOSP Bluetooth debugging](https://source.android.com/docs/core/connect/bluetooth/verifying_debugging).
No phone setup, root access, new hardware purchase or RF sniffer is a prerequisite
for the current probe workflow. A future traffic capture still needs analysis;
it does not itself prove cold-start operation, cloud independence or ECU safety.
