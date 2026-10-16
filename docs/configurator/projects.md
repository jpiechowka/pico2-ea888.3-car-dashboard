# Complete Studio projects

Use **Diagnostics → Configuration files → Export complete project (.eavp)** for a portable backup. It includes dashboard views, startup settings, exact converted artwork and the original artwork when available. Original PNG/GIF/sprite-sheet bytes stay in the host project; only validated dashboard/profile/converted pixels can enter firmware.

**Smaller project: converted animation only** exports the same accepted dashboard, profile and pixels without the original source. The current Studio project is unchanged. Importing that smaller file is valid, but changing its crop or conversion requires the original artwork again. The source hash and conversion recipe remain as provenance; they cannot reconstruct missing source bytes. Exporting it later as a complete project does not invent the original.

**Dashboard-only .eav** and **dashboard-only JSON** omit startup and artwork. Importing either into an existing project preserves its current startup. Starting from a fresh editor uses startup defaults. A standalone `.easp` is a profile only and likewise does not include custom pixels.

## Accepted state, undo and storage

Startup settings and artwork share the dashboard's accepted state and Undo/Redo history. A profile change or accepted conversion is one undoable transaction. Invalid raw dashboard edits, pending JSON, active placement and in-progress imports block project export and new startup changes. Invalid numeric startup changes restore the last accepted number and explain the rejection. Startup-only edits do not restart acquisition simulation.

Changing from custom artwork to a procedural theme replaces the current custom artwork; Undo restores it. Complete project import is atomic. A malformed file or stale callback leaves current accepted state intact. File reads have format-specific limits checked before reading and again before parsing.

Studio saves the current project, selection and original-artwork reference together in **IndexedDB**, per browser and origin. Original images live in a separate immutable Blob store. Startup has no independent mutable save path. Earlier dashboard drafts and the earlier separate startup profile are migration inputs; an existing complete project takes priority unless editing began before the asynchronous restore completed. In that case current edits are kept and the user chooses whether to replace the stored project.

Writes are serialized within the UI and compare the previous selection/project envelope inside a read/write transaction. A cross-tab conflict requires explicit **Save here**. The Saved indicator follows transaction completion, never a queued write or image conversion. Storage denial, corruption and quota failure keep the working project and offer file export. Browser storage is not a portable backup.

Undo history retains small original references instead of original byte arrays. Converted pixels remain immutable shared allocations: current plus 32 history entries can retain at most 16.5 MiB of converted assets, before temporary buffers and browser copies. Complete export and reconversion read and verify the captured original only when needed. A converted-only export leaves the stored original intact.

Web Locks serialize source-store changes. Live tab pins protect current, undo/redo, export and incoming artwork; saved-current artwork also stays protected after closing the tab. Up to 35 distinct originals per tab and eight pinned tabs are supported. Browser quota can reject a new import earlier. Closed or crashed tabs release their lifetime lock, allowing unused originals to be reclaimed during a later storage operation. If backing storage or required locks are unavailable, admission fails visibly rather than keeping large originals in memory or silently dropping them. These are allocation and retention limits, not a measured browser-process memory claim.

## EAVP v1 format

The container is binary and uncompressed, with no file paths, links or archive extraction. All integers are little-endian. Integrity checks are not authentication or signatures.

- 32-byte header: `EAVPROJ\0`, u16 version 1, u16 descriptor count 5, u32 exact total length, u32 table offset 32, eight zero reserved bytes, CRC32 of the first 28 bytes.
- Five 48-byte descriptors in fixed order: u16 section kind, u16 zero flags, u32 absolute offset, u32 length, u32 CRC32, 32-byte SHA256.
- Section kinds: canonical dashboard EAV, EASP startup profile, optional EASA converted asset, optional canonical typed recipe JSON, optional original artwork.
- Present sections are tightly concatenated. Missing optional sections have zero offset/length/checksums. Duplicate/reordered sections, overlaps, gaps, trailing bytes and unknown flags are rejected.

| Component | Bound |
| --- | --- |
| Complete project | 9 MiB |
| Project without original artwork | 576 KiB |
| Canonical dashboard EAV | 16,384 bytes |
| Shipped EASP v1 profile | Exactly 16 bytes |
| Converted EASA | 524,288 bytes, fully validated by the shared asset parser |
| Typed conversion recipe | 4,096 bytes, unknown fields rejected |
| Original source | 8,388,608 bytes |

The profile uses the existing EASP16 implementation, rather than the earlier prospective 32-byte research design. Custom theme requires asset and recipe; procedural themes carry neither. Full original hash, length and PNG/GIF kind must agree with the recipe. Original decoding remains the converter's responsibility on import/reconversion; the package reader and native build helper do not decode PNG/GIF. Recipe serialization is canonical and includes converter version, source/output hashes, source presence, scaling/matte/timing/sprite-sheet options and a bounded label.

`dashboard-config-tools::project::Project` is the shared codec. `Editor::accepted_project()` returns an immutable snapshot only through existing readiness gates. Build preparation must capture its Editor stamp and recheck that stamp/readiness after asynchronous work before dispatch. Profile/artwork changes alter accepted project revision even when dashboard EAV is unchanged; a dashboard-only hash cannot prove a complete build current.
