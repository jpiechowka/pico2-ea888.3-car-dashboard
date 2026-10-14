use dashboard_configurator::workspace::Command;

use super::*;

fn preset_result(
    ui: Ui,
    result: Result<(), PresetError>,
) {
    ui.event(Notice {
        category: Category::Editor,
        severity: if result.is_ok() {
            dashboard_configurator::events::Severity::Info
        } else {
            dashboard_configurator::events::Severity::Warn
        },
        code: if result.is_ok() {
            "presets.changed"
        } else {
            "presets.rejected"
        },
        subject: Some("Appearance presets"),
        message: match result {
            Ok(()) => "Presets changed in memory; explicit save required",
            Err(error) => error.message(),
        }
        .into(),
    });
    ui.notice.set(match result {
        Ok(()) => "Presets changed in memory. Save presets or export a backup.".into(),
        Err(error) => error.message().into(),
    });
}
fn choose_appearance(
    ui: Ui,
    source: AppearanceSource,
    palette: Option<ColorPalette>,
) {
    let mut result = Ok(());
    ui.appearance.update(|session| result = session.choose(source, palette));
    if let Err(error) = result {
        ui.notice.set(error.message().into());
    }
}

#[component]
fn AppearancePanel(ui: Ui) -> impl IntoView {
    let name = RwSignal::new(String::new());
    view! {
        <details class="appearance-tools"><summary>"Copy appearance and presets"</summary>
            <p class="hint">"Colors, gauge style, decimal precision and supported visibility only. Ranges, units, bands and warning rules stay as configured."</p>
            <label class="field"><span>"Appearance source"</span>
                <select id="studio-appearance-source" prop:value=move ||ui.appearance.with(|s|match s.source(){AppearanceSource::Selected=>"selected",AppearanceSource::Copied=>"copied",AppearanceSource::Preset(_)=>"preset"})
                    on:change=move |event|{let source=match event_target_value(&event).as_str(){"selected"=>AppearanceSource::Selected,"copied"=>AppearanceSource::Copied,_=>return};let palette=ui.appearance.with_untracked(AppearanceSession::palette);choose_appearance(ui,source,palette);}>
                    <option value="selected">"Selected widget"</option><option value="copied">"Copied appearance"</option><option value="preset" disabled=true>"Named preset snapshot"</option>
                </select>
            </label>
            <label class="field"><span>"Color palette over this source"</span>
                <select prop:value=move ||ui.appearance.with(|s|match s.palette(){None=>"source",Some(ColorPalette::Default)=>"default",Some(ColorPalette::Monochrome)=>"mono",Some(ColorPalette::RedAccent)=>"red"})
                    on:change=move |event|{let palette=match event_target_value(&event).as_str(){"default"=>Some(ColorPalette::Default),"mono"=>Some(ColorPalette::Monochrome),"red"=>Some(ColorPalette::RedAccent),_=>None};let source=ui.appearance.with_untracked(AppearanceSession::source);choose_appearance(ui,source,palette);}>
                    <option value="source">"Keep source colors"</option><option value="default">"Default color palette"</option><option value="mono">"High-contrast monochrome"</option><option value="red">"Red accent"</option>
                </select>
            </label>
            {move || {
                let stamp=ui.editor.with(Editor::stamp);
                let offer=command_palette::offer(ui,stamp);
                let copy=Command::CopyAppearance;
                view! {<div class="appearance-actions"><button disabled=move ||command_palette::availability(ui,copy,stamp,offer).is_some()
                    on:click=move |_|{command_palette::execute(ui,copy,stamp,offer);}>"Copy selected appearance"</button>
                    {AppearanceScope::ALL.into_iter().map(move |scope|{let command=Command::ApplyAppearance(scope);view!{<button id=scope.preview_control_id() disabled=move ||command_palette::availability(ui,command,stamp,offer).is_some()
                        on:click=move |_|{command_palette::execute(ui,command,stamp,offer);}>{format!("Preview: {}",scope.label())}</button>}}).collect_view()}
                    <p class="hint">{if offer.snapshot().is_none(){"Choose a clean selected widget, usable copied appearance or named preset."}else{"Preview shows all settings and omissions before Apply."}}</p>
                </div>}
            }}
            {move ||ui.appearance.with(|session|session.request()).map(|request| {
                let stamp=ui.editor.with(Editor::stamp);
                let offer=command_palette::offer(ui,stamp);
                let snapshot=request.snapshot();
                let result=ui.editor.with(|editor|editor.appearance_preview(request));
                let can_apply=result.as_ref().is_ok_and(|preview|preview.error.is_none()&&preview.changed);
                let description=match result {
                    Ok(preview)=>format!("{} targets · {} omitted fields (history {}, average {}, extrema {}, trend {}). {}",preview.target_count,preview.omitted_fields,preview.omissions[0],preview.omissions[1],preview.omissions[2],preview.omissions[3],match preview.error {Some(error)=>error_text(error),None if !preview.changed=>"No changes".into(),None=>"Ready to Apply as one undo entry".into()}),
                    Err(error)=>error_text(error),
                };
                view!{<section id="studio-appearance-preview" class="appearance-preview" tabindex="-1" aria-label="Appearance preview">
                    <h3>{format!("Appearance preview: {}",request.scope().label())}</h3>
                    <p>{format!("{} · {} decimal places · history {} · average {} · extrema {} · trend {}",snapshot.gauge_style.label(),snapshot.decimals,if snapshot.graph{"On"}else{"Off"},if snapshot.average{"On"}else{"Off"},if snapshot.extrema{"On"}else{"Off"},if snapshot.trend{"On"}else{"Off"})}</p>
                    <h4>"Frozen colors represented by RGB565"</h4>
                    <dl class="appearance-color-summary">{ColorRole::ALL.into_iter().map(move |role|{
                        let hex=role.get(&snapshot.colors).hex().to_string();
                        let swatch=format!("background-color:{hex}");
                        view!{<div><dt>{role.label()}</dt><dd><span class="appearance-swatch" aria-hidden="true" style=swatch></span><code>{hex}</code></dd></div>}
                    }).collect_view()}</dl>
                    <p role="status">{description}</p>
                    <p class="hint">"Gauges omit history and trend; compact gauges also omit average and extrema. Unsupported visibility stays unchanged. Every stored band must contrast with the new track; a conflict rejects the whole operation."</p>
                    <div class="appearance-actions"><button class="primary" disabled=!can_apply on:click=move |_| {
                        if !ui.appearance.with_untracked(|s|s.is_current(offer)&&s.request()==Some(request)){ui.notice.set("Appearance source changed; preview again".into());return;}
                        if ui.run(|editor|editor.apply(Edit::ApplyAppearance(request))) {command_palette::close_appearance(ui,request,offer);}
                    }>"Apply appearance"</button><button on:click=move |_|command_palette::close_appearance(ui,request,offer)>"Cancel preview"</button></div>
                </section>}
            })}
            <h3>"Named appearance presets"</h3>
            <label class="field"><span>"Preset name"</span><input maxlength="32" prop:value=move ||name.get() on:input=move |event|{
                let input=event_target::<web_sys::HtmlInputElement>(&event);
                if let Some(text)=io::bounded_input(&input,32){name.set(text);}else{input.set_value(&name.get_untracked());ui.notice.set("Preset names are limited to 32 bytes".into());}
            }/><small>"1–32 printable ASCII bytes, including spaces; cannot be blank"</small></label>
            {move || {
                let stamp=ui.editor.with(Editor::stamp);let offer=command_palette::offer(ui,stamp);
                view!{<button disabled=offer.snapshot().is_none() on:click=move |_|{
                    if !ui.appearance.with_untracked(|s|s.is_current(offer)) || ui.editor.with_untracked(Editor::stamp)!=stamp || command_palette::offer(ui,stamp)!=offer {ui.notice.set("Appearance changed; choose it again".into());return;}
                    let Some(snapshot)=offer.snapshot() else{return;};let mut result=Ok(());ui.presets.update(|store|result=store.add(&name.get_untracked(),snapshot));preset_result(ui,result);
                }>"Add named preset in memory"</button>}
            }}
            <ul class="appearance-presets">{move ||ui.presets.with(|store| {
                let version=store.version();
                store.records().iter().enumerate().map(|(index,record)| {
                    let label=record.name().to_owned();let snapshot=record.snapshot;
                    let stamp=ui.editor.with(Editor::stamp);let offer=command_palette::offer(ui,stamp);
                    view!{<li><span>{label}</span><div class="appearance-actions">
                        <button on:click=move |_|{if ui.presets.with_untracked(|s|s.record(version,index).is_ok()){choose_appearance(ui,AppearanceSource::Preset(snapshot),None);}else{ui.notice.set(PresetError::Stale.message().into());}}>"Use snapshot"</button>
                        <button on:click=move |_|{
                            if !ui.appearance.with_untracked(|s|s.is_current(offer))||ui.editor.with_untracked(Editor::stamp)!=stamp||command_palette::offer(ui,stamp)!=offer{ui.notice.set("Appearance changed; choose it again".into());return;}
                            let Some(snapshot)=offer.snapshot() else{ui.notice.set("Choose a usable source first".into());return;};
                            let mut result=Ok(());ui.presets.update(|s|result=s.replace(version,index,snapshot));preset_result(ui,result);
                        }>"Replace with source"</button>
                        <button on:click=move |_|{let mut result=Ok(());ui.presets.update(|s|result=s.remove(version,index));preset_result(ui,result);}>"Delete"</button>
                    </div></li>}
                }).collect_view()
            })}</ul>
            <p role="status">{move ||ui.presets.with(|store|match store.status(){
                PresetStatus::Empty=>"No saved presets".into(),PresetStatus::Saved=>"Presets Saved in this browser".into(),PresetStatus::Dirty=>"Presets unsaved in memory".into(),PresetStatus::Saving=>"Saving presets…".into(),PresetStatus::Conflict=>PresetError::Conflict.message().into(),
                PresetStatus::Failed(StorageFailure::Quota)=>"Preset storage full; memory presets remain usable. Export a backup.".into(),PresetStatus::Failed(_)=>"Preset storage unavailable; memory presets remain usable. Export a backup.".into(),PresetStatus::Unavailable(error)=>format!("Presets unavailable: {}. Existing storage preserved.",error.message()),
            })}</p>
            <div class="appearance-actions"><button on:click=move |_|io::save_presets(ui,false)>"Save presets"</button>
                <button on:click=move |_|io::save_presets(ui,true)>"Replace stored record explicitly"</button><button on:click=move |_|io::reload_presets(ui)>"Reload stored presets"</button>
                <button on:click=move |_|io::export_presets(ui)>"Export presets"</button></div>
            <p class="hint">"Replacing captures and compares the current record under its lock. Reload discards only the in-memory preset list. Export first to keep it."</p>
            <label class="field"><span>"Import preset record into memory"</span><input type="file" accept=".json,application/json" disabled=move ||ui.preset_import_busy.get() on:change=move |event|{let input=event_target::<web_sys::HtmlInputElement>(&event);if let Some(file)=input.files().and_then(|f|f.get(0)){io::import_presets(ui,file);}input.set_value("");}/><small>"16 records maximum; complete file at most 32 KiB. Import does not write browser storage."</small></label>
            <p role="status" hidden=move ||!ui.preset_import_busy.get()>"Reading one bounded preset file…"</p>
        </details>
    }
}

#[component]
pub fn Inspector(ui: Ui) -> impl IntoView {
    view! {
        <aside id="studio-widget-inspector" class="inspector panel" hidden=move ||ui.workspace.with(|s|s.active()!=Workspace::Views)>
            <div class="panel-heading"><p class="eyebrow">"WIDGET INSPECTOR"</p><h2>{move || ui.inspected_widget().map(|(_,channel)|channel.label()).unwrap_or("Make it yours")}</h2></div>
            <Show when=move || ui.inspected_widget().is_none()>
                <div class="empty-inspector"><div class="grid-icon" aria-hidden="true">"▦"</div><h3>"Select a widget"</h3><p>"Click a tile on the display to edit its label, size, graph and warnings."</p><p>"Drag a sensor from the library into a free cell. Create an empty view for a fresh layout."</p><div class="size-samples"><span>"1 × 1"</span><span>"2 × 1"</span><span>"2 × 2"</span></div></div>
            </Show>
            <AppearancePanel ui/>
            <For each=move ||{ui.inspected_widget().into_iter().collect::<Vec<_>>()} key=|key|(key.0,key.1.index()) children=move |(id,_)|view!{<WidgetInspector ui id/>}/>

        </aside>
    }
}

fn flag(
    ui: Ui,
    id: u16,
    label: &'static str,
    get: fn(&DisplayOptions) -> bool,
    set: fn(&mut DisplayOptions, bool),
) -> impl IntoView {
    view! { <Toggle ui label value=Signal::derive(move || ui.widget(id).is_some_and(|w|get(&w.display)))
    on_change=Callback::new(move |(token,value)| ui.update_widget(token,id,|w| {
        let previous_afr=w.display.afr;
        set(&mut w.display,value);
        if w.display.afr!=previous_afr {
            w.advanced.readout=if w.display.afr {Readout::LambdaAndAfr}else{Readout::Lambda};
        }
    }))/> }
}

#[component]
fn WidgetInspector(
    ui: Ui,
    id: u16,
) -> impl IntoView {
    let channel = ui.widget(id).map(|w| w.channel).unwrap_or(Channel::Oil);
    view! {
        <div class="inspector-body">
            <Group ui group=GroupId::Layout>
            <TextField ui field=RawField::Widget(id,WidgetField::Label) label="Label" hint="ASCII · 12 characters at 1×1, 24 at wider sizes"/>
            <Choice ui layout=true label="Size" value=Signal::derive(move || ui.widget(id).map(|w|match w.placement.span { Span::OneByOne=>"1",Span::TwoByOne=>"2",Span::TwoByTwo=>"4" }.to_string()).unwrap_or_default())
                options=choices(&[("1","1 × 1 · compact"),("2","2 × 1 · wide"),("4","2 × 2 · large")])
                on_change=Callback::new(move |(token,value): (FieldToken,String)| { let span=match value.as_str(){"1"=>Span::OneByOne,"2"=>Span::TwoByOne,"4"=>Span::TwoByTwo,_=>return};ui.typed(token,Edit::ResizeWidget { id,span }); })/>
            <div class="field-pair">
                <Choice ui layout=true label="Column" value=Signal::derive(move ||ui.widget(id).map_or(String::new(),|w|w.placement.column.to_string())) options=choices(&[("0","1"),("1","2"),("2","3"),("3","4")])
                    on_change=Callback::new(move |(token,value): (FieldToken,String)| {if let Ok(column)=value.parse::<u8>() {ui.place_widget(token,id,|p|p.column=column);}})/>
                <Choice ui layout=true label="Row" value=Signal::derive(move ||ui.widget(id).map_or(String::new(),|w|w.placement.row.to_string())) options=choices(&[("0","Top"),("1","Bottom")])
                    on_change=Callback::new(move |(token,value): (FieldToken,String)| {if let Ok(row)=value.parse::<u8>() {ui.place_widget(token,id,|p|p.row=row);}})/>
            </div>
            </Group>
            <Group ui group=GroupId::Gauge>

            <Choice ui repair_invalid_draft=true label="Gauge style" value=Signal::derive(move ||ui.widget(id).map(|w|format!("{:?}",w.gauge.kind)).unwrap_or_default())
                options=GaugeKind::ALL.into_iter().map(|kind|(format!("{kind:?}"),kind.label().to_string())).collect()
                on_change=Callback::new(move |(token,value):(FieldToken,String)|{if let Some(kind)=GaugeKind::ALL.into_iter().find(|kind|format!("{kind:?}")==value) {ui.update_widget(token,id,|w|w.gauge.kind=kind);}})/>
            <Show when=move ||ui.widget(id).is_some_and(|w|!w.gauge.kind.is_numeric()) || ui.editor.with(|e|e.widget_fields_need_repair(id,&[WidgetField::GaugeMinimum,WidgetField::GaugeMaximum],Field::GaugeRange))>
            <Show when=move ||ui.widget(id).is_some_and(|w|w.gauge.kind.is_numeric())><p class="hint">"Stored gauge limits remain visible to repair this edit. Numeric style does not use the gauge scale."</p></Show>
                <div class="field-pair"><TextField ui field=RawField::Widget(id,WidgetField::GaugeMinimum) label="Scale minimum"/><TextField ui field=RawField::Widget(id,WidgetField::GaugeMaximum) label="Scale maximum"/></div>
                <Toggle ui repair_invalid_draft=true label="Expand with observed values" value=Signal::derive(move ||ui.widget(id).is_some_and(|w|w.gauge.expand)) on_change=Callback::new(move |(token,value)|ui.update_widget(token,id,|w|w.gauge.expand=value))/>
                <p class="hint">"Limits use canonical units (boost in bar), at least 0.001 apart. Expansion lasts until statistics reset. Fixed scales clamp the indicator; numbers still show the actual reading."</p>
                <p class="hint">"Gauges replace history and trend. Average and extrema are available at 2 × 2. Numeric-view preferences are preserved."</p>
            </Show>
            <super::advanced::GaugeControls ui id/>
            </Group>
            <Group ui group=GroupId::Readout>
            <TextField ui field=RawField::Widget(id,WidgetField::Decimals) label="Decimal places" hint="0–3; units stay canonical in rules"/>
            <super::advanced::DisplayControls ui id/>
            </Group>
            <Group ui group=GroupId::HistoryStatistics>
            <Show when=move ||ui.widget(id).is_some_and(|w|w.gauge.kind.is_numeric())>
            <span title="History keeps the last qualified reading in each of 60 timed buckets. Slow sensors can leave gaps; no values are backfilled. Faults can discard one preceding point to show a brief outage. This is a trend view, not a lossless waveform or peak recorder.">{flag(ui,id,"History graph",|d|d.graph,|d,v|d.graph=v)}</span>
            </Show>
            <Show when=move ||ui.widget(id).is_some_and(|w|w.supports_statistics())>
            {flag(ui,id,"Average",|d|d.average,|d,v|d.average=v)}
            {flag(ui,id,"Minimum & maximum",|d|d.extrema,|d,v|d.extrema=v)}
            </Show>
            <Show when=move ||ui.widget(id).is_some_and(|w|w.gauge.kind.is_numeric())>
            {flag(ui,id,"Trend arrow",|d|d.trend,|d,v|d.trend=v)}
            </Show>
            <Show when=move ||ui.widget(id).is_some_and(|w|w.capabilities().history) || ui.editor.with(|e|e.widget_fields_need_repair(id,&[WidgetField::GraphMinimum,WidgetField::GraphMaximum],Field::GraphRange))>
            <Show when=move ||ui.widget(id).is_some_and(|w|!w.capabilities().history)><p class="hint">"Stored graph limits remain visible to repair this edit. This gauge style does not draw history."</p></Show>
            <Choice ui repair_invalid_draft=true label="Graph range" value=Signal::derive(move ||ui.widget(id).map(|w|if matches!(w.display.graph_range,GraphRange::Auto){"auto"}else{"fixed"}.to_string()).unwrap_or_default()) options=choices(&[("auto","Automatic"),("fixed","Fixed canonical range")])
                on_change=Callback::new(move |(token,value):(FieldToken,String)|ui.update_widget(token,id,|w|w.display.graph_range=if value=="fixed" {let(min,max)=w.channel.domain();GraphRange::Fixed{min,max}}else{GraphRange::Auto}))/>
            <Show when=move || ui.widget(id).is_some_and(|w|matches!(w.display.graph_range,GraphRange::Fixed{..}))>
                <div class="field-pair"><TextField ui field=RawField::Widget(id,WidgetField::GraphMinimum) label="Graph minimum"/><TextField ui field=RawField::Widget(id,WidgetField::GraphMaximum) label="Graph maximum"/></div>
            </Show>
            </Show>
            <button on:click=move |_|workspaces::open(ui,Workspace::DataWarnings,GroupId::Channels)>"Open channel-shared history"</button>
            </Group>
            <Group ui group=GroupId::Appearance>
            <details class="palette-panel"><summary>"Widget colours"</summary>
                <p class="hint">"Exact panel colours · #RRGGBB quantized to RGB565. Foregrounds need 3:1 contrast. Inactive roles stay stored for other styles; edit them here to keep future changes valid."</p>
                {ColorRole::ALL.into_iter().map(move |role|view! { <ColorField ui field=RawField::Widget(id,WidgetField::Color(role)) label=role.label()/> }).collect_view()}
                <p class="hint">"Gauge styles use indicator, track and ticks. Numeric uses history and trend. Compact gauges hide statistics. Warning colours apply to this tile and cycle-one popups; list warnings use the separate combined palette."</p>
            </details>
            </Group>
            <Group ui group=GroupId::Warnings>

            <Toggle ui repair_invalid_draft=true label="Enable warning" value=Signal::derive(move ||ui.widget(id).is_some_and(|w|w.warning.enabled)) on_change=Callback::new(move |(token,value)|ui.update_widget(token,id,|w|w.warning.enabled=value))/>
            <Choice ui repair_invalid_draft=true label="Applies to" value=Signal::derive(move ||ui.widget(id).map(|w|if w.warning.applicability==Applicability::DemoOnly{"demo"}else{"vehicle"}.to_string()).unwrap_or_default()) options=choices(&[("demo","Demo only"),("vehicle","Vehicle and demo")])
                on_change=Callback::new(move |(token,value):(FieldToken,String)|ui.update_widget(token,id,|w|w.warning.applicability=if value=="demo"{Applicability::DemoOnly}else{Applicability::VehicleAndDemo}))/>
            <p class="hint">"Demo limits are examples. Vehicle thresholds are your own configuration; source and operating-context guards remain enforced."</p>
            <Choice ui repair_invalid_draft=true label="Rule type" value=Signal::derive(move ||ui.widget(id).map(|w|match w.warning.mode {LimitMode::High=>"high",LimitMode::Low=>"low",LimitMode::LambdaDeviation=>"lambda",LimitMode::OutsideWindow=>"outside",LimitMode::InsideWindow=>"inside"}.to_string()).unwrap_or_default())
                options=if channel==Channel::Lambda {choices(&[("lambda","Deviation from target lambda")])} else {choices(&[("high","High value"),("low","Low value"),("outside","Outside window"),("inside","Inside window")])}
                on_change=Callback::new(move |(token,value):(FieldToken,String)| {
                    let mode=match value.as_str(){"low"=>LimitMode::Low,"high"=>LimitMode::High,"outside"=>LimitMode::OutsideWindow,"inside"=>LimitMode::InsideWindow,_=>return};
                    ui.typed(token,Edit::SetWarningMode{id,mode});
                })/>
            <div class="field-pair"><TextField ui field=RawField::Widget(id,WidgetField::WarningTrigger) label="Trigger"/><TextField ui field=RawField::Widget(id,WidgetField::WarningClear) label="Clear"/></div>
            <Show when=move || channel==Channel::Lambda || ui.editor.with(|e|e.widget_fields_need_repair(id,&[WidgetField::WarningLoad],Field::WarningLoad))>
                <Show when=move ||channel!=Channel::Lambda><p class="hint">"This stored load preference is shown for repair; this channel's rule does not use it."</p></Show>
                <TextField ui field=RawField::Widget(id,WidgetField::WarningLoad) label="Minimum load (%)" hint="0–100; required lambda context stays enforced"/>
            </Show>
            <div class="field-pair"><TextField ui field=RawField::Widget(id,WidgetField::WarningOn) label="On dwell (ms)" hint="1–30000"/><TextField ui field=RawField::Widget(id,WidgetField::WarningOff) label="Off dwell (ms)" hint="1–30000"/></div>
            <super::advanced::WarningControls ui id/>
            <TextField ui field=RawField::Widget(id,WidgetField::WarningMessage) label="Warning message" hint="1–20 ASCII characters; one line per warning"/>
            <div class="field"><span>"Warning order within this view"</span><div class="field-pair">
                <button disabled=move ||ui.invalid() on:click=move |_|reorder(ui,id,false)>"↑ Earlier"</button>
                <button disabled=move ||ui.invalid() on:click=move |_|reorder(ui,id,true)>"↓ Later"</button>
            </div></div>
            </Group>
            <button on:click=move |_|{ui.source_channel.set(channel);workspaces::open(ui,Workspace::DataWarnings,GroupId::Sources);}>"Open channel source & decoder"</button>
            <div class="widget-actions">
                <button disabled=move ||ui.invalid() on:click=move |_| {
                    if let Some(widget)=ui.widget(id) {if let Some(placement)=ui.free(widget.placement.span,None){ui.apply(Edit::DuplicateWidget{id,view:widget.view,placement});}else{ui.notice.set("No free space for this size in the current view.".into());}}
                }>"Duplicate widget"</button>
                <button class="danger-text" disabled=move ||ui.layout_locked() on:click=move |_|ui.apply(Edit::DeleteWidget(id))>"Remove widget"</button>
            </div>
        </div>
    }
}

fn reorder(
    ui: Ui,
    id: u16,
    later: bool,
) {
    let destination = ui.editor.with_untracked(|e| {
        let widgets = &e.preview().config().widgets;
        let old = widgets.iter().position(|w| w.id == id)?;
        let view = widgets[old].view;
        if later {
            ((old + 1)..widgets.len()).find(|&i| widgets[i].view == view)
        } else {
            (0..old).rev().find(|&i| widgets[i].view == view)
        }
    });
    if let Some(index) = destination {
        ui.apply(Edit::ReorderWidget { id, index });
    }
}

#[component]
pub fn WarningSettingsPanel(ui: Ui) -> impl IntoView {
    view! {
        <div class="settings-content"><div class="settings-copy"><h3>"Give each warning its moment."</h3><p>"Messages follow priority first, then stable view and widget order. All eligible warnings cycle, including lower priorities. A new episode restarts paging; button feedback takes display priority."</p><p class="hint">"The appearance Warnings button uses synthetic data with compressed dwell. Diagnostics controls the persistent timing simulation in the shared preview dock, with Dashboard pixels, real configured dwell and completed-page acknowledgment. Neither qualifies ECU support."</p></div><div class="settings-form">
            <Choice ui repair_invalid_draft=true label="Show warnings from" value=Signal::derive(move ||ui.editor.with(|e|if e.editing_config().warnings.scope==WarningScope::AllViews{"all"}else{"view"}.to_string())) options=choices(&[("all","All views"),("view","Current view only")]) on_change=Callback::new(move |(token,value):(FieldToken,String)| {
                let mut settings=ui.editor.with_untracked(|e|e.editing_config().warnings);settings.scope=if value=="all"{WarningScope::AllViews}else{WarningScope::CurrentView};ui.typed(token,Edit::SetWarningPresentation(settings));
            })/>
            <Choice ui repair_invalid_draft=true label="Presentation" value=Signal::derive(move ||ui.editor.with(|e|if e.editing_config().warnings.mode==WarningMode::CycleOne{"one"}else{"three"}.to_string())) options=choices(&[("one","Cycle one warning"),("three","List up to three warnings")]) on_change=Callback::new(move |(token,value):(FieldToken,String)| {
                let mut settings=ui.editor.with_untracked(|e|e.editing_config().warnings);settings.mode=if value=="one"{WarningMode::CycleOne}else{WarningMode::ListThree};ui.typed(token,Edit::SetWarningPresentation(settings));
            })/>
            <TextField ui field=RawField::WarningPageMilliseconds label="Page interval (ms)" hint="500–10000; elapsed time, independent of FPS"/>
            <h3>"Combined warning colours"</h3>
            <p class="hint">"Used by List up to three, even with one active warning. Cycle one uses that widget's warning pair. Blink exchanges foreground and background; minimum contrast 3:1."</p>
            <ColorField ui field=RawField::WarningColor{foreground:true} label="Combined text"/>
            <ColorField ui field=RawField::WarningColor{foreground:false} label="Combined background"/>
        </div></div>
    }
}

#[component]
pub fn FuelSettings(ui: Ui) -> impl IntoView {
    view! {
        <div class="settings-content"><div class="settings-copy"><h3>"Lambda, in your fuel's language."</h3><p>"AFR = lambda × stoichiometric ratio. E5 means up to 5% ethanol, not a measured tank composition."</p><p>"An ethanol blend remains an estimate. Declare the inputs and their basis; the Pico resolves the ratio once at boot."</p><div class="ratio-readout"><small>"RESOLVED STOICHIOMETRY"</small><b>{move || ui.editor.with(|e|format!("{:.4} : 1",e.preview().stoich()))}</b></div></div><div class="settings-form">
            <Choice ui repair_invalid_draft=true label="AFR scale" value=Signal::derive(move ||ui.editor.with(|e|match e.editing_config().fuel{Fuel::EquivalentGasoline=>"eq",Fuel::Stoichiometric{..}=>"ratio",Fuel::EthanolBlend(_)=>"blend"}.to_string())) options=choices(&[("eq","Gasoline-equivalent 14.7"),("ratio","Explicit stoichiometric ratio"),("blend","Ethanol blend estimate")]) on_change=Callback::new(move |(token,value):(FieldToken,String)|ui.typed(token,Edit::SetFuel(match value.as_str(){"ratio"=>Fuel::Stoichiometric{ratio:14.7},"blend"=>Fuel::EthanolBlend(Blend::illustrative()),_=>Fuel::EquivalentGasoline})))/>
            <Show when=move ||ui.editor.with(|e|matches!(e.editing_config().fuel,Fuel::Stoichiometric{..}))><TextField ui field=RawField::Fuel(FuelField::Ratio) label="Stoichiometric ratio" hint="5–25"/></Show>
            <Show when=move ||ui.editor.with(|e|matches!(e.editing_config().fuel,Fuel::EthanolBlend(_)))><BlendSettings ui/></Show>
        </div></div>
    }
}

fn blend(ui: Ui) -> Blend {
    ui.editor.with(|e| {
        match e.editing_config().fuel {
            Fuel::EthanolBlend(b) => b,
            _ => Blend::illustrative(),
        }
    })
}
fn edit_blend(
    ui: Ui,
    token: FieldToken,
    f: impl FnOnce(&mut Blend),
) {
    let mut b = blend(ui);
    f(&mut b);
    ui.typed(token, Edit::SetFuel(Fuel::EthanolBlend(b)));
}

#[component]
fn BlendSettings(ui: Ui) -> impl IntoView {
    view! {
        <TextField ui field=RawField::Fuel(FuelField::Percent) label="Ethanol content (%)" hint="0–100; enter an explicit assumption or measurement"/>
        <Choice ui repair_invalid_draft=true label="Percentage basis" value=Signal::derive(move ||if blend(ui).basis==BlendBasis::Volume{"volume"}else{"mass"}.to_string()) options=choices(&[("volume","Component volume"),("mass","Mass")]) on_change=Callback::new(move |(token,value):(FieldToken,String)|edit_blend(ui,token,|b|b.basis=if value=="volume"{BlendBasis::Volume}else{BlendBasis::Mass}))/>
        <Choice ui repair_invalid_draft=true label="Composition input" value=Signal::derive(move ||if blend(ui).composition_origin==CompositionOrigin::Assumed{"assumed"}else{"measured"}.to_string()) options=choices(&[("assumed","Assumed"),("measured","Measured")]) on_change=Callback::new(move |(token,value):(FieldToken,String)|edit_blend(ui,token,|b|b.composition_origin=if value=="measured"{CompositionOrigin::Measured}else{CompositionOrigin::Assumed}))/>
        <Choice ui repair_invalid_draft=true label="Density reference" value=Signal::derive(move ||if matches!(blend(ui).density_reference,DensityReference::UnknownAssumed){"unknown"}else{"common"}.to_string()) options=choices(&[("unknown","Unknown / assumed"),("common","Common temperature")]) on_change=Callback::new(move |(token,value):(FieldToken,String)|edit_blend(ui,token,|b|b.density_reference=if value=="common"{DensityReference::CommonCelsius(20.0)}else{DensityReference::UnknownAssumed}))/>
        <Show when=move ||matches!(blend(ui).density_reference,DensityReference::CommonCelsius(_))><TextField ui field=RawField::Fuel(FuelField::ReferenceCelsius) label="Reference temperature (°C)" hint="−30 to 60; both component densities at this temperature"/></Show>
        <div class="field-pair"><TextField ui field=RawField::Fuel(FuelField::GasolineRatio) label="Gasoline ratio" hint="5–25"/><TextField ui field=RawField::Fuel(FuelField::EthanolRatio) label="Ethanol ratio" hint="5–25; below gasoline"/></div>
        <div class="field-pair"><TextField ui field=RawField::Fuel(FuelField::GasolineDensity) label="Gasoline kg/L" hint="0.5–1.2"/><TextField ui field=RawField::Fuel(FuelField::EthanolDensity) label="Ethanol kg/L" hint="0.5–1.2"/></div>
    }
}

#[component]
pub fn SourceSettings(ui: Ui) -> impl IntoView {
    let selected = ui.source_channel;
    view! {
        <div class="settings-content"><div class="settings-copy"><h3>"Know where a value comes from."</h3><p>"Each channel shares one source across all views. Catalogue entries retain request, decoder and documentary evidence."</p><p class="hint">"DNUC / Simos candidates are unverified on your ECU. A configuration cannot qualify a transport or enable an unimplemented decoder."</p>
            <label class="field"><span>"Channel"</span><select disabled=move ||ui.editor.with(Editor::has_pending_json) prop:value=move ||selected.get().index().to_string() on:change=move |event|{if let Ok(n)=event_target_value(&event).parse::<usize>() && let Some(channel)=Channel::ALL.get(n){selected.set(*channel);}}>
                {Channel::ALL.into_iter().map(|channel|view!{<option value=channel.index().to_string()>{channel.label()}</option>}).collect_view()}
            </select></label>
        </div><div class="settings-form"><For each=move ||vec![selected.get()] key=|channel|channel.index() children=move |channel|view!{<SourcePanel ui channel/>}/></div></div>
    }
}

#[component]
fn SourcePanel(
    ui: Ui,
    channel: Channel,
) -> impl IntoView {
    let source = move || ui.editor.with(|e| e.editing_config().sources[channel.index()]);
    let options = DEFINITIONS
        .into_iter()
        .filter(|id| definition(*id).channel.is_none_or(|c| c == channel))
        .map(|id| (format!("{id:?}"), definition(id).name.to_string()))
        .collect::<Vec<_>>();
    view! {
        <div class="source-panel">
            <Choice ui repair_invalid_draft=true label="Source definition" value=Signal::derive(move ||format!("{:?}",source().definition)) options on_change=Callback::new(move |(token,value):(FieldToken,String)| {
                if let Some(id)=DEFINITIONS.into_iter().find(|id|format!("{id:?}")==value){ui.typed(token,Edit::SetSource{channel,source:definition(id).source(channel)});}
            })/>
            <div class="source-evidence"><span class="chip">{move ||format!("{:?}",source().evidence)}</span><code>{move ||format!("{:?}",source().access)}</code><p>{move ||definition(source().definition).interpretation}</p>
                <p class="hint">{move ||match definition(source().definition).unavailable{Some(reason)=>format!("Unavailable in vehicle mode: {reason:?}"),None if source().definition==DefinitionId::Disabled=>"Channel disabled".into(),None=>"Implemented generic decoder. Deployment qualification and ECU capability are still required.".into()}}</p>
                <div class="source-links">{move ||definition(source().definition).references().iter().enumerate().map(|(n,url)|view!{<a href=*url target="_blank" rel="noopener noreferrer">{format!("Reference {} ↗",n+1)}</a>}).collect_view()}</div>
            </div>
            <Choice ui repair_invalid_draft=true label="Route" value=Signal::derive(move ||match source().route{Route::DeploymentProfile=>"profile",Route::Unknown=>"unknown",Route::Physical11{..}=>"physical"}.to_string()) options=choices(&[("profile","Deployment profile"),("unknown","Unknown"),("physical","Explicit physical 11-bit pair")]) on_change=Callback::new(move |(token,value):(FieldToken,String)| {
                let mut s=source();s.route=match value.as_str(){"profile"=>Route::DeploymentProfile,"physical"=>Route::Physical11{request:0x7e0,response:0x7e8},_=>Route::Unknown};ui.typed(token,Edit::SetSource{channel,source:s});
            })/>
            <Show when=move ||matches!(source().route,Route::Physical11{..})><p class="hint">"Explicit IDs must match the reviewed deployment pair. Defaults shown here are examples, not a discovered ECU."</p><div class="field-pair"><TextField ui field=RawField::Source(channel,SourceField::Request) label="Request ID" hint="0x000–0x7FF physical route"/><TextField ui field=RawField::Source(channel,SourceField::Response) label="Response ID" hint="11-bit; distinct from request"/></div></Show>
            <Toggle ui repair_invalid_draft=true label="Polling requested" value=Signal::derive(move ||matches!(source().poll,Poll::EveryMs(_))) on_change=Callback::new(move |(token,value)| {let mut s=source();s.poll=if value{Poll::EveryMs(minimum_period(channel))}else{Poll::Off};ui.typed(token,Edit::SetSource{channel,source:s});})/>
            <Show when=move ||matches!(source().poll,Poll::EveryMs(_))><TextField ui field=RawField::Source(channel,SourceField::PollMilliseconds) label="Poll period (ms)" hint="Channel floor applies; maximum 10000"/></Show>
            <TextField ui field=RawField::Source(channel,SourceField::FreshnessMilliseconds) label="Freshness limit (ms)" hint="May tighten the channel cap; cannot extend it"/>
        </div>
    }
}
