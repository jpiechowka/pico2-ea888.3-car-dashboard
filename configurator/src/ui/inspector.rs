use super::*;

#[component]
pub fn Inspector(ui: Ui) -> impl IntoView {
    view! {
        <aside class="inspector panel">
            <div class="panel-heading"><p class="eyebrow">"WIDGET INSPECTOR"</p><h2>{move || ui.editor.with(|e|e.selection().widget.and_then(|id|e.editing_config().widgets.iter().find(|w|w.id==id)).map(|w|w.channel.label()).unwrap_or("Make it yours"))}</h2></div>
            <Show when=move || ui.editor.with(|e|e.selection().widget.is_none())>
                <div class="empty-inspector"><div class="grid-icon" aria-hidden="true">"▦"</div><h3>"Select a widget"</h3><p>"Click a tile on the display to edit its label, size, graph and warnings."</p><p>"Drag a sensor from the library into a free cell. Create an empty view for a fresh layout."</p><div class="size-samples"><span>"1 × 1"</span><span>"2 × 1"</span><span>"2 × 2"</span></div></div>
            </Show>
            <For each=move || ui.editor.with(|e|e.selection().widget.and_then(|id|e.editing_config().widgets.iter().find(|w|w.id==id).map(|w|(id,w.channel))).into_iter().collect::<Vec<_>>()) key=|key|(key.0,key.1.index()) children=move |(id,_)| view! { <WidgetInspector ui id/> }/>
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
    on_change=Callback::new(move |(token,value)| ui.update_widget(token,id,|w|set(&mut w.display,value)))/> }
}

#[component]
fn WidgetInspector(
    ui: Ui,
    id: u16,
) -> impl IntoView {
    let channel = ui
        .editor
        .with_untracked(|e| {
            e.editing_config()
                .widgets
                .iter()
                .find(|w| w.id == id)
                .map(|w| w.channel)
        })
        .unwrap_or(Channel::Oil);
    view! {
        <div class="inspector-body">
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
            <h3 class="subheading">"Display"</h3>
            <TextField ui field=RawField::Widget(id,WidgetField::Decimals) label="Decimal places" hint="0–3; units stay canonical in rules"/>
            {flag(ui,id,"History graph",|d|d.graph,|d,v|d.graph=v)}
            {flag(ui,id,"Average",|d|d.average,|d,v|d.average=v)}
            {flag(ui,id,"Minimum & maximum",|d|d.extrema,|d,v|d.extrema=v)}
            {flag(ui,id,"Trend arrow",|d|d.trend,|d,v|d.trend=v)}
            <Show when=move || channel==Channel::Lambda>
                {flag(ui,id,"AFR alongside lambda",|d|d.afr,|d,v|d.afr=v)}
                <p class="hint">"AFR uses the Fuel & AFR assumptions below the preview. Graphs and rules still use lambda."</p>
            </Show>
            <Show when=move || channel==Channel::Boost><p class="hint">"The dashboard button switches bar / PSI, including statistics. Warning and graph limits are in bar."</p></Show>
            <Choice ui label="Graph range" value=Signal::derive(move ||ui.widget(id).map(|w|if matches!(w.display.graph_range,GraphRange::Auto){"auto"}else{"fixed"}.to_string()).unwrap_or_default()) options=choices(&[("auto","Automatic"),("fixed","Fixed canonical range")])
                on_change=Callback::new(move |(token,value):(FieldToken,String)|ui.update_widget(token,id,|w|w.display.graph_range=if value=="fixed" {let(min,max)=w.channel.domain();GraphRange::Fixed{min,max}}else{GraphRange::Auto}))/>
            <Show when=move || ui.widget(id).is_some_and(|w|matches!(w.display.graph_range,GraphRange::Fixed{..}))>
                <div class="field-pair"><TextField ui field=RawField::Widget(id,WidgetField::GraphMinimum) label="Graph minimum"/><TextField ui field=RawField::Widget(id,WidgetField::GraphMaximum) label="Graph maximum"/></div>
            </Show>
            <h3 class="subheading">"Warning rule"</h3>
            <Toggle ui label="Enable warning" value=Signal::derive(move ||ui.widget(id).is_some_and(|w|w.warning.enabled)) on_change=Callback::new(move |(token,value)|ui.update_widget(token,id,|w|w.warning.enabled=value))/>
            <Choice ui label="Applies to" value=Signal::derive(move ||ui.widget(id).map(|w|if w.warning.applicability==Applicability::DemoOnly{"demo"}else{"vehicle"}.to_string()).unwrap_or_default()) options=choices(&[("demo","Demo only"),("vehicle","Vehicle and demo")])
                on_change=Callback::new(move |(token,value):(FieldToken,String)|ui.update_widget(token,id,|w|w.warning.applicability=if value=="demo"{Applicability::DemoOnly}else{Applicability::VehicleAndDemo}))/>
            <p class="hint">"Demo limits are examples. Vehicle thresholds are your own configuration; source and operating-context guards remain enforced."</p>
            <Choice ui label="Rule type" value=Signal::derive(move ||ui.widget(id).map(|w|match w.warning.mode {LimitMode::High=>"high",LimitMode::Low=>"low",LimitMode::LambdaDeviation=>"lambda"}.to_string()).unwrap_or_default())
                options=if channel==Channel::Lambda {choices(&[("lambda","Deviation from target lambda")])} else {choices(&[("high","High value"),("low","Low value")])}
                on_change=Callback::new(move |(token,value):(FieldToken,String)| {
                    let mode=match value.as_str(){"low"=>LimitMode::Low,"high"=>LimitMode::High,_=>return};
                    ui.typed(token,Edit::SetWarningMode{id,mode});
                })/>
            <div class="field-pair"><TextField ui field=RawField::Widget(id,WidgetField::WarningTrigger) label="Trigger"/><TextField ui field=RawField::Widget(id,WidgetField::WarningClear) label="Clear"/></div>
            <Show when=move || channel==Channel::Lambda>
                <TextField ui field=RawField::Widget(id,WidgetField::WarningLoad) label="Minimum load (%)" hint="0–100; required lambda context stays enforced"/>
            </Show>
            <div class="field-pair"><TextField ui field=RawField::Widget(id,WidgetField::WarningOn) label="On dwell (ms)" hint="1–30000"/><TextField ui field=RawField::Widget(id,WidgetField::WarningOff) label="Off dwell (ms)" hint="1–30000"/></div>
            <TextField ui field=RawField::Widget(id,WidgetField::WarningMessage) label="Warning message" hint="1–20 ASCII characters; one line per warning"/>
            <div class="field"><span>"Warning order within this view"</span><div class="field-pair">
                <button disabled=move ||ui.invalid() on:click=move |_|reorder(ui,id,false)>"↑ Earlier"</button>
                <button disabled=move ||ui.invalid() on:click=move |_|reorder(ui,id,true)>"↓ Later"</button>
            </div></div>
            <details><summary>"Source & decoder"</summary><SourcePanel ui channel/></details>
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
        <div class="settings-content"><div class="settings-copy"><h3>"Give each warning its moment."</h3><p>"Messages follow view order, then widget order. Button feedback takes priority; warnings resume afterward."</p><p class="hint">"Preview warning appearance uses synthetic data and compressed dwell. It does not test ECU support or physical timing."</p></div><div class="settings-form">
            <Choice ui label="Show warnings from" value=Signal::derive(move ||ui.editor.with(|e|if e.preview().config().warnings.scope==WarningScope::AllViews{"all"}else{"view"}.to_string())) options=choices(&[("all","All views"),("view","Current view only")]) on_change=Callback::new(move |(token,value):(FieldToken,String)| {
                let mut settings=ui.editor.with_untracked(|e|e.preview().config().warnings);settings.scope=if value=="all"{WarningScope::AllViews}else{WarningScope::CurrentView};ui.typed(token,Edit::SetWarningPresentation(settings));
            })/>
            <Choice ui label="Presentation" value=Signal::derive(move ||ui.editor.with(|e|if e.preview().config().warnings.mode==WarningMode::CycleOne{"one"}else{"three"}.to_string())) options=choices(&[("one","Cycle one warning"),("three","List up to three warnings")]) on_change=Callback::new(move |(token,value):(FieldToken,String)| {
                let mut settings=ui.editor.with_untracked(|e|e.preview().config().warnings);settings.mode=if value=="one"{WarningMode::CycleOne}else{WarningMode::ListThree};ui.typed(token,Edit::SetWarningPresentation(settings));
            })/>
            <TextField ui field=RawField::WarningPageMilliseconds label="Page interval (ms)" hint="500–10000; elapsed time, independent of FPS"/>
        </div></div>
    }
}

#[component]
pub fn FuelSettings(ui: Ui) -> impl IntoView {
    view! {
        <div class="settings-content"><div class="settings-copy"><h3>"Lambda, in your fuel's language."</h3><p>"AFR = lambda × stoichiometric ratio. E5 means up to 5% ethanol, not a measured tank composition."</p><p>"An ethanol blend remains an estimate. Declare the inputs and their basis; the Pico resolves the ratio once at boot."</p><div class="ratio-readout"><small>"RESOLVED STOICHIOMETRY"</small><b>{move || ui.editor.with(|e|format!("{:.4} : 1",e.preview().stoich()))}</b></div></div><div class="settings-form">
            <Choice ui label="AFR scale" value=Signal::derive(move ||ui.editor.with(|e|match e.preview().config().fuel{Fuel::EquivalentGasoline=>"eq",Fuel::Stoichiometric{..}=>"ratio",Fuel::EthanolBlend(_)=>"blend"}.to_string())) options=choices(&[("eq","Gasoline-equivalent 14.7"),("ratio","Explicit stoichiometric ratio"),("blend","Ethanol blend estimate")]) on_change=Callback::new(move |(token,value):(FieldToken,String)|ui.typed(token,Edit::SetFuel(match value.as_str(){"ratio"=>Fuel::Stoichiometric{ratio:14.7},"blend"=>Fuel::EthanolBlend(Blend::illustrative()),_=>Fuel::EquivalentGasoline})))/>
            <Show when=move ||ui.editor.with(|e|matches!(e.preview().config().fuel,Fuel::Stoichiometric{..}))><TextField ui field=RawField::Fuel(FuelField::Ratio) label="Stoichiometric ratio" hint="5–25"/></Show>
            <Show when=move ||ui.editor.with(|e|matches!(e.preview().config().fuel,Fuel::EthanolBlend(_)))><BlendSettings ui/></Show>
        </div></div>
    }
}

fn blend(ui: Ui) -> Blend {
    ui.editor.with(|e| {
        match e.preview().config().fuel {
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
        <Choice ui label="Percentage basis" value=Signal::derive(move ||if blend(ui).basis==BlendBasis::Volume{"volume"}else{"mass"}.to_string()) options=choices(&[("volume","Component volume"),("mass","Mass")]) on_change=Callback::new(move |(token,value):(FieldToken,String)|edit_blend(ui,token,|b|b.basis=if value=="volume"{BlendBasis::Volume}else{BlendBasis::Mass}))/>
        <Choice ui label="Composition input" value=Signal::derive(move ||if blend(ui).composition_origin==CompositionOrigin::Assumed{"assumed"}else{"measured"}.to_string()) options=choices(&[("assumed","Assumed"),("measured","Measured")]) on_change=Callback::new(move |(token,value):(FieldToken,String)|edit_blend(ui,token,|b|b.composition_origin=if value=="measured"{CompositionOrigin::Measured}else{CompositionOrigin::Assumed}))/>
        <Choice ui label="Density reference" value=Signal::derive(move ||if matches!(blend(ui).density_reference,DensityReference::UnknownAssumed){"unknown"}else{"common"}.to_string()) options=choices(&[("unknown","Unknown / assumed"),("common","Common temperature")]) on_change=Callback::new(move |(token,value):(FieldToken,String)|edit_blend(ui,token,|b|b.density_reference=if value=="common"{DensityReference::CommonCelsius(20.0)}else{DensityReference::UnknownAssumed}))/>
        <Show when=move ||matches!(blend(ui).density_reference,DensityReference::CommonCelsius(_))><TextField ui field=RawField::Fuel(FuelField::ReferenceCelsius) label="Reference temperature (°C)" hint="−30 to 60; both component densities at this temperature"/></Show>
        <div class="field-pair"><TextField ui field=RawField::Fuel(FuelField::GasolineRatio) label="Gasoline ratio" hint="5–25"/><TextField ui field=RawField::Fuel(FuelField::EthanolRatio) label="Ethanol ratio" hint="5–25; below gasoline"/></div>
        <div class="field-pair"><TextField ui field=RawField::Fuel(FuelField::GasolineDensity) label="Gasoline kg/L" hint="0.5–1.2"/><TextField ui field=RawField::Fuel(FuelField::EthanolDensity) label="Ethanol kg/L" hint="0.5–1.2"/></div>
    }
}

#[component]
pub fn SourceSettings(ui: Ui) -> impl IntoView {
    let selected = RwSignal::new(Channel::Oil);
    view! {
        <div class="settings-content"><div class="settings-copy"><h3>"Know where a value comes from."</h3><p>"Each channel shares one source across all views. Catalogue entries retain request, decoder and documentary evidence."</p><p class="hint">"DNUC / Simos candidates are unverified on your ECU. A configuration cannot qualify a transport or enable an unimplemented decoder."</p>
            <label class="field"><span>"Channel"</span><select disabled=move ||ui.invalid() prop:value=move ||selected.get().index().to_string() on:change=move |event|{if let Ok(n)=event_target_value(&event).parse::<usize>() && let Some(channel)=Channel::ALL.get(n){selected.set(*channel);}}>
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
    let source = move || ui.editor.with(|e| e.preview().config().sources[channel.index()]);
    let options = DEFINITIONS
        .into_iter()
        .filter(|id| definition(*id).channel.is_none_or(|c| c == channel))
        .map(|id| (format!("{id:?}"), definition(id).name.to_string()))
        .collect::<Vec<_>>();
    view! {
        <div class="source-panel">
            <Choice ui label="Source definition" value=Signal::derive(move ||format!("{:?}",source().definition)) options on_change=Callback::new(move |(token,value):(FieldToken,String)| {
                if let Some(id)=DEFINITIONS.into_iter().find(|id|format!("{id:?}")==value){ui.typed(token,Edit::SetSource{channel,source:definition(id).source(channel)});}
            })/>
            <div class="source-evidence"><span class="chip">{move ||format!("{:?}",source().evidence)}</span><code>{move ||format!("{:?}",source().access)}</code><p>{move ||definition(source().definition).interpretation}</p>
                <p class="hint">{move ||match definition(source().definition).unavailable{Some(reason)=>format!("Unavailable in vehicle mode: {reason:?}"),None if source().definition==DefinitionId::Disabled=>"Channel disabled".into(),None=>"Implemented generic decoder. Deployment qualification and ECU capability are still required.".into()}}</p>
                <div class="source-links">{move ||definition(source().definition).references().iter().enumerate().map(|(n,url)|view!{<a href=*url target="_blank" rel="noopener noreferrer">{format!("Reference {} ↗",n+1)}</a>}).collect_view()}</div>
            </div>
            <Choice ui label="Route" value=Signal::derive(move ||match source().route{Route::DeploymentProfile=>"profile",Route::Unknown=>"unknown",Route::Physical11{..}=>"physical"}.to_string()) options=choices(&[("profile","Deployment profile"),("unknown","Unknown"),("physical","Explicit physical 11-bit pair")]) on_change=Callback::new(move |(token,value):(FieldToken,String)| {
                let mut s=source();s.route=match value.as_str(){"profile"=>Route::DeploymentProfile,"physical"=>Route::Physical11{request:0x7e0,response:0x7e8},_=>Route::Unknown};ui.typed(token,Edit::SetSource{channel,source:s});
            })/>
            <Show when=move ||matches!(source().route,Route::Physical11{..})><p class="hint">"Explicit IDs must match the reviewed deployment pair. Defaults shown here are examples, not a discovered ECU."</p><div class="field-pair"><TextField ui field=RawField::Source(channel,SourceField::Request) label="Request ID" hint="0x000–0x7FF physical route"/><TextField ui field=RawField::Source(channel,SourceField::Response) label="Response ID" hint="11-bit; distinct from request"/></div></Show>
            <Toggle ui label="Polling requested" value=Signal::derive(move ||matches!(source().poll,Poll::EveryMs(_))) on_change=Callback::new(move |(token,value)| {let mut s=source();s.poll=if value{Poll::EveryMs(minimum_period(channel))}else{Poll::Off};ui.typed(token,Edit::SetSource{channel,source:s});})/>
            <Show when=move ||matches!(source().poll,Poll::EveryMs(_))><TextField ui field=RawField::Source(channel,SourceField::PollMilliseconds) label="Poll period (ms)" hint="Channel floor applies; maximum 10000"/></Show>
            <TextField ui field=RawField::Source(channel,SourceField::FreshnessMilliseconds) label="Freshness limit (ms)" hint="May tighten the channel cap; cannot extend it"/>
        </div>
    }
}
