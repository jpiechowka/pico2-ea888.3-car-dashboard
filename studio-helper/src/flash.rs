//! Fixed native flash adapter. No browser-provided paths, tools, switches or shell.
use std::path::Path;

use dashboard_helper_protocol::*;

use crate::artifacts::Artifact;
use crate::flash_output::Command;
pub(crate) fn admit(
    request: &FlashRequest,
    session: &SessionId,
    artifact: &Artifact,
    device: &DeviceSummary,
    inspection: &DeviceInspection,
) -> Result<(), Error> {
    request.validate()?;
    if artifact.summary.session_id != *session
        || artifact.summary.artifact_id != request.artifact_id
        || artifact.summary.project_sha256 != request.project_sha256
        || artifact.summary.config_sha256 != request.config_sha256
        || artifact.summary.editor_revision != request.editor_revision
        || artifact.summary.options != normalize(&request.expected_options)?
        || device.device_id != request.device_id
        || device.status != DeviceStatus::DescriptorOnly
        || inspection.device_id != device.device_id
        || inspection.descriptor_sha256 != device.descriptor_sha256
        || inspection.session_id != *session
        || !inspection.complete
        || request.board_acknowledgement.session_id != *session
        || inspection.information_sha256.as_ref() != Some(&request.board_acknowledgement.information_sha256)
        || inspection
            .facts
            .as_ref()
            .is_none_or(|f| f.flash_size_estimate.is_some_and(|size| size > 4194304))
    {
        return Err(Error::InvalidState);
    }
    inspection.validate()?;
    Ok(())
}
fn fresh(
    repo: &Path,
    session: &SessionId,
    device: &DeviceSummary,
    inspection: &DeviceInspection,
) -> Result<(), String> {
    let observed = crate::device_inspection::inspect(repo, session.clone(), device.clone());
    cancellation()?;
    if !observed.complete || observed.descriptor_sha256 != inspection.descriptor_sha256 {
        return Err("Selected device unavailable or changed during fresh inspection".into());
    }
    let mut before = inspection.facts.clone().ok_or("Missing admitted device facts")?;
    let mut after = observed.facts.ok_or("Missing fresh device facts")?;
    if after.flash_size_estimate.is_some_and(|n| n > 4194304) {
        return Err("Flash estimate contradicts acknowledged board capacity".into());
    }
    // Content-derived estimates may change after programming. They are not chip identity.
    before.flash_size_estimate = None;
    after.flash_size_estimate = None;
    if before != after {
        return Err("Selected chip, boot observation, security or partition policy changed".into());
    }
    Ok(())
}
fn cancellation() -> Result<(), String> {
    if crate::process::cancelled() {
        Err("cancelled: no further command dispatched".into())
    } else {
        Ok(())
    }
}
pub(crate) struct Failure {
    pub reason: String,
    pub exit: Option<RawExit>,
}
pub(crate) struct Operation<'a> {
    pub repo: &'a Path,
    pub session: &'a SessionId,
    pub job: &'a JobId,
    pub request: &'a FlashRequest,
    pub artifact: &'a Artifact,
    pub device: &'a DeviceSummary,
    pub inspection: &'a DeviceInspection,
}
pub(crate) fn run(
    operation: Operation<'_>,
    transition: &mut dyn FnMut(FirmwareStage, StageState) -> Result<(), String>,
    log: &mut dyn FnMut(FirmwareStage, bool, &str),
) -> Result<Option<RawExit>, Failure> {
    let Operation {
        repo,
        session,
        job,
        request,
        artifact,
        device,
        inspection,
    } = operation;
    admit(request, session, artifact, device, inspection).map_err(|e| {
        Failure {
            reason: e.to_string(),
            exit: None,
        }
    })?;
    let mut journal = None;
    let mut outcome = WriteOutcome::NotStarted;
    let mut exit = None;
    let result = (|| {
        cancellation()?;
        let lease = artifact.verify_lease()?;
        transition(FirmwareStage::Prerequisites, StageState::Succeeded)?;
        transition(FirmwareStage::Validation, StageState::Running)?;
        artifact.verify_lease()?;
        transition(FirmwareStage::Validation, StageState::Succeeded)?;
        transition(FirmwareStage::DeviceSelection, StageState::Running)?;
        fresh(repo, session, device, inspection)?;
        cancellation()?;
        transition(FirmwareStage::DeviceSelection, StageState::Succeeded)?;
        let serial = device.serial.as_ref().ok_or("Missing stored serial")?.as_str();
        if serial.starts_with('-') || !serial.is_ascii() || serial.chars().any(char::is_control) {
            return Err("Invalid stored selector".into());
        }
        let elf = lease.elf.to_str().ok_or("Invalid frozen ELF pathname")?;
        journal = Some(crate::flash_intent::Journal::new(repo, job, session, request)?);
        journal
            .as_mut()
            .unwrap()
            .bind_artifact(&artifact.summary, &lease.verification_ranges);
        journal
            .as_mut()
            .unwrap()
            .publish("intent_armed", WriteOutcome::Unknown, None)?;
        for (command, stage) in [
            (Command::Load, FirmwareStage::Transfer),
            (Command::Verify, FirmwareStage::Verification),
            (Command::Reboot, FirmwareStage::Reboot),
        ] {
            cancellation()?;
            if command != Command::Load {
                fresh(repo, session, device, inspection)?;
                cancellation()?;
            }
            artifact.verify_lease()?;
            if command == Command::Reboot {
                journal.as_mut().unwrap().publish("reboot_armed", outcome, exit)?;
            }
            transition(stage, StageState::Running)?;
            if command == Command::Load {
                outcome = WriteOutcome::Unknown;
            }
            let mut args = match command {
                Command::Load => vec!["load", "-u", "--ignore-partitions", elf, "-t", "elf"],
                Command::Verify => vec!["verify", elf, "-t", "elf"],
                Command::Reboot => vec!["reboot", "-a", "-c", "arm"],
            };
            args.extend(["--vid", "0x2e8a", "--pid", "0x000f", "--ser", serial]);
            let output = crate::process::run_flash(
                &lease.converter.path,
                &args,
                lease.frozen.source_root(),
                command == Command::Load,
                command == Command::Reboot,
                &mut |stderr, text| log(stage, stderr, text),
            )?;
            exit = output.code;
            if !crate::flash_output::complete(
                command,
                output.code,
                &output.raw_stdout,
                &output.raw_stderr,
                lease.verification_ranges.len(),
            ) {
                return Err(format!(
                    "{stage:?} did not complete the exact pinned command semantics; exit {:?}",
                    output.code
                ));
            }
            // Independent process closure and exact semantics precede each success fact.
            transition(stage, StageState::Succeeded)?;
            outcome = match command {
                Command::Load => WriteOutcome::TransferredUnverified,
                Command::Verify | Command::Reboot => WriteOutcome::Verified,
            };
            journal.as_mut().unwrap().publish(
                match command {
                    Command::Load => "load_completed",
                    Command::Verify => "verification_confirmed",
                    Command::Reboot => "reboot_completed",
                },
                outcome,
                exit,
            )?;
        }
        Ok(exit.map(|code| {
            RawExit {
                windows_u32: code as u32,
                interpreted_i32: code,
            }
        }))
    })();
    if result.is_err()
        && let Some(journal) = &mut journal
    {
        let _ = journal.publish("interrupted", outcome, exit);
    }
    result.map_err(|reason| {
        Failure {
            reason,
            exit: exit.map(|code| {
                RawExit {
                    windows_u32: code as u32,
                    interpreted_i32: code,
                }
            }),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn admission_binds_complete_project_session_options_and_inspection() {
        let session = SessionId::new("1".repeat(32)).unwrap();
        let device_id = DeviceId::new("2".repeat(32)).unwrap();
        let hash = Sha256::new("3".repeat(64)).unwrap();
        let artifact = Artifact {
            summary: ArtifactSummary {
                artifact_id: ArtifactId::new("4".repeat(64)).unwrap(),
                session_id: session.clone(),
                editor_revision: 7,
                project_sha256: hash.clone(),
                config_sha256: hash.clone(),
                startup_profile_sha256: hash.clone(),
                startup_asset_sha256: hash.clone(),
                source_sha256: hash.clone(),
                elf_sha256: hash.clone(),
                uf2_sha256: hash.clone(),
                converter_sha256: hash.clone(),
                options: normalize(&BuildOptions::default()).unwrap(),
                uf2_bytes: 512,
                hardware_qualified: false,
                flash_ready: false,
            },
            uf2: vec![0; 512].into(),
            lease: None,
        };
        let capacity = DeviceCapacity {
            state: DeviceCapacityState::Unverified,
            bytes: None,
            provenance: None,
        };
        let device = DeviceSummary {
            device_id: device_id.clone(),
            descriptor_sha256: hash.clone(),
            vid: 0x2E8A,
            pid: 0x000F,
            serial: Some(Text::new("ABC123").unwrap()),
            product: None,
            status: DeviceStatus::DescriptorOnly,
            reason: Text::new("observed").unwrap(),
            capacity: capacity.clone(),
            flash_ready: false,
        };
        let inspection = DeviceInspection {
            session_id: session.clone(),
            device_id: device_id.clone(),
            descriptor_sha256: hash.clone(),
            complete: true,
            facts: Some(BootselFacts {
                revision: Text::new("A2").unwrap(),
                chip_id: Text::new("0x0123456789abcdef").unwrap(),
                boot_random: Text::new("01234567:89abcdef:01234567:89abcdef").unwrap(),
                available_cpus: Text::new("ARM").unwrap(),
                secure_boot: false,
                debug_enabled: true,
                secure_debug_enabled: true,
                unpartitioned_families: Text::new("rp2350-arm-s").unwrap(),
                flash_size_estimate: Some(4194304),
            }),
            information_sha256: Some(hash.clone()),
            reason: Text::new("read").unwrap(),
            capacity,
            flash_ready: false,
        };
        let request = FlashRequest {
            artifact_id: artifact.summary.artifact_id.clone(),
            device_id: device_id.clone(),
            editor_revision: 7,
            project_sha256: hash.clone(),
            config_sha256: hash.clone(),
            expected_options: BuildOptions::default(),
            retry_of: None,
            board_acknowledgement: BoardAcknowledgement {
                device_id,
                session_id: session.clone(),
                information_sha256: hash,
                board: BoardContract::GenuinePico2,
            },
        };
        assert!(admit(&request, &session, &artifact, &device, &inspection).is_ok());
        let mut changed = request.clone();
        changed.project_sha256 = Sha256::new("5".repeat(64)).unwrap();
        assert!(admit(&changed, &session, &artifact, &device, &inspection).is_err());
        changed = request.clone();
        changed.editor_revision += 1;
        assert!(admit(&changed, &session, &artifact, &device, &inspection).is_err());
        changed = request.clone();
        changed.board_acknowledgement.session_id = SessionId::new("6".repeat(32)).unwrap();
        assert!(admit(&changed, &session, &artifact, &device, &inspection).is_err());
        let mut changed = inspection.clone();
        changed.facts.as_mut().unwrap().flash_size_estimate = Some(8388608);
        assert!(admit(&request, &session, &artifact, &device, &changed).is_err());
        changed = inspection;
        changed.complete = false;
        assert!(admit(&request, &session, &artifact, &device, &changed).is_err());
    }
}
