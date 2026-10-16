//! Retained, offline reference panel; owns no editor, timers or acquisition state.
use dashboard_configurator::ecu_reference::{ReferenceState, Section, Status};

use super::*;

#[component]
pub fn ReferenceWorkspace(ui: Ui) -> impl IntoView {
    let state = ui.reference;
    view! {
        <div class="ecu-reference">
            <header class="reference-header"><div><p class="eyebrow">"DNUC / EA888 Gen3"</p><h1>"ECU Reference"</h1><p>"Understand a source before configuring a view."</p></div><span class="chip">"Offline · research reviewed 15 Oct 2026"</span></header>
            <p class="reference-boundary">"No live qualification. Generic implementation, family evidence and exact-controller support are separate. Reading this workspace changes no source, polling or warning setting."</p>
            <div class="reference-toolbar">
                <label class="field"><span>"Search ECU reference"</span><input type="search" placeholder="39C0, boost, DQ381, E5…" prop:value=move ||state.with(|s|s.query().to_owned()) on:input=move |event|state.update(|s|s.set_query(&event_target_value(&event)))/></label>
                <label class="field"><span>"Section"</span><select prop:value=move ||state.with(|s|s.section.map(|v|v.label()).unwrap_or("all")) on:change=move |event|{let value=event_target_value(&event);state.update(|s|s.section=Section::ALL.into_iter().find(|v|v.label()==value));}><option value="all">"All sections"</option>{Section::ALL.into_iter().map(|section|view!{<option value=section.label()>{section.label()}</option>}).collect_view()}</select></label>
                <label class="field"><span>"Evidence / implementation status"</span><select prop:value=move ||state.with(|s|s.status.map(|v|v.label()).unwrap_or("all")) on:change=move |event|{let value=event_target_value(&event);state.update(|s|s.status=Status::ALL.into_iter().find(|v|v.label()==value));}><option value="all">"All statuses"</option>{Status::ALL.into_iter().map(|status|view!{<option value=status.label()>{status.label()}</option>}).collect_view()}</select></label>
            </div>
            <p class="hint" role="status">{move ||state.with(|s|format!("{} references{}",s.results().len(),if s.truncated{" · search limited to 256 UTF-8 bytes"}else{""}))}</p>
            <div class="reference-browser">
                <nav class="reference-results" aria-label="ECU reference results">
                    <Show when=move ||state.with(|s|s.results().is_empty())><p>"No matching references. Try fewer terms or clear the filters. Your selected detail remains open."</p><button on:click=move |_|state.update(|s|{s.set_query("");s.section=None;s.status=None;})>"Clear search and filters"</button></Show>
                    <For each=move ||state.with(ReferenceState::results) key=|entry|entry.id children=move |entry|view!{
                        <button class="reference-result" class:active=move ||state.with(|s|s.selected().id==entry.id) aria-pressed=move ||state.with(|s|s.selected().id==entry.id).to_string() on:click=move |_|{state.update(|s|{s.select(entry.id);});}>
                            <span class="reference-section">{entry.section.label()}</span><strong>{entry.title}</strong><span class="reference-status">{entry.status.label()}</span>
                        </button>
                    }/>
                </nav>
                <article class="reference-detail" aria-label="Selected ECU reference" tabindex="0">
                    {move ||state.with(|s|{
                        let entry=s.selected();
                        view!{
                            <p class="reference-section">{entry.section.label()}</p><h2>{entry.title}</h2><p class="reference-status">{entry.status.label()}</p>
                            <dl><dt>"Applicability"</dt><dd>{entry.scope}</dd><dt>"Meaning and qualification"</dt><dd>{entry.detail}</dd><dt>"Units"</dt><dd>{entry.units}</dd><dt>"Documentary decoding / scaling"</dt><dd><code>{entry.formula}</code></dd><dt>"Live qualification"</dt><dd>"Not established on the current vehicle. A catalogue entry cannot grant qualification."</dd></dl>
                            {entry.binding().map(|binding|view!{
                                <h3>"Current dashboard definition"</h3>
                                <dl><dt>"Documentary request bytes"</dt><dd><code>{binding.request}</code></dd><dt>"Decoder identifier"</dt><dd><code>{binding.decoder}</code></dd><dt>"Reply shape and implementation note"</dt><dd>{binding.interpretation}</dd><dt>"Implementation availability"</dt><dd>{binding.availability}</dd></dl>
                                <p class="hint">"Read-only projection of the shipped catalogue. Request bytes exclude CAN / ISO-TP framing. Research conflicts above remain unresolved even where a candidate decoder has a name. Your configured route and polling settings are separate."</p>
                            })}
                            <h3>"Public evidence"</h3><p class="hint">"Pinned code shows an implementation; historical standards copies and family documentation have the stated scope. Links open only when selected."</p>
                            <ul class="reference-links">{entry.references.iter().enumerate().map(|(index,url)|view!{<li><a href=*url target="_blank" rel="noopener noreferrer">{format!("Source {} ↗",index+1)}<span>{*url}</span></a></li>}).collect_view()}</ul>
                        }
                    })}
                    <div class="reference-actions"><button on:click=move |_|workspaces::activate(ui,Workspace::Views)>"Return to Views"</button><button on:click=move |_|workspaces::open(ui,Workspace::DataWarnings,GroupId::Sources)>"Open channel source settings"</button></div>
                    <p class="hint">"Source settings retain your selected channel. Navigation does not apply the reference to a widget."</p>
                </article>
            </div>
        </div>
    }
}
