//! Opt-in, bounded browser evidence. No telemetry is transmitted.
use serde::Serialize;
use wasm_bindgen::closure::Closure;

use super::*;

#[derive(Clone, Default, Serialize)]
pub struct Trace {
    operations: Operations,
    id: u64,
    enabled: bool,
    long_tasks_supported: bool,
    start_ms: f64,
    end_ms: f64,
    samples: Vec<Sample>,
    long_tasks: Vec<(f64, f64)>,
    dropped: usize,
    in_flight: u32,
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
    json: bool,
    bytes: usize,
    started: f64,
) {
    let timing = FileTiming {
        format: if json { "json" } else { "eav" },
        bytes,
        elapsed_ms: (now() - started).max(0.0),
    };
    ui.trace.update(|t| {
        if import {
            t.operations.last_import = Some(timing);
        } else if json {
            t.operations.export_json = Some(timing);
        } else {
            t.operations.export_binary = Some(timing);
        }
    });
}

impl Trace {
    fn includes(
        &self,
        time: f64,
    ) -> bool {
        self.id != 0 && time >= self.start_ms && (self.enabled || time <= self.end_ms)
    }
}

pub fn begin(
    ui: Ui,
    input: f64,
) -> Option<u64> {
    let mut id = None;
    ui.trace.update(|t| {
        if t.includes(input) {
            t.in_flight += 1;
            id = Some(t.id);
        }
    });
    id
}

#[derive(Clone, Serialize)]
struct Sample {
    input_ms: f64,
    canvas_ms: f64,
    presentation_boundary_ms: f64,
    render_ms: f64,
    widgets: usize,
    views: usize,
    warning_preview: bool,
}

pub fn record(
    ui: Ui,
    id: Option<u64>,
    input: f64,
    canvas: f64,
    render: f64,
    dimensions: (usize, usize),
    warnings: bool,
) {
    let boundary = now();
    let _ = ui.trace.try_update(|trace| {
        if id != Some(trace.id) {
            return;
        }
        trace.in_flight = trace.in_flight.saturating_sub(1);
        if !trace.includes(input) || !boundary.is_finite() || boundary < input {
            return;
        }
        if trace.samples.len() == 512 {
            trace.dropped += 1;
            return;
        }
        trace.samples.push(Sample {
            input_ms: input,
            canvas_ms: canvas,
            presentation_boundary_ms: boundary,
            render_ms: render,
            widgets: dimensions.0,
            views: dimensions.1,
            warning_preview: warnings,
        });
    });
}

fn record_tasks(
    ui: Ui,
    entries: js_sys::Array,
) {
    let _ = ui.trace.try_update(|trace| {
        for value in entries.iter() {
            let entry = value.unchecked_into::<web_sys::PerformanceEntry>();
            if !trace.includes(entry.start_time()) {
                continue;
            }
            if trace.long_tasks.len() == 512 {
                trace.dropped += 1;
                break;
            }
            trace.long_tasks.push((entry.start_time(), entry.duration()));
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
    if trace.samples.is_empty() {
        return "No recorded interactions".into();
    }
    let mut values = trace
        .samples
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
        trace.long_tasks.len(),
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
            <p class="hint">"Warm the preview, then record edits. Measures handler entry to the next presentation opportunity after canvas submission, not physical screen FPS. Up to 512 inputs and long tasks, kept locally."</p>
            <p class="trace-summary">{move ||ui.trace.with(summary)}</p>
            <p class="hint">{move ||ui.trace.with(|t|t.operations.startup.as_ref().map(|s|format!("First preview: {:.1} ms after navigation; {:.1} ms after Rust mount. File operation timings are included in the exported trace.",s.navigation_to_preview_ms,s.mount_to_preview_ms)).unwrap_or_else(||"Waiting for first preview".into()))}</p>
            <div class="field-pair"><button on:click=move |_|{
                ui.pending_at.set(None);
                drain(ui);
                ui.trace.update(|t|{let id=t.id.checked_add(1).expect("trace identity exhausted");let supported=t.long_tasks_supported;let operations=t.operations.clone();*t=Trace{id,enabled:true,long_tasks_supported:supported,start_ms:now(),operations,..Trace::default()};});
            }>"Start new trace"</button><button on:click=move |_|{ui.trace.update(|t|{t.enabled=false;t.end_ms=now();});drain(ui);}>"Stop trace"</button></div>
            <p class="hint">{move ||ui.trace.with(|t|if t.enabled{"Recording interactions"}else{"Recording stopped"})}</p>
            <button disabled=move ||ui.trace.with(|t|t.enabled || t.in_flight!=0) || ui.pending_at.get().is_some() on:click=move |_|{
                drain(ui);
                let json=ui.trace.with_untracked(serde_json::to_vec_pretty);
                match json {Ok(bytes)=>{if io::download("studio-performance.json","application/json",&bytes).is_err(){ui.notice.set("Could not download performance trace".into());}},Err(_)=>ui.notice.set("Could not serialize performance trace".into())}
            }>"Export performance trace"</button>
        </details>
    }
}
