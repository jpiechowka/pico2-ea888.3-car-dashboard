//! Host-only, bounded verification. Never extracts or executes vendor/archive input.
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

pub const MANIFEST_LIMIT: usize = 1024 * 1024;
pub const FILE_LIMIT: u64 = 64 * 1024 * 1024;
pub const TOTAL_LIMIT: u64 = 512 * 1024 * 1024;
pub const ENTRY_LIMIT: usize = 10_000;
const DEPTH_LIMIT: usize = 64;
const PATH_LIMIT: usize = 4096;
pub type Result<T> = std::result::Result<T, String>;

#[derive(Debug, Default, Deserialize)]
#[serde(transparent)]
pub(crate) struct Hashes(#[serde(deserialize_with = "unique_hashes")] pub BTreeMap<String, String>);
fn unique_hashes<'de, D: Deserializer<'de>>(de: D) -> std::result::Result<BTreeMap<String, String>, D::Error> {
    struct Visitor;
    impl<'de> serde::de::Visitor<'de> for Visitor {
        type Value = BTreeMap<String, String>;

        fn expecting(
            &self,
            f: &mut std::fmt::Formatter<'_>,
        ) -> std::fmt::Result {
            f.write_str("unique canonical paths and SHA256 hashes")
        }

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
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Maps {
    pub original: Hashes,
    pub modified: Hashes,
}
pub(crate) fn parse<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T> {
    if bytes.len() > MANIFEST_LIMIT {
        return Err("manifest size limit".into());
    }
    serde_json::from_slice(bytes).map_err(|e| format!("manifest: {e}"))
}
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
pub(crate) fn valid_hash(hash: &str) -> Result<()> {
    if hash.len() != 64 || !hash.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) {
        return Err("invalid SHA256".into());
    }
    Ok(())
}
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
pub(crate) fn hex(bytes: [u8; 32]) -> String { bytes.iter().map(|b| format!("{b:02x}")).collect() }
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
pub fn verify_tree(
    root: &Path,
    json: &[u8],
    ignore_target: bool,
) -> Result<()> {
    let maps: Maps = parse(json)?;
    tree(root, &maps.original, &maps.modified, ignore_target)
}
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
