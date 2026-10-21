//! Append-only bounded flash observations. Recovery never resumes an operation.
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use dashboard_helper_protocol::*;
use serde_json::{Value, json};
/// Maximum UTF-8 JSON bytes in one durable flash observation.
const MAX_RECORD: usize = 65536;
/// One generated flash-attempt directory with at most eight immutable published observations.
pub(crate) struct Journal {
    /// Generated job-ID directory under the repository's private flash-attempt root.
    directory: PathBuf,
    /// Next unpublished generation in `0..=8`; eight means exhausted.
    generation: u8,
    /// Current JSON candidate, which may be newer than the last acknowledged publication after an error.
    record: Value,
    /// Windows ancestor-directory handles retaining the generated attempt pathname.
    _parents: Vec<File>,
}
/// Returns the private flash-attempt root below `repo` without creating it or admitting arbitrary paths.
fn root(repo: &Path) -> PathBuf { repo.join("target/studio-helper/flash-attempts") }
impl Journal {
    /// Creates one fresh attempt for `job` in `repo`, binding `session` and explicit `request` as historical evidence.
    /// Returns a journal with no published generation; directory/link/open failures or 16 existing attempts reject
    /// creation. Errors can leave newly created empty directories; caller has already admitted the request and owns
    /// operation serialization.
    pub fn new(
        repo: &Path,
        job: &JobId,
        session: &SessionId,
        request: &FlashRequest,
    ) -> Result<Self, String> {
        let parent = root(repo);
        // Verify each existing parent before and after creation; no external pathname input.
        for path in [repo.join("target"), repo.join("target/studio-helper"), parent.clone()] {
            if !path.exists() {
                std::fs::create_dir(&path).map_err(|e| e.to_string())?;
            }
            crate::file_locks::ancestors(&[path.join("sentinel")])?;
        }
        let count = std::fs::read_dir(&parent).map_err(|e| e.to_string())?.take(17).count();
        if count >= 16 {
            return Err("Flash history full; retain and review prior attempts before cleanup".into());
        }
        let directory = parent.join(job.as_str());
        std::fs::create_dir(&directory).map_err(|e| e.to_string())?;
        let parents = crate::file_locks::ancestors(&[directory.join("record")])?;
        Ok(Self {
            directory,
            generation: 0,
            record: json!({"schema_version":1,"session_id":session,"job_id":job,
            "request":request,"running_firmware_observed":false,"hardware_qualified":false}),
            _parents: parents,
        })
    }

    /// Adds checked `artifact` and half-open flash-address `ranges` to the in-memory record before publication.
    /// Records 4 MiB as manufacturer/user-confirmed capacity, never measured capacity; does not validate or write here.
    pub fn bind_artifact(
        &mut self,
        artifact: &ArtifactSummary,
        ranges: &[(u32, u32)],
    ) {
        self.record["artifact"] = serde_json::to_value(artifact).unwrap();
        self.record["verification_ranges"] = json!(ranges);
        self.record["verification_scope"] = json!("admitted_elf_payload_and_internal_gaps_at_verification_time");
        self.record["capacity_bytes"] = json!(4194304);
        self.record["capacity_provenance"] = json!("manufacturer4_mb_user_confirmed");
    }

    /// Publishes fixed `phase`, conservative write `outcome` and optional child `exit` as the next generation.
    /// Returns success only after bounded JSON is written/synced, renamed and reread identically; then advances
    /// generation. Errors retain prior published observations, but mutate the in-memory candidate and can leave
    /// `.pending` or a published-but-unacknowledged file. Existing pending files prevent automatic retry; eight
    /// generations is the cap.
    pub fn publish(
        &mut self,
        phase: &str,
        outcome: WriteOutcome,
        exit: Option<i32>,
    ) -> Result<(), String> {
        if self.generation >= 8 {
            return Err("Flash journal generation bound".into());
        }
        self.record["phase"] = json!(phase);
        self.record["write_outcome"] = json!(outcome);
        self.record["generation"] = json!(self.generation);
        self.record["raw_exit"] = json!(exit.map(|n| n as u32));
        self.record["interpreted_exit"] = json!(exit);
        let data = serde_json::to_vec(&self.record).map_err(|e| e.to_string())?;
        if data.len() > MAX_RECORD {
            return Err("Flash record byte bound".into());
        }
        let temporary = self.directory.join(format!("{}.pending", self.generation));
        let published = self.directory.join(format!("{}.json", self.generation));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|e| e.to_string())?;
        file.write_all(&data)
            .and_then(|_| file.sync_all())
            .map_err(|e| e.to_string())?;
        drop(file);
        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStrExt;

            use windows_sys::Win32::Storage::FileSystem::{MOVEFILE_WRITE_THROUGH, MoveFileExW};
            let from: Vec<u16> = temporary.as_os_str().encode_wide().chain(Some(0)).collect();
            let to: Vec<u16> = published.as_os_str().encode_wide().chain(Some(0)).collect();
            // SAFETY: both buffers are live NUL-terminated UTF-16 paths; flags request same-volume durable rename.
            if unsafe { MoveFileExW(from.as_ptr(), to.as_ptr(), MOVEFILE_WRITE_THROUGH) } == 0 {
                return Err(std::io::Error::last_os_error().to_string());
            }
        }
        #[cfg(not(windows))]
        {
            std::fs::rename(&temporary, &published).map_err(|e| e.to_string())?;
        }
        let observed = read(&published)?;
        if observed != self.record {
            return Err("Flash publication acknowledgment mismatch".into());
        }
        self.generation += 1;
        Ok(())
    }
}
/// Returns JSON from checked regular file `path`, rejecting read/link errors, malformed JSON and more than 65,536
/// bytes.
fn read(path: &Path) -> Result<Value, String> {
    let mut bytes = Vec::new();
    crate::file_locks::read_file(path)?
        .take((MAX_RECORD + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > MAX_RECORD {
        return Err("Flash record bound".into());
    }
    serde_json::from_slice(&bytes).map_err(|_| "Invalid flash record".into())
}
/// Historical consent is evidence only. Never deserialize it into a live FlashRequest.
struct HistoricalRequest {
    /// Original selected artifact identity, not a currently retained lease.
    artifact_id: ArtifactId,
    /// Original selected device identity, not current-device authority.
    device_id: DeviceId,
    /// Original board-acknowledgment session, checked against the enclosing record.
    session_id: SessionId,
}
/// Returns historical identities from strict request-shaped `value`, accepting the frozen legacy Pico2 spelling
/// only as history. Schema, options, IDs, hashes or acknowledgment mismatch reject the record without creating live
/// consent.
fn historical_request(value: &Value) -> Result<HistoricalRequest, String> {
    /// Returns success only when JSON `value` is an object with exactly `names`; rejects missing and unknown fields.
    fn fields(
        value: &Value,
        names: &[&str],
    ) -> Result<(), String> {
        let object = value.as_object().ok_or("Invalid historical request object")?;
        if object.len() != names.len() || names.iter().any(|name| !object.contains_key(*name)) {
            return Err("Invalid historical request fields".into());
        }
        Ok(())
    }
    fields(
        value,
        &[
            "artifact_id",
            "device_id",
            "editor_revision",
            "project_sha256",
            "config_sha256",
            "expected_options",
            "retry_of",
            "board_acknowledgement",
        ],
    )?;
    let acknowledgement = &value["board_acknowledgement"];
    fields(
        acknowledgement,
        &["device_id", "session_id", "information_sha256", "board"],
    )?;
    if !matches!(
        acknowledgement["board"].as_str(),
        Some("genuine_pico2" | "genuine_pico2_w")
    ) {
        return Err("Invalid historical board".into());
    }
    /// Decodes the named historical object member as the supplied type, propagating a bounded field-specific error.
    macro_rules! field {
        ($object:expr, $name:literal, $ty:ty) => {
            serde_json::from_value::<$ty>($object[$name].clone()).map_err(|_| concat!("Invalid historical ", $name))?
        };
    }
    let artifact_id = field!(value, "artifact_id", ArtifactId);
    let device_id = field!(value, "device_id", DeviceId);
    let acknowledged_device = field!(acknowledgement, "device_id", DeviceId);
    let session_id = field!(acknowledgement, "session_id", SessionId);
    let _ = field!(value, "editor_revision", u64);
    let _ = field!(value, "project_sha256", Sha256);
    let _ = field!(value, "config_sha256", Sha256);
    let _ = field!(value, "retry_of", Option<JobId>);
    let _ = field!(acknowledgement, "information_sha256", Sha256);
    let options = field!(value, "expected_options", BuildOptions);
    normalize(&options).map_err(|e| e.to_string())?;
    if acknowledged_device != device_id {
        return Err("Invalid historical device acknowledgement".into());
    }
    Ok(HistoricalRequest {
        artifact_id,
        device_id,
        session_id,
    })
}
/// Returns at most 16 historical attempts beneath `repo`, reading published generations 0..7 and ignoring pending
/// files. Missing root returns empty history; unknown/malformed/oversized records, links, identities or filesystem
/// failures fail the whole read instead of falling back to older success. Only explicit Verified remains verified on
/// recovery; other published outcomes become Unknown. No operation is resumed and running firmware/hardware remain
/// unobserved.
pub(crate) fn history(repo: &Path) -> Result<FlashHistory, String> {
    let parent = root(repo);
    let mut result = Vec::new();
    if parent.exists() {
        let _parents = crate::file_locks::ancestors(&[parent.join("sentinel")])?;
        for entry in std::fs::read_dir(&parent).map_err(|e| e.to_string())?.take(17) {
            if result.len() == 16 {
                return Err("Flash history bound".into());
            }
            let path = entry.map_err(|e| e.to_string())?.path();
            let job = JobId::new(
                path.file_name()
                    .and_then(|p| p.to_str())
                    .ok_or("Invalid attempt name")?,
            )
            .map_err(|_| "Invalid attempt name")?;
            let _hold = crate::file_locks::ancestors(&[path.join("record")])?;
            let mut latest = None;
            for generation in 0..8 {
                let record = path.join(format!("{generation}.json"));
                if record.exists() {
                    latest = Some(read(&record)?);
                }
            }
            let mut summary = FlashAttemptSummary {
                job_id: job.clone(),
                original_session: None,
                artifact_id: None,
                device_id: None,
                phase: FlashPhase::Unpublished,
                write_outcome: WriteOutcome::NotStarted,
                recovery_write_outcome: WriteOutcome::NotStarted,
                exit: None,
                historical_observation: true,
                automatic_resume: false,
                running_firmware_observed: false,
                hardware_qualified: false,
            };
            if let Some(value) = latest {
                if value["schema_version"] != 1 || value["job_id"] != job.as_str() {
                    return Err("Invalid flash history identity".into());
                }
                let request = historical_request(&value["request"])?;
                let session: SessionId =
                    serde_json::from_value(value["session_id"].clone()).map_err(|_| "Invalid history session")?;
                if session != request.session_id {
                    return Err("Invalid historical session acknowledgement".into());
                }
                summary.original_session = Some(session);
                summary.artifact_id = Some(request.artifact_id);
                summary.device_id = Some(request.device_id);
                summary.phase = serde_json::from_value(value["phase"].clone()).map_err(|_| "Invalid history phase")?;
                summary.write_outcome =
                    serde_json::from_value(value["write_outcome"].clone()).map_err(|_| "Invalid history outcome")?;
                summary.recovery_write_outcome = if summary.write_outcome == WriteOutcome::Verified {
                    WriteOutcome::Verified
                } else {
                    WriteOutcome::Unknown
                };
                if !value["raw_exit"].is_null() {
                    summary.exit = Some(RawExit {
                        windows_u32: serde_json::from_value(value["raw_exit"].clone())
                            .map_err(|_| "Invalid raw exit")?,
                        interpreted_i32: serde_json::from_value(value["interpreted_exit"].clone())
                            .map_err(|_| "Invalid interpreted exit")?,
                    });
                }
            }
            summary.validate().map_err(|e| e.to_string())?;
            result.push(summary);
        }
    }
    let history = FlashHistory {
        schema_version: 1,
        attempts: BoundedVec::new(result).map_err(|e| e.to_string())?,
    };
    history.validate().map_err(|e| e.to_string())?;
    Ok(history)
}
#[cfg(test)]
mod tests {
    //! Durable generation publication and bounded-history fixture checks without hardware operations.
    use super::*;
    #[test]
    /// Acknowledged generations recover coherently, with history capacity reclaimed only by explicit owned cleanup.
    fn publication_is_coherent_and_retains_verified_history() {
        let repo = std::env::temp_dir().join(format!(
            "studio-intent-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&repo).unwrap();
        let job = JobId::new("a".repeat(32)).unwrap();
        let session = SessionId::new("b".repeat(32)).unwrap();
        let device = DeviceId::new("c".repeat(32)).unwrap();
        let hash = Sha256::new("d".repeat(64)).unwrap();
        let request = FlashRequest {
            artifact_id: ArtifactId::new("e".repeat(64)).unwrap(),
            device_id: device.clone(),
            editor_revision: 1,
            project_sha256: hash.clone(),
            config_sha256: hash.clone(),
            expected_options: Default::default(),
            retry_of: None,
            board_acknowledgement: BoardAcknowledgement {
                device_id: device,
                session_id: session.clone(),
                information_sha256: hash,
                board: BoardContract::GenuinePico2W,
            },
        };
        let mut journal = Journal::new(&repo, &job, &session, &request).unwrap();
        journal.publish("intent_armed", WriteOutcome::Unknown, None).unwrap();
        assert_eq!(
            history(&repo).unwrap().attempts[0].recovery_write_outcome,
            WriteOutcome::Unknown
        );
        journal
            .publish("verification_confirmed", WriteOutcome::Verified, Some(0))
            .unwrap();
        assert_eq!(
            history(&repo).unwrap().attempts[0].recovery_write_outcome,
            WriteOutcome::Verified
        );
        drop(journal);
        #[cfg(windows)]
        {
            for n in 0..15 {
                let other = JobId::new(format!("{n:032x}")).unwrap();
                drop(Journal::new(&repo, &other, &session, &request).unwrap());
            }
            let next = JobId::new("f".repeat(32)).unwrap();
            assert!(Journal::new(&repo, &next, &session, &request).is_err());
            crate::flash_cleanup::remove(&repo, &AttemptId::new(job.as_str()).unwrap()).unwrap();
            drop(Journal::new(&repo, &next, &session, &request).unwrap());
            assert_eq!(history(&repo).unwrap().attempts.len(), 16);
        }
        std::fs::remove_dir_all(repo).unwrap();
    }
}

#[cfg(test)]
mod recovery_tests {
    //! Frozen legacy/current history, torn publication and conservative recovery regressions.
    use super::*;
    /// Frozen historical wire shape; never regenerate using the current Journal/FlashRequest.
    const LEGACY_JOURNAL: &str = include_str!("../tests/fixtures/flash-journal-v1-pico2.json");
    /// Returns an isolated temporary repository and unpublished journal; caller drops the journal before cleanup.
    fn fixture() -> (PathBuf, Journal) {
        /// Process-local suffix preventing collisions among concurrent same-timestamp fixtures.
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let repo = std::env::temp_dir().join(format!(
            "studio-journal-recovery-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir(&repo).unwrap();
        let job = JobId::new("a".repeat(32)).unwrap();
        let session = SessionId::new("b".repeat(32)).unwrap();
        let device = DeviceId::new("c".repeat(32)).unwrap();
        let hash = Sha256::new("d".repeat(64)).unwrap();
        let request = FlashRequest {
            artifact_id: ArtifactId::new("e".repeat(64)).unwrap(),
            device_id: device.clone(),
            editor_revision: 1,
            project_sha256: hash.clone(),
            config_sha256: hash.clone(),
            expected_options: Default::default(),
            retry_of: None,
            board_acknowledgement: BoardAcknowledgement {
                device_id: device,
                session_id: session.clone(),
                information_sha256: hash,
                board: BoardContract::GenuinePico2W,
            },
        };
        let journal = Journal::new(&repo, &job, &session, &request).unwrap();
        (repo, journal)
    }
    /// Removes test-owned `repo` only after checking its temporary-root and fixture-prefix bounds; panics on
    /// mismatch/I/O failure.
    fn cleanup(repo: PathBuf) {
        assert!(repo.starts_with(std::env::temp_dir()));
        assert!(
            repo.file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("studio-journal-recovery-")
        );
        std::fs::remove_dir_all(repo).unwrap();
    }
    #[test]
    /// Frozen legacy consent remains readable historical evidence but cannot deserialize as a current flash request.
    fn legacy_and_current_journals_remain_read_only_history() {
        let (repo, mut journal) = fixture();
        journal
            .publish("verification_confirmed", WriteOutcome::Verified, Some(0))
            .unwrap();
        let legacy: Value = serde_json::from_str(LEGACY_JOURNAL).unwrap();
        let legacy_job = "f".repeat(32);
        let directory = root(&repo).join(&legacy_job);
        std::fs::create_dir(&directory).unwrap();
        std::fs::write(directory.join("0.json"), LEGACY_JOURNAL).unwrap();
        assert!(decode::<FlashRequest>(&serde_json::to_vec(&legacy["request"]).unwrap()).is_err());
        let recovered = history(&repo).unwrap();
        assert_eq!(recovered.attempts.len(), 2);
        for attempt in recovered.attempts.iter() {
            assert!(!attempt.automatic_resume);
            assert!(!attempt.hardware_qualified);
            assert!(!attempt.running_firmware_observed);
            assert!(attempt.historical_observation);
            assert_eq!(attempt.device_id.as_ref().unwrap().as_str(), "c".repeat(32));
            let expected = if attempt.job_id.as_str() == legacy_job {
                WriteOutcome::Unknown
            } else {
                WriteOutcome::Verified
            };
            assert_eq!(attempt.recovery_write_outcome, expected);
        }
        drop(journal);
        cleanup(repo);
    }
    #[test]
    /// Unknown versions, malformed options/identity and extra fields reject historical request admission.
    fn legacy_history_rejects_malformed_requests_and_unknown_versions() {
        let (repo, mut journal) = fixture();
        journal.publish("intent_armed", WriteOutcome::Unknown, None).unwrap();
        let path = journal.directory.join("0.json");
        let mut legacy: Value = serde_json::from_str(LEGACY_JOURNAL).unwrap();
        legacy["job_id"] = json!("a".repeat(32));
        std::fs::write(&path, serde_json::to_vec(&legacy).unwrap()).unwrap();
        assert!(history(&repo).is_ok());
        for change in 0..10 {
            let mut bad = legacy.clone();
            match change {
                0 => bad["schema_version"] = json!(2),
                1 => bad["request"]["board_acknowledgement"]["device_id"] = json!("e".repeat(32)),
                2 => bad["request"]["board_acknowledgement"]["board"] = json!("unknown"),
                3 => bad["request"]["extra"] = json!(true),
                4 => bad["request"]["expected_options"] = json!({}),
                5 => bad["request"]["project_sha256"] = json!("invalid"),
                6 => bad["request"]["board_acknowledgement"]["extra"] = json!(true),
                7 => bad["request"]["editor_revision"] = json!(-1),
                8 => bad["request"]["board_acknowledgement"]["session_id"] = json!("f".repeat(32)),
                _ => bad["request"]["retry_of"] = json!("invalid"),
            }
            std::fs::write(&path, serde_json::to_vec(&bad).unwrap()).unwrap();
            assert!(history(&repo).is_err(), "case {change}");
        }
        drop(journal);
        cleanup(repo);
    }
    #[test]
    /// Torn or complete pending bytes cannot upgrade recovered outcome until the generation is actually renamed.
    fn pending_generation_never_upgrades_recovered_write_outcome() {
        let (repo, mut journal) = fixture();
        journal.publish("intent_armed", WriteOutcome::Unknown, None).unwrap();
        let published = std::fs::read(journal.directory.join("0.json")).unwrap();
        let mut verified = journal.record.clone();
        verified["phase"] = json!("verification_confirmed");
        verified["write_outcome"] = json!(WriteOutcome::Verified);
        verified["generation"] = json!(1);
        verified["raw_exit"] = json!(0);
        verified["interpreted_exit"] = json!(0);
        let complete = serde_json::to_vec(&verified).unwrap();
        // Simulate a torn subsequent write, then a complete but unrenamed write.
        for pending in [b"{\"phase\":".as_slice(), complete.as_slice()] {
            std::fs::write(journal.directory.join("1.pending"), pending).unwrap();
            let recovered = history(&repo).unwrap();
            assert_eq!(recovered.attempts[0].recovery_write_outcome, WriteOutcome::Unknown);
            assert!(!recovered.attempts[0].automatic_resume);
            assert!(!recovered.attempts[0].running_firmware_observed);
            assert!(!recovered.attempts[0].hardware_qualified);
            assert!(
                journal
                    .publish("verification_confirmed", WriteOutcome::Verified, Some(0))
                    .is_err()
            );
            assert_eq!(journal.generation, 1);
            assert_eq!(std::fs::read(journal.directory.join("0.json")).unwrap(), published);
            assert!(!journal.directory.join("1.json").exists());
        }
        drop(journal);
        assert_eq!(
            history(&repo).unwrap().attempts[0].recovery_write_outcome,
            WriteOutcome::Unknown
        );
        // Prove the completed candidate is admissible once actually published.
        // Otherwise malformed pending data could hide an accidental early read.
        std::fs::rename(
            root(&repo).join("a".repeat(32)).join("1.pending"),
            root(&repo).join("a".repeat(32)).join("1.json"),
        )
        .unwrap();
        assert_eq!(
            history(&repo).unwrap().attempts[0].recovery_write_outcome,
            WriteOutcome::Verified
        );
        cleanup(repo);
    }
    #[test]
    /// A corrupt newer published record fails history instead of silently retaining an older verified result.
    fn corrupt_published_generation_fails_history_instead_of_falling_back_to_verified() {
        let (repo, mut journal) = fixture();
        journal
            .publish("verification_confirmed", WriteOutcome::Verified, Some(0))
            .unwrap();
        std::fs::write(journal.directory.join("1.json"), b"{").unwrap();
        assert!(history(&repo).is_err());
        std::fs::write(journal.directory.join("1.json"), vec![b' '; MAX_RECORD + 1]).unwrap();
        assert!(history(&repo).is_err());
        drop(journal);
        cleanup(repo);
    }
    #[test]
    /// Rename failure retains prior bytes and next generation number while exposing the leftover pending record.
    fn failed_rename_preserves_last_record_and_never_acknowledges_generation() {
        let (repo, mut journal) = fixture();
        journal.publish("intent_armed", WriteOutcome::Unknown, None).unwrap();
        let first = std::fs::read(journal.directory.join("0.json")).unwrap();
        std::fs::create_dir(journal.directory.join("1.json")).unwrap();
        assert!(
            journal
                .publish("verification_confirmed", WriteOutcome::Verified, Some(0))
                .is_err()
        );
        assert_eq!(journal.generation, 1);
        assert_eq!(std::fs::read(journal.directory.join("0.json")).unwrap(), first);
        assert!(journal.directory.join("1.pending").is_file());
        assert!(history(&repo).is_err());
        drop(journal);
        cleanup(repo);
    }
}
