//! Command-line adapter for fixed vendor checks and bounded manifest-on-stdin tree verification.
use std::io::Read;
use std::path::Path;

use dashboard_provenance::{MANIFEST_LIMIT, check_hal, check_radio, verify_tree};
/// Parse exact OS-argument command shapes and return a verification/help message or diagnostic.
/// Tree mode reads at most MANIFEST_LIMIT + 1 stdin bytes; HAL archives are optional, radio checks
/// use local trees only. Unknown/missing/reordered options fail; no network fetch or extraction occurs.
fn run() -> Result<String, String> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let usage = "usage: dashboard-provenance hal --vendor-root PATH [--archive FILE] | radio --vendor-root PATH | \
                 tree --root PATH (manifest JSON on stdin)";
    match args.as_slice() {
        [kind, flag, root] if kind == "hal" && flag == "--vendor-root" => check_hal(Path::new(root), None),
        [kind, flag, root, option, archive] if kind == "hal" && flag == "--vendor-root" && option == "--archive" => {
            check_hal(Path::new(root), Some(Path::new(archive)))
        }
        [kind, flag, root] if kind == "radio" && flag == "--vendor-root" => check_radio(Path::new(root)),
        [kind, flag, root] if kind == "tree" && flag == "--root" => {
            let mut bytes = Vec::new();
            std::io::stdin()
                .take(MANIFEST_LIMIT as u64 + 1)
                .read_to_end(&mut bytes)
                .map_err(|e| format!("stdin: {e}"))?;
            verify_tree(Path::new(root), &bytes, true)?;
            Ok("Source tree verified".into())
        }
        [help] if help == "--help" => Ok(usage.into()),
        _ => Err(usage.into()),
    }
}
/// Print successful verification/help to stdout; print prefixed failure to stderr and exit with 1.
fn main() {
    match run() {
        Ok(message) => println!("{message}"),
        Err(error) => {
            eprintln!("provenance: {error}");
            std::process::exit(1);
        }
    }
}
