fn main() {
    let out = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    std::fs::write(out.join("memory.x"), include_bytes!("memory.x")).unwrap();
    println!("cargo:rustc-link-search={}", out.display());
    println!("cargo:rerun-if-changed=memory.x");
    println!("cargo:rerun-if-env-changed=BLE_PROBE_SOURCE_DIGEST");
    let digest = std::env::var("BLE_PROBE_SOURCE_DIGEST").ok();
    let mut bytes = [0u8; 32];
    if let Some(value) = &digest {
        assert!(
            value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit()),
            "invalid probe source digest"
        );
        for (out, text) in bytes.iter_mut().zip(value.as_bytes().as_chunks::<2>().0) {
            *out = u8::from_str_radix(std::str::from_utf8(text).unwrap(), 16).unwrap();
        }
    }
    std::fs::write(
        out.join("probe_identity.rs"),
        format!(
            "#[used]\n#[unsafe(no_mangle)]\n#[unsafe(link_section = \".probe_identity\")]\npub static \
             BLE_PROBE_SOURCE_DIGEST: [u8;32] = {bytes:?};\npub const SOURCE_IDENTITY_AVAILABLE: bool = {};\n",
            digest.is_some()
        ),
    )
    .unwrap();
}
