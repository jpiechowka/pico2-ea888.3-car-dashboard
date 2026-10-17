use dashboard_build_profiles::{ClockSelection, ProfileId};
use dashboard_helper_protocol::*;

#[test]
fn normalized_defaults_and_false_features_are_exact() {
    let mut options = BuildOptions::default();
    let n = normalize(&options).unwrap();
    assert_eq!(
        n.feature_names(),
        ["cpu150-spi62-1v10", "demo", "frame-trace", "simple-outline", "te-trace"]
    );
    assert_eq!(n.clock.actual_spi_hz, 37_500_000);
    options.mode = Mode::Vehicle;
    options.frame_trace = false;
    options.te_trace = false;
    options.simple_outline = false;
    assert_eq!(
        normalize(&options).unwrap().feature_names(),
        ["cpu150-spi62-1v10", "longan-rx12"]
    );
}

#[test]
fn expert_and_legacy_admission_preserve_requested_identity() {
    let mut options = BuildOptions {
        clock: ClockSelection::Named(ProfileId::Legacy290Spi72),
        ..Default::default()
    };
    assert!(normalize(&options).is_err());
    options.expert = true;
    let n = normalize(&options).unwrap();
    assert_eq!(
        (n.clock.requested_cpu_hz, n.clock.actual_cpu_hz, n.clock.actual_spi_hz),
        (290_000_000, 290_400_000, 48_400_000)
    );
    options.clock = ClockSelection::Named(ProfileId::Timing150Spi15);
    options.expert = false;
    assert!(normalize(&options).is_ok());
}

#[test]
fn integer_and_timestamp_wire_never_rounds() {
    for n in [
        0,
        8_640_000_000_000_000,
        8_640_000_000_000_001,
        (1u64 << 53) - 1,
        1u64 << 53,
        (1u64 << 53) + 1,
        u64::MAX,
    ] {
        let r = BuildRequest {
            editor_revision: n,
            config_sha256: Sha256::new("a".repeat(64)).unwrap(),
            eav_hex: Text::new("00").unwrap(),
            options: Default::default(),
            retry_of: None,
        };
        let b = encode(&r).unwrap();
        assert_eq!(decode::<BuildRequest>(&b).unwrap().editor_revision, n);
        assert_eq!(browser_utc_ms(n).is_some(), n <= 8_640_000_000_000_000);
    }
    for bad in ["-1", "1.0", "1e0", "18446744073709551616", "\"12\""] {
        let s = format!(
            "{{\"editor_revision\":{bad},\"config_sha256\":\"{}\",\"eav_hex\":\"00\",\"options\":{},\"retry_of\":\
             null}}",
            "a".repeat(64),
            serde_json::to_string(&BuildOptions::default()).unwrap()
        );
        assert!(decode::<BuildRequest>(s.as_bytes()).is_err());
    }
}

#[test]
fn bounded_requests_and_exact_ids_reject_bad_input() {
    assert!(ArtifactId::new("a".repeat(32)).is_err());
    assert!(AttemptId::new("a".repeat(64)).is_err());
    assert!(Sha256::new("A".repeat(64)).is_err());
    let s = format!(
        "{{\"attempts\":[{}]}}",
        (0..17).map(|i| format!("\"{i:032x}\"")).collect::<Vec<_>>().join(",")
    );
    assert!(decode::<CleanupRequest>(s.as_bytes()).is_err());
    let dup = format!("{{\"attempts\":[\"{}\",\"{}\"]}}", "a".repeat(32), "a".repeat(32));
    assert!(decode::<CleanupRequest>(dup.as_bytes()).is_err());
    assert!(decode::<CleanupRequest>(br#"{"attempts":[],"path":"x"}"#).is_err());
    assert!(decode::<CleanupRequest>(br#"{"attempts":[],"attempts":[]}"#).is_err());
    assert!(decode::<BuildRequest>(&vec![b' '; 65537]).is_err());
}

#[test]
fn serialized_event_limit_includes_envelope_and_escapes() {
    let event = Event {
        sequence: u64::MAX - 1,
        utc_ms: Some(u64::MAX),
        elapsed_ms: u64::MAX,
        stage: StageId::Setup(SetupStage::Download),
        kind: EventKind::Diagnostic,
        level: Level::Warning,
        source: Source::Internal,
        text: Text::new("\\\"".repeat(400)).unwrap(),
        progress: Some(Progress {
            completed: u64::MAX,
            total: u64::MAX,
            unit: ProgressUnit::Bytes,
        }),
        tool: Some(ToolId::RustToolchain),
        setup_step: None,
    };
    assert!(encode(&event).is_err());
    let mut event = event;
    event.text = Text::new("界😀").unwrap();
    assert_eq!(decode::<Event>(&encode(&event).unwrap()).unwrap(), event);
    event.progress.as_mut().unwrap().total = 0;
    assert!(encode(&event).is_err());
}

#[test]
fn layouts_and_board_confirmation_are_closed() {
    assert_eq!(OperationKind::Build.layout().len(), 4);
    assert_eq!(OperationKind::Flash.layout().len(), 6);
    assert_eq!(OperationKind::Setup.layout().len(), 5);
    assert_eq!(OperationKind::PrerequisiteCheck.layout().len(), 1);
    assert_eq!(OperationKind::DeviceScan.layout().len(), 1);
    assert_eq!(OperationKind::Cleanup.layout().len(), 1);
    let id = DeviceId::new("1".repeat(32)).unwrap();
    let mut r = FlashRequest {
        artifact_id: ArtifactId::new("a".repeat(64)).unwrap(),
        device_id: id.clone(),
        editor_revision: u64::MAX,
        project_sha256: Sha256::new("d".repeat(64)).unwrap(),
        config_sha256: Sha256::new("b".repeat(64)).unwrap(),
        expected_options: Default::default(),
        retry_of: None,
        board_acknowledgement: BoardAcknowledgement {
            device_id: id,
            session_id: SessionId::new("2".repeat(32)).unwrap(),
            information_sha256: Sha256::new("c".repeat(64)).unwrap(),
            board: BoardContract::GenuinePico2W,
        },
    };
    assert!(encode(&r).is_ok());
    let mut wire = serde_json::to_value(&r).unwrap();
    for unsupported in ["genuine_pico2", "genuine_pico", "genuine_pico2w", "", "unknown"] {
        wire["board_acknowledgement"]["board"] = unsupported.into();
        assert!(decode::<FlashRequest>(&serde_json::to_vec(&wire).unwrap()).is_err());
    }
    wire["board_acknowledgement"]["board"] = "genuine_pico2_w".into();
    assert!(decode::<FlashRequest>(&serde_json::to_vec(&wire).unwrap()).is_ok());
    r.board_acknowledgement.device_id = DeviceId::new("3".repeat(32)).unwrap();
    assert!(encode(&r).is_err());
}

#[test]
fn actual_maximum_event_envelopes_include_setup_step_and_escape_budget() {
    use dashboard_helper_protocol::setup::*;
    let mut shapes = Vec::new();
    for op in [
        OperationKind::Build,
        OperationKind::Flash,
        OperationKind::Setup,
        OperationKind::PrerequisiteCheck,
        OperationKind::DeviceScan,
        OperationKind::Cleanup,
    ] {
        for stage in op.layout() {
            shapes.push((stage, None, None));
            if matches!(stage, StageId::Setup(_)) {
                for tool in TOOLS {
                    shapes.push((stage, Some(tool), None));
                }
            }
        }
    }
    for step in setup_steps() {
        shapes.push((StageId::Setup(step.stage), Some(step.tool), Some(step)));
    }
    let mut max = 0;
    let mut largest = None;
    for (stage, tool, setup_step) in shapes {
        for kind in [
            EventKind::Log,
            EventKind::Progress,
            EventKind::Stage,
            EventKind::Diagnostic,
        ] {
            for level in [Level::Info, Level::Warning, Level::Error] {
                for source in [Source::Stdout, Source::Stderr, Source::Internal] {
                    for unit in [ProgressUnit::Bytes, ProgressUnit::Items, ProgressUnit::Percent] {
                        let event = Event {
                            sequence: u64::MAX - 1,
                            utc_ms: Some(u64::MAX),
                            elapsed_ms: u64::MAX,
                            stage,
                            kind,
                            level,
                            source,
                            text: Text::new("").unwrap(),
                            progress: Some(Progress {
                                completed: if unit == ProgressUnit::Percent { 100 } else { u64::MAX },
                                total: if unit == ProgressUnit::Percent { 100 } else { u64::MAX },
                                unit,
                            }),
                            tool,
                            setup_step,
                        };
                        let b = encode(&event).unwrap();
                        if b.len() > max {
                            max = b.len();
                            largest = Some(event);
                        }
                    }
                }
            }
        }
    }
    let mut event = largest.unwrap();
    let remaining = 1024 - max;
    event.text = Text::new("x".repeat(remaining)).unwrap();
    assert_eq!(encode(&event).unwrap().len(), 1024);
    event.text = Text::new("x".repeat(remaining + 1)).unwrap();
    assert!(encode(&event).is_err());
    event.text = Text::new("\0".repeat(remaining / 6)).unwrap();
    assert!(encode(&event).is_ok());
    event.text = Text::new("\0".repeat(remaining / 6 + 1)).unwrap();
    assert!(encode(&event).is_err());
    println!("Largest enumerated empty event envelope={max}; plain text budget={remaining}");
}
#[test]
fn session_cap_and_normalized_tampering_are_enforced() {
    let response = SessionResponse {
        protocol_version: PROTOCOL_VERSION,
        session_id: SessionId::new("f".repeat(32)).unwrap(),
        ready: false,
        reason: Some(Text::new("\0".repeat(512)).unwrap()),
    };
    let bytes = encode(&response).unwrap();
    assert!(bytes.len() < 65536);
    assert!(decode::<SessionResponse>(&bytes).is_ok());
    let mut n = normalize(&BuildOptions::default()).unwrap();
    n.clock.spi_neighbors.below_or_equal = None;
    assert!(encode(&n).is_err());
}

#[test]
fn non_wireless_helper_sessions_cannot_supply_current_board_consent() {
    for (version, accepted) in [(0, false), (1, false), (2, true), (3, false), (u16::MAX, false)] {
        let wire = format!(
            r#"{{"protocol_version":{version},"session_id":"{}","ready":false,"reason":null}}"#,
            "a".repeat(32)
        );
        assert_eq!(decode::<SessionResponse>(wire.as_bytes()).is_ok(), accepted);
    }
}
