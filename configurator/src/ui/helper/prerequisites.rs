//! Version summaries stay compact; full tool output remains available on demand.
use super::*;

#[component]
pub fn Results(report: serde_json::Value) -> impl IntoView {
    let rows = report["checks"].as_array().cloned().unwrap_or_default();
    let summary = dashboard_configurator::helper_client::prerequisite_summary(&report);
    view! {
        <details class="build-disclosure" open><summary>"Prerequisite results · installed tool versions"</summary>
            <p>{summary}</p>
            <div class="helper-checks">{rows.into_iter().map(|row| {
                let id=row["id"].as_str().unwrap_or("check").replace('_'," ");
                let status=row["status"].as_str().unwrap_or("unknown").to_owned();
                let evidence=row["evidence"].as_str().unwrap_or("").to_owned();
                let first=evidence.lines().next().unwrap_or("").chars().take(220).collect::<String>();
                let setup=row["setup"].as_str().unwrap_or("").to_owned();
                let full=evidence.len()>first.len();
                view! {<div class=format!("helper-check {status}")><strong>{id}</strong><span>{status.replace('_'," ")}</span>
                    <p>{first}</p>
                    {full.then(||view!{<details class="helper-evidence"><summary>"Full tool output"</summary><pre>{evidence}</pre></details>})}
                    {(!setup.is_empty()).then(||view!{<small>{setup}</small>})}
                </div>}
            }).collect_view()}</div>
        </details>
    }
}
