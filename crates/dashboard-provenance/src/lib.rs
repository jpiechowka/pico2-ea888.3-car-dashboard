//! Host-only, bounded source-tree and gzip/tar provenance verification; no extraction or execution.
//!
//! Reviewed manifests define byte identity, not software correctness or publisher authentication.
//! Keep inputs unchanged while checking: filesystem inspections and hashing are not an atomic
//! snapshot. Symlinks, Windows reparse points and multi-link regular files are rejected; Windows
//! link counts use [GetFileInformationByHandle](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-getfileinformationbyhandle).
mod archive;
mod manifests;
use std::collections::BTreeMap;
use std::fs::{self, File, Metadata};
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

pub use archive::verify_archive;
pub use manifests::{check_hal, check_radio};
use serde::{Deserialize, Deserializer};
use sha2_const_stable::Sha256;

/// Maximum JSON manifest bytes, enforced before deserialization.
pub const MANIFEST_LIMIT: usize = 1024 * 1024;
/// Maximum bytes in each regular source file, compressed archive or uncompressed member.
pub const FILE_LIMIT: u64 = 64 * 1024 * 1024;
/// Maximum aggregate source-file bytes or all decompressed tar bytes, including padding.
pub const TOTAL_LIMIT: u64 = 512 * 1024 * 1024;
/// Maximum entries per hash map/tree/archive; directory entries count for traversals.
pub const ENTRY_LIMIT: usize = 10_000;
/// Maximum descended directory depth, with the supplied root at depth zero.
const DEPTH_LIMIT: usize = 64;
/// Maximum canonical relative path length in UTF-8 bytes.
const PATH_LIMIT: usize = 4096;
/// Verification result with a diagnostic string on failure; strings are not a versioned error protocol.
pub type Result<T> = std::result::Result<T, String>;

/// Duplicate-rejecting map of validated slash-separated relative paths to lowercase SHA256 digests.
#[derive(Debug, Default, Deserialize)]
#[serde(transparent)]
pub(crate) struct Hashes(
    /// Sorted validated entries; constructors inside this crate must preserve path/hash invariants.
    #[serde(deserialize_with = "unique_hashes")]
    pub BTreeMap<String, String>,
);
/// Deserialize `de` into at most ENTRY_LIMIT unique canonical path/digest pairs.
/// Return the deserializer's error for bad paths/hashes, duplicate keys or excessive entries.
fn unique_hashes<'de, D: Deserializer<'de>>(de: D) -> std::result::Result<BTreeMap<String, String>, D::Error> {
    /// Map visitor that validates before insertion so duplicate JSON keys cannot overwrite evidence.
    struct Visitor;
    impl<'de> serde::de::Visitor<'de> for Visitor {
        /// Owned, sorted canonical source path to lowercase SHA256 map.
        type Value = BTreeMap<String, String>;

        /// Write the accepted map description to `f`; return any formatting-sink error.
        fn expecting(
            &self,
            f: &mut std::fmt::Formatter<'_>,
        ) -> std::fmt::Result {
            f.write_str("unique canonical paths and SHA256 hashes")
        }

        /// Consume `map`; return its validated entries or a parse/duplicate/limit error.
        /// Errors discard the local partially collected map.
        fn visit_map<A: serde::de::MapAccess<'de>>(
            self,
            mut map: A,
        ) -> std::result::Result<Self::Value, A::Error> {
            let mut result = BTreeMap::new();
            while let Some((path, hash)) = map.next_entry::<String, String>()? {
                canonical(&path).map_err(serde::de::Error::custom)?;
                valid_hash(&hash).map_err(serde::de::Error::custom)?;
                if result.len() >= ENTRY_LIMIT || result.insert(path, hash).is_some() {
                    return Err(serde::de::Error::custom("duplicate path or manifest entry limit"));
                }
            }
            Ok(result)
        }
    }
    de.deserialize_map(Visitor)
}
/// Generic original/overlay manifest; unknown JSON fields are rejected.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Maps {
    /// Required nonempty upstream file inventory, also used for archive comparison.
    pub original: Hashes,
    /// Reviewed replacements/additions, taking precedence only for current-tree verification.
    pub modified: Hashes,
}
/// Deserialize bounded JSON `bytes` as `T`; return size/serde errors without filesystem access.
/// Individual schemas enforce their own unknown-field, duplicate-key and semantic constraints.
pub(crate) fn parse<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T> {
    if bytes.len() > MANIFEST_LIMIT {
        return Err("manifest size limit".into());
    }
    serde_json::from_slice(bytes).map_err(|e| format!("manifest: {e}"))
}
/// Return success for a nonempty relative slash-separated `path` of at most 4096 bytes.
/// Reject empty/dot/dot-dot components, backslash, colon and NUL. This checks lexical syntax,
/// not filesystem case equivalence, Windows reserved names or containment against concurrent edits.
pub(crate) fn canonical(path: &str) -> Result<()> {
    if path.is_empty()
        || path.len() > PATH_LIMIT
        || path.contains(['\\', ':', '\0'])
        || path
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(format!("noncanonical source path: {path:?}"));
    }
    Ok(())
}
/// Return success only when `hash` is exactly 64 lowercase ASCII hexadecimal SHA256 characters.
pub(crate) fn valid_hash(hash: &str) -> Result<()> {
    if hash.len() != 64 || !hash.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) {
        return Err("invalid SHA256".into());
    }
    Ok(())
}
/// Return an error for symlink `metadata` or any Windows reparse-point attribute.
/// Hard-link multiplicity requires the later open-file checks and is not established here.
fn reject_link(metadata: &Metadata) -> Result<()> {
    if metadata.file_type().is_symlink() {
        return Err("symlink rejected".into());
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err("reparse point rejected".into());
        }
    }
    Ok(())
}
/// Inspect `path` and every lexical ancestor without first normalizing away linked components.
/// Relative paths resolve against current_dir; missing/unreadable components or links return an error.
/// Successful inspection is a point-in-time observation, not a lock against path replacement.
pub(crate) fn checked_path(path: &Path) -> Result<()> {
    // Inspect lexical ancestors before any normalization can erase a linked component.
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().map_err(|e| e.to_string())?.join(path)
    };
    for ancestor in absolute.ancestors() {
        reject_link(&fs::symlink_metadata(ancestor).map_err(|e| format!("{}: {e}", ancestor.display()))?)?;
    }
    Ok(())
}
/// Return a read-only regular file for `path`, after ancestor/link/type/size checks before and after open.
/// Reject multi-link files; Unix also compares device/inode across open. Windows obtains the handle's
/// link count but does not compare a pre-open file ID. Callers must keep the tree unchanged while
/// verifying; concurrent replacement/writes are not made impossible by these checks.
/// Return contextual filesystem/identity errors; maximum admitted size is FILE_LIMIT.
pub(crate) fn open_regular(path: &Path) -> Result<File> {
    checked_path(path)?;
    let before = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if !before.is_file() {
        return Err(format!("nonregular file: {}", path.display()));
    }
    let file = File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let metadata = file.metadata().map_err(|e| e.to_string())?;
    reject_link(&metadata)?;
    if !metadata.is_file() || metadata.len() > FILE_LIMIT {
        return Err("file type or size limit".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.nlink() != 1 || metadata.dev() != before.dev() || metadata.ino() != before.ino() {
            return Err("hardlink or changed file identity".into());
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;

        use windows_sys::Win32::Storage::FileSystem::{BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle};
        let mut information = std::mem::MaybeUninit::<BY_HANDLE_FILE_INFORMATION>::uninit();
        // File owns a valid open handle; the OS initializes the complete structure on success.
        if unsafe { GetFileInformationByHandle(file.as_raw_handle(), information.as_mut_ptr()) } == 0 {
            return Err(format!("file identity: {}", std::io::Error::last_os_error()));
        }
        if unsafe { information.assume_init() }.nNumberOfLinks != 1 {
            return Err("hardlink rejected".into());
        }
    }
    checked_path(path)?;
    Ok(file)
}
/// Consume `reader` through EOF and return (lowercase SHA256, total bytes), bounded by `limit`.
/// Return read/overflow/limit errors; a failed limit check may already have consumed an 8192-byte
/// chunk. Interrupted reads are propagated, not retried; no partial digest is returned.
pub(crate) fn hash_reader(
    reader: &mut impl Read,
    limit: u64,
) -> Result<(String, u64)> {
    let mut hash = Sha256::new();
    let mut total = 0u64;
    let mut buffer = [0u8; 8192];
    loop {
        let n = reader.read(&mut buffer).map_err(|e| format!("read: {e}"))?;
        if n == 0 {
            break;
        }
        total = total
            .checked_add(n as u64)
            .filter(|n| *n <= limit)
            .ok_or("byte limit")?;
        hash = hash.update(&buffer[..n]);
    }
    Ok((hex(hash.finalize()), total))
}
/// Return all 32 digest `bytes` as exactly 64 lowercase hexadecimal characters.
pub(crate) fn hex(bytes: [u8; 32]) -> String { bytes.iter().map(|b| format!("{b:02x}")).collect() }
/// Open checked regular `path` and return at most MANIFEST_LIMIT bytes; probe one extra byte to
/// distinguish an exact-size input from excess data. Return path/read/size errors without parsing.
pub(crate) fn read_manifest(path: &Path) -> Result<Vec<u8>> {
    let file = open_regular(path)?;
    let mut bytes = Vec::new();
    file.take(MANIFEST_LIMIT as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > MANIFEST_LIMIT {
        return Err("manifest size limit".into());
    }
    Ok(bytes)
}
/// Verify current files below `root` against original-plus-overlay maps in bounded `json`.
/// `ignore_target` skips only a real top-level target directory after link inspection; nested
/// target paths remain source. Return success only for exact file inventory and hashes, within
/// entry/depth/per-file/total limits. Empty directories need not appear in the manifest.
/// Errors include invalid manifests, extra/missing/changed files, links and filesystem failures.
/// Inputs must remain unchanged throughout; no extraction, mutation or execution occurs.
pub fn verify_tree(
    root: &Path,
    json: &[u8],
    ignore_target: bool,
) -> Result<()> {
    let maps: Maps = parse(json)?;
    tree(root, &maps.original, &maps.modified, ignore_target)
}
/// Verify `root` using validated `original` and `modified` maps; overlay entries replace/add files.
/// `ignore_target` admits one real top-level Cargo output directory, never links or nested targets.
/// Return success only after consuming every expected file and enforcing resource/type/hash bounds.
/// Directory entries count toward ENTRY_LIMIT, while only file bytes count toward TOTAL_LIMIT.
pub(crate) fn tree(
    root: &Path,
    original: &Hashes,
    modified: &Hashes,
    ignore_target: bool,
) -> Result<()> {
    checked_path(root)?;
    if !root.is_dir() || original.0.is_empty() {
        return Err("vendor root or empty original map".into());
    }
    let mut expected = original.0.clone();
    expected.extend(modified.0.clone());
    if original
        .0
        .keys()
        .chain(modified.0.keys())
        .any(|key| ignore_target && key.split('/').next() == Some("target"))
    {
        return Err("source cannot be Cargo output".into());
    }
    let mut pending = vec![(root.to_path_buf(), 0usize)];
    let mut count = 0usize;
    let mut total = 0u64;
    while let Some((directory, depth)) = pending.pop() {
        if depth > DEPTH_LIMIT {
            return Err("directory depth limit".into());
        }
        for entry in fs::read_dir(directory).map_err(|e| format!("directory: {e}"))? {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();
            count += 1;
            if count > ENTRY_LIMIT {
                return Err("tree entry limit".into());
            }
            let metadata = fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
            reject_link(&metadata)?;
            let relative = path.strip_prefix(root).map_err(|e| e.to_string())?;
            let key = relative
                .components()
                .map(|part| part.as_os_str().to_str().ok_or("non-UTF8 source path"))
                .collect::<std::result::Result<Vec<_>, _>>()?
                .join("/");
            canonical(&key)?;
            if ignore_target && key == "target" {
                if !metadata.is_dir() {
                    return Err("Cargo target must be a real directory".into());
                }
                continue;
            }
            if metadata.is_dir() {
                pending.push((path, depth + 1));
            } else if metadata.is_file() {
                let want = expected
                    .remove(&key)
                    .ok_or_else(|| format!("vendor mismatch: extra {key}"))?;
                let (got, size) = hash_reader(&mut open_regular(&path)?, FILE_LIMIT)?;
                total = total
                    .checked_add(size)
                    .filter(|n| *n <= TOTAL_LIMIT)
                    .ok_or("tree byte limit")?;
                if got != want {
                    return Err(format!("vendor mismatch: changed {key}"));
                }
            } else {
                return Err(format!("nonregular source: {key}"));
            }
        }
    }
    if !expected.is_empty() {
        return Err(format!(
            "vendor mismatch: missing {:?}",
            expected.keys().collect::<Vec<_>>()
        ));
    }
    Ok(())
}
/// Return checked regular archive `path` rewound to byte zero after matching lowercase `expected` SHA256.
/// Reject malformed hashes, file/link/size/read mismatches or seek failure; no decompression occurs.
/// Archive verification rehashes consumed compressed bytes separately to detect intervening changes.
pub(crate) fn checked_archive(
    path: &Path,
    expected: &str,
) -> Result<File> {
    valid_hash(expected)?;
    let mut file = open_regular(path)?;
    if hash_reader(&mut file, FILE_LIMIT)?.0 != expected {
        return Err("archive checksum mismatch".into());
    }
    file.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
    Ok(file)
}
