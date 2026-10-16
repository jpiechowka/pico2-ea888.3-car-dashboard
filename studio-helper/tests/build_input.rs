use dashboard_helper_protocol::*;
use studio_helper::build_input::*;
fn request() -> BuildRequest {
    let config = dashboard_config::validate(dashboard_config::default_config()).unwrap();
    let mut buf = vec![0; dashboard_config::MAX_FILE_BYTES];
    let bytes = dashboard_config::encode(&config, &mut buf).unwrap();
    BuildRequest {
        editor_revision: 1,
        config_sha256: Sha256::new(digest(bytes)).unwrap(),
        eav_hex: Text::new(bytes.iter().map(|b| format!("{b:02x}")).collect::<String>()).unwrap(),
        options: BuildOptions::default(),
        retry_of: None,
    }
}
#[test]
fn digest_and_codec_are_both_required() {
    let good = request();
    assert!(AcceptedInput::from_request(&good).is_ok());
    let mut bad = good.clone();
    bad.config_sha256 = Sha256::new("0".repeat(64)).unwrap();
    assert!(AcceptedInput::from_request(&bad).is_err());
    let raw = b"not a configuration";
    bad.config_sha256 = Sha256::new(digest(raw)).unwrap();
    bad.eav_hex = Text::new(raw.iter().map(|b| format!("{b:02x}")).collect::<String>()).unwrap();
    assert!(AcceptedInput::from_request(&bad).is_err());
}

fn project_bytes() -> Vec<u8> {
    let config = dashboard_config::validate(dashboard_config::default_config()).unwrap();
    dashboard_config_tools::project::Project::new(&config, Default::default(), None)
        .unwrap()
        .encode(dashboard_config_tools::project::ExportMode::ConvertedOnly)
        .unwrap()
}
#[test]
fn project_tokens_are_unique_session_bound_and_digest_bound() {
    let session = SessionId::new("1".repeat(32)).unwrap();
    let a = UploadedProject::admit(session.clone(), project_bytes()).unwrap();
    let b = UploadedProject::admit(session.clone(), project_bytes()).unwrap();
    assert_ne!(a.response.project_id, b.response.project_id);
    assert_eq!(a.response.project_sha256, b.response.project_sha256);
    let mut request = ProjectBuildRequest {
        editor_revision: 42,
        project_id: a.response.project_id.clone(),
        project_sha256: a.response.project_sha256.clone(),
        options: BuildOptions::default(),
        retry_of: None,
    };
    assert!(a.accepted(&session, &request).is_ok());
    assert!(a.accepted(&SessionId::new("2".repeat(32)).unwrap(), &request).is_err());
    request.project_id = b.response.project_id;
    assert!(a.accepted(&session, &request).is_err());
    request.project_id = a.response.project_id.clone();
    request.project_sha256 = Sha256::new("0".repeat(64)).unwrap();
    assert!(a.accepted(&session, &request).is_err());
    assert!(UploadedProject::admit(session, vec![0; PROJECT_UPLOAD_BYTES + 1]).is_err());
}
#[test]
fn snapshot_is_independent_and_detects_input_mutation() {
    use std::fs;
    let repo = std::env::temp_dir().join(format!(
        "studio-frozen-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&repo).unwrap();
    for name in [
        "Cargo.toml",
        "Cargo.lock",
        "rust-toolchain.toml",
        "firmware/Cargo.toml",
        "firmware/Cargo.lock",
        "firmware/rust-toolchain.toml",
        "firmware/.cargo/config.toml",
        "crates/example/lib.rs",
        "firmware/pico2/src/main.rs",
        "firmware/vendor/embassy-rp/src/lib.rs",
        "firmware/scripts/capture.py",
        "host/scripts/environment.py",
    ] {
        let path = repo.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, b"source").unwrap();
    }
    let frozen = freeze(&repo, &AcceptedInput::from_request(&request()).unwrap()).unwrap();
    fs::write(repo.join("crates/example/lib.rs"), b"changed live source").unwrap();
    assert!(frozen.verify().is_ok());
    fs::write(
        frozen.attempt_root().join("input/dashboard.eav"),
        b"changed frozen input",
    )
    .unwrap();
    assert!(frozen.verify().is_err());
    // Fixture path was created above under the OS temporary directory.
    assert!(repo.starts_with(std::env::temp_dir()));
    fs::remove_dir_all(repo).unwrap();
}
