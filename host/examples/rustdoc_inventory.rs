//! Source-level Rust documentation inventory, including cfg-disabled/private items.
//!
//! Run from `host/` with `cargo run --example rustdoc_inventory -- ..`.
//! This checks documentation structure, not the truth of prose or macro expansion;
//! supported-target rustdoc builds and semantic review remain separate gates.

#[path = "rustdoc_inventory/mod.rs"]
mod rustdoc_inventory;

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Write};
use std::path::{Component, Path};
use std::process::{Command, ExitCode, Stdio};

/// Maximum Git path-list bytes admitted before terminating the listing process.
const MAX_LIST_BYTES: u64 = 2 * 1024 * 1024;
/// Maximum UTF-8 source size admitted per file, in bytes.
const MAX_SOURCE_BYTES: u64 = 4 * 1024 * 1024;

/// Inventory CLI entry; returns failure for input/I/O errors or requested gaps.
///
/// One root path is required; `--check` additionally fails on uncovered contracts.
/// JSON is written only after the complete source inventory parses successfully.
fn main() -> ExitCode {
    match run() {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(error) => {
            eprintln!("rustdoc inventory: {error}");
            ExitCode::from(2)
        }
    }
}

/// Consume CLI arguments and emit JSON; returns true when the requested gate passes.
///
/// Errors retain path context. No source files, devices or Git objects are changed.
fn run() -> Result<bool, String> {
    let mut args = std::env::args().skip(1);
    let root = args.next().ok_or("usage: rustdoc_inventory ROOT [--check]")?;
    let remaining: Vec<_> = args.collect();
    if remaining.iter().any(|arg| arg != "--check") || remaining.len() > 1 {
        return Err("usage: rustdoc_inventory ROOT [--check]".into());
    }
    let report = inventory(Path::new(&root))?;
    let passes = remaining.is_empty() || report["uncovered_items"] == 0;
    let stdout = std::io::stdout();
    let mut output = stdout.lock();
    serde_json::to_writer_pretty(&mut output, &report).map_err(|error| error.to_string())?;
    output.write_all(b"\n").map_err(|error| error.to_string())?;
    Ok(passes)
}

/// List tracked and new unignored Rust paths beneath `root`; returns unique paths.
///
/// Errors reject non-UTF-8 output, oversized listings and failed Git commands.
/// Git stderr is inherited; overflow kills and joins only the owned child.
fn rust_paths(root: &Path) -> Result<BTreeSet<String>, String> {
    let mut child = Command::new("git")
        .arg("-C")
        .arg(root)
        .args([
            "ls-files",
            "--cached",
            "--others",
            "--exclude-standard",
            "-z",
            "--",
            "*.rs",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|error| format!("git inventory: {error}"))?;
    let mut bytes = Vec::new();
    let read = child
        .stdout
        .take()
        .ok_or("Git stdout unavailable")?
        .take(MAX_LIST_BYTES + 1)
        .read_to_end(&mut bytes);
    if read.is_err() || bytes.len() as u64 > MAX_LIST_BYTES {
        let _ = child.kill();
        let _ = child.wait();
        return Err("Git inventory read failed or exceeded 2 MiB".into());
    }
    if !child.wait().map_err(|error| error.to_string())?.success() {
        return Err("Git inventory command failed".into());
    }
    let text = String::from_utf8(bytes).map_err(|error| format!("Git paths are not UTF-8: {error}"))?;
    let paths: BTreeSet<_> = text
        .split('\0')
        .filter(|path| !path.is_empty())
        .map(str::to_owned)
        .collect();
    if paths.is_empty() {
        return Err("no Rust sources found".into());
    }
    Ok(paths)
}

/// Accept only relative normal components in `path`; returns false for escapes.
///
/// Both platform separator forms are validated, including Windows drive syntax.
fn safe_relative(path: &str) -> bool {
    !path.is_empty()
        && !path.contains(['\\', ':'])
        && Path::new(path)
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
}

/// Parse every listed Rust file under `root`; returns complete source-order JSON.
///
/// Errors reject escaping/symlinked paths, missing or oversized files and syntax
/// the pinned parser cannot understand. Cfg branches are retained, macros unexpanded.
fn inventory(root: &Path) -> Result<serde_json::Value, String> {
    let root = root.canonicalize().map_err(|error| format!("root: {error}"))?;
    let paths = rust_paths(&root)?;
    let mut files = Vec::new();
    let mut total = 0usize;
    let mut uncovered = 0usize;
    let mut sources = BTreeMap::new();
    let mut parsed = Vec::new();
    for relative in paths {
        if !safe_relative(&relative) {
            return Err(format!("unsafe source path: {relative}"));
        }
        let path = root.join(&relative);
        let resolved = path.canonicalize().map_err(|error| format!("{relative}: {error}"))?;
        if !resolved.starts_with(&root)
            || path
                .symlink_metadata()
                .map_err(|error| error.to_string())?
                .file_type()
                .is_symlink()
        {
            return Err(format!("source path escapes or is a symlink: {relative}"));
        }
        let mut bytes = Vec::new();
        std::fs::File::open(&resolved)
            .map_err(|error| format!("{relative}: {error}"))?
            .take(MAX_SOURCE_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| format!("{relative}: {error}"))?;
        if bytes.len() as u64 > MAX_SOURCE_BYTES {
            return Err(format!("source larger than 4 MiB: {relative}"));
        }
        let source = String::from_utf8(bytes).map_err(|error| format!("{relative}: {error}"))?;
        let entries = rustdoc_inventory::inspect_source(&source).map_err(|error| format!("{relative}: {error}"))?;
        sources.insert(relative.clone(), source);
        parsed.push((relative, entries));
    }
    let inherited = rustdoc_inventory::includes::resolve(&sources)?;
    for (relative, mut entries) in parsed {
        let owners = inherited.get(&relative).filter(|_| entries[0].docs.trim().is_empty());
        if let Some(owners) = owners {
            entries[0].docs = owners
                .iter()
                .map(|owner| format!("Inherited from {}:{}\n{}", owner.path, owner.line, owner.docs))
                .collect::<Vec<_>>()
                .join("\n\n");
            entries[0].missing.retain(|gap| gap != "docs");
        }
        total += entries.len();
        let missing = entries.iter().filter(|entry| !entry.missing.is_empty()).count();
        uncovered += missing;
        let items: Vec<_> = entries
            .into_iter()
            .map(|entry| {
                serde_json::json!({
                    "kind":entry.kind,"name":entry.name,"line":entry.line,
                    "docs":entry.docs,"missing":entry.missing,
                })
            })
            .collect();
        let owners: Vec<_> = owners
            .into_iter()
            .flatten()
            .map(|owner| serde_json::json!({"path":owner.path,"line":owner.line}))
            .collect();
        files.push(
            serde_json::json!({"path":relative,"uncovered":missing,"items":items,"included_file_docs_from":owners}),
        );
    }
    Ok(
        serde_json::json!({"schema":1,"file_count":files.len(),"item_count":total,"uncovered_items":uncovered,"files":files}),
    )
}

#[cfg(test)]
mod tests {
    //! Path-boundary tests prevent a source inventory from reading outside its root.

    use super::safe_relative;

    /// Reject cross-platform absolute/traversal spellings while allowing Rust paths.
    #[test]
    fn rejects_escaping_inventory_paths() {
        for path in [
            "",
            "../outside.rs",
            "/tmp/out.rs",
            "C:/outside.rs",
            "a/../b.rs",
            "a\\b.rs",
        ] {
            assert!(!safe_relative(path), "accepted {path}");
        }
        assert!(safe_relative("crates/core/src/lib.rs"));
        assert!(safe_relative("name with spaces.rs"));
    }
}
