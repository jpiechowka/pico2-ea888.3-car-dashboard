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

## Scalar backplane documentation (2026-11-02)

Removed copied slice-read/alignment/prefix text from bp_read32. Its retained
contract describes the scalar register read and selected backplane window.
No bus operation, error handling or API changed.


## Standalone controller consistency (2026-11-02)

The native test manifest and lockfile now select the same local bt-hci 0.10.1
controller patch as firmware and host. This changes test dependency provenance;
no CYW bus behavior or radio binary is changed by this manifest adjustment.
See ../bt-hci/LOCAL_PATCHES.md for the reviewed command boundary changes.

## Multicast response admission (2026-11-02)

Both retained Wi-Fi multicast methods validate the actual IOCTL reply length,
four-byte count, maximum ten addresses and complete six-byte entries before
indexing, publishing output or sending an update. Fixed-buffer padding remains
accepted. list_multicast_addresses now returns Result<usize,
AddMulticastAddressError>; new MalformedResponse preserves the caller output.
No selected Bluetooth consumer uses these APIs. Actual Control/IoctlState tests
reproduce old count11 panics and short-body fabricated output and cover preserved
valid update bytes. See ../../../docs/dependencies/reset-multicast-2026-11-02.md
for protocol scope and the existing standalone Clippy diagnostics.


## Shared WLAN payload and command prefix (2026-11-02)

The runner now owns one initialized 513-word WLAN buffer: one reserved SPI command
word and the unchanged 512-word/2048-byte payload. SPI writes the same command into
that prefix and passes one contiguous command-plus-payload slice to the unchanged
PIO transport. It no longer holds a second 2052-byte copy across the transfer.
IOCTL and Ethernet framing start at payload byte 0; all receive/status/IRQ paths
still receive exactly 512 payload words. SDIO skips the prefix and preserves exact
logical byte lengths through its existing 64-byte block and byte-tail operations.
Bluetooth backplane rings, transaction boundaries and credits are unchanged.

Both private transmit adapters now require 1..=2048 logical bytes, rejecting 0 and
oversize before prefix mutation or I/O. This deliberately narrows invalid private
input: old SPI 0 emitted a command-only transfer whose zero length field means 2048,
while old SDIO 0 emitted nothing. Actual runner calls always include nonempty
SDPCM headers. Valid SPI 1/3-byte prefixes still round to one initialized word;
2047/2048 bytes still use the zero-encoded 2048-byte length and 513 total words.

Eight regressions exercise actual adapters and runner framing: baseline-failing
future-storage bound, literal SPI/SDIO wire fixtures, pending address/byte stability,
status publication, canceled/unpolled ownership, invalid 0/2049/usize::MAX, maximal
IOCTL payload, Ethernet framing and receive-prefix separation. Cancellation still
does not establish radio quiescence or roll back a partially transmitted command.

The manufacturer [CYW43439 datasheet, 002-30348 Rev. B section 4.2.1](https://www.mouser.com/datasheet/2/196/Infineon_CYW43439_DataSheet_v03_00_EN-3074791.pdf)
defines the 32-bit GSPI command and 2048-byte zero encoding. The manufacturer
[SDIO platform guide section 1.4](https://www.infineon.com/assets/row/public/documents/30/44/infineon-sdio-platform-support-guide-usermanual-en.pdf)
separates CMD53 arguments from DAT payload. Locked cyw43-pio 0.10.0 continues one
DMA payload write followed by status read under the same CS transaction. No PIO,
DMA, public transport trait, firmware, feature capacity or image gate changed;
linked-memory admission and physical behavior remain separate evidence.

### Aligned subview provenance

Miri exposed a separate locked `aligned` 0.4.3 indexing defect: its dynamic
`Aligned<A4, [u8]>` range indexing first creates a logical byte-slice reference,
then casts it to an aligned object with a larger padded extent. A 65-byte logical
borrow cannot authorize the resulting 68-byte object, even when the original
allocation has enough initialized space. The new WLAN test exposed the defect;
retained variable-length RX and SDIO paths used the same operation.

Checked private `aligned_range` / `aligned_range_mut` helpers now derive directly
from the complete parent aligned object, with no transient narrowed byte
reference. They require ordered logical bounds and a four-byte-aligned start
before pointer arithmetic. These bounds keep the rounded child extent inside the
rounded parent extent. Dynamic WLAN/RX, SDIO block/tail and backplane scratch
projections use these shared helpers; ordinary byte copies remain byte slices.
No dependency cache, public API, logical transfer length or buffer capacity is
changed. Creating the view does not initialize or read padding, and the existing
word-conversion callers still separately require initialized backing words.

Tests cover shared/exclusive empty and partial views, a real 65-byte padded
parent, nested tail views, invalid alignment/order/overflow-sized bounds, actual
65-byte RX without changing the remaining payload, and the existing SPI/SDIO
wire and cancellation cases. The Rust [layout reference](https://doc.rust-lang.org/reference/type-layout.html)
and [raw slice pointer API](https://doc.rust-lang.org/std/ptr/fn.slice_from_raw_parts.html)
explain padded object extent and raw metadata construction; the locked dependency
source and actual Miri failure are the direct causal evidence.
