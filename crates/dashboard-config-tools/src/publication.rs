//! Publication of user-selected CLI outputs.
use std::fs::File;
use std::io::{self, Write};
use std::path::Path;

/// Publish `bytes` at selected `path`; return write/sync/rename/cleanup errors.
///
/// Writes an exclusively created sibling and renames only after successful sync;
/// failures before rename retain the old destination. This is not crash-durable
/// directory metadata synchronization or a sandbox for attacker-controlled parents.
pub(crate) fn publish(
    path: &Path,
    bytes: &[u8],
) -> io::Result<()> {
    publish_with(path, |file| file.write_all(bytes))
}

/// Invoke `write` on an owned sibling then replace `path`; return the first failure.
///
/// Reject existing symlinks/non-files/read-only outputs; try at most 32 exclusive
/// sibling names. Preserve destination permissions when present. A cleanup failure
/// reports the retained sibling path alongside the original error. No automatic retry
/// follows a write/rename failure; callbacks must not panic if cleanup is required.
fn publish_with(
    path: &Path,
    write: impl FnOnce(&mut File) -> io::Result<()>,
) -> io::Result<()> {
    use std::sync::atomic::{AtomicU64, Ordering};
    /// Best-effort unique suffix; exclusive creation, not this wrapping counter, prevents clobbering.
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let permissions = match std::fs::symlink_metadata(path) {
        Ok(meta) => {
            if !meta.is_file() || meta.file_type().is_symlink() || meta.permissions().readonly() {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "Output must be a writable regular file",
                ));
            }
            Some(meta.permissions())
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => None,
        Err(e) => return Err(e),
    };
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    // Exclusive creation prevents a concurrent invocation or a preexisting file
    // from being truncated. Keep the sibling on the destination filesystem.
    let (temporary, mut file) = (0..32)
        .find_map(|_| {
            let sequence = NEXT.fetch_add(1, Ordering::Relaxed);
            let temporary = parent.join(format!(".dashboard-config-{}-{sequence}.tmp", std::process::id()));
            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)
            {
                Ok(file) => Some(Ok((temporary, file))),
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => None,
                Err(e) => Some(Err(e)),
            }
        })
        .unwrap_or_else(|| {
            Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "Cannot reserve output temporary",
            ))
        })?;
    let result = (|| {
        if let Some(permissions) = permissions {
            file.set_permissions(permissions)?;
        }
        write(&mut file)?;
        file.sync_all()?;
        Ok(())
    })();
    drop(file);
    let result = result.and_then(|()| std::fs::rename(&temporary, path));
    if let Err(error) = result {
        // Only this invocation's exclusively created sibling is eligible here.
        if let Err(cleanup) = std::fs::remove_file(&temporary) {
            return Err(io::Error::other(format!(
                "{}; temporary retained at {}: {cleanup}",
                error,
                temporary.display()
            )));
        }
        return Err(error);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    //! Inject partial-write and Windows sharing failures at the publication boundary.

    use super::*;
    /// A failed callback may write a sibling prefix but cannot replace the old destination.
    #[test]
    fn failed_write_preserves_existing_configuration() {
        let dir = std::env::temp_dir().join(format!(
            "config-publication-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&dir).unwrap();
        let target = dir.join("résumé config.eav");
        std::fs::write(&target, b"previous configuration").unwrap();
        let result = publish_with(&target, |file| {
            file.write_all(b"partial replacement")?;
            Err(io::Error::other("injected write failure"))
        });
        let actual = std::fs::read(&target).unwrap();
        std::fs::remove_file(target).unwrap();
        std::fs::remove_dir(dir).unwrap();
        assert!(result.is_err());
        assert_eq!(actual, b"previous configuration");
    }
    /// Windows sharing denial retains old output and cleans the sibling; explicit retry succeeds.
    #[cfg(windows)]
    #[test]
    fn denied_replace_keeps_original_then_explicit_retry_succeeds() {
        use std::os::windows::fs::OpenOptionsExt;
        let dir = std::env::temp_dir().join(format!("config-publication-replace-{}", std::process::id()));
        std::fs::create_dir(&dir).unwrap();
        let target = dir.join("configuration.eav");
        std::fs::write(&target, b"old").unwrap();
        let held = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(&target)
            .unwrap();
        assert!(publish(&target, b"replacement").is_err());
        assert_eq!(std::fs::read(&target).unwrap(), b"old");
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        drop(held);
        publish(&target, b"replacement").unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"replacement");
        std::fs::remove_file(target).unwrap();
        std::fs::remove_dir(dir).unwrap();
    }
}
