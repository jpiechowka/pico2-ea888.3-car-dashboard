//! Serialized worker admission and operation completion. Transfer cancellation,
//! artifact publication and cleanup failure each keep their operation-specific rules.
use super::*;
/// Quarantines `state` after a worker panic and marks open jobs cleanup-fatal when the owner lock remains usable.
/// Leaves status service running and does not fabricate producer closure; terminal stderr failure can panic.
fn worker_fault(state: &State) {
    state.fatal.store(true, std::sync::atomic::Ordering::SeqCst);
    crate::terminal::event(
        true,
        "worker",
        "Worker panicked; ownership unproven, operations disabled",
    );
    // A poisoned owner is not safe to resume. Session status remains available
    // independently; stateful routes report unavailable instead of mutating it.
    if let Ok(mut inner) = state.inner.lock() {
        for job in inner.jobs.summaries() {
            if !job.events_closed {
                let _ = inner.jobs.note_cleanup_fatal(&job.job_id, true);
            }
        }
    }
}
/// Returns a blocking-task handle running `work` for `state`, catching unwind to latch ownership failure.
/// Started work is not abortable by dropping/aborting the handle; callers retain its admission permit and join it.
/// Panics without a Tokio runtime; process aborts and a panic in fault reporting are not caught by the inner boundary.
fn spawn_worker(
    state: &Arc<State>,
    work: impl FnOnce() + Send + 'static,
) -> tokio::task::JoinHandle<()> {
    let state = state.clone();
    tokio::task::spawn_blocking(move || {
        if std::panic::catch_unwind(std::panic::AssertUnwindSafe(work)).is_err() {
            worker_fault(&state);
        }
    })
}
/// Appends a UTF-8 prefix of `line` to `bytes` within `limit` minus a 512-byte footer reserve.
/// Returns omitted input bytes; never truncates existing bytes when they already exceed the available prefix allowance.
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

/// Sanitizes prerequisite `report` rows into bounded internal logs for `id` in `store`.
/// Missing fields use display defaults; framing/append failures are deliberately ignored and do not alter report
/// readiness.
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
/// Returns admitted job summary for `state`: `device_scan` chooses descriptor scanning, otherwise prerequisite probes;
/// `selected` additionally requests inspection and must be paired with device_scan by the private caller.
/// Returns admission/selection/job errors before launch; inspection requires an exact cached candidate and clean
/// complete scan. The worker retains its permit after HTTP cancellation, publishes bounded reports and closes only
/// after owned-process cleanup. Cancelled inspection clears facts/digest; a new scan clears all prior inspections.
/// Cleanup failure leaves the job unclosed.
pub(super) async fn start_check(
    state: Arc<State>,
    device_scan: bool,
    selected: Option<DeviceSummary>,
) -> Result<JobSummary, Error> {
    let permit = state.reserve()?;
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
    inner.worker = Some(spawn_worker(&state, move || {
        let _permit = permit;
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
            worker_state.fatal.store(true, std::sync::atomic::Ordering::SeqCst);
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
/// Returns a newly admitted Build summary after validating `request` against `state`'s current uploaded
/// project/session. Returns Busy/ownership, missing upload, stale/schema or job errors; upload decoding runs off-thread
/// with the permit retained. Worker freezes source, compiles, validates and closes owned storage before publishing one
/// of at most two artifacts. Cancellation suppresses artifact publication; failures retain diagnostics and may
/// quarantine partial storage, never roll back completed filesystem writes. Worker panic/cleanup failure disables
/// further operations without claiming closure.
pub(super) async fn start_build(
    state: Arc<State>,
    request: ProjectBuildRequest,
) -> Result<JobSummary, Error> {
    let permit = state.reserve()?;
    let project = {
        let inner = idle(&state).await?;
        inner.project.clone().ok_or(Error::NotFoundOrUnavailable)?
    };
    let session = state.session.clone();
    let acceptance_request = request.clone();
    let (input, permit) =
        tokio::task::spawn_blocking(move || (project.accepted(&session, &acceptance_request), permit))
            .await
            .map_err(|_| Error::InvalidState)?;
    let input = input.map_err(|_| Error::Schema)?;
    let mut inner = idle(&state).await?;
    let id = JobId::new(random_hex::<16>().map_err(|_| Error::InvalidState)?)?;
    let result = inner.jobs.start(id.clone(), OperationKind::Build, request.retry_of)?;
    crate::terminal::event(false, id.as_str(), "Accepted project build started");
    let prerequisite = StageId::Firmware(FirmwareStage::Prerequisites);
    inner.jobs.transition(&id, prerequisite, StageState::Running, None)?;
    crate::process::reset_cancel();
    let worker_state = state.clone();
    inner.worker = Some(spawn_worker(&state, move || {
        let _permit = permit;
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
            worker_state.fatal.store(true, std::sync::atomic::Ordering::SeqCst);
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
/// Returns an admitted Flash summary for explicit `request` against `state`'s retained artifact/device/inspection
/// evidence. Returns admission, unavailable selection, consent/identity or job errors before launching; worker
/// revalidates before writing. Transfer cancellation is deferred to safe command boundaries; failure may leave
/// programmed flash and never triggers retry. Completed attempts retire device inspection; cleanup-fatal work stays
/// open/quarantined and does not certify firmware execution.
pub(super) async fn start_flash(
    state: Arc<State>,
    request: FlashRequest,
) -> Result<JobSummary, Error> {
    let permit = state.reserve()?;
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
    inner.worker = Some(spawn_worker(&state, move || {
        let _permit = permit;
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
            worker_state.fatal.store(true, std::sync::atomic::Ordering::SeqCst);
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
/// Returns an admitted cleanup summary for validated `request` in `state`; `build` chooses build attempts, false flash
/// history. Returns activity/ownership/request/job errors before launch. Build cleanup preflights the whole batch and
/// protects held artifacts; flash history validates each attempt when reached. Cancellation is checked between
/// attempts; earlier removals remain on failure.
pub(super) async fn start_cleanup(
    state: Arc<State>,
    request: CleanupRequest,
    build: bool,
) -> Result<JobSummary, Error> {
    let permit = state.reserve()?;
    request.validate()?;
    let mut inner = idle(&state).await?;
    // JobStore admission refuses every open job, including stopped-but-unclosed attempts.
    let id = JobId::new(random_hex::<16>().map_err(|_| Error::InvalidState)?)?;
    let summary = inner.jobs.start(id.clone(), OperationKind::Cleanup, None)?;
    let stage = StageId::Cleanup(CleanupStage::Cleanup);
    inner.jobs.transition(&id, stage, StageState::Running, None)?;
    crate::process::reset_cancel();
    let worker_state = state.clone();
    inner.worker = Some(spawn_worker(&state, move || {
        let _permit = permit;
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

/// Returns `state`'s owner guard after reaping a finished prior worker; returns Busy if it still runs, InvalidState for
/// shutdown/poison/cleanup/worker faults. JobStore separately rejects any unclosed operation under the returned lock.
/// Never awaits with the mutex held; dropping this future while awaiting detaches the finished handle, not native work.
async fn idle(state: &State) -> Result<std::sync::MutexGuard<'_, Inner>, Error> {
    if state.fatal.load(std::sync::atomic::Ordering::SeqCst) || state.inner.is_poisoned() {
        return Err(Error::InvalidState);
    }
    let previous = {
        let mut inner = state.inner.lock().unwrap();
        if inner.worker.as_ref().is_some_and(|w| !w.is_finished()) {
            return Err(Error::Busy);
        }
        inner.worker.take()
    };
    if let Some(previous) = previous
        && previous.await.is_err()
    {
        worker_fault(state);
        return Err(Error::InvalidState);
    }
    let inner = state.inner.lock().unwrap();
    if inner.stopping || state.fatal.load(std::sync::atomic::Ordering::SeqCst) || crate::process::cleanup_failed() {
        return Err(Error::InvalidState);
    }
    Ok(inner)
}

#[cfg(test)]
/// Worker failure visibility, poisoned-owner route rejection and bounded retained-diagnostic regressions.
mod diagnostics_tests {
    use super::*;
    #[tokio::test]
    /// A panicked task latches visible ownership failure without claiming event closure or stopping status service.
    async fn failed_worker_preserves_visible_ownership_fault() {
        let state = State::new(std::env::current_dir().unwrap()).unwrap();
        let id = JobId::new("f".repeat(32)).unwrap();
        {
            let mut inner = state.inner.lock().unwrap();
            inner.jobs.start(id.clone(), OperationKind::Build, None).unwrap();
            let worker = tokio::task::spawn_blocking(|| panic!("injected worker failure"));
            while !worker.is_finished() {
                std::thread::yield_now();
            }
            inner.worker = Some(worker);
        }
        assert!(idle(&state).await.is_err());
        let job = summary(&state, &id).unwrap();
        assert_eq!(job.terminal_cause, Some(TerminalCause::OwnershipCleanupFatal));
        assert!(job.output_incomplete);
        assert!(!job.events_closed, "unknown ownership must not fabricate closure");
        assert!(
            !state.inner.lock().unwrap().stopping,
            "status service must remain available"
        );
    }
    #[tokio::test]
    /// Owner poisoning preserves independent session status while rejecting stateful reads and mutations.
    async fn poisoned_worker_keeps_session_status_and_refuses_mutation() {
        let state = State::new(std::env::current_dir().unwrap()).unwrap();
        let worker_state = state.clone();
        spawn_worker(&state, move || {
            let _owner = worker_state.inner.lock().unwrap();
            panic!("injected owner poisoning");
        })
        .await
        .unwrap();
        assert!(state.inner.is_poisoned());
        assert!(idle(&state).await.is_err());
        for (method, path, expected) in [
            ("GET", "/api/v1/session", StatusCode::OK),
            ("GET", "/api/v1/jobs", StatusCode::SERVICE_UNAVAILABLE),
            ("POST", "/api/v1/devices/refresh", StatusCode::SERVICE_UNAVAILABLE),
        ] {
            let request = Request::builder()
                .method(method)
                .uri(path)
                .header("host", AUTHORITY)
                .header("origin", ORIGINS[0])
                .header("x-studio-capability", &state.capability)
                .body(Body::empty())
                .unwrap();
            assert_eq!(dispatch(state.clone(), request).await.status(), expected);
        }
    }
    #[test]
    /// Retained logs reserve footer/validation space and never split a UTF-8 scalar at the prefix bound.
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
