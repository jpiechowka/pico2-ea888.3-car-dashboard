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

**All four slots may be occupied.** Select the one you intend to replace and
confirm its replacement; no fifth slot or prior Reset is needed. The other
three captures stay unchanged. There is no automatic eviction. Export the old
capture first if you want to keep it. Healthy saved slots offer replacement;
Reset is the recovery action for damaged, unsupported or replacement-blocked slots.

A, X and Y holds do not also perform their short-press action. Slot selection and
saved pages wrap; live selections/pages stop at their ends. Holding Y for two
seconds recovers live scanning without erasing saves. The telemetry above saved
pages describes the running Pico; the evidence below contains frozen capture
timestamps. It is not a replay of historical CPU or stack measurements.

Readable slots show the saved device name, observed address/type (`P` public or
`R` random) and generation. Names are sanitized to at most 31 display bytes;
`Unnamed` means no usable name was captured. Directory labels come from fully
validated saved bytes, including older supported captures, rather than the live
selection. Refresh is read-only. A read/validation failure removes the uncertain
label. A protected readable copy retains its identity and Reset requirement.

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
block saving. Generation exhaustion requires Reset rather than counter wrap,
including immediately after a save reaches the final generation; the saved
capture stays readable. An interrupted Reset can erase either or both copies
of that selected slot. It is not atomic, is never automatically resumed, and
does not promise to retain the old capture. Refresh after a failed operation
before deciding on another explicit action.

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
remains separate. The [full-slot audit](../validation/probe-slot4-followup-2026-10-21.md#full-slots-replacement-and-reset)
records the confirmation, generation, failure-injection and ownership checks.

Back/recovery and a UI timeout do not cancel an in-progress flash operation.
The storage owner finishes and returns its buffer before another operation is
admitted. Do not interpret leaving the page as undoing a save or reset.

The storage task owns one fixed reply record. It transfers an exclusive reference
to core 1 and waits for its return before servicing another request. Core 1 takes
the returned byte lease even for a hidden or stale completion, then returns the
record synchronously before rendering. Flash maintenance has already resumed
both cores before this handoff. This avoids copying the directory through task
stacks; it adds no payload buffer or framebuffer. USB requests can wait behind
that serial owner, and a stalled UI remains subject to the progress watchdog.

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
# Substitute the exact .blecap filename printed by export; old capture.blecap files also work.
cargo run --locked --bin ble_probe_captures -- decode C:\Captures\ngm-attempt-01\YOUR-EXPORTED-FILE.blecap C:\Captures\ngm-decoded-01
```

Use the COM port shown on your PC; `COM7` is an example. Existing Rust/MSVC host
prerequisites are described in [setup](../configurator/setup-tools.md). After
building, `host/target/debug/ble_probe_captures.exe` can be used directly. No
Python serial package, extra debug probe or phone is required for this recovery
route. Normal Windows CDC enumeration and slot-1 export were verified on the
owner's connected Pico on 2026-10-19, after their power-off/reopen check.
This does not qualify every USB host/driver or interrupted-transfer scenario.

Automatic selection is deliberately conservative: it requires exactly one
matching VID/PID and recognized product name. Windows may expose its generic
friendly name, such as `USB Serial Device (COM7)`, instead of the firmware's USB
product string. In that case, or with multiple probes, pass `--port` explicitly.
The selected port must still be an enumerated VID/PID candidate. The tool does
not open unrelated ports to search for the protocol.

Every successful export directory contains:

| File | Meaning |
| --- | --- |
| `<stem>.blecap` | Exact versioned capture payload, at most 8192 bytes |
| `<stem>.json` | Every metadata/record field, raw bytes, unknowns, binary SHA256, naming identity and derived connection hints |
| `<stem>.txt` | Readable report containing the same evidence |
| `COMPLETE` | Written after the three artifacts have been written and synced |

The CLI prints all filenames and the full capture ID. The common stem is
`blecap-<name>-<p-or-r>-<12-hex-address>-<16-hex-hash>`, for example
`blecap-neewer-tl21c-r-c0123456789a-0123456789abcdef` (illustrative address/hash). `p`/`r` distinguish public
and random observed addresses; address bytes use display order. The suffix is
the first 16 lowercase hex digits of the complete binary's SHA256. Use the full
`identity.capture_id` (`sha256:` plus all 64 digits), or compare actual bytes,
when correlating evidence. The short suffix is not a collision-free identifier.
Slot/generation, output directory and export time do not change that content ID.
An address or name is not permanent or authenticated physical device identity.

Name selection is deterministic: first nonempty complete Local Name from scan
response, then advertisement; otherwise the first successful, non-truncated
captured GAP Device Name read (`2A00`); otherwise the first nonempty shortened
Local Name from response, then advertisement. In v4, a complete, non-truncated
cached name follows the current complete packet/GAP read candidates and precedes
shortened packet names; another cached observation is the final fallback.
Duplicate names use the first
field in that source. A malformed AD length stops parsing that packet; valid
earlier fields remain usable. Original packets/read records remain untouched.
An empty name is skipped. The selected name's raw bytes, UTF-8 when valid, source,
complete/shortened status and truncation flag are retained in `identity.device_name`.
For advertising, `complete` describes the AD type and `truncated` describes the
containing captured packet; they can both be true. GAP reads include their
zero-based `records` index. `cached` and `observed_at_boot_ms` distinguish a retained
observation from the current packet/read; absent legacy timestamps remain null.
Cached names never alter raw packets or create read records. Missing selected
names are null. Names and addresses are observations, not authenticated identity.

Filename slugs allow only lowercase ASCII letters, digits and collapsed hyphens,
with at most 32 characters and no leading/trailing hyphen. Other characters become
separators; an empty result or invalid UTF-8 becomes `unnamed`. Valid Unicode stays
in JSON/text even when absent from the slug. The fixed `blecap-` prefix avoids
Windows reserved basenames. This bounds each generated filename to 78 ASCII
characters; the chosen parent path still has operating-system limits. Names with
path syntax, controls, reserved characters or identical sanitized slugs cannot
select a path outside the new export directory. Full hash/raw evidence remains
available to distinguish captures. Filename derivation runs on the PC and needs
no new radio operation. Legacy generic filenames remain accepted as offline decode inputs.

The directory and files must be new; nothing is overwritten. A failed/interrupted
host export can leave partial files; artifact write/sync failures happen before
`COMPLETE` is created. Failure while writing/syncing the marker itself still
returns an error and may leave the marker. A marker alone does not prove storage
durability through power failure. Keep or remove partial output yourself, then
choose a new directory. The saved device slot is unchanged.
Detached `.blecap` files contain acquisition evidence, while slot/generation are
export-envelope metadata: offline decoding without that envelope marks them
unknown. Preserve the complete original export directory.

The [Studio handoff contract](ble-capture-studio-handoff.md) documents the
`export_schema: 1` JSON projection: normalized UUIDs, property flags, exact
endpoint/CCCD record links, duplicate detection and shared-validated address
candidates. These PC-derived fields require no Pico storage or radio work.
The actual binary `schema` stays separate. Studio capture import is future work;
hints leave the connection disabled and profile unknown. Old binaries gain
these hints through offline `decode`, without modifying a slot.

`inspection.termination_assessment` is an additive, versioned PC assessment.
It includes a hexadecimal reason and label, with distinct statuses for normal,
abnormal, unavailable, conflicting, unacknowledged or locally failed termination
and for an unobserved connection. Its `acceptable_for_clean_probe` field only
qualifies termination; it does not replace inventory/read/failure checks or
prove adapter access. Original completion, reason, error and LED fields remain
unchanged even if older firmware reported success with a timeout reason.
Decode into a new directory to reassess old bytes; preserve the original export.
The [termination evidence note](../validation/probe-termination-followup-2026-10-20.md)
records the hardware observation that exposed this distinction.

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
| Cached device name (v4) | Up to 64 raw bytes, source/completeness/truncation and observation time; exact observed peer only, not resolved across private address changes |
| Primary services, characteristics and descriptors | Up to 24 / 64 / 128; exact UUIDs, handles, properties and parent relationships |
| Standard reads | Up to 16 outcomes of 64 bytes; service/handle, result, raw error code when available, duration and bytes |
| Link, ATT MTU, timings and cleanup | Observed connection parameters and negotiated ATT limit when available; no L2CAP CoC negotiation |
| Four stage flags and omissions/rejections | Partial, unrequested or limited work stays explicit; no invented missing records |
| First failure and local/terminal context | Exact available reason, range and elapsed time; absent raw status remains absent |
| First rejected characteristic declaration | Handle, original byte length and up to 19 raw bytes; its value-handle field is unvalidated and can be absent |
| Global LED fault snapshot | First recorded source and all latched bits at save; atomic recording order, not a causal trace or selected-peer attribution |
| Boot reset evidence (v4) | Raw watchdog bits, ROM boot type when available and checked previous stage/time; last observation, not proven reset cause or exact reset time |
| Producer identity | Full source digest when supplied, profile, CPU/LCD SPI and configured radio GSPI rates; not a firmware self-check or physical clock measurement |

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

New captures use **payload schema v4**. This firmware and exporter also read
legacy v1/v2/v3 captures. V1 has no rejected-declaration field; v1/v2 have no LED
fault mask/first source or radio GSPI rate. Those fields remain explicitly
unavailable instead of being reconstructed from current firmware. JSON reports
the actual input schema. V3 adds seven metadata bytes: fault mask `u16`, first
source code `u8`, and radio Hz `u32` (zero denotes unavailable). The first source
must belong to the mask; an empty mask has first code zero. Undefined bits/codes
and implausible clock values are rejected. The live latch is sampled immediately
before encoding, including faults recorded since the preceding UI frame.
V4 appends two optional fields after those seven v3 bytes:

- Cached name: presence flag; source `u8` (1 advertisement, 2 scan response,
  3 GAP), complete flag, boot-relative `u64` milliseconds, `u8` byte count,
  1–64 raw bytes and truncation flag. AD-derived cache bytes are at most 29;
  GAP cache requires a complete, non-truncated successful read. Observation
  times must lie inside the captured peer/time context.
- Boot evidence: presence flag; raw watchdog reason `u32`; optional ROM boot
  byte (presence flag plus `u8`); optional previous record (presence flag,
  stage `u8`, saturated previous-boot observation milliseconds `u32`). Previous
  stages are 1 Booting, 2 Supervised and 17–28 first supervisor failure codes.
  A previous record is admitted only for a watchdog-fault classification.

Integers are little-endian, booleans are only 0/1, and unknown source/stage codes,
invalid lengths or inconsistent timestamps fail admission. Legacy decodes clear
these fields to unavailable. JSON exposes them as `cached_name` and `boot`, with
the same evidence in text. Power removal can erase the scratch record; the record
does not identify which firmware image produced it. See the
[reset/name source review](../validation/probe-slot3-discovery-2026-10-20.md).

Global scan-error observation time must not exceed the capture time. It may
precede the selected peer's discovery because scanning covers all peers.
This check applies to encoder and decoder without changing the payload version.
The [diagnostics re-audit](../validation/diagnostics-reaudit-2026-10-20.md)
also verifies operation durations and the separation of current header metrics
from frozen capture evidence.

The journal and USB protocol versions are unchanged, as is the 8192-byte payload
limit. Existing slots are not rewritten during load/export. Older
firmware/exporters cannot decode newer payload versions; export with the current tool before
downgrading, and retain the original binary. Unknown-format protections still
apply across both journal copies.

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

Export naming regressions cover legacy v1/v2 and current payloads, name-source
precedence, every individual byte value, Unicode/invalid UTF-8, malformed/empty/
duplicate/truncated fields, bounded slugs, identical-byte IDs across envelopes,
distinct captures with equal slugs, actual offline CLI output and invalid input/
destination failures. Naming follows the [Bluetooth Local Name format](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/CSS_v11/out/en/supplement-to-the-bluetooth-core-specification/data-types-specification.html)
(CSS Part A §1.2), [Windows file naming rules](https://learn.microsoft.com/en-us/windows/win32/fileio/naming-a-file)
and Rust's atomic [create_new](https://doc.rust-lang.org/std/fs/struct.OpenOptions.html#method.create_new)
no-overwrite operation. These sources define format/API rules, not physical device
identity or filesystem power-loss guarantees.

The [earlier delivery record](../validation/probe-offline-audit-2026-10-19.md)
records its software checks and images. The [slot-1 follow-up](../hardware/cyw43-transport-follow-up.md)
separately records owner-observed retention and a successful read-only USB export.
Physical interrupted-write/reset and endurance checks remain pending. Saving
still requires a selected peer; global fault details can be viewed without one.
