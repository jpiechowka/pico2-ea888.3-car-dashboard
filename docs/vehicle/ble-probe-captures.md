# Saved BLE probe captures

The [standalone probe](ble-probe.md) saves one selected peer's evidence into one
of four numbered slots. A committed save survives USB power removal. The saved
record can be opened on the Pico or exported later as binary, JSON and text.
Saving, viewing and exporting do not inspect the adapter again.

This is a bounded evidence snapshot, not a Bluetooth sniffer, pairing transcript,
application log or ECU trace. The [adapter research note](adapter-research.md)
explains what can be started offline and what remains unproved.

## Save, open, replace and reset

1. Select the adapter, then inspect using encoder click or a two-second A hold.
   For advertising only, use a short X press from the device list.
2. Wait for Paused, including disconnect cleanup. Partial inspections and old
   advertisements remain useful; save them with their visible failure context.
3. Hold X for two seconds to open the slots. Opening slots from a selected live
   list opens its advertisement view and requests pause; wait if saving is busy.
4. Select slot 1–4 with encoder rotation or short A/B presses. X saves into an
   empty slot. An occupied supported slot asks for confirmation before replacement.
5. Confirm with encoder click or a two-second A hold. Y cancels. Confirmation
   expires after ten seconds, and changed evidence invalidates an overwrite prompt.
6. Wait for the slot to show its new generation. Click/hold A to load the saved
   pages; rotate or use A/B to read them. X or Y returns to slots, then Y returns
   to the live view. Hold X to refresh the slot directory.

A, X and Y holds do not also perform their short-press action. Slot selection and
saved pages wrap; live selections/pages stop at their ends. Holding Y for two
seconds recovers live scanning without erasing saves. The telemetry above saved
pages describes the running Pico; the evidence below contains frozen capture
timestamps. It is not a replay of historical CPU or stack measurements.

| Slot state | Open | X action |
| --- | --- | --- |
| Empty | Explains that no save exists | Save the current paused evidence |
| Saved capture | Open supported saved bytes | Confirm replacement |
| Saved / replacement blocked | Open the readable supported copy | Confirm destructive Reset before this slot can be reused |
| Damaged capture / Unsupported format | Reports the problem | Confirm destructive Reset |
| Not loaded / Storage unavailable | No load | Refresh with hold X; no automatic erase |

**Reset erases both physical copies of the selected slot and all its saved data.**
It does not save the current evidence; save separately after Reset completes.
No other slot is reset. Unknown journal or capture payload formats are never
silently migrated, replaced or discarded. Both physical copies are checked,
including the inactive copy that replacement would erase. A supported copy
alongside an unknown format or malformed capture header remains openable on the
Pico, but replacement is blocked until explicit Reset. Storage read failures also
block saving. Generation exhaustion requires Reset rather than counter wrap.

Export before Reset when the current exporter can decode the selected generation.
USB LIST/GET intentionally use journal integrity and select the newest committed
copy; they do not select an older supported payload as the on-device viewer does.
Thus a future payload can be returned unchanged over raw USB GET, while the
current CLI refuses to publish it as a valid export. The CLI has no forensic/raw
bypass or physical-copy selector. Unknown journal formats cannot be loaded by
this firmware. Leave the slot intact if unavailable data matters, even when an
older copy is still viewable on the Pico. An interrupted first-ever save can
leave a damaged slot that needs explicit Reset. An interrupted replacement keeps
the prior valid generation where one exists, or the newly committed generation.
These are the journal's tested software rules; physical power-cut qualification
remains separate.

Back/recovery and a UI timeout do not cancel an in-progress flash operation.
The storage owner finishes and returns its buffer before another operation is
admitted. Do not interpret leaving the page as undoing a save or reset.

The [RGB status LED](ble-probe.md#rgb-status-and-recovery) is purple while a local
capture operation is pending, unless an error takes priority. Saved browsing is
amber; an old successful result cannot turn the live status green. Storage
failures and unknown/corrupt slots latch red. Empty slots are normal. Y recovery
acknowledges the latch without erasing slots; a pending operation can still finish
with a new error after that acknowledgment. USB export is read-only and has no
separate busy colour; a current-session protocol/transfer failure latches red.

## Recover captures on a PC

Keep the probe firmware installed and plug the Pico into the PC with a USB data
cable. **Do not hold BOOTSEL.** Normal firmware exposes USB CDC serial with
VID `2E8A`, PID `0009`, product `Pico BLE Probe Captures`; BOOTSEL is a different
device and is not the export interface. Merely plugging in sends no BLE requests.
The USB descriptor requests up to 500 mA when configured. That is a host power
budget request, not measured consumption or qualification of a cable or supply.

From the repository's `host/` directory:

```powershell
# List matching USB serial candidates; does not open their ports.
cargo run --locked --bin ble_probe_captures -- ports

# List slot states. With an unambiguous recognized product, --port can be omitted.
cargo run --locked --bin ble_probe_captures -- list --port COM7

# Export slot 1 to a NEW directory whose parent already exists.
cargo run --locked --bin ble_probe_captures -- export 1 C:\Captures\ngm-attempt-01 --port COM7

# Decode an already exported binary without connecting any device.
cargo run --locked --bin ble_probe_captures -- decode C:\Captures\ngm-attempt-01\capture.blecap C:\Captures\ngm-decoded-01
```

Use the COM port shown on your PC; `COM7` is an example. Existing Rust/MSVC host
prerequisites are described in [setup](../configurator/setup-tools.md). After
building, `host/target/debug/ble_probe_captures.exe` can be used directly. No
Python serial package, extra debug probe or phone is required for this recovery
route. Windows USB enumeration and transfer still require physical validation.

Automatic selection is deliberately conservative: it requires exactly one
matching VID/PID and recognized product name. Windows may expose its generic
friendly name, such as `USB Serial Device (COM7)`, instead of the firmware's USB
product string. In that case, or with multiple probes, pass `--port` explicitly.
The selected port must still be an enumerated VID/PID candidate. The tool does
not open unrelated ports to search for the protocol.

Every successful export directory contains:

| File | Meaning |
| --- | --- |
| `capture.blecap` | Exact versioned capture payload, at most 8192 bytes |
| `capture.json` | Every metadata/record field, raw hexadecimal bytes and explicit unknown values |
| `capture.txt` | Readable report containing the same evidence |
| `COMPLETE` | Written after the three artifacts have been written and synced |

The directory and files must be new; nothing is overwritten. A failed/interrupted
host export can leave incomplete files without `COMPLETE`. Keep or remove those
files yourself, then choose a new directory. The saved device slot is unchanged.
Detached `.blecap` files contain acquisition evidence, while slot/generation are
export-envelope metadata: offline decoding without that envelope marks them
unknown. Preserve the complete original export directory.

An interrupted transfer, timeout or protocol error requires **unplugging and
replugging normal USB before retrying**. Firmware abandons that USB session so a
pending endpoint packet cannot become the prefix of another reply. Reopening the
COM port alone is not guaranteed to reset USB endpoint state. No flash or adapter
inspection is needed. Incomplete framing, wrong slot, bad CRC, unsupported schema
or missing End marker fails; the tool does not publish those as valid captures.

The host has a five-second transaction deadline and finite serial I/O timeouts.
There is no erase/save/inspect/pair/reboot command over USB. USB is an evidence
export channel, not a continuously streamed logger or mass-storage drive.

## Evidence and its limits

Each record includes only the selected peer, not every nearby device. Global
scan diagnostics remain labelled global and cannot be blamed on that peer.

| Retained information | Interpretation |
| --- | --- |
| Address/type, first/last seen, reports and RSSI | Observation identity; private addresses may change; RSSI is not distance |
| Separate raw advertisement and scan response | Up to 31 bytes each, own timestamp/kind/RSSI/truncation; not necessarily the same event |
| Primary services, characteristics and descriptors | Up to 24 / 64 / 128; exact UUIDs, handles, properties and parent relationships |
| Standard reads | Up to 16 outcomes of 64 bytes; service/handle, result, raw error code when available, duration and bytes |
| Link, ATT MTU, timings and cleanup | Observed connection parameters and negotiated ATT limit when available; no L2CAP CoC negotiation |
| Four stage flags and omissions/rejections | Partial, unrequested or limited work stays explicit; no invented missing records |
| First failure and local/terminal context | Exact available reason, range and elapsed time; absent raw status remains absent |
| Producer identity | Full source digest when supplied, profile and actual CPU/SPI rates; not a firmware self-check |

All capture times are device boot-relative monotonic milliseconds. No RTC date
or UTC is fabricated. Raw packet/value/UUID fields preserve their byte order;
peer addresses use conventional display order (`address_display_hex` in JSON).
Non-UTF8 bytes remain lossless hexadecimal. Fixed-length standard fields with a
wrong length retain their bytes and an invalid-length marker. A successful read
does not imply permission to write, subscribe or authenticate.

DIS reads are the eight manufacturer/model/serial/hardware/firmware/software/
System-ID/PnP-ID fields. GAP adds Device Name `2A00`, Appearance `2A01` and Preferred
Connection Parameters `2A04`; GATT adds Database Hash `2B2A`. Each requires its
corresponding standard service and readable property. Descriptors are discovered
by identity only. No vendor characteristic or descriptor value is read, no CCCD
is written, and there is no pairing, authentication or ECU request.

## Storage, source identity and verification

Four logical slots each have two 12 KiB copies: 96 KiB total at flash offsets
`0x1E6000..0x1FE000`, below the existing settings sectors. Each copy has explicit
256-byte metadata and commit pages, a bounded payload and CRC32 checks. Save
erases/writes the inactive copy, verifies it, then writes/verifies the final
commit. CRC detects corruption; it is not a signature or authenticity proof.
No automatic rotation, eviction, retry or erase is used.

The checked build wrapper hashes the sorted, compact JSON source inventory,
embeds that digest and binds it to the ELF/UF2 receipt. Full image admission checks
the linked digest bytes and every physical LOAD/erase/UF2 range against the capture
reservation, plus the existing reviewed flash-lockout and memory/stack gates.
Developer builds may report source identity unavailable. A source digest is not a
complete compiler-input closure, and firmware never asserts that it passed a
post-link check. Keep the build receipt for later correlation.

**Export before loading other firmware.** A dashboard or old probe image may use
the capture region; erase-all destroys it. A linker reservation does not prove
that every loader/update preserves saved data. The two fixed 153600-byte display
buffers and 32 KiB core-1 stack remain; new capture buffers and task futures must
pass their own linked image gates.

Host tests cover power-loss simulations, corruption, interrupted reset, selected
slot isolation, schema/bounds/relationships, framing, timeouts and no-overwrite
exports. Those establish software behavior, not physical flash endurance/power
loss, USB driver behavior, RF compatibility, true stack high-water or DNUC data.
See [dependency review](../dependencies/probe-captures-2026-10-19.md).

The [delivery validation record](../validation/probe-offline-audit-2026-10-19.md)
records software checks and exact images; physical retention and USB recovery
are separate pending checks in the linked bench runbook.
