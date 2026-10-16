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
