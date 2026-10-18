use std::collections::BTreeSet;
use std::io::{self, BufRead, BufReader, Read};
use std::path::Path;

use flate2::bufread::GzDecoder;

use crate::{ENTRY_LIMIT, FILE_LIMIT, Maps, Result, TOTAL_LIMIT, canonical, checked_archive, hash_reader, parse};

/// Hash originals without extracting. Input files must stay unchanged during verification.
pub fn verify_archive(
    path: &Path,
    prefix: &str,
    expected_hash: &str,
    json: &[u8],
) -> Result<()> {
    canonical(prefix)?;
    if prefix.contains('/') {
        return Err("archive prefix".into());
    }
    let maps: Maps = parse(json)?;
    if maps.original.0.is_empty() {
        return Err("empty original map".into());
    }
    let file = checked_archive(path, expected_hash)?;
    let decoder = GzDecoder::new(BufReader::new(Compressed {
        inner: file.take(FILE_LIMIT + 1),
        hash: sha2_const_stable::Sha256::new(),
    }));
    let mut bounded = Bounded {
        inner: decoder,
        total: 0,
        tail: [1; 1024],
    };
    let mut expected = maps.original.0;
    let mut seen = BTreeSet::new();
    let mut end_of_members = 0;
    {
        let mut archive = tar::Archive::new(&mut bounded);
        for entry in archive.entries().map_err(|e| format!("tar: {e}"))?.raw(true) {
            let mut entry = entry.map_err(|e| format!("tar entry: {e}"))?;
            if entry.size() > FILE_LIMIT {
                return Err("archive entry size".into());
            }
            end_of_members = entry
                .raw_file_position()
                .checked_add(entry.size().div_ceil(512) * 512)
                .ok_or("archive offset overflow")?;
            if seen.len() >= ENTRY_LIMIT {
                return Err("archive entry limit".into());
            }
            let name = std::str::from_utf8(&entry.path_bytes())
                .map_err(|e| format!("tar path: {e}"))?
                .to_string();
            let kind = entry.header().entry_type();
            let name = if kind.is_dir() {
                name.strip_suffix('/').unwrap_or(&name)
            } else {
                &name
            };
            canonical(name)?;
            if !seen.insert(name.to_string()) {
                return Err("duplicate archive entry".into());
            }
            if name == prefix && kind.is_dir() {
                if entry.size() != 0 {
                    return Err("nonempty archive directory".into());
                }
                continue;
            }
            let relative = name
                .strip_prefix(&format!("{prefix}/"))
                .ok_or("unexpected archive root")?;
            canonical(relative)?;
            if kind.is_dir() {
                if entry.size() != 0 {
                    return Err("nonempty archive directory".into());
                }
                continue;
            }
            // Raw iteration prevents extension/sparse metadata from silently rewriting paths.
            if !kind.is_file() || entry.size() > FILE_LIMIT {
                return Err("archive entry type or size".into());
            }
            let want = expected.remove(relative).ok_or("extra archive file")?;
            let (actual, size) = hash_reader(&mut entry, FILE_LIMIT)?;
            if size != entry.size() || actual != want {
                return Err(format!("original archive mismatch: {relative}"));
            }
        }
    }
    // Drain through gzip CRC/trailer verification. After tar EOF only zero padding is valid.
    let mut bytes = [0u8; 8192];
    loop {
        let n = bounded.read(&mut bytes).map_err(|e| format!("gzip trailer: {e}"))?;
        if n == 0 {
            break;
        }
        if bytes[..n].iter().any(|b| *b != 0) {
            return Err("trailing tar data".into());
        }
    }
    // Zero-valued member data must not stand in for the two terminator blocks.
    let minimum_end = end_of_members.checked_add(1024).ok_or("archive offset overflow")?;
    if bounded.total < minimum_end || bounded.total % 512 != 0 || bounded.tail.iter().any(|b| *b != 0) {
        return Err("missing tar end blocks".into());
    }
    let mut compressed = bounded.inner.into_inner();
    if !compressed.fill_buf().map_err(|e| e.to_string())?.is_empty() {
        return Err("trailing gzip data".into());
    }
    // Recheck the exact compressed bytes consumed, not merely the first pass.
    if crate::hex(compressed.into_inner().hash.finalize()) != expected_hash {
        return Err("archive changed during verification".into());
    }
    if !expected.is_empty() {
        return Err("missing original archive files".into());
    }
    Ok(())
}
struct Bounded<R> {
    inner: R,
    total: u64,
    tail: [u8; 1024],
}
impl<R: Read> Read for Bounded<R> {
    fn read(
        &mut self,
        bytes: &mut [u8],
    ) -> io::Result<usize> {
        let allowance = (TOTAL_LIMIT - self.total).min(bytes.len() as u64) as usize;
        if allowance == 0 && !bytes.is_empty() {
            let mut probe = [0];
            if self.inner.read(&mut probe)? != 0 {
                return Err(io::Error::other("decoded byte limit"));
            }
            return Ok(0);
        }
        let n = self.inner.read(&mut bytes[..allowance])?;
        for (i, b) in bytes[..n].iter().enumerate() {
            self.tail[(self.total as usize + i) % 1024] = *b;
        }
        self.total += n as u64;
        Ok(n)
    }
}

struct Compressed<R> {
    inner: R,
    hash: sha2_const_stable::Sha256,
}
impl<R: Read> Read for Compressed<R> {
    fn read(
        &mut self,
        bytes: &mut [u8],
    ) -> io::Result<usize> {
        let count = self.inner.read(bytes)?;
        self.hash = self.hash.clone().update(&bytes[..count]);
        Ok(count)
    }
}
