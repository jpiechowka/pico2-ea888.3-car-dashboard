//! Advanced controls use the same candidate reducer and shared validator.
use super::*;

#[derive(Clone, Copy)]
/// Typed widget setting family routed through one validated update adapter.
enum Setting {
    /// Primary unit/readout, synchronized with the legacy AFR display flag.
    Readout,
    /// Optional fixed boost companion unit.
    Secondary,
    /// Peak marker selection: off, maximum or minimum.
    Peak,
    /// Reference marker mode, retaining its stored manual value.
    Reference,
    /// Warning ordering priority, independent of qualification.
    Priority,
    /// Global warning message or one line for every affected ignition cylinder.
    IgnitionWarning,
}

/// Returns `setting`'s selector for widget `id` in `ui` with visible `label`.
/// Readouts filter by channel; inactive marker/secondary options remain repairable
/// in invalid drafts. Panics if the selected widget is absent at component creation.
#[component]
fn WidgetChoice(
    /// Shared candidate editor and guarded widget/raw repair adapters.
    ui: Ui,
    /// Stable widget ID resolved through current selection or candidate inspection authority.
    id: u16,
    /// Visible selector label and field-registry binding label.
    label: &'static str,
    /// Advanced widget setting family read and updated by this selector.
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
        Readout::IgnitionNumeric,
        Readout::IgnitionStatus,
        Readout::IgnitionStatusAndNumber,
    ];
    let options = match setting {
        Setting::Readout => {
            readouts
                .into_iter()
                .filter(|r| r.compatible_widget(channel))
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
                            Readout::IgnitionNumeric => "Numeric · signed degrees",
                            Readout::IgnitionStatus => "Status · OK / RET",
                            Readout::IgnitionStatusAndNumber => "Status + number · RET −2.5°",
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
        Setting::IgnitionWarning => {
            choices(&[
                ("Global", "One warning for any cylinder"),
                ("CylinderLines", "Separate affected-cylinder lines"),
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
                    Setting::IgnitionWarning => format!("{:?}", w.advanced.ignition_warning),
                }
            })
            .unwrap_or_default()
    });
    view! { <Choice ui label value options repair_invalid_draft=true
        disabled_values=Signal::derive(move ||if matches!(setting,Setting::Readout) && !ui.invalid() && ui.widget(id).is_some_and(|w|!w.capabilities().secondary) {vec!["LambdaAndAfr".into()]}else{Vec::new()})
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
            Setting::IgnitionWarning=>w.advanced.ignition_warning=match value.as_str(){"CylinderLines"=>IgnitionWarningPresentation::CylinderLines,_=>IgnitionWarningPresentation::Global},
        }))/>
    }
}

/// Returns a preset selector for raw `field` in `ui` with `label` and value/label
/// `options`. Unknown current text selects Custom, which focuses/selects the raw input
/// without changing it; preset values use current raw repair authority.
#[component]
fn Preset(
    /// Shared candidate editor and guarded widget/raw repair adapters.
    ui: Ui,
    /// Raw parser target receiving preset text or focus for custom repair.
    field: RawField,
    /// Visible preset selector label.
    label: &'static str,
    /// Ordered serialized text and display-label pairs; Custom is appended automatically.
    options: Vec<(String, String)>,
) -> impl IntoView {
    let known = options.iter().map(|(v, _)| v.clone()).collect::<Vec<_>>();
    let mut options = options;
    options.push(("custom".into(), "Custom (edit below)".into()));
    view! { <Choice ui label dynamic_id=Signal::derive(move || ui.editor.with(|e| FieldId::Raw(field).preset(e).unwrap_or_else(|_|format!("{}-preset",fields::field_id(field))))) repair_invalid_draft=true options value=Signal::derive(move ||{
        let raw=ui.editor.with(|e|fields::raw_value(e,field));
        if known.contains(&raw) {raw}else{"custom".into()}
    }) on_change=Callback::new(move |(token,value):(FieldToken,String)|{
        if value!="custom" {fields::preset_raw(ui,token,field,value);}else if let Some(input)=web_sys::window().and_then(|w|w.document()).and_then(|d|d.get_element_by_id(&ui.raw_dom(field))).and_then(|e|e.dyn_into::<web_sys::HtmlInputElement>().ok()){let _=input.focus();input.select();}
    })/> }
}

/// Returns readout and smoothing controls for widget `id` in `ui`, retaining canonical
/// thresholds/history while changing presentation. Panics if the widget is absent on mount.
#[component]
pub fn DisplayControls(
    /// Shared candidate editor and guarded widget/raw repair adapters.
    ui: Ui,
    /// Stable widget ID resolved through current selection or candidate inspection authority.
    id: u16,
) -> impl IntoView {
    let channel = ui.widget(id).expect("selected widget").channel;
    view! {

        <WidgetChoice ui id label="Primary readout" setting=Setting::Readout/>
        <Show when=move ||channel==WidgetChannel::Boost || (channel.scalar().is_none() && ui.inspection_active())>
            <WidgetChoice ui id label="Secondary boost unit" setting=Setting::Secondary/>
            <p class="hint">"Follow dashboard uses the physical or preview bar / PSI toggle. Fixed bar, PSI and kPa stay fixed. An equal primary and secondary unit is shown once."</p>
        </Show>
        <Show when=move ||channel==WidgetChannel::Lambda><p class="hint">"AFR = lambda × the Fuel & AFR assumption, never measured fuel composition or ECU requested AFR. Lambda and AFR share the same prepared current reading."</p></Show>
        <Show when=move ||channel.scalar().is_some() && ui.widget(id).is_some_and(|w|!w.capabilities().secondary)><p class="hint">"A compact 1 × 1 gauge shows one readout. Secondary boost units and the AFR companion return at 2 × 1 or 2 × 2; stored choices are retained."</p></Show>
        <Show when=move ||channel.scalar().is_none()><p class="hint">"Four cylinders: 1 / 2 above 3 / 4, in every size. Signed correction degrees; RET means the fresh raw value is negative. OK means no negative correction in this sample, not engine health. Status + number shows both together, including OK 0.0°. Missing, stale or faulted samples never show OK. Studio and firmware Demo use synthetic values; vehicle acquisition is unavailable. Gauges, history, smoothing and secondary units are unsupported."</p></Show>
        <Show when=move ||channel.scalar().is_some() || ui.inspection_active()>
        <Preset ui field=RawField::Widget(id,WidgetField::SmoothingMilliseconds) label="Current-value smoothing" options=choices(&[("0","Off"),("250","Gentle · 250 ms"),("750","Strong · 750 ms")])/>
        <TextField ui field=RawField::Widget(id,WidgetField::SmoothingMilliseconds) label="Smoothing time (ms)" hint="0 = Off; custom 50–5000. Current number and gauge only; history, statistics, peaks and warning thresholds use raw canonical readings."/>
        </Show>
    }
}

/// Returns marker/reference/band controls for widget `id` in `ui`; inactive values
/// remain retained and bounded for repair, and manual references are not ECU targets.
#[component]
pub fn GaugeControls(
    /// Shared candidate editor and guarded widget/raw repair adapters.
    ui: Ui,
    /// Stable widget ID resolved through current selection or candidate inspection authority.
    id: u16,
) -> impl IntoView {
    view! {
        <h3 class="subheading">"Peak & reference markers"</h3>
        <WidgetChoice ui id label="Peak marker" setting=Setting::Peak/>
        <TextField ui field=RawField::Widget(id,WidgetField::PeakHoldMilliseconds) label="Peak hold (ms)" hint="100–60000; default 750. At expiry the next fresh sample starts a new hold; stale/fault clears the marker."/>
        <WidgetChoice ui id label="Reference marker" setting=Setting::Reference/>
        <TextField ui field=RawField::Widget(id,WidgetField::Reference) label="Manual reference (canonical units)" hint="Finite; when Manual is selected, use the channel domain. This is your reference, not an ECU target. Off/Zero retain a finite manual value."/>
        <p class="hint">{move ||if ui.widget(id).is_some_and(|w|w.capabilities().markers){"Markers clip to the gauge interior and never expand its scale. Zero uses actual zero when inside the scale."}else{"Peak, reference and bands are unavailable in Numeric style. Stored values remain bounded and editable for repair."}}</p>
        <BandControls ui id/>
    }
}

/// Returns up to three band editors for widget `id` in `ui`. Add proposes the next
/// channel-domain third from the last maximum and applies only a valid bounded Band;
/// removal and raw repairs use shared reducer validation without silently dropping bands.
#[component]
fn BandControls(
    /// Shared candidate editor and guarded widget/raw repair adapters.
    ui: Ui,
    /// Stable widget ID resolved through current selection or candidate inspection authority.
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

/// Returns window, priority, latch and snooze controls for widget `id` in `ui`.
/// Invalid drafts expose high-bound repair fields; shared validation requires reachable
/// onboard acknowledgement and canonical thresholds independent of presentation units.
#[component]
pub fn WarningControls(
    /// Shared candidate editor and guarded widget/raw repair adapters.
    ui: Ui,
    /// Stable widget ID resolved through current selection or candidate inspection authority.
    id: u16,
) -> impl IntoView {
    view! {
        <Show when=move ||ui.widget(id).is_some_and(|w|w.channel==WidgetChannel::IgnitionRetard) || ui.inspection_active()>
            <WidgetChoice ui id label="Ignition warning popup" setting=Setting::IgnitionWarning/>
            <Show when=move ||ui.widget(id).is_some_and(|w|w.channel==WidgetChannel::IgnitionRetard)><p class="hint">"All four cylinders share this message and signed-degree thresholds. Trigger is strictly below the configured value; clear is at or above its value. Both limits may be zero. RET in the widget still means any raw negative value, independently of the warning threshold. Choose one global message or separate lines for the affected cylinders. Only fresh qualified samples can activate a warning; vehicle data is unavailable."</p></Show>
        </Show>
        <Show when=move ||ui.invalid() || ui.widget(id).is_some_and(|w|matches!(w.warning.mode,LimitMode::OutsideWindow|LimitMode::InsideWindow))>
            <div class="field-pair"><TextField ui field=RawField::Widget(id,WidgetField::WarningTriggerHigh) label="Trigger high"/><TextField ui field=RawField::Widget(id,WidgetField::WarningClearHigh) label="Clear high"/></div>
            <p class="hint">"Outside: trigger low < clear low ≤ clear high < trigger high. Inside: clear low < trigger low ≤ trigger high < clear high. Selecting a window starts with channel-domain example bounds; configure your own thresholds."</p>
        </Show>
        <WidgetChoice ui id label="Warning priority" setting=Setting::Priority/>
        <Toggle ui repair_invalid_draft=true label="Latch until acknowledged and cleared" value=Signal::derive(move ||ui.widget(id).is_some_and(|w|w.warning.advanced.latch)) on_change=Callback::new(move |(token,value)|ui.update_widget(token,id,|w|w.warning.advanced.latch=value))/>
        <TextField ui field=RawField::Widget(id,WidgetField::WarningSnoozeSeconds) label="Acknowledgment snooze (seconds)" hint="0 = Off; otherwise 5–900. Acknowledgment suppresses attention for this time, preserving active/latch indication; a new episode interrupts snooze."/>
        <p class="hint">"Enabled latch or snooze requires an onboard Dashboard button bound to Acknowledge warnings in Device controls. Stale latches retain LATCHED / DATA STALE; statistics reset does not acknowledge a warning."</p>
        <p class="hint">{move ||ui.widget(id).map(|w|{
            let (low,high)=if w.channel==WidgetChannel::Lambda{(0.,2.)}else{w.channel.domain()};
            format!("Threshold units: {} · canonical domain {} to {}. Display units and smoothing never change these thresholds.",match w.channel{WidgetChannel::Boost=>"relative bar",WidgetChannel::Battery=>"volts",WidgetChannel::Lambda=>"lambda deviation",WidgetChannel::IgnitionRetard=>"signed degrees",_=>"°C"},low,high)
        }).unwrap_or_default()}</p>
    }
}
