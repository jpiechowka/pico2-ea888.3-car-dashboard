//! One persistent bounded store, rendered as Leptos text nodes.
use dashboard_configurator::event_adapters::{
    ConsoleSink,
    PauseState,
    TransferFailure,
    checked_wall_clock,
    emit,
    transfer_failure,
};
use dashboard_configurator::events::{Category, DiagnosticCount, EventFilter, EventStatus, Severity};
use wasm_bindgen_futures::{JsFuture, spawn_local};

use super::*;

struct BrowserConsole;
// The pinned web-sys write_text binding returns a Promise but does not catch a
// synchronous host exception. Keep the same Clipboard API with a caught call.
#[wasm_bindgen::prelude::wasm_bindgen]
extern "C" {
    #[wasm_bindgen::prelude::wasm_bindgen(extends=web_sys::Clipboard,js_name=Clipboard)]
    type ClipboardWriter;
    #[wasm_bindgen::prelude::wasm_bindgen(method,catch,js_name=writeText)]
    fn write_text(
        this: &ClipboardWriter,
        text: &str,
    ) -> Result<js_sys::Promise, wasm_bindgen::JsValue>;
}
impl ConsoleSink for BrowserConsole {
    fn write(
        &mut self,
        severity: Severity,
        text: &str,
    ) {
        let value = wasm_bindgen::JsValue::from_str(text);
        match severity {
            Severity::Error => web_sys::console::error_1(&value),
            Severity::Warn => web_sys::console::warn_1(&value),
            Severity::Debug | Severity::Trace => web_sys::console::debug_1(&value),
            Severity::Info => web_sys::console::info_1(&value),
        }
    }
}

pub(super) fn record(
    ui: Ui,
    notice: &Notice,
) {
    let elapsed = now() - ui.mount_started;
    let elapsed = if elapsed.is_finite() && elapsed >= 0. && elapsed < u64::MAX as f64 {
        elapsed.floor() as u64
    } else {
        0
    };
    let wall = checked_wall_clock(js_sys::Date::now());
    let mut outcome = Ok(());
    let enabled = ui.console_enabled.try_get_untracked().unwrap_or(false);
    if ui.events.try_with_untracked(|store| store.status()) != Some(EventStatus::Accepting) {
        let _ = ui
            .notice
            .try_set("Diagnostics recording stopped: sequence exhausted".into());
        return;
    }
    let _ = ui.events.try_update(|store| {
        outcome = emit(store, &mut BrowserConsole, enabled, notice.input(elapsed, wall)).map(|_| ());
    });
    if outcome.is_err() {
        let _ = ui
            .notice
            .try_set("Diagnostics identity exhausted or event tag invalid; recording stopped".into());
    }
}

fn count(count: DiagnosticCount) -> String {
    format!("{}{}", count.value, if count.saturated { " or more" } else { "" })
}

const SEVERITIES: [Severity; 5] = [
    Severity::Trace,
    Severity::Debug,
    Severity::Info,
    Severity::Warn,
    Severity::Error,
];
const CATEGORIES: [Category; 8] = [
    Category::Editor,
    Category::Validation,
    Category::Storage,
    Category::ImportExport,
    Category::Simulation,
    Category::Rendering,
    Category::HostTools,
    Category::Device,
];

fn filter(
    query: &str,
    minimum: usize,
    category: usize,
) -> EventFilter {
    let filter = EventFilter::new(query).minimum(SEVERITIES.get(minimum).copied().unwrap_or(Severity::Trace));
    CATEGORIES.get(category.wrapping_sub(1)).map_or(filter, |category| {
        EventFilter::new(query)
            .minimum(SEVERITIES.get(minimum).copied().unwrap_or(Severity::Trace))
            .category(*category)
    })
}

fn failed(
    ui: Ui,
    failure: TransferFailure,
) {
    let event = transfer_failure(failure);
    let _ = ui.notice.try_set(event.message.clone());
    ui.event(event);
}

fn copy(
    ui: Ui,
    bytes: Vec<u8>,
    fallback: RwSignal<String>,
    busy: RwSignal<bool>,
) {
    let Ok(text) = String::from_utf8(bytes) else {
        failed(ui, TransferFailure::Serialization);
        return;
    };
    if busy.get_untracked() {
        return;
    }
    busy.set(true);
    // Invoke writeText synchronously in the user gesture. Reflect's caught call
    // also handles unavailable getters and synchronous host exceptions.
    let promise = (|| {
        let navigator = web_sys::window().ok_or(wasm_bindgen::JsValue::NULL)?.navigator();
        let clipboard = js_sys::Reflect::get(navigator.as_ref(), &"clipboard".into())?;
        if clipboard.is_null() || clipboard.is_undefined() {
            return Err(wasm_bindgen::JsValue::NULL);
        }
        clipboard.unchecked_into::<ClipboardWriter>().write_text(&text)
    })();
    spawn_local(async move {
        let result = match promise {
            Ok(promise) => JsFuture::from(promise).await,
            Err(error) => Err(error),
        };
        let _ = busy.try_set(false);
        if fallback.try_with_untracked(|_| ()).is_none() {
            return;
        }
        if result.is_err() {
            let _ = fallback.try_set(text);
            failed(ui, TransferFailure::Clipboard);
        } else {
            let _ = fallback.try_set(String::new());
            let _ = ui.notice.try_set("Filtered diagnostic JSONL copied".into());
        }
    });
}

#[component]
pub fn EventLog(ui: Ui) -> impl IntoView {
    let query = RwSignal::new(String::new());
    let minimum = RwSignal::new(0usize);
    let category = RwSignal::new(0usize);
    let pause = RwSignal::new(PauseState::default());
    let debug = RwSignal::new(false);
    let fallback = RwSignal::new(String::new());
    let copy_busy = RwSignal::new(false);
    let rows: NodeRef<leptos::html::Div> = NodeRef::new();
    let queued = StoredValue::new(false);
    Effect::new(move |_| {
        ui.events.track();
        query.track();
        minimum.track();
        category.track();
        if pause.get().is_paused() || rows.get().is_none() || queued.get_value() {
            return;
        }
        queued.set_value(true);
        let result = leptos::leptos_dom::helpers::request_animation_frame_with_handle(move || {
            if queued.try_update_value(|queued| *queued = false).is_none()
                || pause.try_get_untracked().is_none_or(|state| state.is_paused())
            {
                return;
            }
            if let Some(Some(element)) = rows.try_get_untracked() {
                element.set_scroll_top(element.scroll_height());
            }
        });
        if result.is_err() {
            let _ = queued.try_update_value(|queued| *queued = false);
            let _ = ui
                .notice
                .try_set("Live log scrolling unavailable; retained rows remain selectable".into());
        }
    });
    view! {
        <section class="event-log" aria-label="Diagnostic events">
            <h2>"Event log"</h2>
            <p class="hint">"512 retained events · local only · helper job logs are separate. Routine samples, keystrokes and successful frames are quiet."</p>
            <div class="event-filters">
                <label class="field"><span>"Minimum severity"</span><select on:change=move |event|minimum.set(event_target_value(&event).parse().unwrap_or(0))>
                    {SEVERITIES.into_iter().enumerate().map(|(index,severity)|view!{<option value=index.to_string()>{format!("{severity:?}")}</option>}).collect_view()}
                </select></label>
                <label class="field"><span>"Category"</span><select on:change=move |event|category.set(event_target_value(&event).parse().unwrap_or(0))><option value="0">"All categories"</option>
                    {CATEGORIES.into_iter().enumerate().map(|(index,category)|view!{<option value=(index+1).to_string()>{format!("{category:?}")}</option>}).collect_view()}
                </select></label>
                <label class="field"><span>"Search retained events"</span><input type="search" maxlength="128" prop:value=move ||query.get() on:input=move |event|{
                    let input=event_target::<web_sys::HtmlInputElement>(&event);if let Some(text)=io::bounded_input(&input,128){query.set(text);}else{input.set_value(&query.get_untracked());}
                }/></label>
            </div>
            <div class="event-actions">
                <button on:click=move |_|{let snapshot=ui.events.with_untracked(|store|store.snapshot(&EventFilter::default().limit(0)));pause.update(|state|state.pause(&snapshot));}>"Pause"</button>
                <button on:click=move |_|pause.update(PauseState::resume)>"Resume live"</button>
                <button on:click=move |_|{ui.events.update(|store|store.clear());fallback.set(String::new());}>"Clear retained events"</button>
                <button disabled=move ||copy_busy.get() on:click=move |_|{
                    let json=ui.events.with_untracked(|store|store.snapshot(&filter(&query.get_untracked(),minimum.get_untracked(),category.get_untracked())).jsonl());
                    match json {Ok(bytes)=>copy(ui,bytes,fallback,copy_busy),Err(_)=>failed(ui,TransferFailure::Serialization)}
                }>"Copy filtered JSONL"</button>
                <button on:click=move |_|{
                    let json=ui.events.with_untracked(|store|store.snapshot(&filter(&query.get_untracked(),minimum.get_untracked(),category.get_untracked())).jsonl());
                    match json {Ok(bytes)=>{if io::download(ui,"studio-events.jsonl","application/x-ndjson",&bytes).is_err(){failed(ui,TransferFailure::Download);}},Err(_)=>failed(ui,TransferFailure::Serialization)}
                }>"Download filtered JSONL"</button>
            </div>
            <div class="event-console">
                <label><input type="checkbox" prop:checked=move ||ui.console_enabled.get() on:change=move |event|ui.console_enabled.set(event_target_checked(&event))/><span>"Browser console output (Warn/Error by default)"</span></label>
                <label><input type="checkbox" prop:checked=move ||debug.get() on:change=move |event|{let enabled=event_target_checked(&event);debug.set(enabled);ui.events.update(|store|store.set_debug_console(enabled));}/><span>"Opt in to Debug/Trace console · shared 10 per rolling second"</span></label>
            </div>
            <p class="hint" role="status">{move ||ui.events.with(|store|{
                let view=store.snapshot(&filter(&query.get(),minimum.get(),category.get()));let report=pause.get().report(store);
                format!("{} retained · {} matching · {} omitted · {} evicted · {} repeated · {} console suppressed{}{}{}",view.retained,view.matching,view.omitted,count(view.counters.dropped),count(view.counters.repeated),count(view.counters.console_suppressed),if view.status==EventStatus::Exhausted{" · RECORDING EXHAUSTED"}else{""},if report.paused {format!(" · PAUSED · {} new events",count(report.new_events))}else{" · LIVE".into()},if report.gap {report.anchor.map(|seq|format!(" · GAP: paused anchor evicted; oldest retained #{}",seq.get())).unwrap_or_else(||" · GAP: paused anchor evicted; no retained records".into())}else{String::new()})
            })}</p>
            <div class="event-rows" node_ref=rows role="log" aria-label="Retained diagnostic rows" aria-live="off">
                <For each=move ||ui.events.with(|store|{
                    let snapshot=store.snapshot(&filter(&query.get(),minimum.get(),category.get()));let report=pause.get().report(store);
                    snapshot.records().iter().filter(|event|!report.paused || report.anchor.is_some_and(|anchor|event.seq.get()<=anchor.get())).cloned().collect::<Vec<_>>()
                }) key=|event|event.seq.get() children=move |event|{let seq=event.seq;view!{
                    <article class="event-row"><header><strong>{format!("#{} {:?} {:?}",event.seq.get(),event.severity,event.category)}</strong><span>{move ||ui.events.with(|store|store.get(seq).map(|event|format!("{} ms · {} ×",event.last_elapsed_ms,count(event.count))).unwrap_or_default())}</span></header>
                        <code>{event.code.get()}</code><b>{event.subject.unwrap_or_default()}</b><p>{event.message}</p>
                        <small>{if event.message_truncated || event.subject_truncated {"Text truncated at its UTF-8 byte limit"}else{""}}</small>
                    </article>
                }}/>
            </div>
            <label class="field event-fallback" hidden=move ||fallback.get().is_empty()><span>"Clipboard failed: select and copy this JSONL"</span><textarea readonly rows="8" prop:value=move ||fallback.get()></textarea></label>
        </section>
    }
}
