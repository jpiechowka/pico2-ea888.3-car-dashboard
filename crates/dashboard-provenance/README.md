# Typed vendor provenance

Host-only Rust admission for the reviewed HAL, radio source overlays and radio
firmware blobs. It is outside the firmware dependency graph.

From the repository root:

```text
cargo run --locked --release -p dashboard-provenance -- hal --vendor-root firmware/vendor
cargo run --locked --release -p dashboard-provenance -- hal --vendor-root firmware/vendor --archive path/to/embassy-rp-0.10.0.crate
cargo run --locked --release -p dashboard-provenance -- radio --vendor-root firmware/vendor
cargo test --locked -p dashboard-provenance
```

Existing `firmware/scripts/check_vendor.py` and `check_radio_vendor.py` commands
remain thin compatibility launchers. They require the pinned Rust toolchain and
Cargo dependencies; their first run can compile the release verifier. They launch
from the root workspace, so firmware ARM target/runner settings do not apply.
A failed verifier raises an explicit error even with `python -O`. No device or
flashing command is invoked. The internal `tree --root PATH` command accepts an
`original`/`modified` JSON map on stdin for existing checker/test callers.

The HAL release, URL, archive hash and exact patch inventory are pinned. Radio
release identities, firmware revision and binary/license inventory are pinned.
Manifest paths/hashes are validated before overlays are combined; duplicate keys,
unknown schema fields, absolute/noncanonical paths and non-lowercase SHA256 reject.
Every regular file is hashed, with missing/extra/changed bytes rejected. Only a
real top-level `target` directory in HAL and radio trees is excluded, permitting
their native regression builds; nested `target` directories remain source.
The Bluetooth overlay additionally binds the exact upstream header, extraction
array, commit, byte count and resulting binary hash.

Root and lexical ancestors, source entries and ignored top-level target entries
reject symlinks and Windows reparse points/junctions. Files reject multiple hard
links (Unix metadata or Windows open-handle link count). Verification assumes a
stable local source tree; it is not a race-proof snapshot against a hostile writer
changing directory entries during traversal. No source files are written.

Archive mode verifies the compressed SHA256 both before parsing and over the bytes
actually consumed. Gzip CRC/trailer, tar checksums/end blocks, canonical members,
exact root and original file hashes must match. Duplicate members, links, sparse/
extension metadata, nonregular files, trailing nonzero tar data and concatenated/
trailing gzip data reject. The reviewed published HAL archive uses supported plain
members. No archive member is extracted or executed.

## Limits

All limits reject explicitly; none truncates or silently skips admitted source.

| Resource | Limit |
| --- | ---: |
| Manifest / compatibility JSON stdin | 1 MiB |
| Individual source file / compressed archive | 64 MiB |
| Tree file bytes / decompressed archive bytes | 512 MiB |
| Map entries / tree entries / archive entries | 10000 |
| Directory descent | 64 |
| Canonical relative path | 4096 UTF-8 bytes |

Hashing uses an 8192-byte buffer. Archive parsing streams and retains only bounded
maps plus a 1024-byte end-block history. These are work/data limits, not guarantees
about filesystem response time; compatibility launchers have a 300-second process
timeout. No network download occurs in the verifier, although Cargo may fetch its
pinned dependencies on the first invocation.

Dependencies: serde/json for typed manifests, sha2-const-stable for streaming
SHA256, flate2 1.1.10 and tar 0.4.46 for gzip/raw tar reading. Tar extraction APIs,
permission changes and xattr support are unused/disabled. Windows uses windows-sys
0.61.2 only to query open-handle link count. Source inventory automatically includes
this crate's root/src and root Cargo manifests alongside existing shared crates.

Tests include real reviewed trees, corrupt/missing/extra/CRLF source, overlays,
malformed manifests and limits, hardlinks, platform links/junctions, malformed
archives and real process invocation. Physical hardware and instruction proof are
unrelated to these software checks.
