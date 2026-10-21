//! Private generated build-attempt ownership, bounded retained accounting and cleanup.
use std::collections::BTreeSet;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use dashboard_helper_protocol::*;
use serde_json::{Value, json};
/// Bytes per binary gibibyte used for retained and scratch budgets.
pub(crate) const GIB: u64 = 1024 * 1024 * 1024;
/// Maximum files/directories in one inventory or combined cleanup batch.
const MAX_ENTRIES: usize = 65536;
/// Maximum serialized manifest bytes, also the combined cleanup-manifest allowance.
const MAX_INDEX: usize = 32 * 1024 * 1024;
/// Returns generated attempt and ownership-index directories beneath `repo`; creates neither.
fn roots(repo: &Path) -> (PathBuf, PathBuf) {
    (
        repo.join("target/studio-helper/attempts"),
        repo.join("target/studio-helper/attempt-index"),
    )
}
/// Returns JSON from regular `path` within `max` bytes; rejects open/read, size and syntax failures.
/// Private callers use 4096; `max` must leave room for a one-byte overflow sentinel.
fn read(
    path: &Path,
    max: usize,
) -> Result<Value, String> {
    let mut b = Vec::new();
    crate::file_locks::read_file(path)?
        .take((max + 1) as u64)
        .read_to_end(&mut b)
        .map_err(|e| e.to_string())?;
    if b.len() > max {
        return Err("Storage metadata bound".into());
    }
    serde_json::from_slice(&b).map_err(|_| "Invalid storage metadata".into())
}
/// Publishes `value` at `path` via exclusive `.pending` write, sync and replacement rename.
/// Returns serialization, 32 MiB bound or filesystem errors; a failed write/rename can retain pending evidence.
/// Existing pending files refuse retries. Windows requests write-through replacement; this is not a disk guarantee.
fn publish(
    path: &Path,
    value: &Value,
) -> Result<(), String> {
    let raw = serde_json::to_vec(value).map_err(|e| e.to_string())?;
    if raw.len() > MAX_INDEX {
        return Err("Storage metadata bound".into());
    }
    let pending = path.with_extension("pending");
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&pending)
        .map_err(|e| e.to_string())?;
    f.write_all(&raw)
        .and_then(|_| f.sync_all())
        .map_err(|e| e.to_string())?;
    drop(f);
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;

        use windows_sys::Win32::Storage::FileSystem::{MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW};
        let a: Vec<u16> = pending.as_os_str().encode_wide().chain(Some(0)).collect();
        let b: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        if unsafe {
            MoveFileExW(
                a.as_ptr(),
                b.as_ptr(),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        } == 0
        {
            return Err(std::io::Error::last_os_error().to_string());
        }
    }
    #[cfg(not(windows))]
    {
        std::fs::rename(pending, path).map_err(|e| e.to_string())?;
    }
    Ok(())
}
/// Returns up to `limit` surviving immediate children of `path`, rejecting links/reparse points and I/O errors.
/// Holds ancestors during enumeration; vanished entries are skipped. Private `limit` values are at most 65536,
/// leaving one overflow sentinel; this does not retain child handles after returning.
fn checked_children(
    path: &Path,
    limit: usize,
) -> Result<Vec<PathBuf>, String> {
    let hold = crate::file_locks::ancestors(&[path.join("sentinel")])?;
    let mut entries = Vec::new();
    for entry in std::fs::read_dir(path).map_err(|e| e.to_string())?.take(limit + 1) {
        let path = entry.map_err(|e| e.to_string())?.path();
        let m = match std::fs::symlink_metadata(&path) {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e.to_string()),
        };
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if m.file_attributes() & 0x400 != 0 {
                return Err("Storage reparse point is quarantined".into());
            }
        }
        if m.file_type().is_symlink() {
            return Err("Storage link is quarantined".into());
        }
        entries.push(path);
    }
    drop(hold);
    if entries.len() > limit {
        return Err("Storage entry bound".into());
    }
    Ok(entries)
}
/// Returns file-byte total and sorted `[path, directory, length, identity]` rows below generated `root`.
/// `identities` obtains handle identities and treats disappearance as failure; false permits vanished entries
/// during active-build polling and emits empty identities. Rejects over 65536 entries, 64 path components,
/// 1024 platform path units, 16 GiB, non-UTF-8 paths, links and unsupported members; creates/deletes nothing.
fn inventory(
    root: &Path,
    identities: bool,
) -> Result<(u64, Vec<Value>), String> {
    let mut queue = vec![root.to_path_buf()];
    let mut rows = Vec::new();
    let mut bytes = 0u64;
    while let Some(dir) = queue.pop() {
        let children = match checked_children(&dir, MAX_ENTRIES.saturating_sub(rows.len())) {
            Ok(c) => c,
            Err(_) if !identities && !dir.exists() => continue,
            Err(e) => return Err(e),
        };
        for path in children {
            let relative = path.strip_prefix(root).map_err(|_| "Storage escape")?;
            if relative.components().count() > 64 || relative.as_os_str().len() > 1024 {
                return Err("Storage path bound".into());
            }
            let m = match std::fs::symlink_metadata(&path) {
                Ok(m) => m,
                Err(e) if !identities && e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => return Err(e.to_string()),
            };
            let identity = if identities {
                if m.is_dir() {
                    crate::file_locks::directory_identity(&path)?
                } else {
                    crate::file_locks::file_identity(&crate::file_locks::read_file(&path)?)?
                }
            } else {
                String::new()
            };
            if m.is_dir() {
                queue.push(path.clone());
            } else if m.is_file() {
                bytes = bytes.checked_add(m.len()).ok_or("Storage size overflow")?;
            } else {
                return Err("Unexpected storage member".into());
            }
            if bytes > 16 * GIB {
                return Err("Storage accounting bound exceeded".into());
            }
            rows.push(json!([
                relative.to_str().ok_or("Invalid storage path")?,
                m.is_dir(),
                m.len(),
                identity
            ]));
        }
    }
    rows.sort_by(|a, b| a[0].as_str().cmp(&b[0].as_str()));
    Ok((bytes, rows))
}
/// Returns at most 21 ownership records under `repo`, or empty when both storage roots are absent.
/// Rejects legacy/unowned entries, pending publications, malformed records and changed directory identities;
/// this validates ownership metadata, not the contents of each closed attempt.
fn records(repo: &Path) -> Result<Vec<Value>, String> {
    let (attempts, index) = roots(repo);
    if !attempts.exists() && !index.exists() {
        return Ok(vec![]);
    }
    if !index.exists() {
        return Err("Legacy build directories have no ownership index; review them manually before building".into());
    }
    let mut records = Vec::new();
    let mut ids = BTreeSet::new();
    let mut manifests = BTreeSet::new();
    for path in checked_children(&index, 42)? {
        if path.extension().and_then(|s| s.to_str()) == Some("files") {
            if !std::fs::symlink_metadata(&path).map_err(|e| e.to_string())?.is_file() {
                return Err("Invalid storage manifest".into());
            }
            manifests.insert(
                path.file_stem()
                    .and_then(|s| s.to_str())
                    .ok_or("Invalid storage manifest")?
                    .to_string(),
            );
            continue;
        }
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            return Err("Interrupted storage metadata publication; review cached files".into());
        }
        let id = path.file_stem().and_then(|s| s.to_str()).ok_or("Invalid storage ID")?;
        AttemptId::new(id).map_err(|_| "Invalid storage ID")?;
        let record = read(&path, 4096)?;
        if record["schema"] != 1
            || record["id"] != id
            || record["root_identity"].as_str() != Some(&crate::file_locks::directory_identity(&attempts.join(id))?)
        {
            return Err("Storage ownership changed; quarantined".into());
        }
        ids.insert(id.to_string());
        records.push(record);
    }
    if !manifests.is_subset(&ids) {
        return Err("Unowned storage manifest quarantined".into());
    }
    if records.len() > 21 {
        return Err("Storage record bound".into());
    }
    for path in checked_children(&attempts, 21)? {
        if !ids.contains(
            path.file_name()
                .and_then(|n| n.to_str())
                .ok_or("Invalid attempt path")?,
        ) {
            return Err("Unknown build directory quarantined; manual review required".into());
        }
    }
    Ok(records)
}
/// Returns validated storage summaries for `repo`, marking IDs in `protected` and all active records protected.
/// Byte counts are published charges, not a new disk scan. Returns ownership/schema/protocol errors without mutation.
pub(crate) fn list(
    repo: &Path,
    protected: &BTreeSet<String>,
) -> Result<BuildStorage, String> {
    let mut attempts = Vec::new();
    for r in records(repo)? {
        let id = r["id"].as_str().ok_or("Missing storage ID")?;
        let state = match r["state"].as_str() {
            Some("artifact") => BuildAttemptState::Artifact,
            Some("failed") => BuildAttemptState::Failed,
            Some("active") => BuildAttemptState::Incomplete,
            _ => return Err("Invalid storage state".into()),
        };
        attempts.push(BuildAttemptSummary {
            attempt_id: AttemptId::new(id).map_err(|e| e.to_string())?,
            state,
            bytes: r["bytes"].as_u64().ok_or("Missing storage size")?,
            protected: protected.contains(id) || state == BuildAttemptState::Incomplete,
        });
    }
    let report = BuildStorage {
        attempts: BoundedVec::new(attempts).map_err(|e| e.to_string())?,
        reason: Text::new(
            "16 artifacts / 8 GiB; 4 failures / 1 GiB; one 4 GiB active reservation plus 256 MiB free headroom. \
             Incomplete records remain quarantined.",
        )
        .unwrap(),
    };
    report.validate().map_err(|e| e.to_string())?;
    Ok(report)
}
/// Single generated build attempt with durable active record and held ancestry; dropping does not close or delete it.
pub(crate) struct Reservation {
    /// Exclusive generated directory receiving frozen inputs, compiler scratch and evidence.
    pub path: PathBuf,
    /// Mutable ownership record; publication failure can leave memory ahead of disk.
    record: Value,
    /// Generated `.json` ownership record beside the `.files` manifest.
    index: PathBuf,
    /// Attempt-directory hold released after close inventory before deletion/publication.
    root_lock: Option<std::fs::File>,
    /// Ancestor handles retained for the reservation lifetime.
    _parents: Vec<std::fs::File>,
}
/// Returns a new active attempt beneath `repo` after ownership, retained-quota and Windows free-space checks.
/// Requires no incomplete record, fewer than 16 artifacts/4 failures, artifact charge at most 4 GiB minus
/// 64 MiB and failure charge below 1 GiB; Windows also requires 4 GiB plus 256 MiB available.
/// Errors can leave newly created directories or active/pending metadata quarantined; no automatic rollback/adoption.
pub(crate) fn reserve(repo: &Path) -> Result<Reservation, String> {
    let report = list(repo, &BTreeSet::new())?;
    let mut count = 0;
    let mut bytes = 0;
    let mut failed = 0;
    let mut failure_bytes = 0;
    for a in report.attempts.iter() {
        match a.state {
            BuildAttemptState::Artifact => {
                count += 1;
                bytes += a.bytes;
            }
            BuildAttemptState::Failed => {
                failed += 1;
                failure_bytes += a.bytes;
            }
            BuildAttemptState::Incomplete => {
                return Err("Incomplete build reservation requires manual review; no new build admitted".into());
            }
        }
    }
    if count >= 16 || bytes > 4 * GIB - 64 * 1024 * 1024 || failed >= 4 || failure_bytes >= GIB {
        return Err("Build storage quota reached; explicitly remove eligible closed attempts".into());
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;

        use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
        let path: Vec<u16> = repo.as_os_str().encode_wide().chain(Some(0)).collect();
        let mut available = 0;
        if unsafe {
            GetDiskFreeSpaceExW(
                path.as_ptr(),
                &mut available,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        } == 0
            || available < 4 * GIB + 256 * 1024 * 1024
        {
            return Err("Build requires 4 GiB scratch reservation and 256 MiB free-space headroom".into());
        }
    }
    let (attempts, index) = roots(repo);
    for path in [
        repo.join("target"),
        repo.join("target/studio-helper"),
        attempts.clone(),
        index.clone(),
    ] {
        if !path.exists() {
            std::fs::create_dir(&path).map_err(|e| e.to_string())?;
        }
        crate::file_locks::ancestors(&[path.join("sentinel")])?;
    }
    let mut random = [0; 16];
    getrandom::fill(&mut random).map_err(|_| "Entropy unavailable")?;
    let id = random.iter().map(|n| format!("{n:02x}")).collect::<String>();
    let path = attempts.join(&id);
    std::fs::create_dir(&path).map_err(|e| e.to_string())?;
    let record = json!({"schema":1,"id":id,"root_identity":crate::file_locks::directory_identity(&path)?,"state":"active","bytes":4*GIB});
    let index = index.join(format!("{id}.json"));
    publish(&index, &record)?;
    let parents = crate::file_locks::ancestors(&[path.clone(), index.clone()])?;
    let root_lock = Some(crate::file_locks::directory_file(&path)?);
    Ok(Reservation {
        path,
        record,
        index,
        root_lock,
        _parents: parents,
    })
}
impl Reservation {
    /// Writes and syncs at most 256 KiB of `bytes` to an exclusively created diagnostic file.
    /// Returns oversize/open/write/sync errors; failures can retain a partial file and retries never overwrite it.
    pub(crate) fn write_diagnostics(
        &self,
        bytes: &[u8],
    ) -> Result<(), String> {
        if bytes.len() > 256 * 1024 {
            return Err("Build diagnostics exceed 256 KiB".into());
        }
        let mut output = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(self.path.join("build-diagnostics.log"))
            .map_err(|e| e.to_string())?;
        output
            .write_all(bytes)
            .and_then(|_| output.sync_all())
            .map_err(|e| e.to_string())
    }

    /// Copies the fixed capture diagnostic file into retained evidence, if present.
    /// Returns link/read, over-256-KiB or exclusive-create/write/sync errors; failures can retain partial output.
    fn retain_cargo_diagnostics(&self) -> Result<(), String> {
        // The fixed capture path belongs to this closed attempt, never a caller path.
        let source = self
            .path
            .join("source/firmware/target/bench-captures/native/cargo-diagnostics.log");
        match std::fs::symlink_metadata(&source) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(e.to_string()),
            Ok(_) => {}
        }
        let _parents = crate::file_locks::ancestors(std::slice::from_ref(&source))?;
        let mut bytes = Vec::new();
        crate::file_locks::read_file(&source)?
            .take(256 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() > 256 * 1024 {
            return Err("Cargo failure diagnostics exceed 256 KiB; quarantined".into());
        }
        let mut output = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(self.path.join("cargo-diagnostics.log"))
            .map_err(|e| e.to_string())?;
        output
            .write_all(&bytes)
            .and_then(|_| output.sync_all())
            .map_err(|e| e.to_string())
    }

    /// Closes this stopped attempt as an `artifact` or failure; `reason` supplies failure text (first 1024 chars),
    /// with a default when absent. Call only after compiler cleanup and, for failure, frozen-reader release.
    /// Returns identity/budget/filesystem/publication errors; prior pruning and writes are not rolled back.
    /// Artifacts retain at most 4 GiB; failures prune generated source/scratch and retain at most 16 MiB of
    /// inputs/diagnostics/evidence. Publishes identity manifest then charged record, including metadata bytes.
    /// No cancellation is polled inside close; a failed close remains quarantined and is not safely retryable.
    pub(crate) fn close(
        &mut self,
        artifact: bool,
        reason: Option<&str>,
    ) -> Result<(), String> {
        if crate::file_locks::directory_identity(&self.path)?.as_str()
            != self.record["root_identity"].as_str().ok_or("Missing root identity")?
        {
            return Err("Build storage root identity changed".into());
        }
        let diagnostics = self.path.join("build-diagnostics.log");
        if diagnostics.exists() {
            let file = crate::file_locks::read_file(&diagnostics)?;
            if file.metadata().map_err(|e| e.to_string())?.len() > 256 * 1024 {
                return Err("Build diagnostics exceed 256 KiB; quarantined".into());
            }
        }
        if !artifact {
            self.retain_cargo_diagnostics()?;
        }
        let (bytes, rows) = inventory(&self.path, true)?;
        drop(self.root_lock.take());
        if artifact && bytes > 4 * GIB {
            return Err("Closed build exceeds its 4 GiB scratch reservation; quarantined".into());
        }
        if !artifact {
            // The compiler has closed and the frozen lease has been released. Keep input
            // and bounded diagnostics/source manifest, remove generated source/build intermediates.
            let disposable = rows
                .iter()
                .filter(|r| {
                    let n = r[0].as_str().unwrap_or("");
                    n != "input"
                        && !n.starts_with("input\\")
                        && !n.starts_with("input/")
                        && n != "source-manifest.json"
                        && n != "cargo-diagnostics.log"
                        && n != "build-diagnostics.log"
                })
                .cloned()
                .collect::<Vec<_>>();
            let prepared = Tree::prepare(
                &self.path,
                self.record["root_identity"].as_str().ok_or("Missing root identity")?,
                &rows,
            )?;
            prepared.remove_selected(&disposable)?;
        }
        if !artifact {
            let reason = reason
                .unwrap_or("Build failed or was cancelled")
                .chars()
                .take(1024)
                .collect::<String>();
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(self.path.join("failure.json"))
                .map_err(|e| e.to_string())?;
            file.write_all(
                serde_json::to_string(&json!({"schema":1,"reason":reason,"scratch_disposed":true}))
                    .unwrap()
                    .as_bytes(),
            )
            .and_then(|_| file.sync_all())
            .map_err(|e| e.to_string())?;
        }
        let (bytes, rows) = inventory(&self.path, true)?;
        if !artifact && bytes > 16 * 1024 * 1024 {
            return Err("Failure evidence exceeds 16 MiB; quarantined".into());
        }
        publish(&self.index.with_extension("files"), &json!(rows))?;
        self.record["state"] = json!(if artifact { "artifact" } else { "failed" });
        let manifest_bytes = std::fs::metadata(self.index.with_extension("files"))
            .map_err(|e| e.to_string())?
            .len();
        // Include both private metadata files in retained accounting; record length
        // converges when the decimal digit count of the charged byte value stabilizes.
        self.record["bytes"] = json!(bytes + manifest_bytes);
        loop {
            let charged =
                bytes + manifest_bytes + serde_json::to_vec(&self.record).map_err(|e| e.to_string())?.len() as u64;
            if self.record["bytes"].as_u64() == Some(charged) {
                break;
            }
            self.record["bytes"] = json!(charged);
        }
        publish(&self.index, &self.record)
    }
}
/// Checks generated attempt `path`; returns an error above 4 GiB or below 256 MiB available, or on accounting/I/O
/// failure. Caller stops the owned process on failure. Polling is not an instantaneous filesystem quota.
#[cfg(windows)]
pub(crate) fn check_scratch(path: &Path) -> Result<(), String> {
    let (bytes, _) = inventory(path, false)?;
    if bytes > 4 * GIB {
        return Err("Build scratch exceeds 4 GiB; owned process will be stopped".into());
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;

        use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
        let path: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        let mut available = 0;
        if unsafe {
            GetDiskFreeSpaceExW(
                path.as_ptr(),
                &mut available,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        } == 0
            || available < 256 * 1024 * 1024
        {
            return Err("Build free-space headroom fell below 256 MiB; owned process will be stopped".into());
        }
    }
    Ok(())
}

/// Closed manifest bound to held volume/file identities; preparation rejects incompatible active readers.
struct Tree {
    /// Held generated directory, removed only by whole-tree cleanup.
    root: crate::file_locks::DeleteFile,
    /// Relative name, directory flag and deletion handle, ordered parents before descendants.
    members: Vec<(String, bool, crate::file_locks::DeleteFile)>,
}
impl Tree {
    /// Returns a held deletion tree for `root` matching `identity` and complete manifest `rows`.
    /// Rejects malformed/duplicate/escaping paths, changed identity/length, unexpected/missing entries and
    /// handle conflicts before deleting anything; rows are limited to 65536 and names to 1024 UTF-8 bytes.
    fn prepare(
        root: &Path,
        identity: &str,
        rows: &[Value],
    ) -> Result<Self, String> {
        if rows.len() > MAX_ENTRIES {
            return Err("Cleanup entry bound".into());
        }
        let held = crate::file_locks::DeleteFile::directory(root)?;
        if held.identity()? != identity {
            return Err("Cleanup root identity changed".into());
        }
        let mut ordered = rows.iter().collect::<Vec<_>>();
        ordered.sort_by_key(|r| {
            (
                r[0].as_str().unwrap_or("").split(['/', '\\']).count(),
                !r[1].as_bool().unwrap_or(false),
            )
        });
        let mut members = Vec::new();
        let mut expected = BTreeSet::new();
        for r in ordered {
            let name = r[0].as_str().ok_or("Invalid cleanup path")?;
            let relative = Path::new(name);
            if name.is_empty()
                || name.len() > 1024
                || relative
                    .components()
                    .any(|c| !matches!(c, std::path::Component::Normal(_)))
                || name.contains(':')
                || !expected.insert(name.to_string())
            {
                return Err("Invalid cleanup member".into());
            }
            let is_dir = r[1].as_bool().ok_or("Invalid cleanup type")?;
            let path = root.join(relative);
            let file = if is_dir {
                crate::file_locks::DeleteFile::directory(&path)?
            } else {
                crate::file_locks::DeleteFile::open(&path)?
            };
            if file.identity()?.as_str() != r[3].as_str().ok_or("Missing member identity")?
                || (!is_dir && Some(file.len()?) != r[2].as_u64())
            {
                return Err("Cleanup member changed".into());
            }
            members.push((name.to_string(), is_dir, file));
        }
        // Enumerate each already-held directory, bounded by the manifest, detecting
        // unknown children without following or deleting them.
        let mut observed = BTreeSet::new();
        for directory in
            std::iter::once(root.to_path_buf()).chain(members.iter().filter(|m| m.1).map(|m| root.join(&m.0)))
        {
            for entry in std::fs::read_dir(directory)
                .map_err(|e| e.to_string())?
                .take(MAX_ENTRIES + 1)
            {
                let path = entry.map_err(|e| e.to_string())?.path();
                let name = path
                    .strip_prefix(root)
                    .map_err(|_| "Cleanup escape")?
                    .to_str()
                    .ok_or("Invalid member name")?
                    .to_string();
                if !expected.contains(&name) || !observed.insert(name) {
                    return Err("Unexpected cleanup member; nothing removed".into());
                }
            }
        }
        if observed != expected {
            return Err("Cleanup inventory changed".into());
        }
        Ok(Self { root: held, members })
    }

    /// Removes held members named by `selected` rows, descendants first, retaining the root and other members.
    /// Returns the first disposition error after any earlier deletions; unknown/malformed selected names are ignored.
    fn remove_selected(
        self,
        selected: &[Value],
    ) -> Result<(), String> {
        let names = selected.iter().filter_map(|r| r[0].as_str()).collect::<BTreeSet<_>>();
        for (name, _, file) in self.members.into_iter().rev() {
            if names.contains(name.as_str()) {
                file.remove()?;
            }
        }
        Ok(())
    }

    /// Removes held members descendants first, then root; returns the first error with earlier deletions retained.
    fn remove(self) -> Result<(), String> {
        for (_, _, file) in self.members.into_iter().rev() {
            file.remove()?;
        }
        self.root.remove()
    }
}
/// Fully preflighted closed attempt and metadata held until explicit removal or nonmutating drop.
pub(crate) struct Prepared {
    /// Complete owned content with deletion handles.
    tree: Tree,
    /// Held manifest then ownership-record handles, deleted only after content removal.
    metadata: Vec<crate::file_locks::DeleteFile>,
    /// Ancestors pinned throughout preflight/removal.
    _parents: Vec<std::fs::File>,
}
impl Prepared {
    /// Removes the prepared tree then metadata; returns the first error without rolling back earlier deletions.
    /// Cancellation is checked by the caller between attempts, not within this deletion sequence.
    pub(crate) fn remove(self) -> Result<(), String> {
        self.tree.remove()?;
        for file in self.metadata {
            file.remove()?;
        }
        Ok(())
    }
}
/// Returns held cleanup plans for every ID in validated `request` beneath `repo`, preserving request order.
/// Rejects IDs in `protected`, nonclosed attempts, changed ownership/content, active readers and combined
/// limits of 65536 entries/32 MiB manifest bytes. Errors drop all holds without deleting any requested attempt.
pub(crate) fn prepare(
    repo: &Path,
    request: &CleanupRequest,
    protected: &BTreeSet<String>,
) -> Result<Vec<Prepared>, String> {
    request.validate().map_err(|e| e.to_string())?;
    let records = records(repo)?;
    let (attempts, index) = roots(repo);
    let mut result = Vec::new();
    let mut entries = 0usize;
    let mut metadata_bytes = 0u64;
    for id in request.attempts.iter() {
        let record = records
            .iter()
            .find(|r| r["id"] == id.as_str())
            .ok_or("Unknown build attempt")?;
        if protected.contains(id.as_str()) || !matches!(record["state"].as_str(), Some("artifact" | "failed")) {
            return Err("Build attempt is retained or not closed; nothing removed".into());
        }
        let path = attempts.join(id.as_str());
        let meta = index.join(format!("{}.json", id.as_str()));
        let files = meta.with_extension("files");
        let parents = crate::file_locks::ancestors(&[path.clone(), meta.clone()])?;
        let held_meta = crate::file_locks::DeleteFile::open(&meta)?;
        let held_files = crate::file_locks::DeleteFile::open(&files)?;
        if serde_json::from_slice::<Value>(&held_meta.bytes_bounded(4096)?).map_err(|e| e.to_string())? != *record {
            return Err("Build ownership index changed".into());
        }
        metadata_bytes = metadata_bytes
            .checked_add(held_files.len()?)
            .ok_or("Cleanup metadata bound")?;
        if metadata_bytes > MAX_INDEX as u64 {
            return Err("Cleanup metadata budget exceeded; choose fewer attempts".into());
        }
        let rows: Vec<Value> =
            serde_json::from_slice(&held_files.bytes_bounded(MAX_INDEX)?).map_err(|e| e.to_string())?;
        entries = entries.checked_add(rows.len()).ok_or("Cleanup entry bound")?;
        if entries > MAX_ENTRIES {
            return Err("Cleanup batch exceeds entry budget; choose fewer attempts".into());
        }
        let tree = Tree::prepare(
            &path,
            record["root_identity"].as_str().ok_or("Missing root identity")?,
            &rows,
        )?;
        result.push(Prepared {
            tree,
            metadata: vec![held_files, held_meta],
            _parents: parents,
        });
    }
    Ok(result)
}

#[cfg(all(test, windows))]
/// Windows identity/handle and retained-quota regressions using private temporary directories.
mod tests {
    use super::*;
    /// Returns a newly created unique temporary repository root; panics on clock/directory failure.
    fn fixture() -> PathBuf {
        /// Process-local collision discriminator for concurrent fixtures.
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let root = std::env::temp_dir().join(format!(
            "studio-storage-{}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&root).unwrap();
        root
    }
    /// Returns cleanup IDs from `reservations`; panics for malformed fixture metadata or protocol bounds.
    fn request(reservations: &[&Reservation]) -> CleanupRequest {
        CleanupRequest {
            attempts: BoundedVec::new(
                reservations
                    .iter()
                    .map(|r| AttemptId::new(r.record["id"].as_str().unwrap()).unwrap())
                    .collect(),
            )
            .unwrap(),
        }
    }
    /// Removes test-owned `repo` after asserting its temporary-root prefix; panics on refusal or cleanup failure.
    fn cleanup_fixture(repo: PathBuf) {
        assert!(repo.starts_with(std::env::temp_dir()));
        std::fs::remove_dir_all(repo).unwrap();
    }
    #[test]
    /// Failure close preserves bounded diagnostics/inputs, prunes both scratch layouts and permits explicit removal.
    fn failure_disposes_generated_source_retains_diagnostics_and_frees_explicitly() {
        let repo = fixture();
        let mut r = reserve(&repo).unwrap();
        std::fs::create_dir_all(r.path.join("source/nested")).unwrap();
        std::fs::write(r.path.join("source/nested/object"), b"compiler output").unwrap();
        for name in ["f", "h", "n"] {
            let scratch = r.path.join("b").join(name);
            std::fs::create_dir_all(&scratch).unwrap();
            std::fs::write(scratch.join("object"), b"short-path compiler output").unwrap();
        }
        std::fs::create_dir(r.path.join("input")).unwrap();
        std::fs::write(r.path.join("input/project"), b"accepted").unwrap();
        let diagnostics = r
            .path
            .join("source/firmware/target/bench-captures/native/cargo-diagnostics.log");
        std::fs::create_dir_all(diagnostics.parent().unwrap()).unwrap();
        std::fs::write(&diagnostics, b"error: fixture missing linker\n").unwrap();
        assert!(r.write_diagnostics(&vec![b'x'; 256 * 1024 + 1]).is_err());
        r.write_diagnostics(b"Validation stderr: actual validator failure\n")
            .unwrap();
        assert!(r.write_diagnostics(b"replacement").is_err());
        r.close(false, Some("fixture compiler failure")).unwrap();
        assert_eq!(
            std::fs::read(r.path.join("build-diagnostics.log")).unwrap(),
            b"Validation stderr: actual validator failure\n"
        );
        assert_eq!(
            std::fs::read(r.path.join("cargo-diagnostics.log")).unwrap(),
            b"error: fixture missing linker\n"
        );
        assert!(!r.path.join("source").exists());
        assert!(!r.path.join("b").exists());
        assert!(r.path.join("input/project").exists());
        assert!(r.path.join("failure.json").exists());
        assert_eq!(list(&repo, &BTreeSet::new()).unwrap().attempts.len(), 1);
        for p in prepare(&repo, &request(&[&r]), &BTreeSet::new()).unwrap() {
            p.remove().unwrap();
        }
        assert_eq!(list(&repo, &BTreeSet::new()).unwrap().attempts.len(), 0);
        drop(r);
        cleanup_fixture(repo);
    }
    #[test]
    /// Oversize captured diagnostics fail before source pruning and retain the original failure evidence.
    fn oversized_failure_diagnostics_quarantine_without_pruning_source() {
        let repo = fixture();
        let mut r = reserve(&repo).unwrap();
        let diagnostics = r
            .path
            .join("source/firmware/target/bench-captures/native/cargo-diagnostics.log");
        std::fs::create_dir_all(diagnostics.parent().unwrap()).unwrap();
        std::fs::write(&diagnostics, vec![b'x'; 256 * 1024 + 1]).unwrap();
        assert!(r.close(false, Some("fixture failure")).unwrap_err().contains("256 KiB"));
        assert!(diagnostics.exists());
        assert!(!r.path.join("cargo-diagnostics.log").exists());
        drop(r);
        cleanup_fixture(repo);
    }
    #[test]
    /// Batch preflight leaves earlier attempts intact for protection, readers, extra files and same-length replacement.
    fn preflight_all_refuses_unknown_members_replacements_readers_and_protected() {
        let repo = fixture();
        let mut a = reserve(&repo).unwrap();
        std::fs::write(a.path.join("artifact"), b"first").unwrap();
        a.close(true, None).unwrap();
        let mut b = reserve(&repo).unwrap();
        std::fs::write(b.path.join("artifact"), b"second").unwrap();
        b.close(true, None).unwrap();
        let req = request(&[&a, &b]);
        let protected = BTreeSet::from([b.record["id"].as_str().unwrap().to_string()]);
        assert!(prepare(&repo, &req, &protected).is_err());
        let held = crate::file_locks::read_file(&b.path.join("artifact")).unwrap();
        assert!(prepare(&repo, &req, &BTreeSet::new()).is_err());
        drop(held);
        std::fs::write(b.path.join("user-note"), b"do not delete").unwrap();
        assert!(prepare(&repo, &req, &BTreeSet::new()).is_err());
        assert!(a.path.join("artifact").exists());
        std::fs::remove_file(b.path.join("user-note")).unwrap();
        std::fs::rename(b.path.join("artifact"), b.path.join("old")).unwrap();
        std::fs::write(b.path.join("artifact"), b"second").unwrap();
        std::fs::remove_file(b.path.join("old")).unwrap();
        assert!(prepare(&repo, &req, &BTreeSet::new()).is_err());
        assert!(a.path.join("artifact").exists());
        drop((a, b));
        cleanup_fixture(repo);
    }
    #[test]
    /// Four retained failures block admission until explicit removal; an active reservation cannot be cleaned or
    /// bypassed.
    fn failure_cap_requires_explicit_cleanup_and_active_restart_is_quarantined() {
        let repo = fixture();
        let mut all = Vec::new();
        for _ in 0..4 {
            let mut r = reserve(&repo).unwrap();
            r.close(false, None).unwrap();
            all.push(r);
        }
        assert!(reserve(&repo).unwrap_err_text().contains("quota"));
        for p in prepare(&repo, &request(&[&all[0]]), &BTreeSet::new()).unwrap() {
            p.remove().unwrap();
        }
        let active = reserve(&repo).unwrap();
        assert!(reserve(&repo).unwrap_err_text().contains("Incomplete"));
        assert!(prepare(&repo, &request(&[&active]), &BTreeSet::new()).is_err());
        drop((active, all));
        cleanup_fixture(repo);
    }
    /// Extracts failure text without requiring successful reservations to implement Debug.
    trait ErrText {
        /// Returns the error string; panics if the fixture unexpectedly reserved an attempt.
        fn unwrap_err_text(self) -> String;
    }
    impl ErrText for Result<Reservation, String> {
        /// Returns the reservation error; panics on unexpected success.
        fn unwrap_err_text(self) -> String {
            match self {
                Ok(_) => panic!("unexpected reservation"),
                Err(e) => e,
            }
        }
    }
    #[test]
    /// Sixteen artifacts exhaust count quota, charges include both metadata files, and selected removal frees one slot.
    fn artifact_count_and_exact_accounting_release_only_selected_capacity() {
        let repo = fixture();
        let mut all = Vec::new();
        for _ in 0..16 {
            let mut r = reserve(&repo).unwrap();
            std::fs::write(r.path.join("artifact"), b"payload").unwrap();
            r.close(true, None).unwrap();
            let expected = 7
                + std::fs::metadata(&r.index).unwrap().len()
                + std::fs::metadata(r.index.with_extension("files")).unwrap().len();
            assert_eq!(r.record["bytes"].as_u64(), Some(expected));
            all.push(r);
        }
        assert!(reserve(&repo).unwrap_err_text().contains("quota"));
        for p in prepare(&repo, &request(&[&all[0]]), &BTreeSet::new()).unwrap() {
            p.remove().unwrap();
        }
        let next = reserve(&repo).unwrap();
        assert_eq!(list(&repo, &BTreeSet::new()).unwrap().attempts.len(), 16);
        drop((next, all));
        cleanup_fixture(repo);
    }
    #[test]
    /// Unindexed legacy directories block reservation and remain untouched.
    fn unknown_legacy_storage_is_never_adopted() {
        let repo = fixture();
        std::fs::create_dir_all(repo.join("target/studio-helper/attempts/legacy")).unwrap();
        assert!(reserve(&repo).is_err());
        assert!(repo.join("target/studio-helper/attempts/legacy").exists());
        cleanup_fixture(repo);
    }
}

#[cfg(all(test, windows))]
/// Interrupted metadata publication must preserve committed evidence and block further admission.
mod publication_failure_tests {
    use super::*;
    #[test]
    /// An existing partial pending file is neither overwritten nor adopted; listing and reservation reject the store.
    fn interrupted_metadata_publication_preserves_committed_file_and_quarantines_storage() {
        let repo = std::env::temp_dir().join(format!(
            "studio-publication-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&repo).unwrap();
        let mut reservation = reserve(&repo).unwrap();
        reservation.close(false, None).unwrap();
        let committed = std::fs::read(&reservation.index).unwrap();
        let pending = reservation.index.with_extension("pending");
        std::fs::write(&pending, b"{\"schema\":").unwrap();
        assert!(publish(&reservation.index, &json!({"schema":1,"state":"artifact"})).is_err());
        assert_eq!(std::fs::read(&reservation.index).unwrap(), committed);
        assert_eq!(std::fs::read(&pending).unwrap(), b"{\"schema\":");
        assert!(list(&repo, &BTreeSet::new()).is_err());
        assert!(reserve(&repo).is_err());
        assert!(reservation.path.exists());
        drop(reservation);
        assert!(repo.starts_with(std::env::temp_dir()));
        assert!(
            repo.file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("studio-publication-")
        );
        std::fs::remove_dir_all(repo).unwrap();
    }
}
