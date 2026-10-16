//! Opt-in, bounded browser evidence. No telemetry is transmitted.
use std::cell::RefCell;
use std::rc::Rc;

use dashboard_configurator::timing::{TraceCapture, TracePermit, TraceSample, TraceStatus};
use serde::Serialize;
use wasm_bindgen::closure::Closure;

use super::*;

#[derive(Default, Serialize)]
pub struct Trace {
    operations: Operations,
    long_tasks_supported: bool,
    #[serde(flatten)]
    capture: TraceCapture,
}

#[derive(Clone, Default, Serialize)]
struct Operations {
    startup: Option<Startup>,
    last_import: Option<FileTiming>,
    export_json: Option<FileTiming>,
    export_binary: Option<FileTiming>,
}

#[derive(Clone, Serialize)]
struct Startup {
    navigation_to_preview_ms: f64,
    mount_to_preview_ms: f64,
    widgets: usize,
    views: usize,
}

#[derive(Clone, Serialize)]
struct FileTiming {
    format: &'static str,
    bytes: usize,
    elapsed_ms: f64,
}

pub fn first_frame(ui: Ui) {
    if ui.trace.with_untracked(|t| t.operations.startup.is_some()) {
        return;
    }
    let (widgets, views) = ui
        .editor
        .with_untracked(|e| (e.preview().config().widgets.len(), e.preview().config().views.len()));
    leptos::leptos_dom::helpers::request_animation_frame(move || {
        let boundary = now();
        let _ = ui.trace.try_update(|t| {
            if t.operations.startup.is_none() {
                t.operations.startup = Some(Startup {
                    navigation_to_preview_ms: boundary,
                    mount_to_preview_ms: (boundary - ui.mount_started).max(0.0),
                    widgets,
                    views,
                });
            }
        });
    });
}

pub fn file_operation(
    ui: Ui,
    import: bool,
    format: ImportFormat,
    bytes: usize,
    started: f64,
) {
    let timing = FileTiming {
        format: match format {
            ImportFormat::Project => "eavp",
            ImportFormat::Json => "json",
            ImportFormat::Binary => "eav",
        },
        bytes,
        elapsed_ms: (now() - started).max(0.0),
    };
    ui.trace.update(|t| {
        if import {
            t.operations.last_import = Some(timing);
        } else if matches!(format, ImportFormat::Json) {
            t.operations.export_json = Some(timing);
        } else {
            t.operations.export_binary = Some(timing);
        }
    });
}

pub fn begin(
    ui: Ui,
    input: f64,
) -> Option<TracePermit> {
    let mut permit = None;
    ui.trace.update(|t| {
        permit = t.capture.begin(input).ok();
    });
    permit
}

/// The shared cell contains one non-clone permit, never a second authority.
/// Explicit scheduling failure abandons immediately; dropping a canceled
/// callback also abandons. Disposal uses fallible signal access.
struct PendingTrace {
    ui: Ui,
    permit: Option<TracePermit>,
}

impl PendingTrace {
    fn abandon(&mut self) {
        if let Some(mut permit) = self.permit.take() {
            let _ = self.ui.trace.try_update(|t| {
                let _ = t.capture.abandon(&mut permit);
            });
        }
    }
}

impl Drop for PendingTrace {
    fn drop(&mut self) { self.abandon(); }
}

/// Schedule the existing presentation-opportunity callback once. Leptos's
/// no-handle helper discards errors and frees uncanceled callback captures only
/// when invoked/GC'd, so keep an explicit error-side reference to the permit.
pub fn schedule_record(
    ui: Ui,
    permit: Option<TracePermit>,
    input: f64,
    canvas: f64,
    render: f64,
    dimensions: (usize, usize),
    warnings: bool,
) {
    let Some(permit) = permit else {
        return;
    };
    let pending = Rc::new(RefCell::new(PendingTrace {
        ui,
        permit: Some(permit),
    }));
    let callback_pending = Rc::clone(&pending);
    let result = leptos::leptos_dom::helpers::request_animation_frame_with_handle(move || {
        let Some(mut permit) = callback_pending.borrow_mut().permit.take() else {
            return;
        };
        let sample = TraceSample {
            input_ms: input,
            canvas_ms: canvas,
            presentation_boundary_ms: now(),
            render_ms: render,
            widgets: dimensions.0,
            views: dimensions.1,
            warning_preview: warnings,
        };
        let _ = ui.trace.try_update(|trace| {
            let _ = trace.capture.finish(&mut permit, sample);
        });
        ui.render_outcome(RenderLane::Timing, true);
    });
    if result.is_err() {
        ui.render_outcome(RenderLane::Timing, false);
        pending.borrow_mut().abandon();
        let _ = ui
            .notice
            .try_set("Could not schedule browser performance timing".into());
    }
}

fn record_tasks(
    ui: Ui,
    entries: js_sys::Array,
) {
    let _ = ui.trace.try_update(|trace| {
        for value in entries.iter() {
            let entry = value.unchecked_into::<web_sys::PerformanceEntry>();
            let _ = trace.capture.record_long_task(entry.start_time(), entry.duration());
        }
    });
}

fn drain(ui: Ui) {
    if let Some(observer) = ui.observer.get_untracked() {
        record_tasks(ui, observer.take_records());
    }
}

pub fn install(ui: Ui) {
    let observer_type = js_sys::Reflect::get(&js_sys::global(), &"PerformanceObserver".into()).ok();
    let supported = observer_type
        .and_then(|t| js_sys::Reflect::get(&t, &"supportedEntryTypes".into()).ok())
        .is_some_and(|types| js_sys::Array::from(&types).includes(&"longtask".into(), 0));
    if !supported {
        return;
    }
    let callback = Closure::<dyn FnMut(web_sys::PerformanceObserverEntryList)>::new(
        move |list: web_sys::PerformanceObserverEntryList| {
            record_tasks(ui, list.get_entries());
        },
    );
    let Ok(observer) = web_sys::PerformanceObserver::new(callback.as_ref().unchecked_ref()) else {
        return;
    };
    let types = js_sys::Array::new();
    types.push(&"longtask".into());
    observer.observe(&web_sys::PerformanceObserverInit::new(&types));
    ui.trace.update(|t| t.long_tasks_supported = true);
    ui.observer.set(Some(observer.clone()));
    let owned = StoredValue::new_local((observer, callback));
    on_cleanup(move || {
        let _ = owned.try_with_value(|(observer, _)| observer.disconnect());
        owned.dispose();
    });
}

fn summary(trace: &Trace) -> String {
    if trace.capture.samples().is_empty() {
        return "No recorded interactions".into();
    }
    let mut values = trace
        .capture
        .samples()
        .iter()
        .map(|s| s.presentation_boundary_ms - s.input_ms)
        .collect::<Vec<_>>();
    values.sort_by(f64::total_cmp);
    let index = (values.len() * 95).div_ceil(100) - 1;
    format!(
        "{} inputs · p95 {:.1} ms · max {:.1} ms · {} long tasks{}",
        values.len(),
        values[index],
        values[values.len() - 1],
        trace.capture.long_tasks().len(),
        if trace.long_tasks_supported {
            ""
        } else {
            " (unsupported)"
        }
    )
}

#[component]
pub fn Measurements(ui: Ui) -> impl IntoView {
    view! {
        <details class="measurements"><summary>"Browser performance trace"</summary>
            <p class="hint">"Warm the preview, then record edits. Measures handler entry to the next browser frame opportunity after canvas submission and CPU work. Up to 512 inputs and 512 long tasks, kept locally."</p>
            <p class="trace-summary">{move ||ui.trace.with(summary)}</p>
            <p class="hint">{move ||ui.trace.with(|t|t.operations.startup.as_ref().map(|s|format!("First preview: {:.1} ms after navigation; {:.1} ms after Rust mount. File operation timings are included in the exported trace.",s.navigation_to_preview_ms,s.mount_to_preview_ms)).unwrap_or_else(||"Waiting for first preview".into()))}</p>
            <div class="field-pair"><button on:click=move |_|{
                ui.pending_at.set(None);
                drain(ui);
                ui.trace.update(|t|{let _=t.capture.start(now());});
            } disabled=move ||ui.trace.with(|t|matches!(t.capture.status(),TraceStatus::Exhausted(_)))>"Start new trace"</button><button on:click=move |_|{ui.trace.update(|t|{let _=t.capture.stop(now());});drain(ui);}>"Stop trace"</button></div>
            <p class="hint" role="status">{move ||ui.trace.with(|t|match t.capture.status(){TraceStatus::Recording=>"Recording interactions",TraceStatus::Stopped=>"Recording stopped",TraceStatus::Exhausted(_)=>"Trace capacity exhausted; collection stopped. Reload Studio to record again."})}</p>
            <p class="hint">{move ||ui.trace.with(|t|{let dropped=t.capture.dropped();format!("{}{} trace records omitted",dropped.value,if dropped.saturated{" or more"}else{""})})}</p>
            <button disabled=move ||ui.trace.with(|t|t.capture.status()==TraceStatus::Recording || t.capture.in_flight()!=0) || ui.pending_at.get().is_some() on:click=move |_|{
                drain(ui);
                let json=ui.trace.with_untracked(serde_json::to_vec_pretty);
                match json {Ok(bytes)=>{if io::download(ui,"studio-performance.json","application/json",&bytes).is_err(){ui.event(dashboard_configurator::event_adapters::transfer_failure(dashboard_configurator::event_adapters::TransferFailure::Download));ui.notice.set("Could not download performance trace".into());}},Err(_)=>{ui.event(dashboard_configurator::event_adapters::transfer_failure(dashboard_configurator::event_adapters::TransferFailure::Serialization));ui.notice.set("Could not serialize performance trace".into())}}
            }>"Export performance trace"</button>
        </details>
    }
}
