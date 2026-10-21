//! Fixed, read-only prerequisite probes. Version evidence is not build/flash admission.
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::process;
/// Returns the canonical first absolute-PATH file named `name`, or None for absence/canonicalization failure.
fn find(name: &str) -> Option<PathBuf> { find_on_path(&std::env::var_os("PATH").unwrap_or_default(), name) }
/// Returns the canonical first file named `name` in absolute entries of PATH value `path`.
/// Relative entries/directories are skipped; canonicalization failure for the first matching file returns None.
fn find_on_path(
    path: &OsStr,
    name: &str,
) -> Option<PathBuf> {
    std::env::split_paths(path)
        .filter(|p| p.is_absolute())
        .map(|p| p.join(name))
        .find(|p| p.is_file())
        .and_then(|p| p.canonicalize().ok())
}
/// Returns absolute-PATH python.exe, falling back to python3.exe only when that search fails; None means neither
/// resolved.
pub(crate) fn python_executable() -> Option<PathBuf> { python_on_path(&std::env::var_os("PATH").unwrap_or_default()) }
/// Returns the canonical Python executable from PATH value `path`, searching all python.exe candidates before
/// python3.exe. None means no file could be resolved; this does not execute or validate Python.
fn python_on_path(path: &OsStr) -> Option<PathBuf> {
    find_on_path(path, "python.exe").or_else(|| find_on_path(path, "python3.exe"))
}
/// Runs fixed executable `path` with `args` in `repo` under owned `process` supervision.
/// Returns trimmed stdout for exit zero; returns supervisor/cancellation/timeout/output or nonzero-exit diagnostics
/// otherwise. The caller restricts arguments to read-only probes; no shell interpolation occurs here.
fn probe(
    process: &mut process::ProcessContext,
    path: &Path,
    args: &[&str],
    repo: &Path,
) -> Result<String, String> {
    let output = process.run(path, args, repo)?;
    if output.code != Some(0) {
        return Err(format!("probe exited {:?}: {}", output.code, output.stderr.trim()));
    }
    Ok(output.stdout.trim().to_owned())
}
/// Appends a prerequisite row to `rows`: stable `id`, `status`, observed `evidence`, corrective `setup` text and
/// `required` gate. Inputs are private report construction values; this does not validate or execute setup
/// instructions.
fn row(
    rows: &mut Vec<Value>,
    id: &str,
    status: &str,
    evidence: String,
    setup: &str,
    required: bool,
) {
    rows.push(
        json!({"id":id,"stage":"prerequisites","status":status,"evidence":evidence,"setup":setup,"required":required}),
    );
}
#[allow(clippy::too_many_arguments)]
/// Appends exact-version result to `rows` for `id`, probing optional `path` with fixed `args` in `repo` through
/// `process`. A trimmed stdout line must equal `expected`; absent path or probe/version failure becomes a required
/// failure row with `setup`.
fn version(
    process: &mut process::ProcessContext,
    rows: &mut Vec<Value>,
    id: &str,
    path: Option<PathBuf>,
    args: &[&str],
    expected: &str,
    repo: &Path,
    setup: &str,
) {
    match path {
        None => {
            row(
                rows,
                id,
                "missing",
                "Executable not found at the configured location".into(),
                setup,
                true,
            )
        }
        Some(p) => {
            match probe(process, &p, args, repo) {
                Ok(v) if v.lines().any(|line| line.trim() == expected) => {
                    row(rows, id, "passed", format!("{}: {v}", p.display()), "", true)
                }
                Ok(v) => {
                    row(
                        rows,
                        id,
                        "incompatible",
                        format!("{}: expected {expected}; observed {v}", p.display()),
                        setup,
                        true,
                    )
                }
                Err(e) => row(rows, id, "failed", e, setup, true),
            }
        }
    }
}
/// Returns the quoted channel line from `repo`/rust-toolchain.toml after dated-nightly shape checks.
/// Returns read/missing/shape errors; this is a line parser, not general TOML parsing or calendar-date validation.
fn channel(repo: &Path) -> Result<String, String> {
    let text = std::fs::read_to_string(repo.join("rust-toolchain.toml")).map_err(|e| e.to_string())?;
    let value = text
        .lines()
        .find_map(|l| l.trim().strip_prefix("channel = \"").and_then(|s| s.strip_suffix('"')))
        .ok_or("No exact channel in rust-toolchain.toml")?;
    if value.len() != 18
        || !value.starts_with("nightly-")
        || !value[8..].bytes().all(|b| b.is_ascii_digit() || b == b'-')
    {
        return Err("Expected a dated nightly channel".into());
    }
    Ok(value.into())
}
/// Returns read-only installed-tool evidence for `repo` using a fresh process context; failures appear in the JSON
/// report. Canonicalizes the repository, probes native tools only on the supported host, and performs no
/// downloads/builds/USB commands. Report readiness covers installed versions only, not host linking, image admission or
/// device access.
pub fn check(repo: &Path) -> Value { check_with_process(&mut process::ProcessContext::default(), repo) }
/// Returns prerequisite JSON for `repo`, supervising fixed version/help probes through `process`.
/// Invalid repository/channel yields an early not-ready report; later failures accumulate required rows.
/// Cancellation/cleanup faults become probe failures, not an interrupted report. Tool/setup suggestions are text only
/// and are never executed.
pub(crate) fn check_with_process(
    process: &mut process::ProcessContext,
    repo: &Path,
) -> Value {
    let mut rows = Vec::new();
    let repo = match repo.canonicalize() {
        Ok(p) if p.join("studio-helper/Cargo.toml").is_file() && p.join("configurator/Trunk.toml").is_file() => p,
        _ => {
            return json!({"schema_version":1,"ready":false,"checks":[{"id":"repository","status":"missing","required":true,"evidence":"Pass --repo with an existing Studio repository"}]});
        }
    };
    let selected = match channel(&repo) {
        Ok(v) => v,
        Err(e) => {
            return json!({"schema_version":1,"ready":false,"checks":[{"id":"toolchain","status":"incompatible","required":true,"evidence":e}]});
        }
    };
    let (native, host_evidence) = crate::host::host();
    row(
        &mut rows,
        "host",
        if native { "passed" } else { "unsupported" },
        host_evidence,
        "Use Windows 10/11 x64 for native checks; browser editing/export is independent",
        true,
    );
    if native {
        let rustup = find("rustup.exe");
        let rust_setup = format!(
            "rustup toolchain install {selected} --profile minimal --component llvm-tools-preview --component rustfmt \
             --component clippy --target thumbv8m.main-none-eabihf --target wasm32-unknown-unknown"
        );
        if let Some(rustup) = rustup {
            let mut sysroot = None;
            for tool in ["rustc", "cargo"] {
                match probe(process, &rustup, &["which", "--toolchain", &selected, tool], &repo).and_then(|s| {
                    let p = PathBuf::from(s);
                    if p.is_absolute() && p.is_file() {
                        Ok(p)
                    } else {
                        Err("rustup returned no installed absolute executable".into())
                    }
                }) {
                    Ok(path) => {
                        match probe(process, &path, &["--version", "--verbose"], &repo) {
                            Ok(v) if tool == "cargo" || v.lines().any(|s| s == "host: x86_64-pc-windows-msvc") => {
                                row(&mut rows, tool, "passed", format!("{}: {v}", path.display()), "", true)
                            }
                            Ok(v) => row(&mut rows, tool, "incompatible", v, &rust_setup, true),
                            Err(e) => row(&mut rows, tool, "failed", e, &rust_setup, true),
                        }
                        if tool == "rustc" {
                            sysroot = probe(process, &path, &["--print", "sysroot"], &repo)
                                .ok()
                                .map(PathBuf::from);
                        }
                    }
                    Err(e) => row(&mut rows, tool, "missing", e, &rust_setup, true),
                }
            }
            for (id, args, expected) in [
                (
                    "arm_target",
                    vec!["target", "list", "--installed", "--toolchain", &selected],
                    "thumbv8m.main-none-eabihf",
                ),
                (
                    "wasm_target",
                    vec!["target", "list", "--installed", "--toolchain", &selected],
                    "wasm32-unknown-unknown",
                ),
                (
                    "llvm_component",
                    vec!["component", "list", "--installed", "--toolchain", &selected],
                    "llvm-tools-x86_64-pc-windows-msvc",
                ),
            ] {
                match probe(process, &rustup, &args, &repo) {
                    Ok(v) if v.lines().any(|s| s == expected) => {
                        row(&mut rows, id, "passed", expected.into(), "", true)
                    }
                    Ok(v) => {
                        row(
                            &mut rows,
                            id,
                            "missing",
                            format!("Expected {expected}; installed: {v}"),
                            &rust_setup,
                            true,
                        )
                    }
                    Err(e) => row(&mut rows, id, "failed", e, &rust_setup, true),
                }
            }
            if let Some(root) = sysroot.filter(|p| p.is_absolute()) {
                let llvm = root.join("lib/rustlib/x86_64-pc-windows-msvc/bin/llvm-objdump.exe");
                match probe(process, &llvm, &["--version"], &repo) {
                    Ok(v) if v.contains("arm") && v.contains("thumb") => {
                        row(
                            &mut rows,
                            "llvm_objdump",
                            "passed",
                            format!("{}: {v}", llvm.display()),
                            "",
                            true,
                        )
                    }
                    Ok(v) => row(&mut rows, "llvm_objdump", "incompatible", v, &rust_setup, true),
                    Err(e) => row(&mut rows, "llvm_objdump", "missing", e, &rust_setup, true),
                }
            } else {
                row(
                    &mut rows,
                    "llvm_objdump",
                    "missing",
                    "Selected sysroot could not be resolved".into(),
                    &rust_setup,
                    true,
                );
            }
        } else {
            row(
                &mut rows,
                "rustup",
                "missing",
                "rustup.exe not found on PATH".into(),
                "Install Rust using https://rustup.rs then run the displayed pinned toolchain setup",
                true,
            );
        }
        let python = python_executable();
        match python {
            Some(p) => {
                match probe(
                    process,
                    &p,
                    &[
                        "-I",
                        "-B",
                        "-c",
                        "import sys,tomllib,json,hashlib,struct,subprocess; print(sys.version); sys.exit(0 if \
                         sys.version_info >= (3,11) else 1)",
                    ],
                    &repo,
                ) {
                    Ok(v) => row(&mut rows, "python", "passed", format!("{}: {v}", p.display()), "", true),
                    Err(e) => {
                        row(
                            &mut rows,
                            "python",
                            "failed",
                            e,
                            "Install Python >=3.11 from python.org and add it to PATH",
                            true,
                        )
                    }
                }
            }
            None => {
                row(
                    &mut rows,
                    "python",
                    "missing",
                    "Python executable not found".into(),
                    "Install Python >=3.11 from python.org and add it to PATH",
                    true,
                )
            }
        }
        let tools = repo.join("target/configurator-tools/bin");
        for (id, file, expected) in [
            ("trunk", "trunk.exe", "trunk 0.21.14"),
            ("wasm_bindgen", "wasm-bindgen.exe", "wasm-bindgen 0.2.129"),
        ] {
            let p = tools.join(file);
            version(
                process,
                &mut rows,
                id,
                p.is_file().then_some(p),
                &["--version"],
                expected,
                &repo,
                "Run configurator/Studio.ps1 -Setup to install pinned editor tools",
            );
        }
        let optimizer = tools.join("wasm-opt.exe");
        version(
            process,
            &mut rows,
            "wasm_opt",
            optimizer.is_file().then_some(optimizer),
            &["--version"],
            "wasm-opt version 133 (version_133)",
            &repo,
            "Run configurator/Studio.ps1 -Setup to install pinned Binaryen version_133",
        );
        let picotool = repo.join("target/configurator-tools/bin/picotool.exe");
        version(
            process,
            &mut rows,
            "picotool",
            picotool.is_file().then_some(picotool.clone()),
            &["version", "-s"],
            "2.3.1",
            &repo,
            "Run configurator/Studio.ps1 -Setup to install pinned official picotool 2.3.1",
        );
        if picotool.is_file() {
            match probe(process, &picotool, &["help", "load"], &repo) {
                Ok(v) if v.contains("--ignore-partitions") => {
                    row(
                        &mut rows,
                        "picotool_load_help",
                        "passed",
                        "load help advertises --ignore-partitions; no USB access attempted".into(),
                        "",
                        true,
                    )
                }
                Ok(v) => {
                    row(
                        &mut rows,
                        "picotool_load_help",
                        "incompatible",
                        v,
                        "Use the official USB-enabled picotool 2.3.1 build",
                        true,
                    )
                }
                Err(e) => {
                    row(
                        &mut rows,
                        "picotool_load_help",
                        "failed",
                        e,
                        "Use official picotool 2.3.1",
                        true,
                    )
                }
            }
        }
        let vswhere = std::env::var_os("ProgramFiles(x86)")
            .map(PathBuf::from)
            .map(|p| p.join("Microsoft Visual Studio/Installer/vswhere.exe"));
        match vswhere.filter(|p| p.is_file()) {
            Some(p) => {
                match probe(
                    process,
                    &p,
                    &[
                        "-latest",
                        "-products",
                        "*",
                        "-requires",
                        "Microsoft.VisualStudio.Component.VC.Tools.x86.x64",
                        "-property",
                        "installationPath",
                    ],
                    &repo,
                ) {
                    Ok(v) if !v.is_empty() => {
                        row(
                            &mut rows,
                            "msvc",
                            "passed",
                            format!("Visual Studio C++ x64/x86 tools registered at {v}; host link not exercised"),
                            "",
                            true,
                        )
                    }
                    _ => {
                        row(
                            &mut rows,
                            "msvc",
                            "missing",
                            "No registered Visual Studio C++ toolset".into(),
                            "Install Visual Studio Build Tools Desktop development with C++, including Windows SDK",
                            true,
                        )
                    }
                }
            }
            None => {
                row(
                    &mut rows,
                    "msvc",
                    "missing",
                    "Visual Studio installer discovery tool not found".into(),
                    "Install Visual Studio Build Tools Desktop development with C++, including Windows SDK",
                    true,
                )
            }
        }
        let sdk = std::env::var_os("ProgramFiles(x86)")
            .map(PathBuf::from)
            .map(|p| p.join("Windows Kits/10/Lib"));
        let sdk_present = sdk.and_then(|p| std::fs::read_dir(p).ok()).is_some_and(|entries| {
            entries
                .filter_map(Result::ok)
                .any(|e| e.path().join("um/x64/kernel32.lib").is_file() && e.path().join("ucrt/x64/ucrt.lib").is_file())
        });
        row(
            &mut rows,
            "windows_sdk",
            if sdk_present { "passed" } else { "missing" },
            "Checked Windows Kits/10/Lib for x64 kernel32.lib and ucrt.lib".into(),
            "Install the Windows SDK through Visual Studio Build Tools",
            true,
        );
    }
    for (id, message) in [
        ("host_link", "Actual MSVC host linking is checked during compilation"),
        (
            "llvm_arm_fixture",
            "Exact Cortex-M33/FPU decoder fixture remains the firmware/scripts/llvm_stack.py build-validation \
             adapter's responsibility",
        ),
        (
            "wasm_pipeline",
            "Trunk/wasm-bindgen/optimizer compatibility is checked by the explicit Studio build",
        ),
        (
            "device_access",
            "USB has not been enumerated or accessed; no flash readiness is asserted",
        ),
    ] {
        row(&mut rows, id, "not_checked", message.into(), "", false);
    }
    let ready = rows.iter().all(|r| r["required"] != true || r["status"] == "passed");
    json!({"schema_version":1,"operation":"prerequisite_check","ready":ready,"readiness_scope":"installed_tool_versions_only","native_build_flash_available":cfg!(windows),"native_build_flash_scope":"adapter_routes_only_not_image_admission","repository":repo,"selected_toolchain":selected,"checks":rows})
}
/// Parses command-line `args` excluding program name, prints help or prerequisite evidence, and returns an exit code.
/// Returns 0 for help/readiness, 2 for usage, 4 for unavailable prerequisites, or 70 for process-cleanup quarantine.
/// An explicit repository must be absolute; absent --repo uses cwd. Duplicate/unknown options fail before probing.
/// Standard-output/error write failures can panic through Rust's print macros; internal JSON shape is assumed after
/// check.
pub fn cli(args: impl IntoIterator<Item = OsString>) -> i32 {
    let args: Vec<_> = args.into_iter().collect();
    if args.len() == 1 && (args[0] == "--help" || args[0] == "-h") {
        println!(
            "studio-helper check [--json] [--repo <absolute repository>]\nRead-only tool checks. Exit 0: installed \
             prerequisites pass; 4: missing/incompatible; 2: usage. No builds, downloads or USB access."
        );
        return 0;
    }
    if args.first().is_none_or(|s| s != "check") {
        eprintln!("Usage: studio-helper check [--json] [--repo <absolute repository>]");
        return 2;
    }
    let mut repo = std::env::current_dir().unwrap_or_default();
    let mut json_output = false;
    let mut i = 1;
    let mut repo_seen = false;
    while i < args.len() {
        if args[i] == "--json" && !json_output {
            json_output = true;
        } else if args[i] == "--repo" && !repo_seen && i + 1 < args.len() {
            i += 1;
            repo = PathBuf::from(&args[i]);
            repo_seen = true;
            if !repo.is_absolute() {
                eprintln!("--repo must be absolute");
                return 2;
            }
        } else {
            eprintln!("Unknown or duplicate check option");
            return 2;
        }
        i += 1;
    }
    let report = check(&repo);
    if json_output {
        println!("{}", serde_json::to_string_pretty(&report).unwrap());
    } else {
        println!("Studio prerequisite check (read-only)");
        if let Some(pin) = report["selected_toolchain"].as_str() {
            println!("Selected toolchain: {pin}");
        }
        for row in report["checks"].as_array().unwrap() {
            println!(
                "[{}] {}: {}",
                row["status"].as_str().unwrap(),
                row["id"].as_str().unwrap(),
                row["evidence"].as_str().unwrap()
            );
            if let Some(setup) = row["setup"].as_str().filter(|s| !s.is_empty())
                && row["status"] != "passed"
            {
                println!("  Setup: {setup}");
            }
        }
        println!(
            "This check covers installed tools only. Builds validate artifacts; Flash separately requires device \
             inspection and board acknowledgement."
        );
    }
    if crate::process::cleanup_failed() {
        70
    } else if report["ready"] == true {
        0
    } else {
        4
    }
}
#[cfg(test)]
/// Executable selection, exact-version classification and CLI rejection regressions.
mod tests {
    use super::*;
    #[test]
    /// Relative PATH entries/directories are ignored and python.exe globally outranks the python3.exe fallback.
    fn python_selection_supports_fallback_and_preserves_absolute_path_priority() {
        let root = std::env::temp_dir().join(format!(
            "studio-python-selection-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let first = root.join("first");
        let second = root.join("second");
        std::fs::create_dir_all(&first).unwrap();
        std::fs::create_dir_all(&second).unwrap();
        let path = std::env::join_paths([Path::new("relative-entry"), &first, &second]).unwrap();
        assert!(python_on_path(&path).is_none());
        assert!(python_on_path(OsStr::new("")).is_none());
        std::fs::create_dir(first.join("python.exe")).unwrap();
        std::fs::write(first.join("python3.exe"), b"selection fixture only").unwrap();
        assert_eq!(
            python_on_path(&path),
            Some(first.join("python3.exe").canonicalize().unwrap())
        );
        std::fs::write(second.join("python.exe"), b"preferred executable").unwrap();
        assert_eq!(
            python_on_path(&path),
            Some(second.join("python.exe").canonicalize().unwrap())
        );
        std::fs::remove_dir(first.join("python.exe")).unwrap();
        std::fs::write(first.join("python.exe"), b"earlier executable").unwrap();
        assert_eq!(
            python_on_path(&path),
            Some(first.join("python.exe").canonicalize().unwrap())
        );
        assert!(root.starts_with(std::env::temp_dir()));
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    /// Missing executable yields a required missing row without starting a process.
    fn missing_version_is_reported() {
        let mut rows = Vec::new();
        version(
            &mut process::ProcessContext::default(),
            &mut rows,
            "missing",
            None,
            &[],
            "wanted",
            Path::new("C:/"),
            "install",
        );
        assert_eq!(rows[0]["status"], "missing");
    }
    #[cfg(windows)]
    #[test]
    /// Real supervised Python output passes only the exact pinned version line, rejecting a suffixed older version.
    fn exact_version_passes_and_wrong_version_fails() {
        let process = &mut process::ProcessContext::default();
        let python = python_executable().expect("Windows runner tests require Python");
        let repo = std::env::current_dir().unwrap();
        for (observed, expected_status) in [("2.3.1", "passed"), ("2.2.0-a4", "incompatible")] {
            let script = format!("print('{observed}')");
            let mut rows = Vec::new();
            version(
                process,
                &mut rows,
                "fixture",
                Some(python.clone()),
                &["-I", "-c", &script],
                "2.3.1",
                &repo,
                "install pinned version",
            );
            assert_eq!(rows[0]["status"], expected_status);
        }
    }
    #[test]
    /// Relative explicit repositories and duplicate JSON flags are usage errors before native probes.
    fn strict_cli() {
        assert_eq!(cli(["check", "--repo", "relative"].map(OsString::from)), 2);
        assert_eq!(cli(["check", "--json", "--json"].map(OsString::from)), 2);
    }
}
