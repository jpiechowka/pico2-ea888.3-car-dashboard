# BLE capture to Studio connection handoff

Updated 2026-10-20. The exporter adds connection evidence to `capture.json` and
`capture.txt`, derived from the validated `capture.blecap`. **Studio does not yet
import captures or create arbitrary adapter profiles.** This is the data
contract and planned admission path for that later feature.

The Pico already saves bounded observations: peer identity, raw advertisements,
primary services, characteristics, descriptor identities, allowlisted standard
reads, link parameters, completion and failures. The additions below run on the
PC and cost no Pico RAM, stack, slot space or BLE traffic. They also work when
decoding legacy v1/v2 binaries.

## What the exporter adds now

The existing binary `schema` reports the actual capture payload version. The
separate **`export_schema: 1`** identifies this JSON projection. An older JSON
without it remains an old report; regenerate it from its binary with offline
`decode`. Never edit the original binary.

| Field | Meaning and consumer rule |
| --- | --- |
| `binary.byte_length`, `binary.sha256` | Length and lowercase SHA256 of the exact complete binary, including its header. Bind reports/notes to those bytes; the hash is not a signature or authenticity proof. |
| Each `uuid.canonical` | Lowercase expanded 128-bit UUID text. `wire_hex`, `width_bytes` and `assigned_u16` remain, preserving original encoding. |
| Each characteristic's `property_flags` | Eight booleans: broadcast, read, write without response, write request, notify, indicate, authenticated signed writes and extended properties. Raw `properties` remains. These describe declared procedures, not verified access permissions. |
| `inspection.termination_assessment` | Version 1 derived classification: `not_observed`, `local_ingress_error`, `cleanup_unconfirmed`, `conflicting_reason`, `reason_unavailable`, `normal_reason`, or `abnormal_reason`. Includes `reason_hex` and `reason_label`, null when unavailable. `acceptable_for_clean_probe` qualifies only acknowledged, consistent normal termination with observed link/end and no local ingress fault; all other inspection checks remain separate. |
| `connection_handoff.version` | Version 1 of the derived hint structure. |
| `status`, `enabled`, `profile` | Always `evidence_only`, `false`, `null`. No name/UUID match selects a protocol or enables a connection. |
| `address_candidate` | Shared `BlePeer` shape: six display-order numeric bytes plus `Public` or `RandomStatic`, only when shared dashboard validation accepts it as an enabled address. Otherwise null. |
| `address_blocker` | Null, `random_address_not_supported`, or `invalid_dashboard_address`. Private/reserved random types and invalid placeholders cannot become settings. |
| `inventory_complete` | GATT was requested, the services/characteristics/descriptors stages finished, and no omitted or rejected records of any kind were counted. Read completion and overall outcome remain separate in `inspection`. |
| `endpoints[]` | Every captured characteristic, including standard ones. Canonical service/characteristic UUIDs and zero-based indexes into the original `records` array retain parent relationships. Application RX/TX roles remain unknown. |
| `uuid_pair_unique_in_capture` | Exactly one captured instance of the service UUID and one instance of the characteristic UUID in that service. Base-UUID aliases compare equal. True in partial data does not prove uniqueness on the device. |
| `cccd_record_indices` | All `2902` descriptors of that exact characteristic, never a sibling with the same UUID. Their handles remain in the referenced records. |
| `subscription_evidence` | `not_advertised`, `cccd_not_observed`, `cccd_observed`, or `multiple_cccd_observed`. Missing in partial data is unknown; duplicates remain visible. No value proves that the endpoint was subscribed to or is writable. |
| `database_hash_read_record_indices` | Successful, non-truncated, exactly 16-byte standard GATT Database Hash reads. Failed/invalid reads remain in `records` but are excluded here. |
| `saved_handles_reusable` | Always false for this handoff. Handles are historical evidence; rediscover live endpoints. |
| `unobserved`, `review_required` | Protocol, authentication, CoC and vendor-traffic gaps that properties or a successful standard read cannot resolve. |

Indexes refer to this report's original record array, not a future rediscovery.
The raw `.blecap` is authoritative for rebuilding the projection. A future
importer must bound inputs, decode/validate the binary, recompute hints and
compare any supplied digest. A matching self-supplied hash does not establish
trusted hardware provenance. Unknown versions, damaged bytes or edited hints
must never become active settings.

Original timestamps, raw bytes, failures, build identity and global error scope
remain intact. No host date replaces boot-relative observation time. Slot and
generation are known together for USB export and both unknown for a detached
offline binary. Invalid/incomplete export envelopes fail before creating files.

An older firmware's `inspection.complete: true` can coexist with an abnormal
termination assessment. Likewise `inventory_complete` can remain true when all
discovery stages finished but the connection ended abnormally.
Consumers must show these distinct facts and preserve the original reason/error
and LED fields. Regenerate assessments from the binary rather than trusting
edited hints. `0x13`/`0x16` alone never establish a successful whole probe.

## Proposed end-to-end flow

Steps 1–2 are implemented. The Studio importer/review flow and new profiles below
are future work, not an available one-click feature.

1. **Probe and save.** Inspect an explicitly selected adapter, retain partial
   results too, and save the attempt in one of four slots.
2. **Recover on the PC.** Connect normal USB and export. Preserve the complete
   directory. Old binaries can be enriched offline without another probe.
3. **Import evidence in Studio.** Reuse the shared Rust capture decoder in Wasm.
   Show identity, completeness, failures, duplicates and unknowns. Keep evidence
   separate from the accepted dashboard project; create only a disabled draft.
4. **Select a qualified protocol.** Review physical identity and a supported
   profile; shared validation admits its peer/settings. An unknown adapter
   remains research evidence until its transport, framing and security have an
   implemented, tested profile. Do not copy free-text command scripts, historical
   handles, keys or credentials into dashboard settings.
5. **Build and explicitly flash.** Use the Studio/helper checked-image workflow
   and explicit device confirmation. Import alone cannot trigger a build, flash,
   connection or ECU command.
6. **Reconnect with live discovery.** Resolve the current peer, rediscover the
   profile's service/characteristics/CCCD, reject ambiguity and validate required
   properties/security. Renegotiate link/ATT constraints; observed timings are
   not guaranteed future deadlines. Only the admitted adapter protocol may run.
   ECU qualification remains independent.

Today [Studio](../configurator/ble-prototype.md) accepts MC+, CX and the synthetic
test peer with explicit public/static addresses. The [runtime](ble-runtime.md)
discovers profile UUIDs on each session. NGM and OBDeleven cannot become supported
merely by importing UUIDs. The generic hints do not duplicate the fixed firmware
profile table or guess an ELM-compatible transport.

## Useful evidence that is still missing

| Gap | How to obtain it when needed |
| --- | --- |
| Physical model/revision, app version, ignition/power state, connected phone, exact action | Record operator notes alongside the export, keyed by `binary.sha256`. A future import form should collect these with a manual-source label. Pico cannot infer them. |
| Vendor framing and notification bytes | Compare public implementation evidence first; collect a separately scoped app-side trace or explicit safe protocol experiment for a concrete gap. |
| Security requirements or proprietary authentication | Standard protected-read errors are clues, not a complete handshake. Do not store passwords, keys or PINs in portable projects. |
| NGM/ECUconnect transport and dynamic SPSM | The family-specific `FFF1:129` public lead is not an observed NGM channel. The probe does not read a vendor SPSM value or establish LE CoC. |
| Secondary/included services and descriptor values | Current inventory covers primary services and descriptor identities. Extend bounded discovery/read policy only if a qualified protocol needs these fields. |
| Reconnect, cold-start and adapter/ECU compatibility | Repeated device observations and profile-specific tests; a clean inventory is insufficient. |

See [NGM/OBDeleven evidence boundaries](adapter-research.md) for pinned research.
The next useful addition to the probe should follow an actual missing protocol
field, preserving its no-pairing and standard-read-only contract.

## Specification cross-checks

- [Bluetooth GATT](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/Core-61/out/en/host/generic-attribute-profile--gatt-.html), sections 2.5.2, 3.1, 3.3.1 and 3.3.3.3: duplicate UUIDs, property bits, CCCD semantics and cache validity inform the projection. Discovery does not reveal all access permissions.
- [Bluetooth Link Layer](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/Core-61/out/en/low-energy-controller/link-layer-specification.html), section 1.3.2: random-address categories; static addresses can change after power cycles. A saved address is not permanent identity proof.
- [Bluetooth L2CAP](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/Core-61/out/en/host/logical-link-control-and-adaptation-protocol-specification.html), Table 4.15: dynamic SPSM discovery rules. A public example's PSM is not observed transport evidence.

Regressions cover UUID widths/base aliases, every property bit, address types and
invalid limits, duplicate services/characteristics, missing/multiple CCCDs,
advertisement-only and partial/omitted inventory, unusable hashes, a frozen legacy
fingerprint, malformed binary rejection and no-overwrite publication. These are
software checks; they do not qualify a Studio importer or adapter connection.
Termination regressions also preserve legacy raw success flags alongside an
abnormal assessment, distinguish missing/contradictory/late local evidence and
keep advertisement-only or failed-connect attempts separate from live links.
