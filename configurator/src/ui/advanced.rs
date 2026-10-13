//! Advanced controls use the same candidate reducer and shared validator.
use super::*;

#[derive(Clone, Copy)]
enum Setting {
    Readout,
    Secondary,
    Peak,
    Reference,
    Priority,
}

#[component]
fn WidgetChoice(
    ui: Ui,
    id: u16,
    label: &'static str,
    setting: Setting,
) -> impl IntoView {
    let channel = ui.widget(id).expect("selected widget").channel;
    let readouts = [
        Readout::Celsius,
        Readout::Fahrenheit,
        Readout::FollowBoost,
        Readout::Bar,
        Readout::Psi,
        Readout::Kpa,
        Readout::Volts,
        Readout::Lambda,
        Readout::Afr,
        Readout::LambdaAndAfr,
    ];
    let options = match setting {
        Setting::Readout => {
            readouts
                .into_iter()
                .filter(|r| r.compatible(channel))
                .map(|r| {
                    (
                        format!("{r:?}"),
                        match r {
                            Readout::Celsius => "Celsius (°C)",
                            Readout::Fahrenheit => "Fahrenheit (°F)",
                            Readout::FollowBoost => "Follow dashboard bar / PSI button",
                            Readout::Bar => "Fixed bar",
                            Readout::Psi => "Fixed PSI",
                            Readout::Kpa => "Fixed kPa",
                            Readout::Volts => "Volts",
                            Readout::Lambda => "Lambda",
                            Readout::Afr => "AFR estimate",
                            Readout::LambdaAndAfr => "Lambda + AFR estimate",
                        }
                        .to_string(),
                    )
                })
                .collect()
        }
        Setting::Secondary => choices(&[("None", "None"), ("Bar", "bar"), ("Psi", "PSI"), ("Kpa", "kPa")]),
        Setting::Peak => choices(&[("Off", "Off"), ("Maximum", "Maximum"), ("Minimum", "Minimum")]),
        Setting::Reference => choices(&[("Off", "Off"), ("Zero", "Zero"), ("Manual", "Manual reference")]),
        Setting::Priority => {
            choices(&[
                ("Low", "Low"),
                ("Normal", "Normal"),
                ("High", "High"),
                ("Critical", "Critical"),
            ])
        }
    };
    let value = Signal::derive(move || {
        ui.widget(id)
            .map(|w| {
                match setting {
                    Setting::Readout => format!("{:?}", w.advanced.readout),
                    Setting::Secondary => format!("{:?}", w.advanced.secondary),
                    Setting::Peak => format!("{:?}", w.advanced.peak_mode),
                    Setting::Reference => format!("{:?}", w.advanced.reference_mode),
                    Setting::Priority => format!("{:?}", w.warning.advanced.priority),
                }
            })
            .unwrap_or_default()
    });
    view! { <Choice ui label value options repair_invalid_draft=true
        disabled_values=Signal::derive(move ||if matches!(setting,Setting::Readout) && !ui.invalid() && ui.widget(id).is_some_and(|w|!w.capabilities().secondary) {vec!["LambdaAndAfr".into()]}else{Vec::new()})
        id=format!("widget-{id}-{}",match setting {Setting::Readout=>"readout",Setting::Secondary=>"secondary",Setting::Peak=>"peak",Setting::Reference=>"reference-mode",Setting::Priority=>"priority"})
        disabled=Signal::derive(move || !ui.invalid() && ui.widget(id).is_some_and(|w| match setting {
            Setting::Secondary=>!w.capabilities().secondary,
            Setting::Peak | Setting::Reference=>!w.capabilities().markers,
            _=>false,
        }))
        on_change=Callback::new(move |(token,value):(FieldToken,String)|ui.update_widget(token,id,|w| match setting {
            Setting::Readout=>if let Some(r)=readouts.into_iter().find(|r|format!("{r:?}")==value){w.advanced.readout=r;w.display.afr=r.includes_afr();},
            Setting::Secondary=>w.advanced.secondary=match value.as_str(){"Bar"=>Secondary::Bar,"Psi"=>Secondary::Psi,"Kpa"=>Secondary::Kpa,_=>Secondary::None},
            Setting::Peak=>w.advanced.peak_mode=match value.as_str(){"Maximum"=>PeakSelection::Maximum,"Minimum"=>PeakSelection::Minimum,_=>PeakSelection::Off},
            Setting::Reference=>w.advanced.reference_mode=match value.as_str(){"Zero"=>ReferenceMode::Zero,"Manual"=>ReferenceMode::Manual,_=>ReferenceMode::Off},
            Setting::Priority=>w.warning.advanced.priority=match value.as_str(){"Low"=>Priority::Low,"High"=>Priority::High,"Critical"=>Priority::Critical,_=>Priority::Normal},
        }))/>
    }
}

#[component]
fn Preset(
    ui: Ui,
    field: RawField,
    label: &'static str,
    options: Vec<(String, String)>,
) -> impl IntoView {
    let known = options.iter().map(|(v, _)| v.clone()).collect::<Vec<_>>();
    let mut options = options;
    options.push(("custom".into(), "Custom (edit below)".into()));
    view! { <Choice ui label repair_invalid_draft=true options value=Signal::derive(move ||{
        let raw=ui.editor.with(|e|fields::raw_value(e,field));
        if known.contains(&raw) {raw}else{"custom".into()}
    }) on_change=Callback::new(move |(token,value):(FieldToken,String)|{
        if value!="custom" {ui.run(move |e|e.set_raw(token,field,value));}else if let Some(input)=web_sys::window().and_then(|w|w.document()).and_then(|d|d.get_element_by_id(&fields::field_id(field))).and_then(|e|e.dyn_into::<web_sys::HtmlInputElement>().ok()){let _=input.focus();input.select();}
    })/> }
}

#[component]
pub fn DisplayControls(
    ui: Ui,
    id: u16,
) -> impl IntoView {
    let channel = ui.widget(id).expect("selected widget").channel;
    view! {
        <h3 class="subheading">"Units & current value"</h3>
        <WidgetChoice ui id label="Primary readout" setting=Setting::Readout/>
        <Show when=move ||channel==Channel::Boost>
            <WidgetChoice ui id label="Secondary boost unit" setting=Setting::Secondary/>
            <p class="hint">"Follow dashboard uses the physical or preview bar / PSI toggle. Fixed bar, PSI and kPa stay fixed. An equal primary and secondary unit is shown once."</p>
        </Show>
        <Show when=move ||channel==Channel::Lambda><p class="hint">"AFR = lambda × the Fuel & AFR assumption, never measured fuel composition or ECU requested AFR. Lambda and AFR share the same prepared current reading."</p></Show>
        <Show when=move ||ui.widget(id).is_some_and(|w|!w.capabilities().secondary)><p class="hint">"A compact 1 × 1 gauge shows one readout. Secondary boost units and the AFR companion return at 2 × 1 or 2 × 2; stored choices are retained."</p></Show>
        <Preset ui field=RawField::Widget(id,WidgetField::SmoothingMilliseconds) label="Current-value smoothing" options=choices(&[("0","Off"),("250","Gentle · 250 ms"),("750","Strong · 750 ms")])/>
        <TextField ui field=RawField::Widget(id,WidgetField::SmoothingMilliseconds) label="Smoothing time (ms)" hint="0 = Off; custom 50–5000. Current number and gauge only; history, statistics, peaks and warning thresholds use raw canonical readings."/>
        <h3 class="subheading">"Channel-shared history"</h3>
        <Preset ui field=RawField::HistorySeconds(channel) label="History duration preset" options=choices(&[("10","10 seconds"),("30","30 seconds"),("120","2 minutes"),("300","5 minutes")])/>
        <TextField ui field=RawField::HistorySeconds(channel) label="Channel-shared history (seconds)" hint="10–900. Every widget using this channel shares the duration; changing it clears its history, preserving independent extrema."/>
        <p class="hint">{move ||if ui.widget(id).is_some_and(|w|w.capabilities().history){"History uses 60 timed buckets. Sparse samples and faults leave visible gaps; nothing is backfilled. Duration is shown while filling. This is a trend view, not a waveform or peak recorder."}else{"Gauge styles replace history. This channel-shared duration remains stored for numeric widgets."}}</p>
        <h3 class="subheading">"Peak & reference markers"</h3>
        <WidgetChoice ui id label="Peak marker" setting=Setting::Peak/>
        <TextField ui field=RawField::Widget(id,WidgetField::PeakHoldMilliseconds) label="Peak hold (ms)" hint="100–60000; default 750. At expiry the next fresh sample starts a new hold; stale/fault clears the marker."/>
        <WidgetChoice ui id label="Reference marker" setting=Setting::Reference/>
        <TextField ui field=RawField::Widget(id,WidgetField::Reference) label="Manual reference (canonical units)" hint="Finite; when Manual is selected, use the channel domain. This is your reference, not an ECU target. Off/Zero retain a finite manual value."/>
        <p class="hint">{move ||if ui.widget(id).is_some_and(|w|w.capabilities().markers){"Markers clip to the gauge interior and never expand its scale. Zero uses actual zero when inside the scale."}else{"Peak, reference and bands are unavailable in Numeric style. Stored values remain bounded and editable for repair."}}</p>
        <BandControls ui id/>
    }
}

#[component]
fn BandControls(
    ui: Ui,
    id: u16,
) -> impl IntoView {
    view! {
        <details open=true class="palette-panel"><summary>"Gauge bands"</summary>
            <p class="hint">"Up to three ordered, non-overlapping intervals in canonical units; touching endpoints are allowed. Colours need 3:1 contrast with the track. Labels appear at 2 × 2; smaller gauges retain them."</p>
            <For each=move ||ui.widget(id).map_or(Vec::new(),|w|(0..w.advanced.bands().len() as u8).collect::<Vec<_>>()) key=|i|*i children=move |i|view!{
                <fieldset><legend>{format!("Band {}",i+1)}</legend>
                    <div class="field-pair"><TextField ui field=RawField::Widget(id,WidgetField::BandMinimum(i)) label="Band minimum"/><TextField ui field=RawField::Widget(id,WidgetField::BandMaximum(i)) label="Band maximum"/></div>
                    <TextField ui field=RawField::Widget(id,WidgetField::BandLabel(i)) label="Band label" hint="0–5 printable ASCII characters; empty is allowed"/>
                    <ColorField ui field=RawField::Widget(id,WidgetField::BandColor(i)) label="Band colour"/>
                    <button on:click=move |_|ui.apply(Edit::RemoveBand{id,index:i})>"Remove band"</button>
                </fieldset>
            }/>
            <button disabled=move ||ui.widget(id).is_none_or(|w|w.advanced.bands().len()>=3 || (!w.capabilities().bands && !ui.invalid())) on:click=move |_|{
                let token=ui.editor.with_untracked(Editor::field_token);
                ui.update_widget(token,id,|w|{
                    let mut bands=w.advanced.bands().to_vec();
                    let (low,high)=w.channel.domain();
                    let min=bands.last().map_or(low,|b|b.max);
                    if let Ok(band)=Band::new(min,(min+(high-low)/3.).min(high),w.colors.gauge,""){bands.push(band);let _=w.advanced.set_bands(&bands);}
                });
            }>"Add band"</button>
        </details>
    }
}

#[component]
pub fn WarningControls(
    ui: Ui,
    id: u16,
) -> impl IntoView {
    view! {
        <Show when=move ||ui.invalid() || ui.widget(id).is_some_and(|w|matches!(w.warning.mode,LimitMode::OutsideWindow|LimitMode::InsideWindow))>
            <div class="field-pair"><TextField ui field=RawField::Widget(id,WidgetField::WarningTriggerHigh) label="Trigger high"/><TextField ui field=RawField::Widget(id,WidgetField::WarningClearHigh) label="Clear high"/></div>
            <p class="hint">"Outside: trigger low < clear low ≤ clear high < trigger high. Inside: clear low < trigger low ≤ trigger high < clear high. Selecting a window starts with channel-domain example bounds; configure your own thresholds."</p>
        </Show>
        <WidgetChoice ui id label="Warning priority" setting=Setting::Priority/>
        <Toggle ui repair_invalid_draft=true label="Latch until acknowledged and cleared" value=Signal::derive(move ||ui.widget(id).is_some_and(|w|w.warning.advanced.latch)) on_change=Callback::new(move |(token,value)|ui.update_widget(token,id,|w|w.warning.advanced.latch=value))/>
        <TextField ui field=RawField::Widget(id,WidgetField::WarningSnoozeSeconds) label="Acknowledgment snooze (seconds)" hint="0 = Off; otherwise 5–900. Acknowledgment suppresses attention for this time, preserving active/latch indication; a new episode interrupts snooze."/>
        <p class="hint">"Enabled latch or snooze requires an onboard Dashboard button bound to Acknowledge warnings in Device controls. Stale latches retain LATCHED / DATA STALE; statistics reset does not acknowledge a warning."</p>
        <p class="hint">{move ||ui.widget(id).map(|w|{
            let (low,high)=if w.channel==Channel::Lambda{(0.,2.)}else{w.channel.domain()};
            format!("Threshold units: {} · canonical domain {} to {}. Display units and smoothing never change these thresholds.",match w.channel{Channel::Boost=>"relative bar",Channel::Battery=>"volts",Channel::Lambda=>"lambda deviation",_=>"°C"},low,high)
        }).unwrap_or_default()}</p>
    }
}
