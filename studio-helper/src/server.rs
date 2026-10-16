//! Bounded loopback transport for explicit prerequisite jobs. No build/flash routes.
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::Router;
use axum::body::{Body, HttpBody};
use axum::http::{HeaderMap, HeaderValue, Method, Request, Response, StatusCode};
use dashboard_helper_protocol::*;
use serde_json::{Value, json};

use crate::jobs::logs::{LogInput, StreamFramer};
use crate::jobs::{JobStore, OutputClosure};
const AUTHORITY: &str = "127.0.0.1:8765";
const ORIGINS: [&str; 2] = ["http://127.0.0.1:8080", "http://localhost:8080"];
struct Inner {
    jobs: JobStore,
    cached: Value,
    devices: DeviceScanReport,
    project: Option<crate::build_input::UploadedProject>,
    worker: Option<tokio::task::JoinHandle<()>>,
    stopping: bool,
}
struct State {
    repo: PathBuf,
    session: SessionId,
    capability: String,
    inner: Mutex<Inner>,
}
fn random_hex<const N: usize>() -> Result<String, String> {
    let mut b = [0u8; N];
    getrandom::fill(&mut b).map_err(|_| "OS entropy unavailable")?;
    Ok(b.iter().map(|x| format!("{x:02x}")).collect())
}
impl State {
    fn new(repo: PathBuf) -> Result<Arc<Self>, String> {
        let session = SessionId::new(random_hex::<16>()?).map_err(|_| "invalid session")?;
        Ok(Arc::new(Self {
            repo,
            session: session.clone(),
            capability: random_hex::<32>()?,
            inner: Mutex::new(Inner {
                jobs: JobStore::new(session.clone()),
                devices: crate::devices::empty(session, "Device scan not run"),
                cached: json!({"schema_version":1,"ready":false,"checks":[],"reason":"Check not run"}),
                project: None,
                worker: None,
                stopping: false,
            }),
        }))
    }
}
fn one<'a>(
    headers: &'a HeaderMap,
    name: &str,
) -> Option<&'a str> {
    let mut values = headers.get_all(name).iter();
    let first = values.next()?.to_str().ok()?;
    if values.next().is_some() { None } else { Some(first) }
}
fn equal_capability(
    actual: &str,
    expected: &str,
) -> bool {
    actual.len() == 64
        && actual.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        && actual.bytes().zip(expected.bytes()).fold(0u8, |d, (a, b)| d | (a ^ b)) == 0
}
fn response(
    status: StatusCode,
    value: Value,
    origin: Option<&str>,
) -> Response<Body> {
    let mut response = Response::builder()
        .status(status)
        .header("content-type", "application/json; charset=utf-8")
        .header("cache-control", "no-store")
        .header("x-content-type-options", "nosniff")
        .header("referrer-policy", "no-referrer")
        .header("content-security-policy", "default-src 'none'; frame-ancestors 'none'")
        .header("connection", "close")
        .body(Body::from(value.to_string()))
        .unwrap();
    if let Some(origin) = origin {
        response
            .headers_mut()
            .insert("access-control-allow-origin", HeaderValue::from_str(origin).unwrap());
        response
            .headers_mut()
            .insert("vary", HeaderValue::from_static("Origin"));
    }
    response
}
fn error(
    status: StatusCode,
    code: &str,
    origin: Option<&str>,
) -> Response<Body> {
    response(status, json!({"error":code}), origin)
}
fn store_error(
    e: Error,
    origin: Option<&str>,
) -> Response<Body> {
    let status = match e {
        Error::Busy | Error::Closed | Error::InvalidState => StatusCode::CONFLICT,
        Error::Expired => StatusCode::GONE,
        Error::NotFoundOrUnavailable => StatusCode::NOT_FOUND,
        _ => StatusCode::BAD_REQUEST,
    };
    error(status, &e.to_string(), origin)
}
fn poll_query(query: Option<&str>) -> Result<PollRequest, Error> {
    let mut after = None;
    let mut limit = None;
    for pair in query.ok_or(Error::Schema)?.split('&') {
        let (k, v) = pair.split_once('=').ok_or(Error::Schema)?;
        if v.is_empty() || !v.bytes().all(|b| b.is_ascii_digit()) {
            return Err(Error::Schema);
        }
        match k {
            "after" if after.is_none() => after = Some(v.parse().map_err(|_| Error::Schema)?),
            "limit" if limit.is_none() => limit = Some(v.parse().map_err(|_| Error::Schema)?),
            _ => return Err(Error::Schema),
        }
    }
    let p = PollRequest {
        after: after.ok_or(Error::Schema)?,
        limit: limit.ok_or(Error::Schema)?,
    };
    p.validate()?;
    Ok(p)
}
async fn bounded_body(
    mut body: Body,
    maximum: usize,
) -> Result<Vec<u8>, ()> {
    tokio::time::timeout(Duration::from_secs(10), async {
        let mut bytes = Vec::new();
        while let Some(frame) = std::future::poll_fn(|cx| std::pin::Pin::new(&mut body).poll_frame(cx)).await {
            let frame = frame.map_err(|_| ())?;
            let data = frame.data_ref().ok_or(())?;
            if data.len() > maximum.saturating_sub(bytes.len()) {
                return Err(());
            }
            bytes.extend_from_slice(data);
        }
        Ok(bytes)
    })
    .await
    .map_err(|_| ())?
}
async fn empty_body(mut body: Body) -> bool {
    tokio::time::timeout(Duration::from_secs(10), async {
        while let Some(frame) = std::future::poll_fn(|cx| std::pin::Pin::new(&mut body).poll_frame(cx)).await {
            match frame {
                Ok(f) if f.data_ref().is_some_and(|b| b.is_empty()) => {}
                _ => return false,
            }
        }
        true
    })
    .await
    .unwrap_or(false)
}
fn summary(
    state: &State,
    id: &JobId,
) -> Result<JobSummary, Error> {
    state
        .inner
        .lock()
        .unwrap()
        .jobs
        .poll(id, PollRequest { after: 0, limit: 1 })
        .map(|b| b.summary)
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
async fn start_check(
    state: Arc<State>,
    device_scan: bool,
) -> Result<JobSummary, Error> {
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
    let mut inner = state.inner.lock().unwrap();
    if inner.stopping || crate::process::cleanup_failed() {
        return Err(Error::InvalidState);
    }
    let id = JobId::new(random_hex::<16>().map_err(|_| Error::InvalidState)?)?;
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
            text: if device_scan {
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
        let report = if device_scan {
            serde_json::to_value(crate::devices::scan(&worker_state.repo, worker_state.session.clone())).unwrap()
        } else {
            crate::prerequisites::check(&worker_state.repo)
        };
        let mut inner = worker_state.inner.lock().unwrap();
        if device_scan {
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
        let _ = inner.jobs.finish(
            &id,
            cause,
            OutputClosure {
                workers_joined: true,
                tree_empty: true,
                output_incomplete: false,
                discarded_bytes: 0,
                discarded_chunks: 0,
            },
            None,
            None,
        );
    }));
    Ok(result)
}
async fn start_build(
    state: Arc<State>,
    request: ProjectBuildRequest,
) -> Result<JobSummary, Error> {
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
    let mut inner = state.inner.lock().unwrap();
    if inner.stopping || crate::process::cleanup_failed() {
        return Err(Error::InvalidState);
    }
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
        let execution = (|| -> Result<(), String> {
            if !crate::host::host().0 {
                return Err("Unsupported native host".into());
            }
            transition(stage, StageState::Succeeded, None).map_err(|e| e.to_string())?;
            stage = StageId::Firmware(FirmwareStage::Snapshot);
            transition(stage, StageState::Running, None).map_err(|e| e.to_string())?;
            let frozen = crate::build_input::freeze(&worker_state.repo, &input)?;
            if crate::process::cancelled() {
                return Err("Build cancelled after snapshot".into());
            }
            transition(stage, StageState::Succeeded, None).map_err(|e| e.to_string())?;
            stage = StageId::Firmware(FirmwareStage::Compilation);
            transition(stage, StageState::Running, None).map_err(|e| e.to_string())?;
            let output = crate::build_input::capture(&frozen)?;
            for (source, text) in [(Source::Stdout, output.stdout), (Source::Stderr, output.stderr)] {
                // Runner already sanitized/bounded these fragments. Limit retained job history in JobStore.
                let mut inner = worker_state.inner.lock().unwrap();
                for line in text.lines() {
                    let _ = inner.jobs.append(
                        &id,
                        LogInput {
                            stage,
                            source,
                            text: line.chars().take(256).collect(),
                            progress: false,
                        },
                    );
                }
            }
            if output.code != Some(0) {
                return Err("Firmware capture failed; inspect bounded output".into());
            }
            transition(stage, StageState::Succeeded, None).map_err(|e| e.to_string())?;
            stage = StageId::Firmware(FirmwareStage::Validation);
            transition(stage, StageState::Running, None).map_err(|e| e.to_string())?;
            Err(
                "Capture completed. Native artifact admission and UF2 equality are not implemented; no flash-ready \
                 artifact was issued"
                    .into(),
            )
        })();
        let mut inner = worker_state.inner.lock().unwrap();
        if crate::process::cleanup_failed() {
            let _ = inner.jobs.note_cleanup_fatal(&id, true);
            inner.stopping = true;
            return;
        }
        let cancelled = crate::process::cancelled();
        let reason = execution.err().unwrap_or_else(|| "Artifact not admitted".into());
        crate::terminal::event(true, id.as_str(), &reason);
        let _ = inner.jobs.transition(
            &id,
            stage,
            if cancelled {
                StageState::Cancelled
            } else {
                StageState::Failed
            },
            Some(Text::new(reason.chars().take(120).collect::<String>()).unwrap()),
        );
        let _ = inner.jobs.finish(
            &id,
            if cancelled {
                TerminalCause::Cancelled
            } else {
                TerminalCause::Failed
            },
            OutputClosure {
                workers_joined: true,
                tree_empty: true,
                output_incomplete: false,
                discarded_bytes: 0,
                discarded_chunks: 0,
            },
            None,
            None,
        );
    }));
    Ok(result)
}
async fn dispatch(
    state: Arc<State>,
    request: Request<Body>,
) -> Response<Body> {
    let headers = request.headers();
    let origin = one(headers, "origin").filter(|o| ORIGINS.contains(o));
    if one(headers, "host") != Some(AUTHORITY)
        || origin.is_none()
        || request.uri().scheme().is_some()
        || request.uri().authority().is_some()
        || request.uri().to_string().len() > 2048
        || headers.contains_key("forwarded")
        || headers.contains_key("x-forwarded-host")
        || headers.contains_key("content-encoding")
    {
        return error(StatusCode::FORBIDDEN, "Forbidden", None);
    }
    let origin = origin.unwrap().to_owned();
    let origin = Some(origin.as_str());
    let path = request.uri().path().to_owned();
    let method = request.method().clone();
    if method == Method::OPTIONS {
        let requested = one(headers, "access-control-request-method");
        let allowed_headers = one(headers, "access-control-request-headers").is_some_and(|v| {
            v.split(',').all(|h| {
                matches!(
                    h.trim().to_ascii_lowercase().as_str(),
                    "x-studio-connect" | "x-studio-capability" | "content-type"
                )
            })
        });
        if !matches!(requested, Some("GET" | "POST")) || !allowed_headers || request.uri().query().is_some() {
            return error(StatusCode::FORBIDDEN, "Forbidden", origin);
        }
        let mut r = response(StatusCode::NO_CONTENT, json!(null), origin);
        *r.body_mut() = Body::empty();
        r.headers_mut()
            .insert("access-control-allow-methods", HeaderValue::from_static("GET, POST"));
        r.headers_mut().insert(
            "access-control-allow-headers",
            HeaderValue::from_static("X-Studio-Connect, X-Studio-Capability, Content-Type"),
        );
        return r;
    }
    let connecting = path == "/api/v1/connect" && method == Method::POST;
    if connecting {
        if one(headers, "x-studio-connect") != Some("1") {
            return error(StatusCode::FORBIDDEN, "Forbidden", origin);
        }
    } else if !one(headers, "x-studio-capability").is_some_and(|c| equal_capability(c, &state.capability)) {
        return error(StatusCode::FORBIDDEN, "Forbidden", origin);
    }
    if method != Method::GET && method != Method::POST {
        return error(StatusCode::METHOD_NOT_ALLOWED, "MethodNotAllowed", origin);
    }
    let query = request.uri().query().map(str::to_owned);
    if !path.ends_with("/events") && query.is_some() {
        return error(StatusCode::BAD_REQUEST, "UnexpectedQuery", origin);
    }
    if headers.contains_key("transfer-encoding") && one(headers, "transfer-encoding") != Some("chunked") {
        return error(StatusCode::BAD_REQUEST, "InvalidFraming", origin);
    }
    if method == Method::POST && path == "/api/v1/projects" {
        if one(headers, "content-type") != Some("application/octet-stream") {
            return error(StatusCode::UNSUPPORTED_MEDIA_TYPE, "ExpectedBinaryProject", origin);
        }
        if method == Method::POST && path == "/api/v1/builds" {
            if one(headers, "content-type") != Some("application/json") {
                return error(StatusCode::UNSUPPORTED_MEDIA_TYPE, "ExpectedJson", origin);
            }
            if one(headers, "content-length").is_some_and(|v| v.parse::<usize>().map_or(true, |n| n > REQUEST_BYTES)) {
                return error(StatusCode::PAYLOAD_TOO_LARGE, "RequestBound", origin);
            }
            let bytes = match bounded_body(request.into_body(), REQUEST_BYTES).await {
                Ok(b) => b,
                Err(_) => return error(StatusCode::PAYLOAD_TOO_LARGE, "RequestBound", origin),
            };
            let request = match decode::<ProjectBuildRequest>(&bytes) {
                Ok(r) => r,
                Err(e) => return store_error(e, origin),
            };
            return match start_build(state.clone(), request).await {
                Ok(job) => response(StatusCode::ACCEPTED, serde_json::to_value(job).unwrap(), origin),
                Err(e) => store_error(e, origin),
            };
        }
        if one(headers, "content-length").is_some_and(|v| v.parse::<usize>().map_or(true, |n| n > PROJECT_UPLOAD_BYTES))
        {
            return error(StatusCode::PAYLOAD_TOO_LARGE, "ProjectBound", origin);
        }
        let bytes = match bounded_body(request.into_body(), PROJECT_UPLOAD_BYTES).await {
            Ok(b) => b,
            Err(_) => return error(StatusCode::PAYLOAD_TOO_LARGE, "ProjectBodyBound", origin),
        };
        let mut inner = state.inner.lock().unwrap();
        if inner.stopping || inner.worker.as_ref().is_some_and(|w| !w.is_finished()) {
            return store_error(Error::Busy, origin);
        }
        let upload = match crate::build_input::UploadedProject::admit(state.session.clone(), bytes) {
            Ok(p) => p,
            Err(_) => return error(StatusCode::BAD_REQUEST, "InvalidConvertedProject", origin),
        };
        let result = serde_json::to_value(&upload.response).unwrap();
        inner.project = Some(upload);
        return response(StatusCode::CREATED, result, origin);
    }
    if one(headers, "content-length").is_some_and(|l| l != "0") {
        return error(StatusCode::BAD_REQUEST, "ExpectedEmptyBody", origin);
    }
    if !empty_body(request.into_body()).await {
        return error(StatusCode::BAD_REQUEST, "ExpectedEmptyBody", origin);
    }
    if connecting {
        return response(
            StatusCode::OK,
            json!({"protocol_version":1,"session_id":state.session,"capability":state.capability}),
            origin,
        );
    }
    match (method.as_str(), path.as_str()) {
        ("GET", "/api/v1/session") => {
            response(
                StatusCode::OK,
                json!({"protocol_version":1,"session_id":state.session,"ready":false,"reason":"Checks, device discovery and project capture available; artifact admission and flashing unavailable"}),
                origin,
            )
        }
        ("GET", "/api/v1/prerequisites") => {
            response(StatusCode::OK, state.inner.lock().unwrap().cached.clone(), origin)
        }
        ("GET", "/api/v1/devices") => {
            response(
                StatusCode::OK,
                serde_json::to_value(&state.inner.lock().unwrap().devices).unwrap(),
                origin,
            )
        }
        ("POST", "/api/v1/devices/refresh") => {
            match start_check(state.clone(), true).await {
                Ok(job) => response(StatusCode::ACCEPTED, serde_json::to_value(job).unwrap(), origin),
                Err(e) => store_error(e, origin),
            }
        }
        ("GET", "/api/v1/jobs") => {
            response(
                StatusCode::OK,
                serde_json::to_value(state.inner.lock().unwrap().jobs.summaries()).unwrap(),
                origin,
            )
        }
        ("POST", "/api/v1/prerequisites/check") => {
            match start_check(state.clone(), false).await {
                Ok(job) => response(StatusCode::ACCEPTED, serde_json::to_value(job).unwrap(), origin),
                Err(e) => store_error(e, origin),
            }
        }
        _ => {
            let parts: Vec<_> = path.split('/').collect();
            if parts.len() != 6 || parts[1] != "api" || parts[2] != "v1" || parts[3] != "jobs" {
                return error(StatusCode::NOT_FOUND, "NotFoundOrUnavailable", origin);
            }
            let id = match JobId::new(parts[4]) {
                Ok(id) => id,
                Err(e) => return store_error(e, origin),
            };
            match (method.as_str(), parts[5]) {
                ("GET", "events") => {
                    match poll_query(query.as_deref()).and_then(|p| state.inner.lock().unwrap().jobs.poll(&id, p)) {
                        Ok(batch) => response(StatusCode::OK, serde_json::to_value(batch).unwrap(), origin),
                        Err(e) => store_error(e, origin),
                    }
                }
                ("POST", "cancel") => {
                    let result = state.inner.lock().unwrap().jobs.cancel(&id);
                    if let Err(e) = result {
                        return store_error(e, origin);
                    }
                    crate::process::request_cancel();
                    match summary(&state, &id) {
                        Ok(job) => response(StatusCode::OK, serde_json::to_value(job).unwrap(), origin),
                        Err(e) => store_error(e, origin),
                    }
                }
                _ => error(StatusCode::NOT_FOUND, "NotFoundOrUnavailable", origin),
            }
        }
    }
}
pub fn serve(repo: PathBuf) -> Result<(), String> {
    let (supported, host) = crate::host::host();
    if !supported {
        return Err(format!("Unsupported native host: {host}"));
    }
    let repo = repo.canonicalize().map_err(|_| "Repository not found")?;
    if !repo.join("studio-helper/Cargo.toml").is_file() {
        return Err("Not a Studio repository".into());
    }
    let state = State::new(repo)?;
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?
        .block_on(serve_async(state))
}
async fn serve_async(state: Arc<State>) -> Result<(), String> {
    let listener = tokio::net::TcpListener::bind(AUTHORITY)
        .await
        .map_err(|e| format!("Could not bind {AUTHORITY}: {e}"))?;
    eprintln!(
        "Studio helper listening at http://{AUTHORITY}; use Studio's Connect button on port 8080. No operation starts \
         on connect."
    );
    let router = Router::new().fallback({
        let state = state.clone();
        move |request: Request<Body>| dispatch(state.clone(), request)
    });
    let permits = Arc::new(tokio::sync::Semaphore::new(32));
    let mut connections = tokio::task::JoinSet::new();
    let mut listener_error = None;
    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => break,
            _ = tokio::time::sleep(Duration::from_millis(100)) => {},
            _ = connections.join_next(), if !connections.is_empty() => {},
            accepted = listener.accept() => {
                let (socket, _) = match accepted {
                    Ok(socket) => socket,
                    Err(e) => { listener_error = Some(e.to_string()); break; }
                };
                let Ok(permit) = permits.clone().try_acquire_owned() else { drop(socket); continue; };
                let service = hyper_util::service::TowerToHyperService::new(router.clone());
                connections.spawn(async move {
                    let _permit = permit;
                    let mut http = hyper::server::conn::http1::Builder::new();
                    http.timer(hyper_util::rt::TokioTimer::new())
                        .header_read_timeout(Duration::from_secs(10))
                        .max_headers(64).max_header_size(16384).max_buf_size(16384)
                        .keep_alive(false).half_close(false);
                    let connection = http.serve_connection(hyper_util::rt::TokioIo::new(socket), service);
                    let _ = tokio::time::timeout(Duration::from_secs(30), connection).await;
                });
            }
        }
        if state.inner.lock().unwrap().stopping {
            break;
        }
    }
    drop(listener);
    let worker = {
        let mut inner = state.inner.lock().unwrap();
        inner.stopping = true;
        for job in inner.jobs.summaries() {
            if !job.events_closed {
                let _ = inner.jobs.cancel(&job.job_id);
            }
        }
        crate::process::request_cancel();
        inner.worker.take()
    };
    connections.abort_all();
    while connections.join_next().await.is_some() {}
    if let Some(worker) = worker {
        worker.await.map_err(|_| "Check worker failed")?;
    }
    if crate::process::cleanup_failed() {
        Err("cleanup fatal: probe cleanup failed".into())
    } else if let Some(e) = listener_error {
        Err(e)
    } else {
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn project_upload_is_bounded_and_replacement_gets_a_new_token() {
        let state = State::new(std::env::current_dir().unwrap()).unwrap();
        let config = dashboard_config::validate(dashboard_config::default_config()).unwrap();
        let bytes = dashboard_config_tools::project::Project::new(&config, Default::default(), None)
            .unwrap()
            .encode(dashboard_config_tools::project::ExportMode::ConvertedOnly)
            .unwrap();
        let mut ids = Vec::new();
        for _ in 0..2 {
            let req = Request::builder()
                .uri("/api/v1/projects")
                .method("POST")
                .header("host", AUTHORITY)
                .header("origin", ORIGINS[0])
                .header("x-studio-capability", &state.capability)
                .header("content-type", "application/octet-stream")
                .body(Body::from(bytes.clone()))
                .unwrap();
            assert_eq!(dispatch(state.clone(), req).await.status(), StatusCode::CREATED);
            ids.push(
                state
                    .inner
                    .lock()
                    .unwrap()
                    .project
                    .as_ref()
                    .unwrap()
                    .response
                    .project_id
                    .clone(),
            );
        }
        assert_ne!(ids[0], ids[1]);
        let req = Request::builder()
            .uri("/api/v1/projects")
            .method("POST")
            .header("host", AUTHORITY)
            .header("origin", ORIGINS[0])
            .header("x-studio-capability", &state.capability)
            .header("content-type", "application/octet-stream")
            .body(Body::from(vec![0; PROJECT_UPLOAD_BYTES + 1]))
            .unwrap();
        assert_eq!(
            dispatch(state.clone(), req).await.status(),
            StatusCode::PAYLOAD_TOO_LARGE
        );
        assert_eq!(
            state
                .inner
                .lock()
                .unwrap()
                .project
                .as_ref()
                .unwrap()
                .response
                .project_id,
            ids[1]
        );
    }
    #[tokio::test]
    async fn build_rejects_stale_token_and_closes_snapshot_failure_without_artifact() {
        let state = State::new(std::env::temp_dir().join("studio-definitely-not-a-repository")).unwrap();
        let config = dashboard_config::validate(dashboard_config::default_config()).unwrap();
        let bytes = dashboard_config_tools::project::Project::new(&config, Default::default(), None)
            .unwrap()
            .encode(dashboard_config_tools::project::ExportMode::ConvertedOnly)
            .unwrap();
        let project = crate::build_input::UploadedProject::admit(state.session.clone(), bytes).unwrap();
        let mut request = ProjectBuildRequest {
            editor_revision: 7,
            project_id: project.response.project_id.clone(),
            project_sha256: project.response.project_sha256.clone(),
            options: BuildOptions::default(),
            retry_of: None,
        };
        state.inner.lock().unwrap().project = Some(project);
        request.project_sha256 = Sha256::new("0".repeat(64)).unwrap();
        assert_eq!(
            start_build(state.clone(), request.clone()).await.unwrap_err(),
            Error::Schema
        );
        request.project_sha256 = state
            .inner
            .lock()
            .unwrap()
            .project
            .as_ref()
            .unwrap()
            .response
            .project_sha256
            .clone();
        let job = start_build(state.clone(), request).await.unwrap();
        let worker = state.inner.lock().unwrap().worker.take().unwrap();
        worker.await.unwrap();
        let result = summary(&state, &job.job_id).unwrap();
        assert!(result.events_closed);
        assert!(result.artifact_id.is_none());
    }
    #[tokio::test]
    async fn rejects_foreign_origin_and_missing_capability() {
        let state = State::new(std::env::current_dir().unwrap()).unwrap();
        let bad = Request::builder()
            .uri("/api/v1/connect")
            .method("POST")
            .header("host", "127.0.0.1:8765")
            .header("origin", "https://evil.example")
            .header("x-studio-connect", "1")
            .body(Body::empty())
            .unwrap();
        assert_eq!(dispatch(state.clone(), bad).await.status(), StatusCode::FORBIDDEN);
        let missing = Request::builder()
            .uri("/api/v1/jobs")
            .header("host", "127.0.0.1:8765")
            .header("origin", ORIGINS[0])
            .body(Body::empty())
            .unwrap();
        assert_eq!(dispatch(state, missing).await.status(), StatusCode::FORBIDDEN);
    }
    #[tokio::test]
    async fn connect_is_explicit_and_does_not_start_jobs() {
        let state = State::new(std::env::current_dir().unwrap()).unwrap();
        let connect = Request::builder()
            .uri("/api/v1/connect")
            .method("POST")
            .header("host", "127.0.0.1:8765")
            .header("origin", ORIGINS[1])
            .header("x-studio-connect", "1")
            .body(Body::empty())
            .unwrap();
        let response = dispatch(state.clone(), connect).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert!(state.inner.lock().unwrap().jobs.summaries().is_empty());
        assert_eq!(response.headers()["cache-control"], "no-store");
        assert_eq!(response.headers()["access-control-allow-origin"], ORIGINS[1]);
    }
    #[tokio::test]
    async fn duplicate_authority_query_and_restart_tokens_are_rejected() {
        let state = State::new(std::env::current_dir().unwrap()).unwrap();
        let restarted = State::new(std::env::current_dir().unwrap()).unwrap();
        assert_ne!(state.capability, restarted.capability);
        for uri in ["/api/v1/jobs?token=secret", "http://127.0.0.1:8765/api/v1/jobs"] {
            let r = Request::builder()
                .uri(uri)
                .header("host", AUTHORITY)
                .header("origin", ORIGINS[0])
                .header("x-studio-capability", &state.capability)
                .body(Body::empty())
                .unwrap();
            assert!(!dispatch(state.clone(), r).await.status().is_success());
        }
        let r = Request::builder()
            .uri("/api/v1/jobs")
            .header("host", AUTHORITY)
            .header("host", AUTHORITY)
            .header("origin", ORIGINS[0])
            .header("x-studio-capability", &state.capability)
            .body(Body::empty())
            .unwrap();
        assert_eq!(dispatch(state.clone(), r).await.status(), StatusCode::FORBIDDEN);
        let r = Request::builder()
            .uri("/api/v1/jobs")
            .header("host", AUTHORITY)
            .header("origin", ORIGINS[0])
            .header("x-studio-capability", &state.capability)
            .body(Body::empty())
            .unwrap();
        assert_eq!(dispatch(restarted, r).await.status(), StatusCode::FORBIDDEN);
        assert!(poll_query(Some("after=9007199254740993&limit=128")).is_ok());
        assert!(poll_query(Some("after=0&after=1&limit=128")).is_err());
        assert!(poll_query(Some("after=0&limit=129")).is_err());
    }
}
