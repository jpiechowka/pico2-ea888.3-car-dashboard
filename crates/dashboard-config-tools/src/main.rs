use std::path::Path;

use dashboard_config::{default_config, to_json, validate};
use dashboard_config_tools::{compile, import, sha256_hex};

fn run() -> Result<(), String> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let usage = "usage: dashboard-config default <output.json|output.eav> | compile <input.json|input.eav> \
                 <output.eav> | inspect <input.json|input.eav> | profiles | build-options <fixed options> | \
                 capture-options <fixed options>";
    match args.as_slice() {
        [command] if command == "profiles" => {
            let bytes = dashboard_config_tools::build_options::profiles_json()?;
            println!("{}", String::from_utf8(bytes).map_err(|e| e.to_string())?);
        }
        [command, rest @ ..] if command == "build-options" || command == "capture-options" => {
            let bytes = if command == "build-options" {
                let options = dashboard_config_tools::build_options::build_options(rest)?;
                dashboard_config_tools::build_options::normalized_json(&options)?
            } else {
                let options = dashboard_config_tools::build_options::capture_options(rest)?;
                dashboard_config_tools::build_options::bounded_json(
                    &options,
                    dashboard_config_tools::build_options::CAPTURE_LIMIT,
                )?
            };
            println!("{}", String::from_utf8(bytes).map_err(|e| e.to_string())?);
        }
        [command, output] if command == "default" => {
            let config = validate(default_config()).expect("reviewed default configuration");
            let bytes = if Path::new(output)
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("json"))
            {
                to_json(&config).map_err(|e| format!("{e:?}"))?.into_bytes()
            } else {
                compile(&config).map_err(|e| e.to_string())?
            };
            std::fs::write(output, &bytes).map_err(|e| e.to_string())?;
            println!("{} bytes; SHA-256 {}", bytes.len(), sha256_hex(&bytes));
        }
        [command, input, output] if command == "compile" => {
            let config = import(Path::new(input)).map_err(|e| e.to_string())?;
            let bytes = compile(&config).map_err(|e| e.to_string())?;
            std::fs::write(output, &bytes).map_err(|e| e.to_string())?;
            println!("{} bytes; SHA-256 {}", bytes.len(), sha256_hex(&bytes));
        }
        [command, input] if command == "inspect" => {
            let config = import(Path::new(input)).map_err(|e| e.to_string())?;
            let bytes = compile(&config).map_err(|e| e.to_string())?;
            println!(
                "{} views; {} widgets; boot view {}; {} binary bytes; SHA-256 {}",
                config.config().views.len(),
                config.config().widgets.len(),
                config.config().boot_view,
                bytes.len(),
                sha256_hex(&bytes)
            );
            println!("{}", to_json(&config).map_err(|e| format!("{e:?}"))?);
        }
        _ => return Err(usage.into()),
    }
    Ok(())
}

// The unoptimized CLI decode/migration path exceeds Windows' 1 MiB main stack.
// Only this host CLI worker gets a fixed 2 MiB reserve; firmware limits stay separate.
const CLI_STACK_BYTES: usize = 2 * 1024 * 1024;

fn main() {
    let result = std::thread::Builder::new()
        .name("dashboard-config-cli".into())
        .stack_size(CLI_STACK_BYTES)
        .spawn(run)
        .map_err(|error| format!("cannot start configuration worker: {error}"))
        .and_then(|worker| {
            worker
                .join()
                .unwrap_or_else(|_| Err("configuration worker panicked".into()))
        });
    if let Err(error) = result {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
