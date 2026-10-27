//! Browser presentation of accepted serialization and separate build evidence.
use dashboard_configurator::config::{Readout, Secondary};
use dashboard_configurator::resources::{
    ConfigResources,
    ResourceAvailability,
    ResourceLimits,
    ResourceReport,
    SourceSelectionStatus,
    canonical_unit,
};

use super::*;

/// Returns a compact summary of `report`, distinguishing current export bytes from
/// last accepted measurements and unavailable/nonexportable state.
pub(super) fn footer(report: &ResourceReport) -> String {
    match (report.availability, report.accepted.as_ref()) {
        (ResourceAvailability::Current, Some(accepted)) => {
            format!(
                "{} widgets · EAV {} B / {} B",
                accepted.widgets_used, accepted.eav_bytes, accepted.limits.eav_bytes
            )
        }
        (ResourceAvailability::LastAccepted(block), Some(accepted)) => {
            format!(
                "not exportable ({block:?}) · last measured accepted EAV {} B",
                accepted.eav_bytes
            )
        }
        (ResourceAvailability::NotExportable(block), _) => {
            format!(
                "not exportable ({block:?}) · EAV limit {} B",
                ResourceLimits::CURRENT.eav_bytes
            )
        }
        _ => "configuration resources unavailable".into(),
    }
}

/// Returns the configured-source status label for `status`; selectable never means live qualified.
fn selection_label(status: SourceSelectionStatus) -> &'static str {
    match status {
        SourceSelectionStatus::Disabled => "Disabled",
        SourceSelectionStatus::Unavailable(_) => "Unsupported candidate",
        SourceSelectionStatus::PollOff => "Polling off",
        SourceSelectionStatus::RouteUnknown => "Route unresolved",
        SourceSelectionStatus::ExecutableSelectionUnqualified => "Selectable; live qualification unverified",
    }
}

/// Returns the presentation unit for `readout`, preserving fixed/toggle and assumed-AFR distinctions.
fn readout_label(readout: Readout) -> &'static str {
    match readout {
        Readout::Celsius => "°C",
        Readout::Fahrenheit => "°F",
        Readout::FollowBoost => "bar / PSI (dashboard toggle)",
        Readout::Bar => "bar (fixed)",
        Readout::Psi => "PSI (fixed)",
        Readout::Kpa => "kPa (fixed)",
        Readout::Volts => "V",
        Readout::Lambda => "λ",
        Readout::Afr => "AFR (configured fuel basis)",
        Readout::LambdaAndAfr => "λ + AFR (configured fuel basis)",
        Readout::IgnitionNumeric => "Signed degrees",
        Readout::IgnitionStatus => "OK / RET",
        Readout::IgnitionStatusAndNumber => "Status + signed degrees",
    }
}

/// Returns the configured `secondary` unit when `available`, an unavailable-span
/// explanation otherwise, or None text for a disabled secondary regardless of capability.
fn secondary_label(
    secondary: Secondary,
    available: bool,
) -> &'static str {
    if secondary == Secondary::None {
        return "None";
    }
    if !available {
        return "Unavailable at this widget span";
    }
    match secondary {
        Secondary::None => "None",
        Secondary::Bar => "bar",
        Secondary::Psi => "PSI",
        Secondary::Kpa => "kPa",
    }
}

/// Returns serialization, occupancy, source and widget facts from `accepted`;
/// `current` controls whether the measurements are labelled exportable now or historical.
/// Values remain configuration facts and do not claim target timing or ECU qualification.
fn accepted_view(
    accepted: ConfigResources,
    current: bool,
) -> impl IntoView {
    let label = if current {
        "Accepted configuration · exportable now"
    } else {
        "Last measured accepted configuration · not exportable now"
    };
    let sources = accepted.sources.clone();
    let widgets = accepted.widgets.clone();
    let views = accepted.views.clone();
    view! {
        <div class="resource-report">
            <p class="resource-state">{label}</p>
            <div class="resource-summary">
                <div><b>{accepted.eav_bytes}</b><span>{format!("EAV bytes / {}", accepted.limits.eav_bytes)}</span></div>
                <div><b>{accepted.json_bytes}</b><span>{format!("Compact JSON bytes / {}", accepted.limits.json_bytes)}</span></div>
                <div><b>{accepted.saved_bytes}</b><span>{format!("Saved envelope bytes / {}", accepted.limits.saved_bytes)}</span></div>
                <div><b>{accepted.saved_overhead}</b><span>"Saved envelope overhead bytes"</span></div>
                <div><b>{format!("{} / {}", accepted.views_used, accepted.limits.views)}</b><span>"Views"</span></div>
                <div><b>{format!("{} / {}", accepted.widgets_used, accepted.limits.widgets)}</b><span>"Widgets"</span></div>
            </div>
            <p class="hint">{format!("Accepted revision {} · view names up to {} ASCII bytes · widget labels up to {} ASCII bytes · warning messages up to {} ASCII bytes.", accepted.accepted_revision, accepted.limits.view_name_bytes, accepted.limits.widget_label_bytes, accepted.limits.warning_message_bytes)}</p>
            <h3>"View occupancy"</h3>
            <div class="workspace-scroll" role="region" aria-label="View occupancy" tabindex="0"><table><thead><tr><th>"View"</th><th>"Grid"</th><th>"Widgets"</th><th>"Cells used"</th><th>"Mask"</th></tr></thead><tbody>
                {views.into_iter().map(|view|view!{<tr><td>{view.name}</td><td>{format!("{} × {}",view.geometry.columns(),view.geometry.rows())}</td><td>{view.widget_count}</td><td>{format!("{} / 8",view.occupied_cells)}</td><td><code>{format!("{:08b}",view.occupied_mask)}</code></td></tr>}).collect_view()}
            </tbody></table></div>
            <h3>"Sources and canonical units"</h3>
            <p class="hint">"Catalogue evidence describes configured requests and decoders. A selectable source still needs ECU, transport, capability and sample qualification at runtime. Source limits are configuration bounds, not operating ranges."</p>
            <div class="workspace-scroll" role="region" aria-label="Source evidence" tabindex="0"><table><thead><tr><th>"Channel"</th><th>"Definition"</th><th>"Selection status"</th><th>"Evidence and route"</th><th>"Poll / freshness"</th><th>"Canonical unit"</th></tr></thead><tbody>
                {sources.into_iter().map(|source|view!{<tr><td>{source.channel.label()}</td><td>{source.catalogue_name}<small>{source.catalogue_interpretation}</small></td><td>{selection_label(source.status)}<small>{source.catalogue_unavailable.map(|reason|format!("{reason:?}")).unwrap_or_default()}</small></td><td>{format!("{:?} · {:?} · {:?} · {:?}",source.configured.evidence,source.configured.access,source.configured.decoder,source.configured.route)}<small>{source.catalogue_references.join(" · ")}</small></td><td>{format!("{:?} / {} ms",source.configured.poll,source.configured.freshness_ms)}</td><td>{canonical_unit(source.channel)}</td></tr>}).collect_view()}
            </tbody></table></div>
            <p class="hint">{format!("Canonical boost is bar and temperature is °C; widget readouts can use other units. Lambda is dimensionless; displayed AFR = lambda × {:.4} from the configured {:?} basis. Fuel composition is not measured by this report.",accepted.stoich_ratio,accepted.fuel)}</p>
            <h3>"Widget capabilities"</h3>
            <div class="workspace-scroll" role="region" aria-label="Widget capabilities" tabindex="0"><table><thead><tr><th>"Widget"</th><th>"View"</th><th>"Canonical"</th><th>"Configured readout"</th><th>"Secondary setting"</th><th>"Capabilities"</th></tr></thead><tbody>
                {widgets.into_iter().map(|widget|view!{<tr><td>{widget.label}</td><td>{widget.view}</td><td>{widget.channel.scalar().map(canonical_unit).unwrap_or("° (synthetic correction)")}</td><td>{readout_label(widget.readout)}</td><td>{secondary_label(widget.secondary,widget.capabilities.secondary)}</td><td>{format!("history {} · markers {} · bands {} · statistics {}",widget.capabilities.history,widget.capabilities.markers,widget.capabilities.bands,widget.capabilities.statistics)}</td></tr>}).collect_view()}
            </tbody></table></div>
        </div>
    }
}

/// Returns `ui`'s accepted-resource report and separate build-evidence navigation.
/// Report errors remain explicit and no draft serialization or physical measurement
/// is substituted for unavailable accepted measurements.
#[component]
pub fn Resources(
    /// Shared accepted project and bounded resource-admission observations.
    ui: Ui
) -> impl IntoView {
    view! { <div class="resource-panel">
        <div class="settings-copy"><h2>"Configuration resources"</h2><p>"Actual accepted serialization sizes and bounded layout facts. Drafts cannot produce export sizes."</p></div>
        {move || match ui.resource_report() {
            Ok(report) => {
                let status = report.availability;
                let summary = if status==ResourceAvailability::Current { "Current accepted export".to_string() } else { footer(&report) };
                let body = match report.accepted {
                    Some(accepted) => accepted_view(accepted,status==ResourceAvailability::Current).into_any(),
                    None => view!{<p class="resource-state">"No accepted serialization has been measured in this session."</p>}.into_any(),
                };
                view! { <div>
                    {summary}
                    {body}
                </div> }.into_any()
            }
            Err(error) => view!{<p class="resource-state">{format!("Configuration resources unavailable: {error:?}")}</p>}.into_any(),
        }}
        <section class="resource-target" aria-label="Target build measurements">
            <h3>"Target measurements"</h3>
            <p>"Inspect admitted firmware files and their validation status in Build & Flash. This panel reports accepted configuration sizes and layout limits."</p>
            <button on:click=move |_| workspaces::activate(ui, Workspace::BuildFlash)>"Open Build & Flash"</button>
            <p class="hint">"Software build evidence requires matching source, compiler, tools, profile, effective options and successful gates. Physical panel speed, voltage accuracy, long-duration stability and ECU qualification require separate hardware evidence."</p>
        </section>
    </div> }
}
