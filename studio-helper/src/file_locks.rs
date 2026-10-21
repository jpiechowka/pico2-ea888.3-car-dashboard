//! Retain Windows deny-write/delete files and deny-delete ancestor directories.
//! Sharing flags follow [`OpenOptionsExt`](https://doc.rust-lang.org/std/os/windows/fs/trait.OpenOptionsExt.html)
//! and [CreateFileW](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfilew).
use std::collections::BTreeSet;
use std::fs::File;
#[cfg(windows)]
use std::fs::OpenOptions;
use std::path::{Path, PathBuf};
/// Returns a read handle for `path`, rejecting a final symlink/reparse point first.
/// Windows permits other readers but denies write/delete sharing until drop;
/// non-Windows only performs the preliminary link check and ordinary open.
/// Errors are metadata/open diagnostics. Callers retain ancestor handles separately.
pub(crate) fn read_file(path: &Path) -> Result<File, String> {
    reject_reparse(path)?;
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(path)
            .map_err(|e| e.to_string())
    }
    #[cfg(not(windows))]
    {
        File::open(path).map_err(|e| e.to_string())
    }
}
/// Returns `Ok(())` if `path` metadata is neither a symlink nor Windows reparse point.
/// Reports missing/inaccessible metadata or link rejection; this pathname check
/// alone does not pin identity against replacement between check and open.
fn reject_reparse(path: &Path) -> Result<(), String> {
    let metadata = std::fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if metadata.file_type().is_symlink() {
        return Err("Symbolic link not admitted".into());
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err("Reparse point not admitted".into());
        }
    }
    Ok(())
}
/// Returns held, deduplicated ancestor-directory handles for `paths`, excluding
/// each supplied path itself. Windows handles permit read/write but deny deletion;
/// drop releases them. Metadata/open errors release any partially acquired handles.
/// On non-Windows, validates ancestor links then returns an empty vector with no locks.
pub(crate) fn ancestors(paths: &[PathBuf]) -> Result<Vec<File>, String> {
    let mut parents = BTreeSet::new();
    for path in paths {
        for parent in path.ancestors().skip(1) {
            parents.insert(parent.to_path_buf());
        }
    }
    #[cfg(windows)]
    let mut files = Vec::new();
    #[cfg(not(windows))]
    let files = Vec::new();
    for parent in parents {
        reject_reparse(&parent)?;
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            files.push(
                OpenOptions::new()
                    .access_mode(0x80)
                    .share_mode(3)
                    .custom_flags(0x02000000)
                    .open(parent)
                    .map_err(|e| e.to_string())?,
            );
        }
    }
    Ok(files)
}

#[cfg(all(test, windows))]
/// Windows sharing regressions using isolated temporary files and directories.
mod tests {
    use super::*;
    #[test]
    /// Assert held file/ancestor handles reject overwrite and directory relocation.
    fn held_file_and_parent_reject_overwrite_and_relocation() {
        let root = std::env::temp_dir().join(format!(
            "studio-file-lock-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&root).unwrap();
        let file = root.join("artifact");
        std::fs::write(&file, b"immutable").unwrap();
        let parents = ancestors(std::slice::from_ref(&file)).unwrap();
        let held = read_file(&file).unwrap();
        assert!(std::fs::write(&file, b"replacement").is_err());
        assert!(std::fs::rename(&root, root.with_extension("moved")).is_err());
        drop(held);
        drop(parents);
        assert!(root.starts_with(std::env::temp_dir()));
        std::fs::remove_file(file).unwrap();
        std::fs::remove_dir(root).unwrap();
    }
}

/// A selected regular file or directory retained with DELETE access; deletion targets
/// this handle, not a subsequently resolved path. Construction is Windows-only.
pub(crate) struct DeleteFile(
    /// Owned read/delete handle, denying concurrent write/delete sharing.
    File,
);
impl DeleteFile {
    /// Returns a held regular file for `path`; rejects links, wrong types and open errors.
    /// Non-Windows returns unsupported. Opening does not delete the file.
    pub(crate) fn open(path: &Path) -> Result<Self, String> { Self::open_kind(path, false) }

    /// Returns a held directory for `path`, rejecting links/wrong types/open errors.
    /// Non-Windows returns unsupported. Removal later requires an empty directory.
    pub(crate) fn directory(path: &Path) -> Result<Self, String> { Self::open_kind(path, true) }

    /// Open `path` without traversing its final reparse point and validate handle
    /// metadata against `directory`. Returns owned deletion capability or a platform,
    /// metadata/type/sharing error. Ancestor pinning remains the caller's responsibility.
    fn open_kind(
        path: &Path,
        directory: bool,
    ) -> Result<Self, String> {
        reject_reparse(path)?;
        #[cfg(windows)]
        {
            use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
            let file = OpenOptions::new()
                .access_mode(0x80000000 | 0x10000)
                .share_mode(1)
                .custom_flags(0x00200000 | 0x02000000)
                .open(path)
                .map_err(|e| e.to_string())?;
            let metadata = file.metadata().map_err(|e| e.to_string())?;
            if metadata.is_dir() != directory
                || (!directory && !metadata.is_file())
                || metadata.file_attributes() & 0x400 != 0
            {
                return Err("Cleanup accepts regular files only".into());
            }
            Ok(Self(file))
        }
        #[cfg(not(windows))]
        {
            let _ = directory;
            Err("Native cleanup requires Windows".into())
        }
    }

    /// Returns the held volume/file identity or its OS query failure.
    pub(crate) fn identity(&self) -> Result<String, String> { file_identity(&self.0) }

    /// Returns held-file length in bytes, or the metadata query error.
    pub(crate) fn len(&self) -> Result<u64, String> { self.0.metadata().map(|m| m.len()).map_err(|e| e.to_string()) }

    /// Returns at most 65,536 bytes from the current file cursor, advancing it.
    /// Propagates read errors or rejects excess bytes; does not rewind for a second read.
    pub(crate) fn bytes(&self) -> Result<Vec<u8>, String> { self.bytes_bounded(65536) }

    /// Returns bytes from the held handle's current cursor up to `limit`, advancing
    /// it even on error. Reads at most `limit + 1` to detect excess; errors report
    /// read failure or oversized records. Internal callers must keep `limit` below
    /// `usize::MAX` because the sentinel addition is unchecked.
    pub(crate) fn bytes_bounded(
        &self,
        limit: usize,
    ) -> Result<Vec<u8>, String> {
        use std::io::Read;
        let mut bytes = Vec::new();
        (&self.0)
            .take((limit + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() > limit {
            return Err("Flash record bound".into());
        }
        Ok(bytes)
    }

    /// Mark the held object for deletion and release this handle; returns `Ok(())`
    /// when Windows accepted disposition, or its OS error. A directory must be empty;
    /// other handles can defer final deletion. Non-Windows is unsupported.
    /// Uses [handle disposition](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-setfileinformationbyhandle).
    pub(crate) fn remove(self) -> Result<(), String> {
        #[cfg(windows)]
        {
            use std::os::windows::io::AsRawHandle;

            use windows_sys::Win32::Storage::FileSystem::{
                FILE_DISPOSITION_INFO,
                FileDispositionInfo,
                SetFileInformationByHandle,
            };
            let info = FILE_DISPOSITION_INFO { DeleteFile: true };
            // SAFETY: self owns a live DELETE-capable handle; the matching structure
            // and byte size remain valid throughout this synchronous Win32 call.
            if unsafe {
                SetFileInformationByHandle(
                    self.0.as_raw_handle(),
                    FileDispositionInfo,
                    &info as *const _ as *const _,
                    std::mem::size_of_val(&info) as u32,
                )
            } == 0
            {
                return Err(std::io::Error::last_os_error().to_string());
            }
            Ok(())
        }
        #[cfg(not(windows))]
        {
            Err("Native cleanup requires Windows".into())
        }
    }
}

/// Stable volume/file identity from a held handle, not a pathname or modification time.
/// Returns fixed-width hexadecimal volume serial and file index for `file`, or an
/// OS query error. Non-Windows returns unsupported; identity is not a content hash.
pub(crate) fn file_identity(file: &File) -> Result<String, String> {
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;

        use windows_sys::Win32::Storage::FileSystem::{BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle};
        // SAFETY: this Win32 output structure consists only of integer/FILETIME fields.
        let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
        // SAFETY: file owns a live handle and info is writable for the synchronous call.
        if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } == 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
        Ok(format!(
            "{:08x}{:08x}{:08x}",
            info.dwVolumeSerialNumber, info.nFileIndexHigh, info.nFileIndexLow
        ))
    }
    #[cfg(not(windows))]
    {
        let _ = file;
        Err("Storage ownership requires Windows".into())
    }
}
/// Returns a checked directory identity for `path`, releasing its temporary handle
/// after the query. Propagates directory admission and identity errors.
pub(crate) fn directory_identity(path: &Path) -> Result<String, String> { file_identity(&directory_file(path)?) }
/// Returns a Windows directory handle for `path`, denying delete sharing while held.
/// Rejects links/reparse points using both pathname and opened-handle metadata,
/// wrong types and OS failures. Non-Windows returns unsupported.
pub(crate) fn directory_file(path: &Path) -> Result<File, String> {
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        reject_reparse(path)?;
        let file = OpenOptions::new()
            .access_mode(0x80)
            .share_mode(3)
            .custom_flags(0x02000000 | 0x00200000)
            .open(path)
            .map_err(|e| e.to_string())?;
        use std::os::windows::fs::MetadataExt;
        let meta = file.metadata().map_err(|e| e.to_string())?;
        if !meta.is_dir() || meta.file_attributes() & 0x400 != 0 {
            return Err("Directory identity is not a regular directory".into());
        }
        Ok(file)
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        Err("Storage ownership requires Windows".into())
    }
}
