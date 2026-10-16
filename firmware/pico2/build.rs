use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

fn main() {
    embed_startup();
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
    let (source, metadata) = generate(selected, &std::env::var("OPT_LEVEL").unwrap(), features)
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
        let mut features: Vec<_> = std::env::vars()
            .filter_map(|(k, _)| {
                k.strip_prefix("CARGO_FEATURE_")
                    .map(|s| s.to_ascii_lowercase().replace('_', "-"))
            })
            .collect();
        features.sort();
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

fn embed_startup() {
    use std::io::Read;

    use dashboard_config::startup::StartupProfile;
    println!("cargo:rerun-if-env-changed=DASHBOARD_STARTUP_PROFILE");
    println!("cargo:rerun-if-env-changed=DASHBOARD_STARTUP_ASSET");
    let profile = match std::env::var_os("DASHBOARD_STARTUP_PROFILE") {
        None => StartupProfile::DEFAULT,
        Some(path) => {
            assert!(!path.is_empty(), "DASHBOARD_STARTUP_PROFILE cannot be empty");
            let path = PathBuf::from(path).canonicalize().expect("startup profile path");
            println!("cargo:rerun-if-changed={}", path.display());
            let mut bytes = Vec::new();
            File::open(path)
                .expect("open startup profile")
                .take(17)
                .read_to_end(&mut bytes)
                .expect("read startup profile");
            StartupProfile::decode(&bytes).expect("invalid .easp startup profile")
        }
    };
    let bytes = profile.encode().expect("validated startup profile");
    let out = PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    let asset = match std::env::var_os("DASHBOARD_STARTUP_ASSET") {
        None => Vec::new(),
        Some(path) => {
            assert!(!path.is_empty(), "DASHBOARD_STARTUP_ASSET cannot be empty");
            let path = PathBuf::from(path).canonicalize().expect("startup asset path");
            println!("cargo:rerun-if-changed={}", path.display());
            let mut bytes = Vec::new();
            File::open(path)
                .expect("open startup asset")
                .take(dashboard_config::startup_asset::MAX_ASSET_BYTES as u64 + 1)
                .read_to_end(&mut bytes)
                .expect("read startup asset");
            dashboard_config::startup_asset::Asset::parse(&bytes).expect("invalid .easa startup asset");
            bytes
        }
    };
    assert_eq!(
        profile.theme == dashboard_config::startup::StartupTheme::Custom,
        !asset.is_empty(),
        "Custom requires an asset; procedural themes must not embed unused artwork"
    );
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
