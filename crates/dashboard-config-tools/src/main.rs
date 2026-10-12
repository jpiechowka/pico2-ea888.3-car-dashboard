use std::path::Path;

use dashboard_config::{default_config, to_json, validate};
use dashboard_config_tools::{compile, import, sha256_hex};

fn run() -> Result<(), String> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let usage = "usage: dashboard-config default <output.json|output.eav> | compile <input.json|input.eav> \
                 <output.eav> | inspect <input.json|input.eav>";
    match args.as_slice() {
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

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
