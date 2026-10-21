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
  record. The compact Bluetooth firmware is fully validated before bus writes;
  upload also independently validates record count, address arithmetic, modes
  and exact end-of-input before consuming records.
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

## Address-backed receive-ring coverage (2026-10-19 offline audit)

Four additional tests place real HCI bytes in an address-backed 4096-byte firmware
ring and run BtRunner::handle_irq, BtDriver::read and locked bt-hci 0.10.1 decoding.
They cover every legacy data length 0..31, all padding residues, a header at the
last word, split payload reads, nonzero padding exclusion, six ordered packets,
four-packet drain bounds, full host queue retry without another interrupt, and
the maximum 255-parameter-byte HCI event. Short inner reports remain InvalidSize,
including the observed declared7/remaining16 shape; no RSSI is synthesized from
padding. Both bluetooth-only/firmware-logs and general bluetooth configurations
pass 16 native library tests. No transport production change was required by
these fixtures, and passing fixtures do not establish the physical source of a
short report.

## Full-code audit (2026-10-20)

Unknown F2 event kinds now preserve their numeric value instead of constructing
an invalid Rust enum. The full 32-bit status remains available. SDIO `read16`
decodes two bytes without attempting a four-byte conversion.

The Bluetooth loader retains extended address mode across records, rejects
truncation/count/address/EOF errors before hardware writes and preserves adjacent
bytes around unaligned records with bounded 260-byte scratch storage. Initialization
uses the same preflight validation. Upstream initialization waits still require
the application's watchdog; no new timeout or cancellation guarantee is implied.

The pinned firmware overlay contains the official CYW43439 Bluetooth 0065
Data Length Extension correction. The Wi-Fi binary and `nvram_rp2040.bin`
(also used by upstream Pico 2 W examples) are unchanged. Exact source/header/blob
identities are in `../cyw43-firmware-provenance.json` and
`../cyw43-firmware/README.md`.

Native regressions cover both Bluetooth configurations, valid upload addresses
and padding, malformed records and unknown event/status values. Independent
decoding compared every old/new firmware record and rejected all 13,134 proper
truncations. See the [full-code audit](../../../docs/validation/full-code-audit-2026-10-20.md).

## Rust contract audit (2026-10-21)

All retained source paths now describe inputs, returned values, partial effects,
cancellation, ownership and hardware assumptions. The documentation review also
identified these narrow corrections:

- SDIO transfers use logical slice lengths rather than the padded size of an
  aligned dynamically sized value. Backplane tails use initialized 1024-byte
  scratch storage; a canceled read can discard its in-flight scratch and a write
  can already have delivered a prefix. Pico 2 W uses the separate SPI path.
- Wi-Fi scan parameters explicitly store their two zero tail bytes instead of
  transmuting implicit C-layout padding into initialized wire bytes. Layout remains
  76 bytes. The selected Bluetooth-only images do not perform Wi-Fi scans.
- SDPCM credit recovery wraps its eight-bit sequence addition at 254/255.
- Join result admission checks the full 32-bit event status before converting to
  the existing eight-bit public failure type; 256/262 cannot alias success states.
- IOCTL, SDPCM, Ethernet/BDC and HCI diagnostics retain metadata rather than raw
  payload dumps. Optional firmware-console strings and explicitly formatted
  credential option structs are not covered by a universal redaction guarantee.
- Canceling scan startup drops the host subscription guard and its pending IOCTL
  ownership. Successful startup transfers subscription cleanup to `Scanner`.
  Neither path sends a firmware scan-abort command.
- Firmware/NVRAM placement is checked before bus initialization: rounding,
  subtraction, RAM bounds, overlap and the 16-bit word count cannot silently wrap.
  Valid pinned image addresses are unchanged. Public initialization still panics
  on a rejected image or bus initialization error.
- The `log` IRQ formatter strips the correct four-character `IRQ_` prefix.

Actual future, wire-layout, logging and in-memory bus regressions accompany these
changes. Enable `bluetooth-only,log` to run the payload-redaction checks; the
commands above without `log` do not exercise that logger. These checks do not
establish hardware timing or diagnose the photographed watchdog/disconnect events.
See the [Rust documentation policy](../../../docs/validation/rust-documentation.md)
and the exact feature routes in CI.
