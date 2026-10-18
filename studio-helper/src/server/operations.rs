//! Serialized worker admission and operation completion. Transfer cancellation,
//! artifact publication and cleanup failure each keep their operation-specific rules.
use super::*;
// Reserve a footer inside the disk bound so omitted output is never silent.
fn retain_diagnostic(
    bytes: &mut Vec<u8>,
    limit: usize,
    line: &str,
) -> usize {
    let available = limit.saturating_sub(512).saturating_sub(bytes.len());
    let end = line.floor_char_boundary(line.len().min(available));
    bytes.extend_from_slice(&line.as_bytes()[..end]);
    line.len() - end
}

fn log_report(
    store: &mut JobStore,
    id: &JobId,
    report: &Value,
) {
    let stage = StageId::Probe(ProbeStage::Prerequisites);
    for row in report["checks"].as_array().into_iter().flatten() {
        let text = format!(
            "[{}] {}: {}",
            row["status"].as_str().unwrap_or("unknown"),
            row["id"].as_str().unwrap_or("check"),
            row["evidence"].as_str().unwrap_or("")
        );
        let mut framer = StreamFramer::new(&[]).unwrap();
        for chunk in text.as_bytes().chunks(4096) {
            let _ = framer.push(chunk, |text, progress| {
                let _ = store.append(
                    id,
                    LogInput {
                        stage,
                        source: Source::Internal,
                        text,
                        progress,
                    },
                );
            });
        }
        let _ = framer.finish(|text, progress| {
            let _ = store.append(
                id,
                LogInput {
                    stage,
                    source: Source::Internal,
                    text,
                    progress,
                },
            );
        });
    }
}
pub(super) async fn start_check(
    state: Arc<State>,
    device_scan: bool,
    selected: Option<DeviceSummary>,
) -> Result<JobSummary, Error> {
    let mut inner = idle(&state).await?;
    let id = JobId::new(random_hex::<16>().map_err(|_| Error::InvalidState)?)?;
    if let Some(selected) = &selected
        && (!inner.devices.complete
            || !inner.devices.devices.iter().any(|d| d == selected)
            || inner
                .devices
                .devices
                .iter()
                .any(|d| d.status != DeviceStatus::DescriptorOnly))
    {
        return Err(Error::InvalidState);
    }
    let inspection = selected.is_some();
    let operation = if device_scan {
        OperationKind::DeviceScan
    } else {
        OperationKind::PrerequisiteCheck
    };
    let result = inner.jobs.start(id.clone(), operation, None)?;
    crate::terminal::event(
        false,
        id.as_str(),
        if device_scan {
            "Device descriptor scan started"
        } else {
            "Prerequisite check started"
        },
    );
    let stage = StageId::Probe(if device_scan {
        ProbeStage::DeviceDetection
    } else {
        ProbeStage::Prerequisites
    });
    inner.jobs.transition(&id, stage, StageState::Running, None)?;
    inner.jobs.append(
        &id,
        LogInput {
            stage,
            source: Source::Internal,
            text: if inspection {
                "Inspecting explicitly selected BOOTSEL device; no persistent programming or reboot; transient \
                 interface/XIP changes possible"
            } else if device_scan {
                "Reading BOOTSEL descriptors only; no interface reset, programming or capacity claim"
            } else {
                "Checking installed tools; no installation, build or USB access"
            }
            .into(),
            progress: false,
        },
    )?;
    crate::process::reset_cancel();
    let worker_state = state.clone();
    inner.worker = Some(tokio::task::spawn_blocking(move || {
        let mut process = crate::process::ProcessContext::default();
        let report = if let Some(selected) = selected {
            serde_json::to_value(crate::device_inspection::inspect(
                &mut process,
                &worker_state.repo,
                worker_state.session.clone(),
                selected,
            ))
            .unwrap()
        } else if device_scan {
            serde_json::to_value(crate::devices::scan(
                &mut process,
                &worker_state.repo,
                worker_state.session.clone(),
            ))
            .unwrap()
        } else {
            crate::prerequisites::check_with_process(&mut process, &worker_state.repo)
        };
        let mut inner = worker_state.inner.lock().unwrap();
        if inspection {
            let mut observed: DeviceInspection = serde_json::from_value(report.clone()).unwrap();
            if crate::process::cancelled()
                || inner
                    .jobs
                    .poll(&id, PollRequest { after: 0, limit: 1 })
                    .is_ok_and(|b| b.summary.cancel_requested)
            {
                observed.complete = false;
                observed.facts = None;
                observed.information_sha256 = None;
                observed.reason = Text::new("Inspection cancelled; no device qualification retained").unwrap();
            }
            let text = observed.reason.as_str().to_owned();
            inner.inspections.insert(observed.device_id.clone(), observed);
            let _ = inner.jobs.append(
                &id,
                LogInput {
                    stage,
                    source: Source::Internal,
                    text,
                    progress: false,
                },
            );
        } else if device_scan {
            inner.inspections.clear();
            inner.devices = serde_json::from_value(report.clone()).unwrap();
            let text = format!(
                "{} BOOTSEL descriptor candidates. {}",
                inner.devices.devices.len(),
                inner.devices.reason.as_str()
            );
            let _ = inner.jobs.append(
                &id,
                LogInput {
                    stage,
                    source: Source::Internal,
                    text,
                    progress: false,
                },
            );
        } else {
            log_report(&mut inner.jobs, &id, &report);
            inner.cached = report.clone();
        }
        if crate::process::cleanup_failed() {
            let _ = inner.jobs.note_cleanup_fatal(&id, true);
            inner.stopping = true;
            return;
        }
        let cancelled = inner
            .jobs
            .poll(&id, PollRequest { after: 0, limit: 1 })
            .is_ok_and(|b| b.summary.cancel_requested);
        let (state, cause, reason) = if cancelled {
            (
                StageState::Cancelled,
                TerminalCause::Cancelled,
                Some(Text::new("Check cancelled; owned probe processes joined").unwrap()),
            )
        } else if report[if device_scan { "complete" } else { "ready" }] == true {
            (StageState::Succeeded, TerminalCause::Succeeded, None)
        } else {
            (
                StageState::Failed,
                TerminalCause::Failed,
                Some(Text::new("Operation incomplete; inspect the cached prerequisite or device report").unwrap()),
            )
        };
        crate::terminal::event(
            state == StageState::Failed,
            id.as_str(),
            &format!("{stage:?}: {state:?}"),
        );
        let _ = inner.jobs.transition(&id, stage, state, reason);
        let _ = inner.jobs.finish(&id, cause, process.closure(0, 0), None, None);
    }));
    Ok(result)
}
pub(super) async fn start_build(
    state: Arc<State>,
    request: ProjectBuildRequest,
) -> Result<JobSummary, Error> {
    let mut inner = idle(&state).await?;
    let input = inner
        .project
        .as_ref()
        .ok_or(Error::NotFoundOrUnavailable)?
        .accepted(&state.session, &request)
        .map_err(|_| Error::Schema)?;
    let id = JobId::new(random_hex::<16>().map_err(|_| Error::InvalidState)?)?;
    let result = inner.jobs.start(id.clone(), OperationKind::Build, request.retry_of)?;
    crate::terminal::event(false, id.as_str(), "Accepted project build started");
    let prerequisite = StageId::Firmware(FirmwareStage::Prerequisites);
    inner.jobs.transition(&id, prerequisite, StageState::Running, None)?;
    crate::process::reset_cancel();
    let worker_state = state.clone();
    inner.worker = Some(tokio::task::spawn_blocking(move || {
        let mut process = crate::process::ProcessContext::default();
        let mut stage = prerequisite;
        let transition = |stage, state, reason| {
            crate::terminal::event(false, id.as_str(), &format!("{stage:?}: {state:?}"));
            worker_state
                .inner
                .lock()
                .unwrap()
                .jobs
                .transition(&id, stage, state, reason)
        };
        let mut reservation = None;
        let mut diagnostics = Vec::new();
        let mut omitted_diagnostic_bytes = 0u64;
        let mut lost_bytes = 0u64;
        let mut lost_chunks = 0u64;
        let mut log_output = |stage, stderr, text: &str| {
            // Process framing already sanitized/redacted this text. Keep a bounded
            // prefix independent of UI polling, including failures before tool exit.
            let line = format!("{stage:?} {}: {text}\n", if stderr { "stderr" } else { "stdout" });
            // Reserve half the evidence budget for post-compilation validation.
            let limit = if stage == StageId::Firmware(FirmwareStage::Validation) {
                256 * 1024usize
            } else {
                128 * 1024usize
            };
            omitted_diagnostic_bytes =
                omitted_diagnostic_bytes.saturating_add(retain_diagnostic(&mut diagnostics, limit, &line) as u64);
            if let Ok(mut inner) = worker_state.inner.try_lock() {
                let _ = inner.jobs.append(
                    &id,
                    LogInput {
                        stage,
                        source: if stderr { Source::Stderr } else { Source::Stdout },
                        text: text.into(),
                        progress: false,
                    },
                );
            } else {
                lost_bytes = lost_bytes.saturating_add(text.len() as u64);
                lost_chunks = lost_chunks.saturating_add(1);
            }
        };
        let execution = (|| -> Result<crate::artifacts::Artifact, String> {
            if !crate::host::host().0 {
                return Err("Unsupported native host".into());
            }
            let converter = crate::artifacts::converter(&mut process, &worker_state.repo)?;
            transition(stage, StageState::Succeeded, None).map_err(|e| e.to_string())?;
            stage = StageId::Firmware(FirmwareStage::Snapshot);
            transition(stage, StageState::Running, None).map_err(|e| e.to_string())?;
            reservation = Some(crate::build_storage::reserve(&worker_state.repo)?);
            let frozen = Arc::new(crate::build_input::freeze_at(
                &worker_state.repo,
                &input,
                &reservation.as_ref().unwrap().path,
            )?);
            let _budget = crate::process::ScratchBudget::new(frozen.attempt_root());
            if crate::process::cancelled() {
                return Err("Build cancelled after snapshot".into());
            }
            transition(stage, StageState::Succeeded, None).map_err(|e| e.to_string())?;
            stage = StageId::Firmware(FirmwareStage::Compilation);
            transition(stage, StageState::Running, None).map_err(|e| e.to_string())?;
            let output = crate::build_input::capture(&mut process, &frozen, &mut |stderr, text| {
                log_output(stage, stderr, text);
            });
            let output = output?;
            if output.code != Some(0) {
                return Err("Firmware capture failed; inspect bounded output".into());
            }
            transition(stage, StageState::Succeeded, None).map_err(|e| e.to_string())?;
            stage = StageId::Firmware(FirmwareStage::Validation);
            transition(stage, StageState::Running, None).map_err(|e| e.to_string())?;
            crate::artifacts::admit(
                &mut process,
                frozen,
                &input,
                worker_state.session.clone(),
                converter,
                &mut |stderr, text| {
                    log_output(stage, stderr, text);
                },
            )
        })();
        if lost_chunks > 0 {
            let _ = worker_state.inner.lock().unwrap().jobs.account_log_loss(
                &id,
                crate::jobs::ingress::LogLoss {
                    bytes: lost_bytes,
                    chunks: lost_chunks,
                    counter_overflow: false,
                },
            );
        }
        if omitted_diagnostic_bytes > 0 {
            diagnostics.extend_from_slice(
                format!(
                    "\n[Retained log truncated: {omitted_diagnostic_bytes} formatted UTF-8 bytes omitted; live job \
                     logs have separate bounds.]\n"
                )
                .as_bytes(),
            );
        }
        let execution = if let Some(reservation) = reservation.as_ref() {
            match reservation.write_diagnostics(&diagnostics) {
                Ok(()) => execution,
                Err(error) => Err(format!("Build diagnostics could not be retained: {error}")),
            }
        } else {
            execution
        };
        let execution = if crate::process::cancelled() {
            drop(execution);
            Err("Cancelled before artifact publication".into())
        } else {
            execution
        };
        let execution = if !crate::process::cleanup_failed() {
            if let Some(mut reservation) = reservation {
                match reservation.close(execution.is_ok(), execution.as_ref().err().map(String::as_str)) {
                    Ok(()) => execution,
                    Err(error) => Err(format!("Build storage quarantined: {error}")),
                }
            } else {
                execution
            }
        } else {
            execution
        };
        let mut inner = worker_state.inner.lock().unwrap();
        if crate::process::cleanup_failed() {
            let _ = inner.jobs.note_cleanup_fatal(&id, true);
            inner.stopping = true;
            return;
        }
        let cancelled = crate::process::cancelled()
            || inner
                .jobs
                .poll(&id, PollRequest { after: 0, limit: 1 })
                .is_ok_and(|b| b.summary.cancel_requested);
        let mut admitted = None;
        let mut artifact_id = None;
        let (state, cause, reason) = match execution {
            Ok(artifact) if !cancelled => {
                artifact_id = Some(artifact.summary.artifact_id.clone());
                admitted = Some(artifact);
                (StageState::Succeeded, TerminalCause::Succeeded, None)
            }
            Ok(_) => {
                (
                    StageState::Cancelled,
                    TerminalCause::Cancelled,
                    Some(Text::new("Cancelled before artifact publication").unwrap()),
                )
            }
            Err(reason) => {
                crate::terminal::event(true, id.as_str(), &reason);
                (
                    if cancelled {
                        StageState::Cancelled
                    } else {
                        StageState::Failed
                    },
                    if cancelled {
                        TerminalCause::Cancelled
                    } else {
                        TerminalCause::Failed
                    },
                    Some(crate::diagnostics::summary(&reason)),
                )
            }
        };
        let transition_ok = inner.jobs.transition(&id, stage, state, reason).is_ok();
        let finished = inner.jobs.finish(&id, cause, process.closure(0, 0), artifact_id, None);
        if transition_ok
            && finished.is_ok()
            && let Some(artifact) = admitted
        {
            if inner.artifacts.len() == 2 {
                inner.artifacts.remove(0);
            }
            inner.artifacts.push(artifact);
        }
    }));
    Ok(result)
}
pub(super) async fn start_flash(
    state: Arc<State>,
    request: FlashRequest,
) -> Result<JobSummary, Error> {
    let mut inner = idle(&state).await?;
    let artifact = inner
        .artifacts
        .iter()
        .find(|a| a.summary.artifact_id == request.artifact_id)
        .cloned()
        .ok_or(Error::NotFoundOrUnavailable)?;
    let device = inner
        .devices
        .devices
        .iter()
        .find(|d| d.device_id == request.device_id)
        .cloned()
        .ok_or(Error::NotFoundOrUnavailable)?;
    let inspection = inner
        .inspections
        .get(&request.device_id)
        .cloned()
        .ok_or(Error::NotFoundOrUnavailable)?;
    crate::flash::admit(&request, &state.session, &artifact, &device, &inspection)?;
    let id = JobId::new(random_hex::<16>().map_err(|_| Error::InvalidState)?)?;
    inner
        .jobs
        .start(id.clone(), OperationKind::Flash, request.retry_of.clone())?;
    inner
        .jobs
        .bind_flash(&id, request.artifact_id.clone(), request.device_id.clone())?;
    inner.jobs.transition(
        &id,
        StageId::Firmware(FirmwareStage::Prerequisites),
        StageState::Running,
        None,
    )?;
    let summary = inner.jobs.poll(&id, PollRequest { after: 0, limit: 1 })?.summary;
    crate::process::reset_cancel();
    let worker_state = state.clone();
    inner.worker = Some(tokio::task::spawn_blocking(move || {
        let mut process = crate::process::ProcessContext::default();
        let mut transition = |stage, status| {
            crate::terminal::event(false, id.as_str(), &format!("{stage:?}: {status:?}"));
            worker_state
                .inner
                .lock()
                .unwrap()
                .jobs
                .transition(&id, StageId::Firmware(stage), status, None)
                .map_err(|e| e.to_string())
        };
        let mut dropped_bytes = 0u64;
        let mut dropped_chunks = 0u64;
        let execution = crate::flash::run(
            &mut process,
            crate::flash::Operation {
                repo: &worker_state.repo,
                session: &worker_state.session,
                job: &id,
                request: &request,
                artifact: &artifact,
                device: &device,
                inspection: &inspection,
            },
            &mut transition,
            &mut |stage, stderr, text| {
                if let Ok(mut inner) = worker_state.inner.try_lock() {
                    let _ = inner.jobs.append(
                        &id,
                        LogInput {
                            stage: StageId::Firmware(stage),
                            source: if stderr { Source::Stderr } else { Source::Stdout },
                            text: text.into(),
                            progress: false,
                        },
                    );
                } else {
                    dropped_bytes = dropped_bytes.saturating_add(text.len() as u64);
                    dropped_chunks = dropped_chunks.saturating_add(1);
                }
            },
        );
        let mut inner = worker_state.inner.lock().unwrap();
        if crate::process::cleanup_failed() {
            let _ = inner.jobs.note_cleanup_fatal(&id, true);
            inner.stopping = true;
            return;
        }
        let cancelled = inner
            .jobs
            .poll(&id, PollRequest { after: 0, limit: 1 })
            .is_ok_and(|b| b.summary.cancel_requested);
        let (cause, exit) = match execution {
            Ok(exit) => {
                (
                    if cancelled {
                        TerminalCause::Cancelled
                    } else {
                        TerminalCause::Succeeded
                    },
                    exit,
                )
            }
            Err(failure) => {
                let reason = failure.reason;
                crate::terminal::event(true, id.as_str(), &reason);
                let stage = inner
                    .jobs
                    .poll(&id, PollRequest { after: 0, limit: 1 })
                    .ok()
                    .and_then(|b| {
                        b.summary
                            .stages
                            .iter()
                            .find(|s| s.state == StageState::Running)
                            .map(|s| s.stage)
                    });
                if let Some(stage) = stage {
                    let _ = inner.jobs.append(
                        &id,
                        LogInput {
                            stage,
                            source: Source::Internal,
                            text: crate::diagnostics::summary::<512>(&reason).as_str().to_owned(),
                            progress: false,
                        },
                    );
                }
                (
                    if cancelled && process.last_failure == Some(crate::process::FailureKind::Cancelled) {
                        TerminalCause::Cancelled
                    } else if process.last_failure == Some(crate::process::FailureKind::Deadline) {
                        TerminalCause::Deadline
                    } else {
                        TerminalCause::Failed
                    },
                    failure.exit,
                )
            }
        };
        let _ = inner
            .jobs
            .finish(&id, cause, process.closure(dropped_bytes, dropped_chunks), None, exit);
        // Every completed attempt invalidates selection evidence; qualification is never sticky.
        inner.inspections.remove(&request.device_id);
    }));
    Ok(summary)
}
pub(super) async fn start_cleanup(
    state: Arc<State>,
    request: CleanupRequest,
    build: bool,
) -> Result<JobSummary, Error> {
    request.validate()?;
    let mut inner = idle(&state).await?;
    // JobStore admission refuses every open job, including stopped-but-unclosed attempts.
    let id = JobId::new(random_hex::<16>().map_err(|_| Error::InvalidState)?)?;
    let summary = inner.jobs.start(id.clone(), OperationKind::Cleanup, None)?;
    let stage = StageId::Cleanup(CleanupStage::Cleanup);
    inner.jobs.transition(&id, stage, StageState::Running, None)?;
    crate::process::reset_cancel();
    let worker_state = state.clone();
    inner.worker = Some(tokio::task::spawn_blocking(move || {
        let process = crate::process::ProcessContext::default();
        let mut failure = None;
        let mut cancelled = false;
        let protected = worker_state
            .inner
            .lock()
            .unwrap()
            .artifacts
            .iter()
            .filter_map(|a| a.lease.as_ref())
            .filter_map(|a| a.frozen.attempt_root().file_name()?.to_str().map(str::to_owned))
            .collect();
        let prepared = if build {
            crate::build_storage::prepare(&worker_state.repo, &request, &protected).map(Some)
        } else {
            Ok(None)
        };
        let mut prepared = match prepared {
            Ok(p) => p.map(Vec::into_iter),
            Err(error) => {
                failure = Some(error);
                None
            }
        };
        for attempt in request.attempts.iter() {
            if failure.is_some() {
                break;
            }
            if crate::process::cancelled() {
                cancelled = true;
                break;
            }
            let result = if let Some(prepared) = prepared.as_mut() {
                prepared
                    .next()
                    .ok_or_else(|| "Missing prepared attempt".to_string())
                    .and_then(|p| p.remove())
            } else {
                crate::flash_cleanup::remove(&worker_state.repo, attempt)
            };
            let text = match result {
                Ok(()) => {
                    format!(
                        "Removed local {} record {}. Device unchanged.",
                        if build { "build storage" } else { "flash" },
                        attempt.as_str()
                    )
                }
                Err(error) => {
                    failure = Some(error);
                    format!(
                        "Could not remove record {}; earlier removals are retained.",
                        attempt.as_str()
                    )
                }
            };
            let _ = worker_state.inner.lock().unwrap().jobs.append(
                &id,
                LogInput {
                    stage,
                    source: Source::Internal,
                    text,
                    progress: false,
                },
            );
            if failure.is_some() {
                break;
            }
        }
        let mut inner = worker_state.inner.lock().unwrap();
        cancelled |= inner
            .jobs
            .poll(&id, PollRequest { after: 0, limit: 1 })
            .is_ok_and(|b| b.summary.cancel_requested);
        let (status, cause, reason) = if let Some(error) = failure {
            (
                StageState::Failed,
                TerminalCause::Failed,
                Some(crate::diagnostics::summary(&error)),
            )
        } else if cancelled {
            (
                StageState::Cancelled,
                TerminalCause::Cancelled,
                Some(Text::new("Cleanup stopped before the next record; completed removals remain").unwrap()),
            )
        } else {
            (StageState::Succeeded, TerminalCause::Succeeded, None)
        };
        let _ = inner.jobs.transition(&id, stage, status, reason);
        let _ = inner.jobs.finish(&id, cause, process.closure(0, 0), None, None);
    }));
    Ok(summary)
}

/// Reap the prior worker before admission; JobStore still rejects any unclosed
/// operation under the returned lock. Never await while holding the owner mutex.
async fn idle(state: &State) -> Result<std::sync::MutexGuard<'_, Inner>, Error> {
    let previous = {
        let mut inner = state.inner.lock().unwrap();
        if inner.worker.as_ref().is_some_and(|w| !w.is_finished()) {
            return Err(Error::Busy);
        }
        inner.worker.take()
    };
    if let Some(previous) = previous {
        previous.await.map_err(|_| Error::InvalidState)?;
    }
    let inner = state.inner.lock().unwrap();
    if inner.stopping || crate::process::cleanup_failed() {
        return Err(Error::InvalidState);
    }
    Ok(inner)
}

#[cfg(test)]
mod diagnostics_tests {
    #[test]
    fn bounded_prefix_reserves_footer_and_validation_space() {
        let mut bytes = Vec::new();
        assert_eq!(super::retain_diagnostic(&mut bytes, 1024, &"x".repeat(512)), 0);
        assert_eq!(super::retain_diagnostic(&mut bytes, 1024, "overflow"), 8);
        assert_eq!(super::retain_diagnostic(&mut bytes, 2048, "validation failure"), 0);
        assert!(std::str::from_utf8(&bytes).unwrap().ends_with("validation failure"));
        let mut unicode = vec![b'x'; 511];
        assert_eq!(super::retain_diagnostic(&mut unicode, 1024, "é"), 2);
        assert_eq!(unicode.len(), 511);
    }
}
