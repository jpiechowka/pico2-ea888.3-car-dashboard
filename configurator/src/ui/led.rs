//! D2 controls retain inactive settings and use shared bounded validation.
use super::*;

fn change(
    ui: Ui,
    token: FieldToken,
    f: impl FnOnce(&mut LedSettings),
) {
    let mut settings = ui.editor.with_untracked(|e| e.editing_config().led);
    f(&mut settings);
    ui.typed(token, Edit::SetLed(settings));
}

#[component]
fn Pattern(
    ui: Ui,
    warning: bool,
) -> impl IntoView {
    let pattern = move || {
        ui.editor.with(|e| {
            if warning {
                e.editing_config().led.warning
            } else {
                e.editing_config().led.normal
            }
        })
    };
    view! {
        <fieldset><legend>{if warning{"Warning indicator"}else{"Normal indicator"}}</legend>
            <Choice ui repair_invalid_draft=true label="Pattern" id=Registry::dom_id(FieldKey::LedPattern,TargetOwner::Led(warning)) accessible_label=if warning{"Warning LED pattern".into()}else{"Normal LED pattern".into()}
                value=Signal::derive(move ||format!("{:?}",pattern().kind)) options=choices(&[("Off","Off"),("Solid","Solid"),("Blink","Blink"),("Breathe","Breathe")])
                on_change=Callback::new(move |(token,value):(FieldToken,String)|change(ui,token,|s|{
                    let p=if warning{&mut s.warning}else{&mut s.normal};
                    p.kind=match value.as_str(){"Off"=>LedPatternKind::Off,"Blink"=>LedPatternKind::Blink,"Breathe"=>LedPatternKind::Breathe,_=>LedPatternKind::Solid};
                }))/>
            <ColorField ui field=RawField::Led{warning,field:LedField::Color} label=if warning{"Warning LED colour"}else{"Normal LED colour"}/>
            <TextField ui field=RawField::Led{warning,field:LedField::Brightness} label=if warning{"Warning LED brightness (%)"}else{"Normal LED brightness (%)"} hint="0–100; zero and black explicitly turn this selected pattern dark. LED colours have no text-contrast requirement."/>
            <TextField ui field=RawField::Led{warning,field:LedField::Period} label=if warning{"Warning LED period (ms)"}else{"Normal LED period (ms)"} hint="200–10000; retained and bounded for every pattern" disabled=Signal::derive(move ||!ui.invalid() && (matches!(pattern().kind,LedPatternKind::Off|LedPatternKind::Solid) || (warning && pattern().kind==LedPatternKind::Blink && ui.editor.with(|e|e.editing_config().led.sync_warning_blink))))/>
            <TextField ui field=RawField::Led{warning,field:LedField::OnPercent} label=if warning{"Warning LED on-time (%)"}else{"Normal LED on-time (%)"} hint="10–90; independent Blink only. Inactive values stay stored and bounded." disabled=Signal::derive(move ||!ui.invalid() && (pattern().kind!=LedPatternKind::Blink || (warning && ui.editor.with(|e|e.editing_config().led.sync_warning_blink))))/>
        </fieldset>
    }
}

#[component]
pub fn LedPanel(ui: Ui) -> impl IntoView {
    view! {
        <div class="settings-content"><div class="settings-copy"><h3>"An indicator for each moment."</h3>
            <p>"Normal and warning colours, brightness and timing are independent. Warning Off stays dark while that warning is selected."</p>
            <p class="hint">"Synchronized warning Blink follows the warning phase actually displayed. With no visible popup, enabled background warnings use stored independent timing. Completed presentation determines selection."</p>
            <Toggle ui repair_invalid_draft=true label="Reduced motion" value=Signal::derive(move ||ui.editor.with(|e|e.editing_config().reduced_motion)) on_change=Callback::new(move |(token,value)|ui.typed(token,Edit::SetReducedMotion(value)))/>
            <p class="hint">"Reduced motion removes decorative interpolation and makes Breathe solid. Explicit warning Blink keeps its timing; choose Solid or Off to remove it."</p>
        </div><div class="settings-form">
            <Pattern ui warning=false/><Pattern ui warning=true/>
            <Toggle ui repair_invalid_draft=true label="Synchronize warning Blink with popup" value=Signal::derive(move ||ui.editor.with(|e|e.editing_config().led.sync_warning_blink)) on_change=Callback::new(move |(token,value)|change(ui,token,|s|s.sync_warning_blink=value))/>
            <Toggle ui repair_invalid_draft=true label="Indicate warnings during Dashboard feedback" value=Signal::derive(move ||ui.editor.with(|e|e.editing_config().led.during_feedback)) on_change=Callback::new(move |(token,value)|change(ui,token,|s|s.during_feedback=value))/>
            <Toggle ui repair_invalid_draft=true label="Indicate warnings on Debug" value=Signal::derive(move ||ui.editor.with(|e|e.editing_config().led.on_debug)) on_change=Callback::new(move |(token,value)|change(ui,token,|s|s.on_debug=value))/>
            <Toggle ui repair_invalid_draft=true label="Indicate warnings on Logs" value=Signal::derive(move ||ui.editor.with(|e|e.editing_config().led.on_logs)) on_change=Callback::new(move |(token,value)|change(ui,token,|s|s.on_logs=value))/>
            <Toggle ui repair_invalid_draft=true label="Allow warning indication with backlight off" value=Signal::derive(move ||ui.editor.with(|e|e.editing_config().led.backlight_off)) on_change=Callback::new(move |(token,value)|change(ui,token,|s|s.backlight_off=value))/>
            <p class="hint">"With the backlight off, this switch and the applicable Debug, Logs or feedback switch must both allow the warning. Source qualification, freshness and warning view scope still apply."</p>
        </div></div>
    }
}
