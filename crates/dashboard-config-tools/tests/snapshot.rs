use std::ffi::OsStr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use dashboard_config::*;
use dashboard_config_tools::*;

struct Temp(PathBuf);

#[test]
fn old_binary_input_is_compiled_to_current_firmware_schema() {
    let temp = Temp::new();
    let file = temp.0.join("old.eav");
    std::fs::write(
        &file,
        include_bytes!("../../dashboard-config/tests/fixtures/v1-default.eav"),
    )
    .unwrap();
    let prepared = prepare(Some(file.as_os_str()), &temp.0).unwrap();
    assert_eq!(prepared.bytes()[4], VERSION as u8);
    assert_eq!(decode(prepared.bytes()).unwrap().config(), &default_config());
}
impl Temp {
    fn new() -> Self {
        static SEQUENCE: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "dashboard-config-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        assert!(self.0.starts_with(std::env::temp_dir()));
        assert!(
            self.0
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("dashboard-config-")
        );
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn absent_selection_is_default_but_explicit_empty_missing_and_invalid_fail() {
    let temp = Temp::new();
    let default = prepare(None, &temp.0).unwrap();
    assert!(default.source().is_none());
    assert_eq!(decode(default.bytes()).unwrap().config(), &default_config());
    assert!(prepare(Some(OsStr::new("")), &temp.0).is_err());
    assert!(prepare(Some(OsStr::new("missing.eav")), &temp.0).is_err());
    assert!(prepare(Some(temp.0.as_os_str()), &temp.0).is_err());
    for bytes in [vec![], b"broken".to_vec(), vec![0; MAX_FILE_BYTES + 1]] {
        std::fs::write(temp.0.join("invalid.eav"), bytes).unwrap();
        assert!(prepare(Some(OsStr::new("invalid.eav")), &temp.0).is_err());
    }
    assert!(prepare(Some(OsStr::new("bad\ncargo:rustc-env=X=1")), &temp.0).is_err());
}

#[test]
fn build_copies_the_validated_snapshot_even_if_source_changes_after_read() {
    let temp = Temp::new();
    let original = prepare(None, &temp.0).unwrap();
    let selected = temp.0.join("selected.eav");
    std::fs::write(&selected, original.bytes()).unwrap();
    let prepared = prepare(Some(OsStr::new("selected.eav")), &temp.0).unwrap();
    assert_eq!(prepared.source().unwrap(), selected);
    assert_eq!(prepared.sha256_hex(), original.sha256_hex());
    std::fs::write(&selected, b"invalid replacement").unwrap();
    let out = temp.0.join("out");
    std::fs::create_dir(&out).unwrap();
    prepared.write_to(&out).unwrap();
    assert_eq!(std::fs::read(out.join("dashboard.eav")).unwrap(), original.bytes());
    assert!(prepare(Some(selected.as_os_str()), &temp.0).is_err());
    let mut changed = default_config();
    changed.views[0].name = "Track".try_into().unwrap();
    let mut bytes = [0; MAX_FILE_BYTES];
    std::fs::write(&selected, encode(&validate(changed).unwrap(), &mut bytes).unwrap()).unwrap();
    let changed = prepare(Some(selected.as_os_str()), &temp.0).unwrap();
    assert_ne!(changed.sha256_hex(), prepared.sha256_hex());
    assert_eq!(decode(changed.bytes()).unwrap().config().views[0].name, "Track");
}

#[test]
fn sha256_snapshot_fingerprint_matches_independent_known_answer() {
    assert_eq!(
        sha256_hex(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_eq!(
        sha256_hex(b""),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
}

#[test]
fn cli_roundtrip_rejects_bad_input_without_replacing_output() {
    use std::process::Command;
    let temp = Temp::new();
    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_dashboard-config"))
            .current_dir(&temp.0)
            .args(args)
            .output()
            .unwrap()
    };
    assert!(run(&["default", "editable.json"]).status.success());
    assert!(run(&["compile", "editable.json", "runtime.eav"]).status.success());
    for (name, fixture) in [
        (
            "v1.json",
            include_bytes!("../../dashboard-config/tests/fixtures/v1-default.json").as_slice(),
        ),
        (
            "v2.json",
            include_bytes!("../../dashboard-config/tests/fixtures/v2-default.json").as_slice(),
        ),
        (
            "v3.json",
            include_bytes!("../../dashboard-config/tests/fixtures/v3-default.json").as_slice(),
        ),
        (
            "v3.eav",
            include_bytes!("../../dashboard-config/tests/fixtures/v3-default.eav").as_slice(),
        ),
        (
            "v1.eav",
            include_bytes!("../../dashboard-config/tests/fixtures/v1-default.eav").as_slice(),
        ),
        (
            "v2.eav",
            include_bytes!("../../dashboard-config/tests/fixtures/v2-default.eav").as_slice(),
        ),
    ] {
        std::fs::write(temp.0.join(name), fixture).unwrap();
        let output = run(&["compile", name, "migrated.eav"]);
        assert!(
            output.status.success(),
            "{name}: {:?} {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(run(&["inspect", "migrated.eav"]).status.success());
        assert_eq!(
            decode(&std::fs::read(temp.0.join("migrated.eav")).unwrap())
                .unwrap()
                .config(),
            &default_config()
        );
    }
    let bytes = std::fs::read(temp.0.join("runtime.eav")).unwrap();
    assert_eq!(decode(&bytes).unwrap().config(), &default_config());
    assert!(run(&["inspect", "runtime.eav"]).status.success());
    std::fs::write(temp.0.join("editable.json"), b"{\"unknown\":1}").unwrap();
    assert!(!run(&["compile", "editable.json", "runtime.eav"]).status.success());
    assert_eq!(std::fs::read(temp.0.join("runtime.eav")).unwrap(), bytes);
    assert!(!run(&["compile", "missing.json", "runtime.eav"]).status.success());
    assert!(!run(&["unexpected"]).status.success());
}

#[test]
fn v3_input_and_complete_v4_globals_use_ordinary_prepare_and_compile() {
    let temp = Temp::new();
    let file = temp.0.join("v3.eav");
    std::fs::write(
        &file,
        include_bytes!("../../dashboard-config/tests/fixtures/v3-default.eav"),
    )
    .unwrap();
    let legacy = prepare(Some(file.as_os_str()), &temp.0).unwrap();
    assert_eq!(decode(legacy.bytes()).unwrap().config(), &default_config());
    let mut c = default_config();
    c.reduced_motion = true;
    c.history_seconds = [900; 8];
    c.led.normal.kind = LedPatternKind::Breathe;
    c.widgets[0].advanced.smoothing_ms = 5000;
    let expected = validate(c).unwrap();
    let bytes = compile(&expected).unwrap();
    let file = temp.0.join("v4.eav");
    std::fs::write(&file, &bytes).unwrap();
    let prepared = prepare(Some(file.as_os_str()), &temp.0).unwrap();
    assert_eq!(prepared.bytes(), bytes);
    assert_eq!(decode(prepared.bytes()).unwrap(), expected);
}

#[test]
fn byte_encoding_is_lowercase_zero_padded_and_roundtrips_every_byte() {
    let bytes = (0..=255u8).collect::<Vec<_>>();
    let text = hex_bytes(&bytes);
    assert_eq!(text.len(), 512);
    assert!(text.starts_with("000102030405060708090a0b0c0d0e0f"));
    let decoded = text
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(decoded, bytes);
    assert_eq!(hex_bytes(&[]), "");
}
