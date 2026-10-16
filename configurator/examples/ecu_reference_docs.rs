//! Deterministic Markdown projection of the same entries embedded in Studio.
use dashboard_configurator::ecu_reference::ENTRIES;
fn main() {
    println!(
        "# ECU signal and research catalogue\n\nGenerated from `configurator/src/ecu_reference.rs`. Research reviewed \
         15 October 2026. No live qualification is established. Formulas are documentary text, never executable \
         decoders.\n"
    );
    for (entry_index, entry) in ENTRIES.iter().enumerate() {
        println!(
            "<a id=\"{}\"></a>\n\n## {}\n\n**{} · {}**\n\nApplicability: {}\n\n{}\n\nUnits: {}\n\nDocumentary \
             decoding/scaling: `{}`\n",
            entry.id,
            entry.title,
            entry.section.label(),
            entry.status.label(),
            entry.scope,
            entry.detail,
            entry.units,
            entry.formula
        );
        if let Some(id) = entry.definition {
            println!("Dashboard binding: `{id:?}`. Live qualification remains required.\n");
        }
        for (index, url) in entry.references.iter().enumerate() {
            println!("- [Source {}]({})", index + 1, url);
        }
        if entry_index + 1 < ENTRIES.len() {
            println!();
        }
    }
}
