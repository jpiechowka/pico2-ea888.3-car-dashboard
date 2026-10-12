use dashboard_config::*;

#[test]
fn startup_loads_one_whole_document_or_persists_the_default_fault() {
    let mut config = default_config();
    config.views[0].name = "Track".try_into().unwrap();
    let valid = validate(config).unwrap();
    let mut out = [0; MAX_FILE_BYTES];
    let bytes = encode(&valid, &mut out).unwrap();
    let loaded = load_at_boot(bytes);
    assert_eq!(loaded.config, valid);
    assert_eq!(loaded.fault, None);
    for cut in [0, 4, 11, bytes.len() - 1] {
        let fallback = load_at_boot(&bytes[..cut]);
        assert!(fallback.fault.is_some());
        assert_eq!(fallback.config.config(), &default_config());
        assert!(
            fallback
                .config
                .config()
                .widgets
                .iter()
                .all(|w| w.warning.applicability == Applicability::DemoOnly)
        );
    }
}
