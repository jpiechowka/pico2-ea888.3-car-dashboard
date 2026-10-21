//! Deterministic Markdown projection of the same entries embedded in Studio.
use dashboard_configurator::ecu_reference::ENTRIES;
/// Writes the fixed catalogue and current binding metadata as Markdown to stdout.
/// Panics if stdout cannot be written; does not fetch evidence or alter project state.
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
        if let Some(binding) = entry.binding() {
            println!(
                "Current catalogue request: `{}`. Decoder identifier: `{}`.\n\nReply/implementation note: \
                 {}\n\nAvailability: {}.\n",
                binding.request, binding.decoder, binding.interpretation, binding.availability
            );
        }
        for (index, url) in entry.references.iter().enumerate() {
            println!("- [Source {}]({})", index + 1, url);
        }
        if entry_index + 1 < ENTRIES.len() {
            println!();
        }
    }
}
