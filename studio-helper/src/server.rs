//! Bounded authenticated loopback transport for explicit helper actions.
//! Browser origin/preflight handling follows the [Fetch CORS protocol](https://fetch.spec.whatwg.org/#http-cors-protocol);
//! the session capability is additionally required for operations and is never accepted from a URL.
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::{HeaderMap, HeaderValue, Method, Request, Response, StatusCode};
use dashboard_helper_protocol::*;
use serde_json::{Value, json};

use crate::jobs::JobStore;
use crate::jobs::logs::{LogInput, StreamFramer};
mod http;
mod operations;
use http::*;
use operations::*;

/// Fixed loopback listener and exact accepted Host authority.
const AUTHORITY: &str = "127.0.0.1:8765";
/// Explicit Studio development origins; no wildcard or alternate ports.
const ORIGINS: [&str; 2] = ["http://127.0.0.1:8080", "http://localhost:8080"];
/// Short-held serialized status/cache owner; filesystem and process work must run outside this mutex.
struct Inner {
    /// Authoritative job state and bounded retained event history.
    jobs: JobStore,
    /// Most recent prerequisite report, initially explicitly unchecked.
    cached: Value,
    /// Last descriptor scan; not proof of current device presence.
    devices: DeviceScanReport,
    /// Latest admitted converted-only upload; replacement retires its old token.
    project: Option<Arc<crate::build_input::UploadedProject>>,
    /// Last two published artifacts and their immutable bytes/optional held source leases.
    artifacts: Vec<crate::artifacts::Artifact>,
    /// Explicit inspection evidence keyed by selected device; scans and completed flashes invalidate entries.
    inspections: std::collections::BTreeMap<DeviceId, DeviceInspection>,
    /// Current or finished blocking operation, reaped before later operation admission.
    worker: Option<tokio::task::JoinHandle<()>>,
    /// Shutdown latch refusing new operations while producers are cancelled/joined.
    stopping: bool,
}
/// Per-helper session identity, authority and admission ownership shared by bounded HTTP connections.
struct State {
    /// Canonical repository root in production; tests may inject missing roots.
    repo: PathBuf,
    /// Random session identity invalidated by helper restart.
    session: SessionId,
    /// Random 256-bit lowercase hexadecimal capability returned only by explicit connect.
    capability: String,
    /// Optional metadata-only browser terminal relay.
    relay: crate::relay::Relay,
    /// Serialized status state; poisoning quarantines stateful routes.
    inner: Mutex<Inner>,
    /// Sticky worker/ownership fault; read-only session status remains available.
    fatal: std::sync::atomic::AtomicBool,
    /// Single permit covers native operations and blocking storage/project admission even after HTTP cancellation.
    activity: Arc<tokio::sync::Semaphore>,
}
/// Returns `2 * N` lowercase hexadecimal characters from OS entropy; returns an entropy error without a token.
fn random_hex<const N: usize>() -> Result<String, String> {
    let mut b = [0u8; N];
    getrandom::fill(&mut b).map_err(|_| "OS entropy unavailable")?;
    Ok(b.iter().map(|x| format!("{x:02x}")).collect())
}
impl State {
    /// Returns fresh empty state for `repo`, random session/capability and detached diagnostic relay.
    /// Returns entropy/identity errors; does not validate the repository, bind a listener or start a native operation.
    fn new(repo: PathBuf) -> Result<Arc<Self>, String> {
        let session = SessionId::new(random_hex::<16>()?).map_err(|_| "invalid session")?;
        Ok(Arc::new(Self {
            repo,
            session: session.clone(),
            capability: random_hex::<32>()?,
            relay: crate::relay::Relay::new(),
            fatal: std::sync::atomic::AtomicBool::new(false),
            activity: Arc::new(tokio::sync::Semaphore::new(1)),
            inner: Mutex::new(Inner {
                jobs: JobStore::new(session.clone()),
                devices: crate::devices::empty(session, "Device scan not run"),
                cached: json!({"schema_version":1,"ready":false,"checks":[],"reason":"Check not run"}),
                project: None,
                artifacts: Vec::new(),
                inspections: std::collections::BTreeMap::new(),
                worker: None,
                stopping: false,
            }),
        }))
    }

    /// Returns the sole owned activity permit, Busy while held/closed, or InvalidState after a fatal fault.
    /// Dropping the permit releases admission; callers move it into blocking work so HTTP cancellation cannot release
    /// it early.
    fn reserve(&self) -> Result<tokio::sync::OwnedSemaphorePermit, Error> {
        if self.fatal.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(Error::InvalidState);
        }
        self.activity.clone().try_acquire_owned().map_err(|_| Error::Busy)
    }
}
/// Returns retained job `id`'s summary from `state`, or lookup/batch error; panics if the owner mutex is poisoned.
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
/// Returns storage read by `read` for `state`'s repository and currently leased attempt IDs, outside the status mutex.
/// Returns Busy for activity/worker contention, Unavailable for owner/task failure, or Quarantined for reader failure.
/// The blocking closure retains its permit if this future is dropped; started [`tokio::task::spawn_blocking`] work is
/// not aborted.
async fn read_build_storage(
    state: Arc<State>,
    read: impl FnOnce(&std::path::Path, &std::collections::BTreeSet<String>) -> Result<BuildStorage, String>
    + Send
    + 'static,
) -> Result<BuildStorage, &'static str> {
    let permit = state.reserve().map_err(|_| "BuildStorageBusy")?;
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let protected = {
            let inner = state.inner.lock().map_err(|_| "BuildStorageUnavailable")?;
            if inner.worker.as_ref().is_some_and(|w| !w.is_finished()) {
                return Err("BuildStorageBusy");
            }
            inner
                .artifacts
                .iter()
                .filter_map(|a| a.lease.as_ref())
                .filter_map(|a| a.frozen.attempt_root().file_name()?.to_str().map(str::to_owned))
                .collect()
        };
        read(&state.repo, &protected).map_err(|_| "BuildStorageQuarantined")
    })
    .await
    .map_err(|_| "BuildStorageUnavailable")?
}
/// Handles `request` against session `state`, returning bounded JSON/errors or retained UF2 bytes with no-store/CORS
/// headers. Enforces exact Host/origin, origin-form URI, capability (except explicit connect/preflight),
/// framing/type/body limits and route-specific typed admission before work. HTTP cancellation does not undo admitted
/// jobs or abort started blocking work. Most failures become HTTP statuses; a concurrent owner poison or internal
/// serialization invariant failure can panic. Connect only returns session credentials; scan, inspection, build,
/// cleanup and Flash require their explicit POST routes.
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
        let query_allowed = request.uri().query().is_none() || {
            let parts: Vec<_> = path.split('/').collect();
            requested == Some("GET")
                && parts.len() == 6
                && parts[1] == "api"
                && parts[2] == "v1"
                && parts[3] == "jobs"
                && parts[5] == "events"
                && JobId::new(parts[4]).is_ok()
                && poll_query(request.uri().query()).is_ok()
        };
        if !matches!(requested, Some("GET" | "POST")) || !allowed_headers || !query_allowed {
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
    if state.fatal.load(std::sync::atomic::Ordering::SeqCst) || state.inner.is_poisoned() {
        if method == Method::GET && path == "/api/v1/session" {
            return response(
                StatusCode::OK,
                json!({"protocol_version":PROTOCOL_VERSION,"session_id":state.session,"ready":false,"reason":"Helper ownership fault; operations disabled. Restart explicitly after inspecting diagnostics."}),
                origin,
            );
        }
        if !connecting && (method != Method::GET || state.inner.is_poisoned()) {
            return error(StatusCode::SERVICE_UNAVAILABLE, "HelperOwnershipFault", origin);
        }
    }
    if method == Method::POST
        && matches!(
            path.as_str(),
            "/api/v1/flash-attempts/cleanup" | "/api/v1/build-attempts/cleanup"
        )
    {
        let build = path == "/api/v1/build-attempts/cleanup";
        let bytes = match json_body(request, REQUEST_BYTES, "RequestBound").await {
            Ok(bytes) => bytes,
            Err((status, code)) => return error(status, code, origin),
        };
        let request = match decode::<CleanupRequest>(&bytes) {
            Ok(r) => r,
            Err(e) => return store_error(e, origin),
        };
        return match start_cleanup(state.clone(), request, build).await {
            Ok(job) => response(StatusCode::ACCEPTED, serde_json::to_value(job).unwrap(), origin),
            Err(e) => store_error(e, origin),
        };
    }
    if method == Method::POST && path == "/api/v1/flashes" {
        let bytes = match json_body(request, REQUEST_BYTES, "RequestBound").await {
            Ok(bytes) => bytes,
            Err((status, code)) => return error(status, code, origin),
        };
        let request = match decode::<FlashRequest>(&bytes) {
            Ok(r) => r,
            Err(e) => return store_error(e, origin),
        };
        return match start_flash(state.clone(), request).await {
            Ok(job) => response(StatusCode::ACCEPTED, serde_json::to_value(job).unwrap(), origin),
            Err(e) => store_error(e, origin),
        };
    }
    if method == Method::POST && path == "/api/v1/diagnostics" {
        let bytes = match json_body(request, diagnostics::REQUEST_BYTES, "DiagnosticBound").await {
            Ok(bytes) => bytes,
            Err((status, code)) => return error(status, code, origin),
        };
        return match state.relay.admit(&bytes) {
            Ok(reply) => response(StatusCode::OK, serde_json::to_value(reply).unwrap(), origin),
            Err(e) => store_error(e, origin),
        };
    }
    if method == Method::POST && path == "/api/v1/builds" {
        let bytes = match json_body(request, REQUEST_BYTES, "RequestBound").await {
            Ok(bytes) => bytes,
            Err((status, code)) => return error(status, code, origin),
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
    if method == Method::POST && path == "/api/v1/projects" {
        if one(headers, "content-type") != Some("application/octet-stream") {
            return error(StatusCode::UNSUPPORTED_MEDIA_TYPE, "ExpectedBinaryProject", origin);
        }
        if one(headers, "content-length").is_some_and(|v| v.parse::<usize>().map_or(true, |n| n > PROJECT_UPLOAD_BYTES))
        {
            return error(StatusCode::PAYLOAD_TOO_LARGE, "ProjectBound", origin);
        }
        let bytes = match bounded_body(request.into_body(), PROJECT_UPLOAD_BYTES).await {
            Ok(b) => b,
            Err(_) => return error(StatusCode::PAYLOAD_TOO_LARGE, "ProjectBodyBound", origin),
        };
        let permit = match state.reserve() {
            Ok(p) => p,
            Err(e) => return store_error(e, origin),
        };
        let session = state.session.clone();
        let (upload, _permit) = match tokio::task::spawn_blocking(move || {
            (crate::build_input::UploadedProject::admit(session, bytes), permit)
        })
        .await
        {
            Ok((Ok(p), permit)) => (p, permit),
            Ok((Err(_), _)) => return error(StatusCode::BAD_REQUEST, "InvalidConvertedProject", origin),
            Err(_) => return error(StatusCode::SERVICE_UNAVAILABLE, "ProjectAdmissionUnavailable", origin),
        };
        let mut inner = state.inner.lock().unwrap();
        if inner.stopping
            || state.fatal.load(std::sync::atomic::Ordering::SeqCst)
            || inner.worker.as_ref().is_some_and(|w| !w.is_finished())
        {
            return store_error(Error::Busy, origin);
        }
        let result = serde_json::to_value(&upload.response).unwrap();
        inner.project = Some(Arc::new(upload));
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
            json!({"protocol_version":PROTOCOL_VERSION,"session_id":state.session,"capability":state.capability}),
            origin,
        );
    }
    match (method.as_str(), path.as_str()) {
        ("GET", "/api/v1/session") => {
            response(
                StatusCode::OK,
                json!({"protocol_version":PROTOCOL_VERSION,"session_id":state.session,"ready":false,"reason":"Checks, builds and explicit Flash available; Flash requires a validated artifact, fresh device inspection and genuine Pico 2 W acknowledgment"}),
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
            match start_check(state.clone(), true, None).await {
                Ok(job) => response(StatusCode::ACCEPTED, serde_json::to_value(job).unwrap(), origin),
                Err(e) => store_error(e, origin),
            }
        }
        ("GET", "/api/v1/build-attempts") => {
            match read_build_storage(state.clone(), crate::build_storage::list).await {
                Ok(report) => response(StatusCode::OK, json!(report), origin),
                Err(reason) => error(StatusCode::CONFLICT, reason, origin),
            }
        }
        ("GET", "/api/v1/flash-attempts") => {
            let permit = match state.reserve() {
                Ok(p) => p,
                Err(e) => return store_error(e, origin),
            };
            let repo = state.repo.clone();
            match tokio::task::spawn_blocking(move || {
                let _permit = permit;
                crate::flash_intent::history(&repo)
            })
            .await
            {
                Ok(Ok(records)) => response(StatusCode::OK, json!(records), origin),
                _ => error(StatusCode::CONFLICT, "FlashHistoryUnavailable", origin),
            }
        }
        ("GET", "/api/v1/artifacts") => {
            response(
                StatusCode::OK,
                serde_json::to_value(
                    state
                        .inner
                        .lock()
                        .unwrap()
                        .artifacts
                        .iter()
                        .map(|a| &a.summary)
                        .collect::<Vec<_>>(),
                )
                .unwrap(),
                origin,
            )
        }
        ("GET", "/api/v1/jobs") => {
            response(
                StatusCode::OK,
                serde_json::to_value(state.inner.lock().unwrap().jobs.summaries()).unwrap(),
                origin,
            )
        }
        ("POST", "/api/v1/prerequisites/check") => {
            match start_check(state.clone(), false, None).await {
                Ok(job) => response(StatusCode::ACCEPTED, serde_json::to_value(job).unwrap(), origin),
                Err(e) => store_error(e, origin),
            }
        }
        _ => {
            let parts: Vec<_> = path.split('/').collect();
            if parts.len() == 6 && parts[1] == "api" && parts[2] == "v1" && parts[3] == "devices" {
                let Ok(id) = DeviceId::new(parts[4]) else {
                    return store_error(Error::Schema, origin);
                };
                if method == Method::GET && parts[5] == "inspection" {
                    return match state.inner.lock().unwrap().inspections.get(&id) {
                        Some(report) => response(StatusCode::OK, serde_json::to_value(report).unwrap(), origin),
                        None => store_error(Error::NotFoundOrUnavailable, origin),
                    };
                }
                if method == Method::POST && parts[5] == "inspect" {
                    let selected = state
                        .inner
                        .lock()
                        .unwrap()
                        .devices
                        .devices
                        .iter()
                        .find(|d| d.device_id == id)
                        .cloned();
                    let Some(selected) = selected else {
                        return store_error(Error::NotFoundOrUnavailable, origin);
                    };
                    return match start_check(state.clone(), true, Some(selected)).await {
                        Ok(job) => response(StatusCode::ACCEPTED, serde_json::to_value(job).unwrap(), origin),
                        Err(e) => store_error(e, origin),
                    };
                }
            }
            if method == Method::GET
                && matches!(parts.len(), 5 | 6)
                && parts[1] == "api"
                && parts[2] == "v1"
                && parts[3] == "artifacts"
            {
                let Ok(id) = ArtifactId::new(parts[4]) else {
                    return store_error(Error::Schema, origin);
                };
                let artifact = state
                    .inner
                    .lock()
                    .unwrap()
                    .artifacts
                    .iter()
                    .find(|a| a.summary.artifact_id == id)
                    .cloned();
                let Some(artifact) = artifact else {
                    return store_error(Error::NotFoundOrUnavailable, origin);
                };
                if parts.len() == 5 {
                    return response(StatusCode::OK, serde_json::to_value(artifact.summary).unwrap(), origin);
                }
                if parts[5] == "uf2" {
                    let mut r = response(StatusCode::OK, json!(null), origin);
                    r.headers_mut()
                        .insert("content-type", HeaderValue::from_static("application/octet-stream"));
                    r.headers_mut().insert(
                        "content-disposition",
                        HeaderValue::from_static("attachment; filename=dashboard.uf2"),
                    );
                    r.headers_mut().insert(
                        "content-length",
                        HeaderValue::from_str(&artifact.uf2.len().to_string()).unwrap(),
                    );
                    *r.body_mut() = Body::from(axum::body::Bytes::from_owner(artifact.uf2));
                    return r;
                }
                return store_error(Error::NotFoundOrUnavailable, origin);
            }
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
/// Serves the authenticated loopback helper for `repo` until shutdown; returns host/repository/entropy/runtime/server
/// errors. Creates a current-thread Tokio runtime with separate blocking workers; no device operation starts merely by
/// listening. Panics if invoked inside an incompatible Tokio runtime or if standard-error reporting fails.
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
/// Runs `state` on the fixed loopback socket with at most 32 connections, 64/16-KiB headers, 10-second header and
/// 30-second connection deadlines. Returns bind/listener/owner/worker/cleanup errors; clean shutdown returns success.
/// On Ctrl-C/stopping, closes acceptance, requests cancellation, aborts HTTP connections and awaits the native worker.
/// Started blocking work is cooperative and cannot be aborted by the connection timeout; transfer cancellation stays
/// deferred.
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
        if let Ok(inner) = state.inner.lock() {
            if inner.stopping {
                break;
            }
        } else {
            state.fatal.store(true, std::sync::atomic::Ordering::SeqCst);
        }
    }
    drop(listener);
    let worker = {
        let mut inner = state
            .inner
            .lock()
            .map_err(|_| "Helper owner poisoned; cleanup unproven")?;
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
/// HTTP authority, bounded admission, retained-output and cancellation ownership regressions without physical devices.
mod tests {
    use super::*;
    #[tokio::test]
    /// Slow blocking storage keeps its activity permit through HTTP cancellation without locking status reads.
    async fn slow_storage_read_does_not_hold_status_owner() {
        let state = State::new(std::env::current_dir().unwrap()).unwrap();
        let (entered, ready) = tokio::sync::oneshot::channel();
        let (release, wait) = std::sync::mpsc::channel();
        let reader = tokio::spawn(read_build_storage(state.clone(), move |_, _| {
            let _ = entered.send(());
            let _ = wait.recv_timeout(Duration::from_secs(3));
            Err("injected read failure".into())
        }));
        ready.await.unwrap();
        let available = state.inner.try_lock().is_ok();
        assert!(matches!(state.reserve(), Err(Error::Busy)));
        let request = Request::builder()
            .uri("/api/v1/session")
            .header("host", AUTHORITY)
            .header("origin", ORIGINS[0])
            .header("x-studio-capability", &state.capability)
            .body(Body::empty())
            .unwrap();
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(1), dispatch(state.clone(), request))
                .await
                .unwrap()
                .status(),
            StatusCode::OK
        );
        // Dropping a timed-out HTTP future cannot release a blocking reader's
        // permit. A replacement request stays bounded until the real work ends.
        reader.abort();
        assert!(reader.await.unwrap_err().is_cancelled());
        assert!(matches!(state.reserve(), Err(Error::Busy)));
        release.send(()).unwrap();
        let _permit = tokio::time::timeout(Duration::from_secs(1), state.activity.acquire())
            .await
            .unwrap()
            .unwrap();
        assert!(available, "filesystem work must not hold the status owner mutex");
    }
    #[tokio::test]
    /// Flash-history cleanup rejects missing capability and an existing unclosed operation before worker launch.
    async fn cleanup_requires_authority_and_a_closed_operation() {
        let state = State::new(std::env::current_dir().unwrap()).unwrap();
        let body = serde_json::json!({"attempts":["1".repeat(32)]}).to_string();
        let unauthenticated = Request::builder()
            .method("POST")
            .uri("/api/v1/flash-attempts/cleanup")
            .header("host", AUTHORITY)
            .header("origin", ORIGINS[0])
            .header("content-type", "application/json")
            .body(Body::from(body.clone()))
            .unwrap();
        assert_eq!(
            dispatch(state.clone(), unauthenticated).await.status(),
            StatusCode::FORBIDDEN
        );
        state
            .inner
            .lock()
            .unwrap()
            .jobs
            .start(JobId::new("2".repeat(32)).unwrap(), OperationKind::Build, None)
            .unwrap();
        let authorized = Request::builder()
            .method("POST")
            .uri("/api/v1/flash-attempts/cleanup")
            .header("host", AUTHORITY)
            .header("origin", ORIGINS[0])
            .header("content-type", "application/json")
            .header("x-studio-capability", &state.capability)
            .body(Body::from(body))
            .unwrap();
        assert_eq!(dispatch(state.clone(), authorized).await.status(), StatusCode::CONFLICT);
        assert!(state.inner.lock().unwrap().worker.is_none());
    }

    #[tokio::test]
    /// Build cleanup enforces the same authority and closed-job admission boundary as flash-history cleanup.
    async fn build_cleanup_requires_authority_and_a_closed_operation() {
        let state = State::new(std::env::current_dir().unwrap()).unwrap();
        let body = serde_json::json!({"attempts":["1".repeat(32)]}).to_string();
        let unauthenticated = Request::builder()
            .method("POST")
            .uri("/api/v1/build-attempts/cleanup")
            .header("host", AUTHORITY)
            .header("origin", ORIGINS[0])
            .header("content-type", "application/json")
            .body(Body::from(body.clone()))
            .unwrap();
        assert_eq!(
            dispatch(state.clone(), unauthenticated).await.status(),
            StatusCode::FORBIDDEN
        );
        state
            .inner
            .lock()
            .unwrap()
            .jobs
            .start(JobId::new("2".repeat(32)).unwrap(), OperationKind::Build, None)
            .unwrap();
        let authorized = Request::builder()
            .method("POST")
            .uri("/api/v1/build-attempts/cleanup")
            .header("host", AUTHORITY)
            .header("origin", ORIGINS[0])
            .header("content-type", "application/json")
            .header("x-studio-capability", &state.capability)
            .body(Body::from(body))
            .unwrap();
        assert_eq!(dispatch(state.clone(), authorized).await.status(), StatusCode::CONFLICT);
        assert!(state.inner.lock().unwrap().worker.is_none());
    }

    #[tokio::test]
    /// Legacy/non-W board acknowledgement fails wire admission; valid W spelling proceeds only to unavailable artifact
    /// lookup.
    async fn flash_route_requires_w_consent_before_artifact_lookup_or_work() {
        let state = State::new(std::env::current_dir().unwrap()).unwrap();
        let mut body = serde_json::json!({"artifact_id":"a".repeat(64),"device_id":"b".repeat(32),"editor_revision":1,"project_sha256":"c".repeat(64),"config_sha256":"d".repeat(64),"expected_options":BuildOptions::default(),"retry_of":null,"board_acknowledgement":{"device_id":"b".repeat(32),"session_id":state.session,"information_sha256":"e".repeat(64),"board":"genuine_pico2_w"}});
        for (board, expected) in [
            ("genuine_pico2", StatusCode::BAD_REQUEST),
            ("unknown", StatusCode::BAD_REQUEST),
            ("genuine_pico2_w", StatusCode::NOT_FOUND),
        ] {
            body["board_acknowledgement"]["board"] = serde_json::json!(board);
            let request = Request::builder()
                .method("POST")
                .uri("/api/v1/flashes")
                .header("host", AUTHORITY)
                .header("origin", ORIGINS[0])
                .header("x-studio-capability", &state.capability)
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap();
            assert_eq!(dispatch(state.clone(), request).await.status(), expected);
            let inner = state.inner.lock().unwrap();
            assert!(inner.jobs.summaries().is_empty());
            assert!(inner.worker.is_none());
        }
    }
    #[tokio::test]
    /// Diagnostic relay rejects foreign origin/capability, oversize and private-message-shaped input without creating
    /// jobs.
    async fn diagnostics_route_requires_authority_and_bounds_without_jobs() {
        let state = State::new(std::env::current_dir().unwrap()).unwrap();
        let good = serde_json::json!({"version":1,"events":[{
            "severity":"Warn","category":"Storage","code":"storage.failed",
            "workspace":"Diagnostics","sequence":"1","client_elapsed_ms":"0",
            "accepted_revision":"0"}]})
        .to_string();
        for (capability, origin, body, expected) in [
            ("", ORIGINS[0], good.clone(), StatusCode::FORBIDDEN),
            (
                state.capability.as_str(),
                "http://localhost:8081",
                good.clone(),
                StatusCode::FORBIDDEN,
            ),
            (
                state.capability.as_str(),
                ORIGINS[0],
                "x".repeat(diagnostics::REQUEST_BYTES + 1),
                StatusCode::PAYLOAD_TOO_LARGE,
            ),
            (
                state.capability.as_str(),
                ORIGINS[0],
                "{\"message\":\"secret\"}".into(),
                StatusCode::BAD_REQUEST,
            ),
            (state.capability.as_str(), ORIGINS[0], good, StatusCode::OK),
        ] {
            let request = Request::builder()
                .method("POST")
                .uri("/api/v1/diagnostics")
                .header("host", AUTHORITY)
                .header("origin", origin)
                .header("x-studio-capability", capability)
                .header("content-type", "application/json")
                .body(Body::from(body))
                .unwrap();
            assert_eq!(dispatch(state.clone(), request).await.status(), expected);
        }
        assert!(state.inner.lock().unwrap().jobs.summaries().is_empty());
    }
    #[tokio::test]
    /// Re-uploading identical valid bytes issues a fresh token; oversize replacement retains the previously admitted
    /// project.
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
    /// Stale project digest fails before job admission; a missing prerequisite closes failure without storage/artifact
    /// publication.
    async fn build_rejects_stale_token_and_closes_prerequisite_failure_without_artifact() {
        let repo = std::env::temp_dir().join(format!("studio-missing-{}", random_hex::<16>().unwrap()));
        assert!(!repo.exists());
        let state = State::new(repo.clone()).unwrap();
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
        state.inner.lock().unwrap().project = Some(Arc::new(project));
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
        assert_eq!(result.terminal_cause, Some(TerminalCause::Failed));
        assert_eq!(result.stages[0].state, StageState::Failed);
        assert_eq!(result.stages[1].state, StageState::Skipped);
        assert!(!repo.exists(), "prerequisite failure must not reserve snapshot storage");
    }
    #[tokio::test]
    /// Event preflight enforces typed query bounds; a typed build request admits a job that closes on missing
    /// prerequisites.
    async fn browser_poll_preflight_and_typed_build_route() {
        let repo = std::env::temp_dir().join(format!("studio-missing-{}", random_hex::<16>().unwrap()));
        assert!(!repo.exists());
        let state = State::new(repo.clone()).unwrap();
        for (query, status) in [
            ("after=0&limit=128", StatusCode::NO_CONTENT),
            ("after=0&limit=129", StatusCode::FORBIDDEN),
        ] {
            let request = Request::builder()
                .uri(format!("/api/v1/jobs/{}/events?{query}", "1".repeat(32)))
                .method("OPTIONS")
                .header("host", AUTHORITY)
                .header("origin", ORIGINS[0])
                .header("access-control-request-method", "GET")
                .header("access-control-request-headers", "x-studio-capability")
                .body(Body::empty())
                .unwrap();
            assert_eq!(dispatch(state.clone(), request).await.status(), status);
        }
        let config = dashboard_config::validate(dashboard_config::default_config()).unwrap();
        let bytes = dashboard_config_tools::project::Project::new(&config, Default::default(), None)
            .unwrap()
            .encode(dashboard_config_tools::project::ExportMode::ConvertedOnly)
            .unwrap();
        let upload = crate::build_input::UploadedProject::admit(state.session.clone(), bytes).unwrap();
        let body = ProjectBuildRequest {
            editor_revision: 1,
            project_id: upload.response.project_id.clone(),
            project_sha256: upload.response.project_sha256.clone(),
            options: BuildOptions::default(),
            retry_of: None,
        };
        state.inner.lock().unwrap().project = Some(Arc::new(upload));
        let request = Request::builder()
            .uri("/api/v1/builds")
            .method("POST")
            .header("host", AUTHORITY)
            .header("origin", ORIGINS[0])
            .header("x-studio-capability", &state.capability)
            .header("content-type", "application/json")
            .body(Body::from(encode(&body).unwrap()))
            .unwrap();
        assert_eq!(dispatch(state.clone(), request).await.status(), StatusCode::ACCEPTED);
        let worker = state.inner.lock().unwrap().worker.take().unwrap();
        worker.await.unwrap();
        assert!(state.inner.lock().unwrap().jobs.summaries()[0].events_closed);
        assert!(!repo.exists());
    }
    #[tokio::test]
    /// Authorized download returns exact retained bytes even without a live flashing lease; missing capability is
    /// forbidden.
    async fn artifact_download_requires_capability_and_uses_retained_bytes() {
        let state = State::new(std::env::current_dir().unwrap()).unwrap();
        let hash = Sha256::new("1".repeat(64)).unwrap();
        let id = ArtifactId::new("2".repeat(64)).unwrap();
        let metadata = ArtifactSummary {
            artifact_id: id.clone(),
            session_id: state.session.clone(),
            editor_revision: 1,
            project_sha256: hash.clone(),
            config_sha256: hash.clone(),
            startup_profile_sha256: hash.clone(),
            startup_asset_sha256: hash.clone(),
            source_sha256: hash.clone(),
            elf_sha256: hash.clone(),
            uf2_sha256: hash.clone(),
            converter_sha256: hash,
            options: normalize(&BuildOptions::default()).unwrap(),
            uf2_bytes: 512,
            hardware_qualified: false,
            flash_ready: false,
        };
        state.inner.lock().unwrap().artifacts.push(crate::artifacts::Artifact {
            summary: metadata,
            uf2: vec![7; 512].into(),
            lease: None,
        });
        assert!(state.inner.lock().unwrap().artifacts[0].verify_lease().is_err());
        for authorized in [false, true] {
            let mut builder = Request::builder()
                .uri(format!("/api/v1/artifacts/{}/uf2", id.as_str()))
                .header("host", AUTHORITY)
                .header("origin", ORIGINS[0]);
            if authorized {
                builder = builder.header("x-studio-capability", &state.capability);
            }
            let result = dispatch(state.clone(), builder.body(Body::empty()).unwrap()).await;
            assert_eq!(
                result.status(),
                if authorized {
                    StatusCode::OK
                } else {
                    StatusCode::FORBIDDEN
                }
            );
            if authorized {
                assert_eq!(result.headers()["content-length"], "512");
                assert_eq!(result.headers()["cache-control"], "no-store");
                assert_eq!(
                    bounded_body(result.into_body(), MAX_UF2_BYTES).await.unwrap(),
                    vec![7; 512]
                );
            }
        }
    }
    #[tokio::test]
    /// Unknown selected device ID rejects inspection before job admission or native work.
    async fn device_inspection_requires_cached_selected_id() {
        let state = State::new(std::env::current_dir().unwrap()).unwrap();
        let request = Request::builder()
            .uri(format!("/api/v1/devices/{}/inspect", "1".repeat(32)))
            .method("POST")
            .header("host", AUTHORITY)
            .header("origin", ORIGINS[0])
            .header("x-studio-capability", &state.capability)
            .body(Body::empty())
            .unwrap();
        assert_eq!(dispatch(state.clone(), request).await.status(), StatusCode::NOT_FOUND);
        assert!(state.inner.lock().unwrap().jobs.summaries().is_empty());
    }
    #[tokio::test]
    /// Foreign connect origin and missing operation capability are forbidden.
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
    /// Explicit connect publishes credentials/no-store/CORS response without starting work.
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
    /// Rejects absolute-form URIs, unexpected queries, duplicate authority and pre-restart capability; cursors retain
    /// u64 precision.
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

#[cfg(test)]
/// Interrupted project upload retains previous accepted state and supports an explicit new attempt.
mod upload_failure_tests {
    use super::*;
    /// Body yielding one prefix then a repeated connection-reset error.
    struct InterruptedBody(
        /// Whether the sole successful prefix frame has already been yielded.
        bool,
    );
    impl hyper::body::Body for InterruptedBody {
        /// Owned byte chunks matching the server body adapter.
        type Data = axum::body::Bytes;
        /// Injected transport reset surfaced through bounded body collection.
        type Error = std::io::Error;

        /// Returns the initial project magic once, then an injected reset on every poll; the unused task context is
        /// never parked.
        fn poll_frame(
            mut self: std::pin::Pin<&mut Self>,
            _: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Option<Result<hyper::body::Frame<Self::Data>, Self::Error>>> {
            if !self.0 {
                self.0 = true;
                std::task::Poll::Ready(Some(Ok(hyper::body::Frame::data(axum::body::Bytes::from_static(
                    b"EAVPROJ\0",
                )))))
            } else {
                std::task::Poll::Ready(Some(Err(std::io::Error::new(
                    std::io::ErrorKind::ConnectionReset,
                    "injected upload disconnect",
                ))))
            }
        }
    }
    /// Returns an authorized binary upload request containing `body` for `state`; panics if fixture headers are
    /// invalid.
    fn upload(
        state: &State,
        body: Body,
    ) -> Request<Body> {
        Request::builder()
            .uri("/api/v1/projects")
            .method("POST")
            .header("host", AUTHORITY)
            .header("origin", ORIGINS[0])
            .header("x-studio-capability", &state.capability)
            .header("content-type", "application/octet-stream")
            .body(body)
            .unwrap()
    }
    #[tokio::test]
    /// A disconnected body does not replace accepted upload identity; a later explicit successful upload receives a new
    /// token.
    async fn interrupted_body_keeps_previous_upload_and_allows_explicit_retry() {
        let state = State::new(std::env::current_dir().unwrap()).unwrap();
        let config = dashboard_config::validate(dashboard_config::default_config()).unwrap();
        let bytes = dashboard_config_tools::project::Project::new(&config, Default::default(), None)
            .unwrap()
            .encode(dashboard_config_tools::project::ExportMode::ConvertedOnly)
            .unwrap();
        assert_eq!(
            dispatch(state.clone(), upload(&state, Body::from(bytes.clone())))
                .await
                .status(),
            StatusCode::CREATED
        );
        let before = state.inner.lock().unwrap().project.as_ref().unwrap().response.clone();
        assert_eq!(
            dispatch(state.clone(), upload(&state, Body::new(InterruptedBody(false))))
                .await
                .status(),
            StatusCode::PAYLOAD_TOO_LARGE
        );
        let after = state.inner.lock().unwrap().project.as_ref().unwrap().response.clone();
        assert_eq!(after.project_id, before.project_id);
        assert_eq!(after.project_sha256, before.project_sha256);
        assert!(state.inner.lock().unwrap().jobs.summaries().is_empty());
        assert_eq!(
            dispatch(state.clone(), upload(&state, Body::from(bytes)))
                .await
                .status(),
            StatusCode::CREATED
        );
        assert_ne!(
            state
                .inner
                .lock()
                .unwrap()
                .project
                .as_ref()
                .unwrap()
                .response
                .project_id,
            before.project_id
        );
    }
}
