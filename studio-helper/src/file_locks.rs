//! Retain Windows deny-write/delete files and deny-delete ancestor directories.
use std::collections::BTreeSet;
use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};
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
pub(crate) fn ancestors(paths: &[PathBuf]) -> Result<Vec<File>, String> {
    let mut parents = BTreeSet::new();
    for path in paths {
        for parent in path.ancestors().skip(1) {
            parents.insert(parent.to_path_buf());
        }
    }
    let mut files = Vec::new();
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
mod tests {
    use super::*;
    #[test]
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

/// A selected regular file retained with DELETE access; disposition targets this handle.
pub(crate) struct DeleteFile(File);
impl DeleteFile {
    pub(crate) fn open(path: &Path) -> Result<Self, String> { Self::open_kind(path, false) }

    pub(crate) fn directory(path: &Path) -> Result<Self, String> { Self::open_kind(path, true) }

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

    pub(crate) fn identity(&self) -> Result<String, String> { file_identity(&self.0) }

    pub(crate) fn len(&self) -> Result<u64, String> { self.0.metadata().map(|m| m.len()).map_err(|e| e.to_string()) }

    pub(crate) fn bytes(&self) -> Result<Vec<u8>, String> { self.bytes_bounded(65536) }

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
pub(crate) fn file_identity(file: &File) -> Result<String, String> {
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;

        use windows_sys::Win32::Storage::FileSystem::{BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle};
        let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
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
pub(crate) fn directory_identity(path: &Path) -> Result<String, String> { file_identity(&directory_file(path)?) }
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
