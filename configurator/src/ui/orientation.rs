//! Explicit orientation proposals share the reducer and its one-step history.
use super::*;

#[derive(Clone, Copy, PartialEq, Eq)]
/// Orientation setting targeted by a staged, revision-bound proposal.
pub enum Scope {
    /// Mandatory device default inherited by Debug/Logs and unset overrides.
    Device,
    /// Optional orientation of the view with this stored ID.
    View(
        /// Stable view ID, not its candidate list position.
        u8,
    ),
    /// Optional Boot info orientation in the project startup profile.
    Status,
    /// Optional Splash orientation; artwork pixels remain until explicit reconversion.
    Splash,
}
/// Returns the stored orientation for `scope` in `editor`; `None` means inheritance
/// or a missing view. Device reads accepted configuration, view reads editing layout,
/// and startup scopes read the project startup profile.
fn stored(
    editor: &Editor,
    scope: Scope,
) -> Option<Orientation> {
    match scope {
        Scope::Device => Some(editor.preview().config().orientation),
        Scope::View(id) => {
            editor
                .editing_config()
                .views
                .iter()
                .find(|v| v.id == id)
                .and_then(|v| v.orientation)
        }
        Scope::Status => editor.startup_profile().status_orientation,
        Scope::Splash => editor.startup_profile().splash_orientation,
    }
}
/// Returns the visible registry-compatible label for orientation `scope`.
fn label(scope: Scope) -> &'static str {
    match scope {
        Scope::Device => "Default screen orientation",
        Scope::View(_) => "View orientation",
        Scope::Status => "Boot info orientation",
        Scope::Splash => "Splash orientation",
    }
}
/// Returns pixel dimensions, cell-grid dimensions and mounting-flip text for `orientation`.
fn description(orientation: Orientation) -> String {
    let g = orientation.geometry();
    format!(
        "{} × {} · {} × {} grid{}",
        g.width(),
        g.height(),
        g.columns(),
        g.rows(),
        if orientation.inverted() {
            " · mounting flipped 180°"
        } else {
            ""
        }
    )
}
/// Returns a staged orientation picker for `scope` in `ui`. Proposals capture field
/// authority and are cleared on external edits; layout previews use shared repacking.
/// Apply cancels pointer/menu state, rechecks the token and dispatches one project edit.
/// Failed repacking blocks Apply; artwork bytes remain retained across shape mismatch.
#[component]
pub fn Picker(
    /// Shared editor and layout interaction state.
    ui: Ui,
    /// Device, view or startup override to propose.
    scope: Scope,
) -> impl IntoView {
    let proposed = RwSignal::new(None::<(FieldToken, Option<Orientation>)>);
    let current = move || ui.editor.with(|e| stored(e, scope));
    let effective = move || current().unwrap_or_else(|| ui.editor.with(|e| e.preview().config().orientation));
    let id = move || {
        match scope {
            Scope::Device => Registry::dom_id(FieldKey::Orientation, TargetOwner::Root),
            Scope::View(id) => {
                ui.editor.with(|e| {
                    Registry::dom_id(
                        FieldKey::ViewOrientation,
                        TargetOwner::View(e.editing_config().views.iter().position(|v| v.id == id).unwrap_or(0)),
                    )
                })
            }
            Scope::Status => Registry::dom_id(FieldKey::StatusOrientation, TargetOwner::Root),
            Scope::Splash => Registry::dom_id(FieldKey::SplashOrientation, TargetOwner::Root),
        }
    };
    Effect::new(move |_| {
        let token = ui.editor.with(Editor::field_token);
        if proposed.get_untracked().is_some_and(|p| p.0 != token) {
            proposed.set(None);
        }
    });
    view! { <section class="orientation-card" aria-label=label(scope)>
        <div class="orientation-heading"><strong>{label(scope)}</strong><span class="orientation-effective">{move ||format!("{}{}",if current().is_none(){"Inherited · "}else{""}, description(effective()))}</span></div>
        <select id=id aria-label=label(scope) disabled=move ||ui.inspection_active()
            prop:value=move ||format!("{:?}",proposed.get().map(|p|p.1).unwrap_or_else(current))
            on:change=move |event| { let value=event_target_value(&event); let orientation=Orientation::ALL.into_iter().find(|o|format!("{:?}",Some(*o))==value); proposed.set(Some((ui.editor.with_untracked(Editor::field_token),orientation))); }>
            <Show when=move ||scope!=Scope::Device><option value="None">"Use device default"</option></Show>
            {Orientation::ALL.into_iter().map(|o|view!{<option value=format!("{:?}",Some(o))>{match o {Orientation::Landscape=>"Landscape · 320 × 240",Orientation::Portrait=>"Portrait · 240 × 320",Orientation::LandscapeInverted=>"Landscape · flipped 180°",Orientation::PortraitInverted=>"Portrait · flipped 180°"}}</option>}).collect_view()}
        </select>
        <div class="mini-grid" style=move ||format!("grid-template-columns:repeat({},1fr)",effective().geometry().columns()) aria-hidden="true">{(0..8).map(|_|view!{<i></i>}).collect_view()}</div>
        <Show when=move ||scope==Scope::Device><p class="hint">"Debug and Logs inherit this default. Views, Boot info and Splash can override it. A/B/X/Y and encoder actions keep their physical identities."</p></Show>
        <Show when=move ||proposed.get().is_some()>{move ||proposed.get().map(|(token,orientation)|{
            let result=ui.editor.with(|e| {
                let before=e.preview().config(); let mut next=before.clone();
                match scope {Scope::Device=>next.orientation=orientation.unwrap_or_default(),Scope::View(id)=>{if let Some(view)=next.views.iter_mut().find(|v|v.id==id){view.orientation=orientation;}},_=>{}}
                let affected=next.views.iter().filter(|v|before.view_orientation(v.id).map(|o|o.geometry())!=next.view_orientation(v.id).map(|o|o.geometry())).count();
                dashboard_configurator::orientation::repack_changed_views(before,&mut next).map(|moved|(affected,moved))
            });
            let summary=match result {Ok((views,moved))=>format!("Proposed: {}. {} inherited/selected views repacked; {} widgets moved. Settings, spans and order are preserved. One Undo restores the complete project.",description(orientation.unwrap_or_else(||ui.editor.with(|e|e.preview().config().orientation))),views,moved),Err(error)=>error_text(error)};
            let layouts=ui.editor.with(|e| {
                let before=e.preview().config();let mut next=before.clone();
                match scope {Scope::Device=>next.orientation=orientation.unwrap_or_default(),Scope::View(id)=>{if let Some(v)=next.views.iter_mut().find(|v|v.id==id){v.orientation=orientation;}},_=>{}}
                if dashboard_configurator::orientation::repack_changed_views(before,&mut next).is_err(){return Vec::new();}
                next.views.iter().filter(|v|before.view_orientation(v.id)!=next.view_orientation(v.id)).map(|v|{
                    let g=next.view_orientation(v.id).unwrap().geometry();
                    let tiles=next.widgets.iter().filter(|w|w.view==v.id).filter_map(|w|w.placement.rect_in(g).map(|r|(format!("#{} {}",w.id,w.label),format!("left:{}%;top:{}%;width:{}%;height:{}%",f64::from(r.x)*100./f64::from(g.width()),f64::from(r.y-26)*100./f64::from(g.height()-26),f64::from(r.width)*100./f64::from(g.width()),f64::from(r.height)*100./f64::from(g.height()-26))))).collect::<Vec<_>>();
                    (v.name.to_string(),g,tiles)
                }).collect::<Vec<_>>()
            });
            view!{<div class="orientation-proposal"><p role="status">{summary}</p>
                <div class="orientation-layouts">{layouts.into_iter().map(|(name,g,tiles)|view!{<figure><figcaption>{name}</figcaption><div class="orientation-layout" style=format!("aspect-ratio:{}/{}",g.width(),g.height()-26)>{tiles.into_iter().map(|(label,style)|view!{<span style=style>{label}</span>}).collect_view()}</div></figure>}).collect_view()}</div><p class="hint">"2 × 1 stays horizontal in both shapes. Custom Splash artwork keeps its current pixels until reconverted to the effective shape."</p>
                <button class="primary" disabled=result.is_err() on:click=move |_| {
                    stage::cancel(ui);widget_menu::close(ui,false);
                    let success=ui.run(|e| {
                        if e.field_token()!=token{return Err(EditorError::StaleToken);}
                        match scope {
                            Scope::Device=>e.apply(Edit::SetOrientation(orientation.unwrap_or_default())),
                            Scope::View(id)=>e.apply(Edit::SetViewOrientation{id,orientation}),
                            Scope::Status|Scope::Splash=>{let mut p=e.startup_profile();if scope==Scope::Status{p.status_orientation=orientation;}else{p.splash_orientation=orientation;}e.set_startup_profile(p)}
                        }
                    });
                    if success {ui.event(Notice::info(Category::Editor,"orientation.changed",Some(label(scope)),"Orientation accepted as one project transaction"));proposed.set(None);}
                }>"Apply orientation"</button><button on:click=move |_|proposed.set(None)>"Cancel"</button>
            </div>}
        })}</Show>
    </section> }
}
