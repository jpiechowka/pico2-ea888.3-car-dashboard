# Local CYW43 Bluetooth transport changes

Based on published cyw43 0.7.0; exact original and patched SHA-256 values are in
../cyw43-provenance.json. The local bt-hci-transport 0.1 implementation supports
the locked bt-hci 0.10 host; it is not an upstream CYW43 release.

## Bluetooth-only resource contract

`bluetooth-only` explicitly reserves this driver for BLE. Do not initialize or
use Wi-Fi in this mode. The retained Ethernet channel has one 64-byte slot in
each direction because Embassy channel construction rejects empty channels.
Unsolicited Ethernet frames exceeding that MTU are rejected before copying.
General mode retains 1514-byte Ethernet MTU and four slots in each direction.

Bluetooth retains four RX and four TX slots. Bluetooth-only slots hold 272 bytes;
general Bluetooth slots retain 1024 bytes. The largest HCI command is 3 + 255 bytes,
the largest event 2 + 255 bytes, and the linked host uses at most 251 bytes of ACL
payload plus 4 bytes of ACL header. Round the largest payload to 260 bytes, then
add the one-byte indicator: 261 fits 272. The GSPI four-byte ring header is stored
separately. The locked ExternalController receive buffer is 259 bytes. Oversize
TX returns InvalidInput; oversize/malformed ring RX latches a transport failure.
No packet truncation or silent stream-fragment loss is permitted.

The payload-only reduction relative to 1024-byte Bluetooth slots and the
one-entry 1514-byte Ethernet channel is 6016 + 2900 = 8916 bytes. Actual linked image
and task-future measurements remain required; this arithmetic excludes metadata.

## Ownership and progress

- One radio runner owns bus/PIO/DMA. Never cancel it or its initialization with
  an outer timeout. The application parks only at run_with_boundary, after bus
  calls completed. A call returning includes the locked PIO SPI status transfer
  and CS release; hardware-stalled DMA still requires watchdog recovery.
- Each runner turn explicitly yields. Every selected IOCTL/Wi-Fi/Bluetooth/idle
  branch services Bluetooth RX. Idle fallback is 1 ms, F2 drain at most 4 packets,
  Bluetooth drain at most 4 packets. Full host RX queues retain ring ownership and
  retry even after clearing the IRQ. Busy TX cannot starve RX by restarting a timer.
- Multiple HCI writers serialize through an async mutex. The HCI read endpoint
  has one reader. A shared permanent fault latch wakes blocked reader/writers,
  clears at most four queued TX entries, and stops Bluetooth bus work until reset.
- Ring pointers must be aligned offsets in 0..4096. Empty, oversized or incomplete
  published frames fail closed. HCI parse errors consume their queue slot and
  return an error. Padding sent to hardware is zeroed.
- F2 lengths/offsets are checked before slicing; optional firmware-log bounds
  and UTF-8 are checked. Firmware upload allows 260 bytes for an aligned 255-byte
  record. The trusted pinned firmware parser still requires validated input.
- Initialization retains upstream waits; the maintenance hook is a running-state
  boundary, not an initialization deadline or proof against hardware bus stalls.

## Reproducible host checks

From repository root, use the dated toolchain. Tests use an in-memory bus, never
hardware. Do not run serial integration tests or flash as part of these checks.

```powershell
cargo test --locked --manifest-path firmware/vendor/cyw43/Cargo.toml --target x86_64-pc-windows-msvc --lib --features bluetooth-only,firmware-logs
cargo test --locked --manifest-path firmware/vendor/cyw43/Cargo.toml --target x86_64-pc-windows-msvc --lib --features bluetooth
```

Tests cover real maximum HCI command/event and configured ACL encoding, malformed
HCI consumption, simultaneous blocked writers, fault wakeup, ring wrap, maximum
packet, full queue retry, bad pointers/lengths, F2 bounds, firmware padding, and
maintenance/RX progress under ready TX and sustained F2 traffic. These are host
software checks, not RF, electrical, DMA timing or flash hardware validation.
