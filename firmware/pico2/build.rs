use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

fn main() {
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
