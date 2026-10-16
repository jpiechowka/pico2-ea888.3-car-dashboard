//! Explicit removal of selected local flash observations, never device state.
use std::path::Path;

use dashboard_helper_protocol::*;

pub(crate) fn remove(
    repo: &Path,
    attempt: &AttemptId,
) -> Result<(), String> {
    let parent = repo.join("target/studio-helper/flash-attempts");
    let directory = parent.join(attempt.as_str());
    // This holds every ancestor, including the selected directory, against relocation.
    let _parents = crate::file_locks::ancestors(std::slice::from_ref(&directory))?;
    let held_directory = crate::file_locks::DeleteFile::directory(&directory)?;
    let entries = std::fs::read_dir(&directory)
        .map_err(|e| e.to_string())?
        .take(17)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    if entries.len() > 16 {
        return Err("Unexpected flash record count; nothing removed".into());
    }
    let mut files = Vec::new();
    for entry in entries {
        let name = entry.file_name();
        let name = name.to_str().ok_or("Invalid flash record name")?;
        if !(0..8).any(|n| name == format!("{n}.json") || name == format!("{n}.pending")) {
            return Err("Unexpected flash record member; nothing removed".into());
        }
        let file = crate::file_locks::DeleteFile::open(&entry.path())?;
        if file.len()? > 65536 {
            return Err("Oversized flash record; nothing removed".into());
        }
        // Pending files can be partial after interruption. Published identities must match.
        if name.ends_with(".json") {
            let bytes = file.bytes()?;
            let record: serde_json::Value = serde_json::from_slice(&bytes).map_err(|_| "Invalid flash record")?;
            if record["schema_version"] != 1 || record["job_id"] != attempt.as_str() {
                return Err("Flash record identity mismatch; nothing removed".into());
            }
        }
        files.push(file);
    }
    // All members are preflighted and held before the first deletion. No recursion.
    for file in files {
        file.remove()?;
    }
    // Empty-directory disposition also targets the original held identity.
    held_directory.remove()
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    fn fixture() -> (std::path::PathBuf, AttemptId, std::path::PathBuf) {
        let repo = std::env::temp_dir().join(format!(
            "studio-cleanup-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let id = AttemptId::new("a".repeat(32)).unwrap();
        let dir = repo.join("target/studio-helper/flash-attempts").join(id.as_str());
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("0.json"),
            serde_json::to_vec(&serde_json::json!({"schema_version":1,"job_id":id})).unwrap(),
        )
        .unwrap();
        (repo, id, dir)
    }
    #[test]
    fn removes_only_selected_members_and_releases_capacity() {
        let (repo, id, dir) = fixture();
        let neighbor = dir.parent().unwrap().join("b".repeat(32));
        std::fs::create_dir(&neighbor).unwrap();
        std::fs::write(neighbor.join("0.pending"), b"keep").unwrap();
        std::fs::write(dir.join("1.pending"), b"partial crash record").unwrap();
        remove(&repo, &id).unwrap();
        assert!(!dir.exists());
        assert_eq!(std::fs::read(neighbor.join("0.pending")).unwrap(), b"keep");
        assert_eq!(std::fs::read_dir(neighbor.parent().unwrap()).unwrap().count(), 1);
        std::fs::remove_dir_all(repo).unwrap();
    }
    #[test]
    fn refuses_unknown_member_or_mismatched_identity_before_any_delete() {
        let (repo, id, dir) = fixture();
        std::fs::write(dir.join("notes.txt"), b"owned by user").unwrap();
        assert!(remove(&repo, &id).is_err());
        assert!(dir.join("0.json").exists());
        std::fs::remove_file(dir.join("notes.txt")).unwrap();
        std::fs::write(dir.join("1.json"), br#"{"schema_version":1,"job_id":"other"}"#).unwrap();
        assert!(remove(&repo, &id).is_err());
        assert!(dir.join("0.json").exists());
        std::fs::remove_dir_all(repo).unwrap();
    }
    #[test]
    fn held_record_prevents_cleanup_without_losing_data() {
        let (repo, id, dir) = fixture();
        let held = crate::file_locks::read_file(&dir.join("0.json")).unwrap();
        assert!(remove(&repo, &id).is_err());
        assert!(dir.join("0.json").exists());
        drop(held);
        remove(&repo, &id).unwrap();
        std::fs::remove_dir_all(repo).unwrap();
    }
}
