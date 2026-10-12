mod controls;
mod fields;
mod inspector;
mod io;
mod metrics;
mod stage;
mod widget_menu;

use dashboard_configurator::config::*;
use dashboard_configurator::editor::*;
use fields::{Choice, ColorField, TextField, Toggle, choices, error_text};
use inspector::{FuelSettings, Inspector, SourceSettings, WarningSettingsPanel};
use leptos::leptos_dom::helpers::{event_target_checked, event_target_value};
use leptos::prelude::*;
use stage::{Canvas, Library};
use wasm_bindgen::JsCast;

#[derive(Clone, Copy)]
pub struct Ui {
    mount_started: f64,
    cache_seen: RwSignal<dashboard_core::cache_diagnostics::CacheExhaustionSeen>,
    editor: RwSignal<Editor>,
    notice: RwSignal<String>,
    conflict: RwSignal<bool>,
    saved_bytes: RwSignal<Option<String>>,
    psi: RwSignal<bool>,
    warnings: RwSignal<bool>,
    sample_ms: RwSignal<u64>,
    sample_set: RwSignal<dashboard_configurator::preview::SampleSet>,
    preview_error: RwSignal<Option<String>>,
    pending_at: RwSignal<Option<f64>>,
    render_ms: RwSignal<f64>,
    canvas: NodeRef<leptos::html::Canvas>,
    drag: RwSignal<Option<stage::Drag>>,
    capture: RwSignal<Option<(i32, web_sys::Element)>>,
    menu: RwSignal<Option<widget_menu::Menu>>,
    playing: RwSignal<bool>,
    trace: RwSignal<metrics::Trace>,
    observer: RwSignal<Option<web_sys::PerformanceObserver>>,
}

fn now() -> f64 { web_sys::window().and_then(|w| w.performance()).map_or(0.0, |p| p.now()) }

impl Ui {
    fn observe_cache(
        self,
        status: dashboard_core::cache_diagnostics::CacheExhaustionStatus,
    ) {
        self.cache_seen.update(|seen| {
            dashboard_configurator::cache_reporting::report(
                seen,
                status,
                |cause| self.notice.set(cause.to_string()),
                |cause| web_sys::console::warn_1(&cause.to_string().into()),
            )
        });
    }

    fn invalid(self) -> bool { self.editor.with(Editor::has_invalid_draft) }

    fn layout_locked(self) -> bool { self.editor.with(|e| !e.can_edit_layout()) }

    fn run(
        self,
        action: impl FnOnce(&mut Editor) -> Result<Revision, EditorError>,
    ) -> bool {
        let started = now();
        let mut result = Ok(());
        let mut changed = false;
        self.editor.update(|editor| {
            let before = editor.revision();
            result = action(editor).map(|_| ());
            changed = before != editor.revision();
        });
        if changed && self.pending_at.get_untracked().is_none() {
            self.pending_at.set(Some(started));
        }
        match result {
            Ok(()) => {
                self.notice.set(String::new());
                if changed && !self.editor.with_untracked(Editor::has_invalid_draft) && !self.conflict.get_untracked() {
                    io::save(self, false);
                }
                true
            }
            Err(error) => {
                self.notice.set(error_text(error));
                false
            }
        }
    }

    fn input_started(self) {
        if self.pending_at.get_untracked().is_none() {
            self.pending_at.set(Some(now()));
        }
    }

    fn apply(
        self,
        edit: Edit,
    ) {
        self.run(move |editor| editor.apply(edit));
    }

    fn typed(
        self,
        token: FieldToken,
        edit: Edit,
    ) {
        self.run(move |editor| editor.apply_if_current(token, edit));
    }

    fn widget(
        self,
        id: u16,
    ) -> Option<Widget> {
        self.editor
            .with(|e| e.editing_config().widgets.iter().find(|w| w.id == id).cloned())
    }

    fn place_widget(
        self,
        token: FieldToken,
        id: u16,
        f: impl FnOnce(&mut Placement),
    ) {
        let Some(widget) = self.widget(id) else {
            return;
        };
        let mut placement = widget.placement;
        f(&mut placement);
        self.typed(token, Edit::PlaceWidget { id, placement });
    }

    fn update_widget(
        self,
        token: FieldToken,
        id: u16,
        f: impl FnOnce(&mut Widget),
    ) {
        self.run(move |editor| {
            let mut widget = editor
                .preview()
                .config()
                .widgets
                .iter()
                .find(|w| w.id == id)
                .cloned()
                .ok_or(EditorError::MissingWidget(id))?;
            f(&mut widget);
            editor.apply_if_current(token, Edit::UpdateWidget(widget))
        });
    }

    fn save_label(self) -> String {
        if self.conflict.get() {
            return "Browser draft changed in another tab".into();
        }
        self.editor.with(|e| {
            match e.save_status() {
                SaveStatus::Saved => "Saved in this browser".into(),
                SaveStatus::Dirty => "Unsaved changes".into(),
                SaveStatus::InvalidDraft => "Invalid edits · not saved".into(),
                SaveStatus::Saving => "Saving…".into(),
                SaveStatus::Failed(StorageFailure::Quota) => "Storage full · export to keep changes".into(),
                SaveStatus::Failed(_) => "Browser storage unavailable · export to keep changes".into(),
            }
        })
    }

    fn free(
        self,
        span: Span,
        except: Option<u16>,
    ) -> Option<Placement> {
        self.editor.with_untracked(|e| {
            let occupied = e
                .preview()
                .config()
                .widgets
                .iter()
                .filter(|w| w.view == e.selection().view && Some(w.id) != except)
                .fold(0u8, |mask, w| mask | w.placement.occupied().unwrap_or(u8::MAX));
            for row in 0..2 {
                for column in 0..4 {
                    let p = Placement { column, row, span };
                    if p.occupied().is_some_and(|mask| mask & occupied == 0) {
                        return Some(p);
                    }
                }
            }
            None
        })
    }
}

#[component]
pub fn Studio() -> impl IntoView {
    let mount_started = now();
    let (editor, initial_notice, saved_bytes) = io::restore();
    let ui = Ui {
        cache_seen: RwSignal::new(dashboard_core::cache_diagnostics::CacheExhaustionSeen::new()),
        mount_started,
        editor: RwSignal::new(editor),
        notice: RwSignal::new(initial_notice),
        conflict: RwSignal::new(false),
        saved_bytes: RwSignal::new(saved_bytes),
        psi: RwSignal::new(false),
        warnings: RwSignal::new(false),
        sample_ms: RwSignal::new(118_000),
        sample_set: RwSignal::new(dashboard_configurator::preview::SampleSet::Demo),
        preview_error: RwSignal::new(None),
        pending_at: RwSignal::new(None),
        render_ms: RwSignal::new(0.0),
        canvas: NodeRef::new(),
        drag: RwSignal::new(None),
        capture: RwSignal::new(None),
        menu: RwSignal::new(None),
        playing: RwSignal::new(false),
        trace: RwSignal::new(metrics::Trace::default()),
        observer: RwSignal::new(None),
    };
    stage::install_playback(ui);
    widget_menu::install(ui);
    metrics::install(ui);
    let storage_listener = window_event_listener(leptos::ev::storage, move |event| {
        if event.key().as_deref().is_none_or(|key| key == io::STORAGE_KEY) {
            io::storage_changed(ui);
        }
    });
    on_cleanup(move || storage_listener.remove());
    let global_tab = RwSignal::new("warnings");
    view! {
        <div class="studio" on:pointermove=move |e| stage::drag_move(ui, e)
            on:pointerup=move |e| stage::drag_end(ui,e) on:pointercancel=move |e| stage::cancel_event(ui,e)
            on:lostpointercapture=move |e| stage::cancel_event(ui,e)
            on:keydown=move |e| { if e.key() == "Escape" { stage::cancel(ui); widget_menu::close(ui, true); } }>
            <header class="masthead">
                <a class="brand" href="./" aria-label="EA888 View Studio"><b>"EA"<span>"888"</span></b><div>"VIEW STUDIO"<small>"PICO 2 / DNUC"</small></div></a>
                <div class="device-spec"><span class="dot"></span>"320 × 240"<span>"4 × 2 GRID"</span></div>
                <div class="header-actions">
                    <button title="Undo accepted edit" disabled=move || ui.invalid() || ui.editor.with(|e| e.history_len().0 == 0) on:click=move |_| { ui.run(Editor::undo); }>"↶ Undo"</button>
                    <button title="Redo accepted edit" disabled=move || ui.invalid() || ui.editor.with(|e| e.history_len().1 == 0) on:click=move |_| { ui.run(Editor::redo); }>"↷ Redo"</button>
                    <button class="primary" disabled=move || ui.invalid() on:click=move |_| io::export(ui,false)>"Export for Pico"<span>"↗"</span></button>
                </div>
            </header>
            <div class="workbench">
                <Library ui/>
                <main class="center">
                    <div class="section-heading"><div><p class="eyebrow">"YOUR DASHBOARD"</p><h1>"Build your view."</h1></div><span class="chip">{move || ui.editor.with(|e| format!("{} / 8 views",e.preview().config().views.len()))}</span></div>
                    <Views ui/>
                    <div class="preview-toolbar"><span><i class="status-light"></i>"SYNTHETIC PREVIEW"</span><div>
                        <button class:active=move || ui.psi.get() on:click=move |_| {ui.input_started();ui.psi.update(|v| *v = !*v);}>{move || if ui.psi.get() { "PSI / bar" } else { "bar / PSI" }}</button>
                        <button class:active=move || ui.warnings.get() on:click=move |_| {ui.input_started();ui.warnings.update(|v| *v = !*v);}>"Warnings"</button>
                        <button class:active=move ||ui.playing.get() on:click=move |_|ui.playing.update(|v|*v = !*v)>{move ||if ui.playing.get(){"Pause"}else{"Play"}}</button>
                        <button title="Advance synthetic demo values by one second" disabled=move ||ui.playing.get() on:click=move |_| {ui.input_started();ui.sample_ms.update(|t| *t=t.saturating_add(1000));}>"Next sample →"</button>
                    </div></div>
                    <label class="preview-samples">"Test values"
                        <select aria-label="Preview test values" on:change=move |event|{
                            use dashboard_configurator::preview::SampleSet;
                            ui.input_started();
                            ui.sample_set.set(match event_target_value(&event).as_str(){"minimum"=>SampleSet::Minimum,"maximum"=>SampleSet::Maximum,"width"=>SampleSet::WidthStress,"zero"=>SampleSet::Zero,_=>SampleSet::Demo});
                        }>
                            <option value="demo">"Animated demo"</option><option value="minimum">"Channel minima"</option><option value="maximum">"Channel maxima"</option><option value="width">"Extreme digit widths"</option><option value="zero">"Zero / unavailable AFR"</option>
                        </select><span>"Synthetic only · not operating limits"</span>
                    </label>
                    <Canvas ui/>
                    <div class="canvas-caption"><span>"Drag to place · right-click for actions · arrows to move · Esc to cancel"</span><span>{move || format!("Preview render {:.1} ms",ui.render_ms.get())}</span></div>
                    <div class="draft-alert" hidden=move || !ui.invalid() role="alert">
                        <strong>"INVALID PREVIEW · inspection only"</strong>
                        <p>{move || ui.editor.with(|e| e.draft_error().map(error_text).unwrap_or_default())}</p>
                        <p>"Rejected labels and in-bounds placements are shown, including overlaps. Invalid numbers use safe bounds; source and fuel use the last valid settings. Parsed colours are shown even when unreadable. Edited warning colours show a sample popup. Live warning simulation is suspended. Unsupported glyphs appear as ? and long text is clipped. Exact rejected inputs are listed below."</p>
                        <p>{move ||if ui.layout_locked(){"Correct the highlighted text fields or discard the invalid edits to unlock layout controls."}else{"Drag, resize or remove widgets to repair the layout. Right-click an overlap to select a covered widget. Save and export unlock once every error is resolved."}}</p>
                        <ul class="draft-details">{move ||ui.editor.with(|e|e.diagnostic().map(|draft|draft.descriptions(e.preview()).into_iter().map(|line|view!{<li>{line}</li>}).collect_view()))}</ul>
                        <button on:click=move |_| { ui.run(Editor::discard_raw); }>"Discard invalid edits"</button>
                    </div>
                    <section class="global-settings">
                        <nav class="settings-tabs" aria-label="Dashboard settings">
                            <button class:active=move || global_tab.get()=="warnings" on:click=move |_| global_tab.set("warnings")>"Warning display"</button>
                            <button class:active=move || global_tab.get()=="fuel" on:click=move |_| global_tab.set("fuel")>"Fuel & AFR"</button>
                            <button class:active=move || global_tab.get()=="sources" on:click=move |_| global_tab.set("sources")>"ECU sources"</button>
                            <button class:active=move || global_tab.get()=="files" on:click=move |_| global_tab.set("files")>"Files & build"</button>
                            <button class:active=move || global_tab.get()=="controls" on:click=move |_| global_tab.set("controls")>"Device controls"</button>
                        </nav>
                        <div hidden=move || global_tab.get()!="warnings"><WarningSettingsPanel ui/></div>
                        <div hidden=move || global_tab.get()!="fuel"><FuelSettings ui/></div>
                        <div hidden=move || global_tab.get()!="sources"><SourceSettings ui/></div>
                        <div hidden=move || global_tab.get()!="files"><io::Files ui/></div>
                        <div hidden=move || global_tab.get()!="controls"><controls::DeviceControls ui/></div>
                    </section>
                </main>
                <Inspector ui/>
            </div>
            <footer class="statusbar"><div><span class="dot" class:error=move || ui.invalid()></span><span>{move || ui.save_label()}</span>
                <button disabled=move || ui.invalid() on:click=move |_| io::save(ui,true)>"Save here"</button></div>
                <span>{move || ui.editor.with(|e| {let size=e.export_binary().map(|v|format!("runtime config {} B",v.len())).unwrap_or_else(|_|"export blocked by invalid edits".into());format!("{} widgets · {size}",e.preview().config().widgets.len())})}</span>
            </footer>
            <div class="notice" role="status" hidden=move || ui.notice.get().is_empty()><span>{move || ui.notice.get()}</span><button aria-label="Dismiss notice" on:click=move |_| ui.notice.set(String::new())>"×"</button></div>
            <widget_menu::Popup ui/>
        </div>
    }
}

#[component]
fn Views(ui: Ui) -> impl IntoView {
    view! {
        <div class="view-tabs" role="navigation" aria-label="Dashboard views">
            <For each=move || ui.editor.with(|e| e.preview().config().views.iter().map(|v| v.id).collect::<Vec<_>>()) key=|id| *id children=move |id| {
                view! { <button class:active=move || ui.editor.with(|e| e.selection().view==id) disabled=move || ui.invalid()
                    on:click=move |_| { ui.run(move |e| e.select(Selection { view:id,widget:None })); }>
                    {move || ui.editor.with(|e| e.preview().config().views.iter().find(|v|v.id==id).map(|v|v.name.to_string()).unwrap_or_default())}
                    <small>{move || ui.editor.with(|e| if e.preview().config().boot_view==id { "BOOT" } else { "" })}</small>
                </button> }
            }/>
            <button class="add-view" title="Add empty view" disabled=move || ui.invalid() || ui.editor.with(|e|e.preview().config().views.len()>=8)
                on:click=move |_| ui.apply(Edit::AddView { name:"New view".into() })>"+ View"</button>
        </div>
        <For each=move || vec![ui.editor.with(|e|e.selection().view)] key=|id| *id children=move |id| view! {
            <div class="view-controls">
                <TextField ui field=RawField::ViewName(id) label="View name" hint="1–14 ASCII characters"/>
                <div class="view-tools">
                    <button disabled=move || ui.invalid() on:click=move |_| ui.apply(Edit::SetBootView(id))>"Set as boot"</button>
                    <button disabled=move || ui.invalid() on:click=move |_| ui.apply(Edit::DuplicateView { id,name:"Copy".into() })>"Duplicate"</button>
                    <button title="Move view earlier in encoder order" disabled=move || ui.invalid() on:click=move |_| {
                        let index=ui.editor.with_untracked(|e|e.preview().config().views.iter().position(|v|v.id==id).unwrap_or(0));
                        ui.apply(Edit::ReorderView { id,index:index.saturating_sub(1) });
                    }>"←"</button>
                    <button title="Move view later in encoder order" disabled=move || ui.invalid() on:click=move |_| {
                        let index=ui.editor.with_untracked(|e| {let c=e.preview().config();(c.views.iter().position(|v|v.id==id).unwrap_or(0)+1).min(c.views.len()-1)});
                        ui.apply(Edit::ReorderView { id,index });
                    }>"→"</button>
                    <button class="danger-text" disabled=move || ui.invalid() || ui.editor.with(|e|e.preview().config().views.len()==1) on:click=move |_| ui.apply(Edit::DeleteView(id))>"Delete"</button>
                </div>
            </div>
        }/>
    }
}
