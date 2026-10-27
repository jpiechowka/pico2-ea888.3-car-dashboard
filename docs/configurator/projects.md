# Complete Studio projects

Use **Diagnostics → Configuration files → Export complete project (.eavp)** for a portable backup. It includes dashboard views, startup settings, exact converted artwork and the original artwork when available. Original PNG/GIF/sprite-sheet bytes stay in the host project; only validated dashboard/profile/converted pixels can enter firmware.

**Smaller project: converted animation only** exports the same accepted dashboard, profile and pixels without the original source. The current Studio project is unchanged. Importing that smaller file is valid, but changing its crop or conversion requires the original artwork again. The source hash and conversion recipe remain as provenance; they cannot reconstruct missing source bytes. Exporting it later as a complete project does not invent the original.

**Dashboard-only .eav** and **dashboard-only JSON** omit startup and artwork. Importing either into an existing project preserves its current startup. Starting from a fresh editor uses startup defaults. A standalone `.easp` is a profile only and likewise does not include custom pixels.

## Accepted state, undo and storage

Startup settings and artwork share the dashboard's accepted state and Undo/Redo history. A profile change or accepted conversion is one undoable transaction. Invalid raw dashboard edits, pending JSON, active placement and in-progress imports block project export and new startup changes. Startup timing text is an editor-owned draft: invalid text remains visible with an INVALID preview label, while pixels and accepted settings retain their last valid values. Even valid unapplied text blocks save/export/build until **Apply timing** or **Discard timing**. Applying both timing fields is one history entry; discarding adds none. Project changes and stale asynchronous completions cannot overwrite the draft. Startup-only edits do not restart acquisition simulation.

Changing from custom artwork to a procedural theme replaces the current custom artwork; Undo restores it. Complete project import is atomic. A malformed file or stale callback leaves current accepted state intact. File reads have format-specific limits checked before reading and again before parsing.

Studio saves the current project, selection and original-artwork reference together in **IndexedDB**, per browser and origin. Original images live in a separate immutable Blob store. Startup has no independent mutable save path. Earlier dashboard drafts and the earlier separate startup profile are migration inputs; an existing complete project takes priority unless editing began before the asynchronous restore completed. In that case current edits are kept and the user chooses whether to replace the stored project.

Writes are serialized within the UI and compare the previous selection/project envelope inside a read/write transaction. A cross-tab conflict requires explicit **Save here**. The Saved indicator follows transaction completion, never a queued write or image conversion. Storage denial, corruption and quota failure keep the working project and offer file export. Browser storage is not a portable backup.

Storage requests and queued origin-lock acquisition have a ten-second deadline while the browser event loop runs; a suspended tab can delay timers. A blocked database upgrade reports failure so you can close older Studio tabs and retry. Only transaction completion marks a save successful. If a deadline reports an unknown commit outcome, export the current project first, then reload to inspect stored state before replacing it.

Artwork allocations are immutable and shared across ordinary history entries. Original bytes are stored as immutable IndexedDB Blobs; accepted states, Undo/Redo and captured operations keep hash/length handles, not original byte arrays. Converted pixels remain shared in memory. The 32-entry history can retain up to 33 artwork bundles: at most 264 MiB of original backing storage and 16.5 MiB of converted data, before transient buffers and browser copies. These are bounds, not measured browser-memory usage.

Origin-wide Web Locks serialize original admission, garbage collection and saved-project transactions. Per-tab live-owner locks and durable pin records protect history and captured operations; admission allows at most 35 original pins per tab and eight pinned tabs. Current saved projects also pin their source. The current record stores reduced project bytes plus its original reference atomically. Restore captures and pins that record under the same origin lock. Dead-tab pins and unreachable originals can be collected without changing live history. Failed backing admission retains the prior accepted project. If backing storage or required locks are unavailable, admission fails visibly without retaining originals in memory. Browser quota can reject admission before the stated limits.

Full export and retained-source reconversion materialize one bounded original at a time per tab and verify its identity. Converted-only export needs no original read. A missing/corrupt source fails full export explicitly; it is never silently omitted. Browser quota, crash and cross-tab interleavings remain integration checks.

## EAVP v2 format

The container is binary and uncompressed, with no file paths, links or archive extraction. All integers are little-endian. Integrity checks are not authentication or signatures.

- 32-byte header: `EAVPROJ\0`, u16 version 2, u16 descriptor count 5, u32 exact total length, u32 table offset 32, eight zero reserved bytes, CRC32 of the first 28 bytes.
- Five 48-byte descriptors in fixed order: u16 section kind, u16 zero flags, u32 absolute offset, u32 length, u32 CRC32, 32-byte SHA256.
- Section kinds: canonical dashboard EAV7, EASP2 startup profile, optional EASA converted asset, optional canonical Recipe2 JSON, optional original artwork.
- Present sections are tightly concatenated. Missing optional sections have zero offset/length/checksums. Duplicate/reordered sections, overlaps, gaps, trailing bytes and unknown flags are rejected.

| Component | Bound |
| --- | --- |
| Complete project | 9 MiB |
| Project without original artwork | 576 KiB |
| Canonical dashboard EAV | 16,384 bytes |
| Current EASP v2 profile | Exactly 16 bytes |
| Converted EASA | 524,288 bytes, fully validated by the shared asset parser |
| Typed conversion recipe | 4,096 bytes, unknown fields rejected |
| Original source | 8,388,608 bytes |

The profile uses the existing EASP16 implementation, rather than the earlier prospective 32-byte research design. Custom theme requires asset and recipe; procedural themes carry neither. Full original hash, length and PNG/GIF kind must agree with the recipe. Original decoding remains the converter's responsibility on import/reconversion; the package reader and native build helper do not decode PNG/GIF. Recipe2 serialization is canonical and includes target geometry, converter version, source/output hashes, source presence, scaling/matte/timing/sprite-sheet options and a bounded label. Asset geometry must equal the effective Splash geometry, resolved from its override and the dashboard default.

EAVP1 import uses frozen canonical EAV4, EASP1 and Recipe1 decoders before migration.
The migrated project writes EAVP2 with EAV7/EASP2/Recipe2; existing EASA1 pixels,
original bytes and their hashes remain unchanged, with `dashboard-startup-tools/1`
attribution retained. EAVP2 admits that legacy landscape artwork. New conversions
use EASA2 and converter/2. A legacy recipe cannot claim portrait or silently claim
a newer producer. Mixed section versions, noncanonical recipes and geometry
mismatches reject. See [orientation and compatibility](orientation.md).

An orientation edit can retain mismatched artwork in a visibly invalid Studio
state for repair, but cannot produce a valid project snapshot or build input.
Reconversion uses the retained immutable original, or requires source reimport
for converted-only projects. Flip-only changes need no reconversion.

`dashboard-config-tools::project::Project` is the shared codec. `Editor::accepted_project()` returns an immutable snapshot only through existing readiness gates. Build preparation must capture its Editor stamp and recheck that stamp/readiness after asynchronous work before dispatch. Profile/artwork changes alter accepted project revision even when dashboard EAV is unchanged; a dashboard-only hash cannot prove a complete build current.

Native builds upload the converted-only accepted project and bind its complete reduced-project SHA256, profile, asset and normalized build options. Original artwork stays in Studio and portable complete exports.

EAVP2 projects containing canonical EAV5 or EAV6 are also accepted and migrate to
EAV7. The frozen decoder verifies the original dashboard bytes before migration;
v5 gains disabled BLE, while v6 retains its BLE settings. Startup profile,
artwork, recipe and original bytes keep their existing checks and are not
discarded. New exports retain EAVP2 framing with an EAV7 dashboard, including
four-cylinder readout and warning-presentation settings. Widget and BLE edits
change the complete project/build identity.

## Immutable asset admission

Host `OwnedAsset` accepts only bytes that pass the complete shared EASA parser.
Its private immutable Arc and validated geometry stay together; clones and Undo/Redo
share those bytes. A borrowed asset view cannot outlive its owner. Studio readiness,
startup seeking and captured startup replay reuse that view without repeating whole
asset CRC and row validation on every frame. The renderer still checks the requested
Splash geometry and decodes bounded rows. Project imports and newly converted bytes
continue through full admission, and wire formats are unchanged. The owner is available
only with the host `json` feature; the allocation-free firmware parser is unchanged.
