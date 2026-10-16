use std::fs::File;
use std::io::{Read, Write};
use std::path::PathBuf;

fn main() {
    use dashboard_config_tools::clocks::{CUSTOM_KEYS, generate, selection};
    for key in CUSTOM_KEYS {
        println!("cargo:rerun-if-env-changed={key}");
    }
    for profile in dashboard_build_profiles::catalogue() {
        println!(
            "cargo:rerun-if-env-changed=CARGO_FEATURE_{}",
            profile.feature.to_ascii_uppercase().replace('-', "_")
        );
    }
    let mut features: Vec<_> = std::env::vars_os()
        .filter_map(|(key, _)| {
            key.to_str()?
                .strip_prefix("CARGO_FEATURE_")
                .map(|s| s.to_ascii_lowercase().replace('_', "-"))
        })
        .collect();
    features.sort();
    let selected = selection(&features, &CUSTOM_KEYS.map(std::env::var_os))
        .unwrap_or_else(|error| panic!("Cannot select dashboard clock: {error}"));
    let (source, metadata) = generate(selected, &std::env::var("OPT_LEVEL").unwrap(), features.clone())
        .unwrap_or_else(|error| panic!("Cannot generate dashboard clock: {error}"));
    let generated_out = PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    std::fs::write(generated_out.join("clock_profile.rs"), &source).expect("write generated clock constants");
    std::fs::write(generated_out.join("clock_profile.json"), &metadata).expect("write generated clock metadata");
    println!(
        "cargo:rustc-env=DASHBOARD_CLOCK_SOURCE_SHA256={}",
        dashboard_config_tools::sha256_hex(&source)
    );
    println!(
        "cargo:rustc-env=DASHBOARD_CLOCK_METADATA_SHA256={}",
        dashboard_config_tools::sha256_hex(&metadata)
    );
    println!("cargo:rerun-if-env-changed=DASHBOARD_CONFIG");
    let manifest = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let config = dashboard_config_tools::prepare(std::env::var_os("DASHBOARD_CONFIG").as_deref(), &manifest)
        .unwrap_or_else(|error| panic!("Cannot embed dashboard configuration: {error}"));
    let configuration = dashboard_config::decode(config.bytes()).expect("prepared current configuration");
    embed_startup(configuration.config().orientation);
    for path in [config.source(), config.canonical_source()].into_iter().flatten() {
        println!("cargo:rerun-if-changed={}", path.display());
    }
    config
        .write_to(&PathBuf::from(std::env::var_os("OUT_DIR").unwrap()))
        .expect("write validated configuration snapshot");
    println!("cargo:rustc-env=DASHBOARD_CONFIG_SHA256={}", config.sha256_hex());
    if std::env::var_os("CARGO_FEATURE_TARGET_BENCH").is_some() {
        let rustc = std::process::Command::new(std::env::var_os("RUSTC").unwrap())
            .arg("--version")
            .output()
            .unwrap();
        assert!(rustc.status.success());
        println!(
            "cargo:rustc-env=BENCH_COMPILER={}",
            String::from_utf8(rustc.stdout).unwrap().trim()
        );
        for key in ["PROFILE", "OPT_LEVEL", "DEBUG", "TARGET"] {
            println!("cargo:rustc-env=BENCH_{key}={}", std::env::var(key).unwrap());
        }
        println!("cargo:rustc-env=BENCH_FEATURES={}", features.join("+"));
        // The external capture manifest supplies exact source/ELF hashes.
        // A revision alone cannot identify a dirty or differently built image.
        println!("cargo:rerun-if-changed=src");
    }
    let out = PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    println!("cargo:rustc-link-search={}", out.display());

    let memory_x = include_bytes!("memory.x");
    let mut f = File::create(out.join("memory.x")).unwrap();
    f.write_all(memory_x).unwrap();
    println!("cargo:rerun-if-changed=memory.x");

    let rp235x_riscv_x = include_bytes!("rp235x_riscv.x");
    let mut f = File::create(out.join("rp235x_riscv.x")).unwrap();
    f.write_all(rp235x_riscv_x).unwrap();
    println!("cargo:rerun-if-changed=rp235x_riscv.x");

    println!("cargo:rerun-if-changed=build.rs");
}

fn embed_startup(default_orientation: dashboard_config::Orientation) {
    use dashboard_config::startup::StartupProfile;
    let profile = startup_input("DASHBOARD_STARTUP_PROFILE", 16).map_or(StartupProfile::DEFAULT, |bytes| {
        StartupProfile::decode(&bytes).expect("invalid .easp startup profile")
    });
    let bytes = profile.encode().expect("validated startup profile");
    let out = PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    let asset = startup_input(
        "DASHBOARD_STARTUP_ASSET",
        dashboard_config::startup_asset::MAX_ASSET_BYTES as u64,
    );
    let geometry = asset.as_deref().map(|bytes| {
        dashboard_config::startup_asset::Asset::parse(bytes)
            .expect("invalid .easa startup asset")
            .geometry()
    });
    dashboard_config_tools::project::validate_startup_geometry(default_orientation, profile, geometry)
        .expect("startup theme and artwork must match the effective Splash geometry");
    let asset = asset.unwrap_or_default();
    std::fs::write(out.join("startup_asset.bin"), &asset).expect("write startup asset");
    println!(
        "cargo:rustc-env=DASHBOARD_STARTUP_ASSET_SHA256={}",
        dashboard_config_tools::sha256_hex(&asset)
    );
    std::fs::write(out.join("startup_profile.bin"), bytes).expect("write startup profile");
    println!(
        "cargo:rustc-env=DASHBOARD_STARTUP_SHA256={}",
        dashboard_config_tools::sha256_hex(&bytes)
    );
}

/// Retain one extra byte so the format validator rejects oversized input.
fn startup_input(
    variable: &str,
    maximum: u64,
) -> Option<Vec<u8>> {
    println!("cargo:rerun-if-env-changed={variable}");
    let path = std::env::var_os(variable)?;
    assert!(!path.is_empty(), "{variable} cannot be empty");
    let path = PathBuf::from(path)
        .canonicalize()
        .unwrap_or_else(|error| panic!("{variable} path: {error}"));
    println!("cargo:rerun-if-changed={}", path.display());
    let mut bytes = Vec::new();
    File::open(path)
        .unwrap_or_else(|error| panic!("open {variable}: {error}"))
        .take(maximum + 1)
        .read_to_end(&mut bytes)
        .unwrap_or_else(|error| panic!("read {variable}: {error}"));
    Some(bytes)
}
