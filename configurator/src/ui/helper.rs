//! Explicit loopback connection; bounded reads and one owned polling chain.
use std::time::Duration;

use dashboard_configurator::helper_client::{
    Connection,
    DeviceSelection,
    JobFeed,
    MAX_RESPONSE_BYTES,
    PreparedBuild,
    verify_artifact_download,
};
use dashboard_helper_protocol::{
    ArtifactSummary,
    BoundedVec,
    DeviceScanReport,
    EventBatch,
    JobSummary,
    ProjectUploadResponse,
    Wire,
};
use leptos::leptos_dom::helpers::{TimeoutHandle, set_timeout_with_handle};
use wasm_bindgen_futures::{JsFuture, spawn_local};
use web_sys::{AbortController, ReadableStreamDefaultReader, Request, RequestInit, Response};

use super::*;
mod artifacts;
mod devices;
mod history;
mod prerequisites;

const BASE: &str = "http://127.0.0.1:8765/api/v1";
#[derive(Clone, Default)]
struct ViewState {
    connected: bool,
    message: String,
    feed: JobFeed,
    report: Option<serde_json::Value>,
    devices: Option<DeviceScanReport>,
    artifacts: Vec<ArtifactSummary>,
    history: Option<dashboard_helper_protocol::FlashHistory>,
}
#[derive(Clone, Copy)]
pub struct State {
    view: RwSignal<ViewState>,
    connection: StoredValue<Option<Connection>>,
    busy: RwSignal<bool>,
    cancel: RwSignal<bool>,
    epoch: StoredValue<u64>,
    timer: StoredValue<Option<TimeoutHandle>>,
    abort: StoredValue<Option<AbortController>, LocalStorage>,
    building: RwSignal<bool>,
    options_revision: RwSignal<Option<u64>>,
    prepared: StoredValue<Option<PreparedBuild>>,
    uploaded: StoredValue<Option<ProjectUploadResponse>>,
    download: StoredValue<Option<ArtifactSummary>>,
    flashing: StoredValue<Option<ArtifactSummary>>,
    device: RwSignal<DeviceSelection>,
    inspecting: StoredValue<bool>,
}
impl State {
    pub fn new() -> Self {
        Self {
            view: RwSignal::new(ViewState {
                message: "Start the local helper, then connect explicitly.".into(),
                ..Default::default()
            }),
            connection: StoredValue::new(None),
            busy: RwSignal::new(false),
            cancel: RwSignal::new(false),
            epoch: StoredValue::new(0),
            timer: StoredValue::new(None),
            abort: StoredValue::new_local(None),
            building: RwSignal::new(false),
            options_revision: RwSignal::new(Some(0)),
            prepared: StoredValue::new(None),
            uploaded: StoredValue::new(None),
            download: StoredValue::new(None),
            flashing: StoredValue::new(None),
            device: RwSignal::new(DeviceSelection::default()),
            inspecting: StoredValue::new(false),
        }
    }

    pub fn install(self) {
        on_cleanup(move || {
            if let Some(Some(timer)) = self.timer.try_get_value() {
                timer.clear();
            }
            if let Some(Some(abort)) = self.abort.try_get_value() {
                abort.abort();
            }
            let _ = self.epoch.try_update_value(|e| *e = e.saturating_add(1));
            let _ = self.connection.try_set_value(None);
            let _ = self.prepared.try_set_value(None);
            let _ = self.uploaded.try_set_value(None);
            let _ = self.download.try_set_value(None);
            let _ = self.flashing.try_set_value(None);
        });
    }
}
#[derive(Clone, Copy)]
enum Action {
    Connect,
    Jobs,
    Check,
    Scan,
    Poll,
    Cancel,
    Report,
    Devices,
    Upload,
    Build,
    Artifacts,
    Download,
    Inspect,
    Inspection,
    Flash,
    History,
}
struct ReadGuard {
    abort: AbortController,
    timer: TimeoutHandle,
}
impl Drop for ReadGuard {
    fn drop(&mut self) {
        self.timer.clear();
        self.abort.abort();
    }
}

async fn request(
    ui: Ui,
    action: Action,
) -> Result<Vec<u8>, &'static str> {
    let state = ui.helper;
    let connection = state.connection.get_value();
    let (method, path) = match action {
        Action::Connect => ("POST", "/connect".into()),
        Action::Jobs => ("GET", "/jobs".into()),
        Action::Check => ("POST", "/prerequisites/check".into()),
        Action::Scan => ("POST", "/devices/refresh".into()),
        Action::Report => ("GET", "/prerequisites".into()),
        Action::Devices => ("GET", "/devices".into()),
        Action::Upload => ("POST", "/projects".into()),
        Action::Build => ("POST", "/builds".into()),
        Action::Flash => ("POST", "/flashes".into()),
        Action::History => ("GET", "/flash-attempts".into()),
        Action::Artifacts => ("GET", "/artifacts".into()),
        Action::Inspect | Action::Inspection => {
            let device = state
                .device
                .with_untracked(|d| d.selected().map(|d| d.device_id.clone()))
                .ok_or("Select a device first")?;
            if matches!(action, Action::Inspect) {
                ("POST", format!("/devices/{}/inspect", device.as_str()))
            } else {
                ("GET", format!("/devices/{}/inspection", device.as_str()))
            }
        }
        Action::Download => {
            (
                "GET",
                format!(
                    "/artifacts/{}/uf2",
                    state
                        .download
                        .get_value()
                        .ok_or("No artifact selected")?
                        .artifact_id
                        .as_str()
                ),
            )
        }
        Action::Poll | Action::Cancel => {
            state.view.with_untracked(|v| -> Result<(&str, String), &'static str> {
                let job = v.feed.summary.as_ref().ok_or("No helper job selected")?;
                Ok(if matches!(action, Action::Cancel) {
                    ("POST", format!("/jobs/{}/cancel", job.job_id.as_str()))
                } else {
                    (
                        "GET",
                        format!(
                            "/jobs/{}/events?after={}&limit=128",
                            job.job_id.as_str(),
                            v.feed.after()
                        ),
                    )
                })
            })?
        }
    };
    let abort = AbortController::new().map_err(|_| "Browser cancellation is unavailable")?;
    state.abort.set_value(Some(abort.clone()));
    let deadline = abort.clone();
    let timer = set_timeout_with_handle(move || deadline.abort(), Duration::from_secs(15))
        .map_err(|_| "Browser request timer is unavailable")?;
    let _guard = ReadGuard {
        abort: abort.clone(),
        timer,
    };
    let init = RequestInit::new();
    init.set_method(method);
    init.set_signal(Some(&abort.signal()));
    init.set_mode(web_sys::RequestMode::Cors);
    init.set_credentials(web_sys::RequestCredentials::Omit);
    init.set_cache(web_sys::RequestCache::NoStore);
    init.set_redirect(web_sys::RequestRedirect::Error);
    let payload = match action {
        Action::Flash => {
            let artifact = state.flashing.get_value().ok_or("No firmware selected")?;
            let connection = connection.as_ref().ok_or("Helper session lost")?;
            let command = ui.editor.with_untracked(|editor| {
                ui.build.with_untracked(|selection| {
                    let prepared = PreparedBuild::capture(editor, selection, 0)?;
                    state
                        .device
                        .with_untracked(|device| prepared.authorize_flash(&artifact, connection, device))
                })
            })?;
            Some((
                "application/json",
                serde_json::to_vec(&command).map_err(|_| "Cannot encode flash request")?,
            ))
        }
        Action::Upload => {
            Some((
                "application/octet-stream",
                state
                    .prepared
                    .with_value(|p| p.as_ref().map(|p| p.bytes().to_vec()))
                    .ok_or("Build snapshot was cancelled")?,
            ))
        }
        Action::Build => {
            let uploaded = state.uploaded.get_value().ok_or("Build upload is missing")?;
            let connection = connection.as_ref().ok_or("Helper session lost")?;
            let revision = state
                .options_revision
                .get_untracked()
                .ok_or("Build counter exhausted; reload Studio")?;
            let command = state.prepared.with_value(|p| {
                let prepared = p.as_ref().ok_or("Build snapshot was cancelled")?;
                ui.editor.with_untracked(|editor| {
                    ui.build.with_untracked(|selection| {
                        prepared.authorize(&uploaded, connection, editor, selection, revision)
                    })
                })
            })?;
            Some((
                "application/json",
                serde_json::to_vec(&command).map_err(|_| "Cannot encode build request")?,
            ))
        }
        _ => None,
    };
    if let Some((_, body)) = &payload {
        init.set_body(&js_sys::Uint8Array::from(body.as_slice()));
    }
    let request =
        Request::new_with_str_and_init(&format!("{BASE}{path}"), &init).map_err(|_| "Cannot create helper request")?;
    if matches!(action, Action::Connect) {
        request.headers().set("X-Studio-Connect", "1")
    } else {
        request.headers().set(
            "X-Studio-Capability",
            connection.as_ref().ok_or("Connect to the helper first")?.capability(),
        )
    }
    .map_err(|_| "Cannot set helper request headers")?;
    if let Some((content_type, _)) = payload {
        request
            .headers()
            .set("Content-Type", content_type)
            .map_err(|_| "Cannot set build content type")?;
    }
    fetch_bounded(
        &request,
        if matches!(action, Action::Download) {
            dashboard_helper_protocol::MAX_UF2_BYTES
        } else {
            MAX_RESPONSE_BYTES
        },
    )
    .await
}

async fn fetch_bounded(
    request: &Request,
    max_bytes: usize,
) -> Result<Vec<u8>, &'static str> {
    let response = JsFuture::from(
        web_sys::window()
            .ok_or("Browser window unavailable")?
            .fetch_with_request(request),
    )
    .await
    .map_err(|_| "Helper unreachable or request timed out. Check the local terminal, then reconnect.")?
    .dyn_into::<Response>()
    .map_err(|_| "Invalid helper response")?;
    if !response.ok() {
        return Err(match response.status() {
            403 => "Helper session expired or origin rejected. Reconnect from Studio on port 8080.",
            409 => "Another helper job is active. Reconnect to inspect its status.",
            404 => "This helper does not support the requested operation.",
            _ => "Helper request failed. Check its terminal for details.",
        });
    }
    let stream = response.body().ok_or("Helper response body unavailable")?;
    let reader = stream
        .get_reader()
        .dyn_into::<ReadableStreamDefaultReader>()
        .map_err(|_| "Cannot read helper response")?;
    let mut bytes = Vec::new();
    let result = async {
        loop {
            let chunk = JsFuture::from(reader.read())
                .await
                .map_err(|_| "Helper response was interrupted")?;
            if js_sys::Reflect::get(&chunk, &"done".into())
                .ok()
                .and_then(|v| v.as_bool())
                == Some(true)
            {
                break;
            }
            let value = js_sys::Reflect::get(&chunk, &"value".into()).map_err(|_| "Invalid helper stream")?;
            let data = value
                .dyn_into::<js_sys::Uint8Array>()
                .map_err(|_| "Invalid helper byte stream")?;
            let count = data.length() as usize;
            if count > max_bytes.saturating_sub(bytes.len()) {
                return Err("Helper response exceeds the allowed size");
            }
            let at = bytes.len();
            bytes.resize(at + count, 0);
            data.copy_to(&mut bytes[at..]);
        }
        Ok(bytes)
    }
    .await;
    reader.release_lock();
    result
}

fn parse<T: serde::de::DeserializeOwned + Wire>(bytes: &[u8]) -> Result<T, &'static str> {
    let value: T = serde_json::from_slice(bytes).map_err(|_| "Malformed helper response")?;
    value.validate().map_err(|_| "Helper response failed validation")?;
    Ok(value)
}
fn apply(
    ui: Ui,
    action: Action,
    bytes: &[u8],
) -> Result<Option<Action>, &'static str> {
    let state = ui.helper;
    match action {
        Action::Connect => {
            let conn = Connection::decode(bytes)?;
            if state
                .connection
                .with_value(|old| old.as_ref().is_none_or(|old| old.session() != conn.session()))
            {
                state.view.update(|v| {
                    v.feed = JobFeed::default();
                    v.report = None;
                    v.devices = None;
                    v.artifacts.clear();
                    v.history = None;
                });
            }
            state.connection.set_value(Some(conn));
            state.view.update(|v| {
                v.connected = true;
                v.message = "Connected to local helper. No tools were installed and no device was written.".into();
            });
            Ok(Some(Action::Jobs))
        }
        Action::Jobs => {
            let jobs: BoundedVec<JobSummary, 16> =
                serde_json::from_slice(bytes).map_err(|_| "Invalid helper job list")?;
            for job in jobs.iter() {
                job.validate().map_err(|_| "Invalid helper job list")?;
            }
            if let Some(job) = jobs.iter().find(|j| !j.events_closed).or_else(|| jobs.last()) {
                let conn = state.connection.get_value().ok_or("Helper session lost")?;
                let mut result = Ok(());
                state.view.update(|v| result = v.feed.start(&conn, job.clone()));
                result?;
                Ok(Some(Action::Poll))
            } else {
                Ok(Some(Action::Report))
            }
        }
        Action::Check | Action::Scan | Action::Build | Action::Inspect | Action::Flash => {
            let job = parse::<JobSummary>(bytes)?;
            let conn = state.connection.get_value().ok_or("Helper session lost")?;
            let mut result = Ok(());
            state.view.update(|v| {
                result = v.feed.start(&conn, job);
                v.message = "Local job running. Detailed status and logs follow below.".into();
            });
            result?;
            if matches!(action, Action::Flash) {
                state.flashing.set_value(None);
                state.device.update(|d| {
                    let _ = d.acknowledge(false);
                });
            }
            if matches!(action, Action::Build) {
                state.prepared.set_value(None);
                state.uploaded.set_value(None);
                state.building.set(false);
            }
            Ok(Some(Action::Poll))
        }
        Action::Upload => {
            let uploaded = parse::<ProjectUploadResponse>(bytes)?;
            let connection = state.connection.get_value().ok_or("Helper session lost")?;
            let revision = state
                .options_revision
                .get_untracked()
                .ok_or("Build counter exhausted; reload Studio")?;
            let accepted = state.prepared.with_value(|p| {
                let prepared = p.as_ref().ok_or("Build snapshot was cancelled")?;
                ui.editor.with_untracked(|editor| {
                    ui.build.with_untracked(|selection| {
                        prepared.authorize(&uploaded, &connection, editor, selection, revision)
                    })
                })
            });
            if let Err(message) = accepted {
                state.view.update(|v| v.message = message.into());
                state.prepared.set_value(None);
                state.building.set(false);
                return Ok(None);
            }
            state.uploaded.set_value(Some(uploaded));
            state
                .view
                .update(|v| v.message = "Project uploaded and identities checked. Starting its frozen build.".into());
            Ok(Some(Action::Build))
        }
        Action::Poll => {
            let batch = parse::<EventBatch>(bytes)?;
            let mut result = Ok(());
            state.view.update(|v| result = v.feed.accept(batch));
            result?;
            if state.view.with_untracked(|v| v.feed.drained()) {
                state.view.update(|v| {
                    use dashboard_helper_protocol::TerminalCause;
                    v.message = match v.feed.summary.as_ref().and_then(|s| s.terminal_cause) {
                        Some(TerminalCause::Succeeded) => "Job completed. Review its stages and results below.",
                        Some(TerminalCause::Cancelled) => {
                            "Job cancelled and cleanup completed. Review the write outcome before retrying."
                        }
                        Some(TerminalCause::Deadline) => {
                            "Job reached its deadline. Review the logs and write outcome before retrying."
                        }
                        Some(TerminalCause::OwnershipCleanupFatal) => {
                            "Cleanup could not be confirmed. Check the helper terminal before starting another job."
                        }
                        _ => "Job failed. Review its stage details and logs before retrying.",
                    }
                    .into();
                });
                Ok(Some(if state.inspecting.get_value() {
                    Action::Inspection
                } else {
                    Action::Report
                }))
            } else {
                Ok(Some(Action::Poll))
            }
        }
        Action::Cancel => {
            let job = parse::<JobSummary>(bytes)?;
            if !state.view.with_untracked(|v| {
                v.feed
                    .summary
                    .as_ref()
                    .is_some_and(|s| s.job_id == job.job_id && s.session_id == job.session_id)
            }) {
                return Err("Cancel response belongs to another job");
            }
            state
                .view
                .update(|v| v.message = "Cancellation requested; waiting for native cleanup to finish.".into());
            Ok(Some(Action::Poll))
        }
        Action::Report => {
            let report: serde_json::Value = serde_json::from_slice(bytes).map_err(|_| "Invalid prerequisite report")?;
            if report["schema_version"] != 1 || !report["checks"].as_array().is_some_and(|v| v.len() <= 64) {
                return Err("Unsupported prerequisite report");
            }
            state.view.update(|v| v.report = Some(report));
            Ok(Some(Action::Devices))
        }
        Action::Devices => {
            let report = parse::<DeviceScanReport>(bytes)?;
            if !state
                .connection
                .with_value(|c| c.as_ref().is_some_and(|c| *c.session() == report.session_id))
            {
                return Err("Device report belongs to another helper session");
            }
            state.view.update(|v| v.devices = Some(report));
            Ok(Some(Action::Artifacts))
        }
        Action::Artifacts => {
            let artifacts: BoundedVec<ArtifactSummary, 2> =
                serde_json::from_slice(bytes).map_err(|_| "Invalid artifact list")?;
            let connection = state.connection.get_value().ok_or("Helper session lost")?;
            for artifact in artifacts.iter() {
                artifact.validate().map_err(|_| "Invalid artifact metadata")?;
                if artifact.session_id != *connection.session() {
                    return Err("Artifact belongs to another helper session");
                }
            }
            state.view.update(|v| v.artifacts = artifacts.iter().cloned().collect());
            Ok(Some(Action::History))
        }
        Action::History => {
            let history = parse::<dashboard_helper_protocol::FlashHistory>(bytes)?;
            state.view.update(|v| v.history = Some(history));
            Ok(None)
        }
        Action::Inspection => {
            let report = parse::<dashboard_helper_protocol::DeviceInspection>(bytes)?;
            let connection = state.connection.get_value().ok_or("Helper session lost")?;
            let mut result = Ok(());
            state.device.update(|d| result = d.accept(&connection, report));
            state.inspecting.set_value(false);
            result?;
            Ok(Some(Action::Report))
        }
        Action::Download => {
            let artifact = state
                .download
                .get_value()
                .ok_or("Artifact download selection changed")?;
            verify_artifact_download(&artifact, bytes)?;
            let name = format!("dashboard-{}.uf2", &artifact.artifact_id.as_str()[..12]);
            io::download(ui, &name, "application/octet-stream", bytes).map_err(|_| "Browser could not save the UF2")?;
            state.download.set_value(None);
            state.view.update(|v| {
                v.message = "UF2 download started after checking its admitted size and SHA256. Downloading does not \
                             program a device."
                    .into()
            });
            Ok(None)
        }
    }
}
fn schedule(
    ui: Ui,
    action: Action,
) {
    let state = ui.helper;
    if let Some(timer) = state.timer.get_value() {
        timer.clear();
    }
    let delay = if matches!(action, Action::Poll) { 800 } else { 0 };
    if let Ok(timer) = set_timeout_with_handle(
        move || {
            let _ = state.timer.try_set_value(None);
            send(ui, action);
        },
        Duration::from_millis(delay),
    ) {
        state.timer.set_value(Some(timer));
    } else {
        state
            .view
            .update(|v| v.message = "Browser polling timer unavailable. Reconnect to resume.".into());
    }
}
fn send(
    ui: Ui,
    action: Action,
) {
    let state = ui.helper;
    if state.busy.get_untracked() {
        return;
    }
    if let Some(timer) = state.timer.get_value() {
        timer.clear();
        state.timer.set_value(None);
    }
    if matches!(action, Action::Connect) {
        let Some(next) = state.epoch.get_value().checked_add(1) else {
            state
                .view
                .update(|v| v.message = "Connection counter exhausted. Reload Studio.".into());
            return;
        };
        state.epoch.set_value(next);
        state.view.update(|v| v.connected = false);
        state.device.update(DeviceSelection::clear);
        state.inspecting.set_value(false);
    }
    if matches!(action, Action::Scan) {
        state.device.update(DeviceSelection::clear);
        state.inspecting.set_value(false);
    }
    state.busy.set(true);
    let epoch = state.epoch.get_value();
    spawn_local(async move {
        let response = request(ui, action).await;
        if state.epoch.try_get_value() != Some(epoch) {
            return;
        }
        state.busy.set(false);
        state.abort.set_value(None);
        let outcome = response.and_then(|bytes| apply(ui, action, &bytes));
        match outcome {
            Ok(next) => {
                if state.cancel.get_untracked() && state.view.with_untracked(|v| v.feed.active()) {
                    state.cancel.set(false);
                    schedule(ui, Action::Cancel);
                } else if let Some(next) = next {
                    schedule(ui, next);
                }
            }
            Err(message) => {
                state.view.update(|v| {
                    v.connected = false;
                    v.message = message.into();
                });
                state.cancel.set(false);
                state.prepared.set_value(None);
                state.uploaded.set_value(None);
                state.building.set(false);
                state.download.set_value(None);
                state.flashing.set_value(None);
                state.device.update(DeviceSelection::clear);
                state.inspecting.set_value(false);
                ui.event(Notice {
                    severity: dashboard_configurator::events::Severity::Warn,
                    ..Notice::info(Category::HostTools, "helper.request_failed", None, message)
                });
            }
        }
    });
}

pub fn options_changed(ui: Ui) {
    ui.helper
        .options_revision
        .update(|value| *value = value.and_then(|v| v.checked_add(1)));
}
pub fn build_disabled(ui: Ui) -> bool {
    let state = ui.helper;
    state.busy.get()
        || state.building.get()
        || state.options_revision.get().is_none()
        || !state.view.with(|v| v.connected)
        || state.view.with(|v| v.feed.active())
        || ui.editor.with(|e| e.action_readiness().block.is_some())
        || ui.build.with(|s| s.normalized().is_err())
}
pub fn start_build(ui: Ui) {
    if build_disabled(ui) {
        return;
    }
    let state = ui.helper;
    let Some(revision) = state.options_revision.get_untracked() else {
        return;
    };
    let prepared = ui.editor.with_untracked(|editor| {
        ui.build
            .with_untracked(|selection| PreparedBuild::capture(editor, selection, revision))
    });
    match prepared {
        Ok(prepared) => {
            state.prepared.set_value(Some(prepared));
            state.uploaded.set_value(None);
            state.building.set(true);
            state.view.update(|v| {
                v.message = "Uploading the accepted project. Original artwork stays in Studio; only converted pixels \
                             enter the build."
                    .into()
            });
            send(ui, Action::Upload);
        }
        Err(message) => state.view.update(|v| v.message = message.into()),
    }
}
fn stage_label(stage: dashboard_helper_protocol::StageId) -> String {
    use dashboard_helper_protocol::{FirmwareStage, ProbeStage, StageId};
    match stage {
        StageId::Probe(ProbeStage::Prerequisites) => "Installed tools".into(),
        StageId::Probe(ProbeStage::DeviceDetection) => "BOOTSEL device discovery".into(),
        StageId::Firmware(stage) => {
            match stage {
                FirmwareStage::Prerequisites => "Required tools",
                FirmwareStage::Snapshot => "Freeze project & source",
                FirmwareStage::Compilation => "Compile Pico 2 firmware",
                FirmwareStage::Validation => "Validate firmware image",
                FirmwareStage::DeviceSelection => "Recheck selected Pico 2",
                FirmwareStage::Transfer => "Write firmware",
                FirmwareStage::Verification => "Verify written bytes",
                FirmwareStage::Reboot => "Request ARM reboot",
            }
            .into()
        }
        _ => format!("{stage:?}"),
    }
}
#[component]
pub fn Panel(ui: Ui) -> impl IntoView {
    let state = ui.helper;
    view! {
        <section class="build-card helper-panel" aria-labelledby="helper-title"><div class="build-section-title"><span>"LOCAL"</span><h2 id="helper-title">"Computer & devices"</h2></div>
            <p>"Run the helper in a terminal, then connect here. Connecting only reads status; checks and scans run when you request them."</p><code class="helper-command">".\\configurator\\Studio.ps1 -Helper"</code>
            <div class="helper-actions"><button class="primary" disabled=move ||state.busy.get()||state.building.get() on:click=move |_|send(ui,Action::Connect)>{move ||if state.view.with(|v|v.connected){"Reconnect"}else{"Connect local helper"}}</button>
                <button disabled=move ||state.busy.get()||state.building.get()||!state.view.with(|v|v.connected)||state.view.with(|v|v.feed.active()) on:click=move |_|send(ui,Action::Check)>"Check prerequisites"</button>
                <button disabled=move ||state.busy.get()||state.building.get()||!state.view.with(|v|v.connected)||state.view.with(|v|v.feed.active()) on:click=move |_|send(ui,Action::Scan)>"Find BOOTSEL devices"</button>
                <button disabled=move ||!state.view.with(|v|v.connected&&v.feed.active())||state.cancel.get() on:click=move |_|{if state.busy.get_untracked(){state.cancel.set(true);}else{send(ui,Action::Cancel);}}>"Cancel job"</button></div>
            <p class="helper-status" role="status"><span class="chip">{move ||if state.busy.get(){"COMMUNICATING"}else if state.view.with(|v|v.connected){"CONNECTED"}else{"DISCONNECTED"}}</span>{move ||state.view.with(|v|v.message.clone())}</p>
            <small class="build-note">"Open Studio at localhost:8080 or 127.0.0.1:8080. The connection stays in memory. Reloading the page does not cancel a native job; reconnect to resume its status."</small>
            {move ||state.view.with(|v|v.feed.summary.clone()).map(|job|view!{<div class="helper-stages">{job.stages.iter().map(|s|view!{<div><strong>{stage_label(s.stage)}</strong><span>{format!("{:?}",s.state)}</span><small>{s.reason.as_ref().map(|r|r.as_str().to_string()).unwrap_or_default()}</small></div>}).collect_view()}</div>})}
            {move ||state.view.with(|v|v.report.clone()).map(|report|view!{<prerequisites::Results report/>})}
            {move ||state.view.with(|v|v.feed.summary.clone()).filter(|job|job.operation==dashboard_helper_protocol::OperationKind::Flash).map(|job| {
                use dashboard_helper_protocol::WriteOutcome;
                let status=match job.write_outcome {WriteOutcome::NotStarted=>"No write started",WriteOutcome::InProgress=>"Write in progress",WriteOutcome::TransferredUnverified=>"Transferred · verification pending",WriteOutcome::Verified=>"Written bytes verified",WriteOutcome::Unknown=>"Write result unknown · inspect before retrying"};
                view!{<div class="helper-write-outcome" role="status"><strong>{status}</strong><small>"A successful reboot request does not prove that the firmware is running correctly. Check the physical display afterward."</small></div>}
            })}
            <devices::Panel ui/>
            <artifacts::Panel ui/>
            <history::Panel ui/>
            <details class="build-disclosure" open><summary>"Job logs"</summary><p>{move ||state.view.with(|v|format!("{} retained events · {} omitted by bounded history",v.feed.events.len(),v.feed.omitted))}</p><div class="helper-log" role="log" aria-live="off">{move ||state.view.with(|v|v.feed.events.iter().map(|e|view!{<div class=match e.level{dashboard_helper_protocol::Level::Error=>"error",dashboard_helper_protocol::Level::Warning=>"warning",_=>"info"}><span>{format!("{:>6} ms",e.elapsed_ms)}</span><code>{e.text.as_str().to_string()}</code></div>}).collect_view())}</div>
                <button disabled=move ||state.view.with(|v|v.feed.events.is_empty()) on:click=move |_|{let text=state.view.with_untracked(|v|v.feed.events.iter().map(|e|format!("{} ms {:?} {}",e.elapsed_ms,e.level,e.text.as_str())).collect::<Vec<_>>().join("\n"));if io::download(ui,"studio-helper.log","text/plain;charset=utf-8",text.as_bytes()).is_err(){state.view.update(|v|v.message="Could not download job logs.".into());}}>"Download logs"</button>
            </details>
        </section>
    }
}

/// The relay owns its queue and cancellation lease; authority stays in this module.
pub fn relay_epoch(ui: Ui) -> Option<u64> {
    if ui.helper.view.try_with(|v| v.connected).unwrap_or(false) {
        ui.helper.epoch.try_get_value()
    } else {
        None
    }
}
pub async fn relay_post(
    ui: Ui,
    epoch: u64,
    body: &[u8],
    abort: &AbortController,
) -> Result<Vec<u8>, &'static str> {
    if body.len() > 16384 || relay_epoch(ui) != Some(epoch) {
        return Err("Diagnostic connection changed");
    }
    let connection = ui
        .helper
        .connection
        .get_value()
        .ok_or("Diagnostic connection unavailable")?;
    let init = RequestInit::new();
    init.set_method("POST");
    init.set_signal(Some(&abort.signal()));
    init.set_mode(web_sys::RequestMode::Cors);
    init.set_credentials(web_sys::RequestCredentials::Omit);
    init.set_cache(web_sys::RequestCache::NoStore);
    init.set_redirect(web_sys::RequestRedirect::Error);
    init.set_body(&js_sys::Uint8Array::from(body));
    let request = Request::new_with_str_and_init(&format!("{BASE}/diagnostics"), &init)
        .map_err(|_| "Cannot create diagnostic request")?;
    request
        .headers()
        .set("X-Studio-Capability", connection.capability())
        .map_err(|_| "Cannot authenticate diagnostic request")?;
    request
        .headers()
        .set("Content-Type", "application/json")
        .map_err(|_| "Cannot set diagnostic content type")?;
    let reply = fetch_bounded(&request, dashboard_helper_protocol::diagnostics::Response::MAX_BYTES).await?;
    if relay_epoch(ui) != Some(epoch) {
        return Err("Diagnostic connection changed");
    }
    Ok(reply)
}
