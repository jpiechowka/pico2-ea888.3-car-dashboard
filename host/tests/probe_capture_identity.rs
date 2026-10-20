//! Exercise identity through validated capture bytes and the real offline CLI.
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

use dashboard_core::discovery::capture::{BuildIdentity, BuildProfile, encode};
use dashboard_core::discovery::*;
use dashboard_host_validation::probe_captures::{decode_json, hex, publish, text_report};
use serde_json::Value;

fn ad(
    kind: u8,
    name: &[u8],
) -> Vec<u8> {
    let mut bytes = vec![u8::try_from(name.len() + 1).unwrap(), kind];
    bytes.extend_from_slice(name);
    bytes
}
fn probe(
    adv: &[u8],
    response: &[u8],
) -> Probe {
    let mut p = Probe::new();
    let peer = Peer {
        address: [0xC0, 0x12, 0x34, 0x56, 0x78, 0x9A],
        random: true,
    };
    p.catalog
        .observe(Advertisement::new(peer, ReportKind::Connectable, -44, adv, 1));
    if !response.is_empty() {
        p.catalog
            .observe(Advertisement::new(peer, ReportKind::ScanResponse, -45, response, 2));
    }
    p.input(Input::Click, 3);
    p
}
fn bytes(p: &Probe) -> Vec<u8> {
    let mut bytes = vec![0; 8192];
    let n = encode(
        p,
        BuildIdentity {
            source_digest: [1; 32],
            source_identity_available: true,
            cpu_hz: 150_000_000,
            spi_hz: 75_000_000,
            profile: BuildProfile::Stock,
        },
        100,
        &mut bytes,
    )
    .unwrap();
    bytes.truncate(n);
    bytes
}
fn gap_name(
    p: &mut Probe,
    name: &[u8],
    result: ReadResult,
) {
    p.inspection.accept(InspectionItem::Service(Service {
        start: 1,
        end: 4,
        uuid: Uuid::from_wire(&[0, 0x18]).unwrap(),
    }));
    p.inspection.accept(InspectionItem::Characteristic(Characteristic {
        service_start: 1,
        declaration: 2,
        handle: 3,
        properties: 2,
        uuid: Uuid::from_wire(&[0, 0x2A]).unwrap(),
    }));
    p.inspection.accept(InspectionItem::Value(InfoValue {
        service_start: 1,
        handle: 3,
        uuid: 0x2A00,
        value: Payload::new(name),
        result,
        failure: None,
        elapsed_ms: 1,
    }));
}
fn identity(p: &Probe) -> Value { decode_json(&bytes(p)).unwrap()["identity"].clone() }

#[test]
fn name_address_and_full_content_id_are_derived_from_validated_bytes() {
    let bytes = bytes(&probe(&ad(9, b"NEEWER-TL21C"), &[]));
    let v = decode_json(&bytes).unwrap();
    let digest = hex(&sha2_const_stable::Sha256::new().update(&bytes).finalize());
    let i = &v["identity"];
    assert_eq!(i["version"], 1);
    assert_eq!(i["capture_id"], format!("sha256:{digest}"));
    assert_eq!(i["short_id"], &digest[..16]);
    assert_eq!(v["binary"]["sha256"], digest);
    assert_eq!(i["device_key"], "r-c0123456789a");
    assert_eq!(
        i["file_stem"],
        format!("blecap-neewer-tl21c-r-c0123456789a-{}", &digest[..16])
    );
    assert_eq!(i["device_name"]["raw_hex"], hex(b"NEEWER-TL21C"));
    assert_eq!(i["device_name"]["utf8"], "NEEWER-TL21C");
    assert_eq!(i["device_name"]["source"], "advertisement");
    assert_eq!(i["device_name"]["complete"], true);
    assert_eq!(i["device_name"]["truncated"], false);
    assert!(text_report(&v).contains(&format!("identity.capture_id: \"sha256:{digest}\"")));
}

#[test]
fn complete_ad_names_win_then_gap_then_shortened_names_with_deterministic_provenance() {
    for (adv, response, expected, source, complete) in [
        (ad(9, b"Advert"), ad(9, b"Response"), "Response", "scan_response", true),
        (ad(9, b"Advert"), ad(8, b"Short"), "Advert", "advertisement", true),
        (ad(8, b"Advert"), ad(8, b"Response"), "GATT", "gap_device_name", true),
    ] {
        let mut p = probe(&adv, &response);
        gap_name(&mut p, b"GATT", ReadResult::Ok);
        let i = identity(&p);
        assert_eq!(i["device_name"]["utf8"], expected);
        assert_eq!(i["device_name"]["source"], source);
        assert_eq!(i["device_name"]["complete"], complete);
    }
    let mut p = probe(&ad(8, b"Advert"), &ad(8, b"Response"));
    gap_name(&mut p, &[], ReadResult::Protected);
    let i = identity(&p);
    assert_eq!(i["device_name"]["utf8"], "Response");
    assert_eq!(i["device_name"]["source"], "scan_response");
    assert_eq!(i["device_name"]["complete"], false);
    assert!(i["device_name"]["record_index"].is_null());
    let mut p = probe(&[], &[]);
    gap_name(&mut p, b"GATT", ReadResult::Ok);
    assert_eq!(identity(&p)["device_name"]["record_index"], 2);
}

#[test]
fn malformed_empty_duplicate_and_truncated_packets_preserve_evidence_without_guessing() {
    let cases = [
        (vec![8, 9, b'A'], None),
        (vec![0, 3, 9, b'N', b'O'], None),
        ([ad(9, b""), ad(9, b"Yes")].concat(), Some("Yes")),
        ([ad(9, b"One"), ad(9, b"Two")].concat(), Some("One")),
        ([ad(9, b"Good"), vec![30, 9, b'X']].concat(), Some("Good")),
    ];
    for (packet, expected) in cases {
        let p = probe(&packet, &[]);
        let v = decode_json(&bytes(&p)).unwrap();
        assert_eq!(v["identity"]["device_name"]["utf8"].as_str(), expected);
        assert_eq!(v["advertisement"]["raw_hex"], hex(&packet));
    }
    let packet = [ad(9, b"Saved"), vec![0; 40]].concat();
    let i = identity(&probe(&packet, &[]));
    assert_eq!(i["device_name"]["utf8"], "Saved");
    assert_eq!(i["device_name"]["complete"], true);
    assert_eq!(i["device_name"]["truncated"], true);
}

#[test]
fn unsafe_non_ascii_and_invalid_utf8_names_never_become_path_syntax() {
    for (name, slug) in [
        (&b"../../CON:<A>\\|?*\r\n. "[..], "con-a"),
        (&b"NUL"[..], "nul"),
        (&b"COM1"[..], "com1"),
        (&b"A\0B\x1B[31m"[..], "a-b-31m"),
        ("東京".as_bytes(), "unnamed"),
        ("Tést 💡 Light".as_bytes(), "t-st-light"),
        (&b"....___--- "[..], "unnamed"),
        (&b"A\xFFB"[..], "unnamed"),
    ] {
        let i = identity(&probe(&ad(9, name), &[]));
        let stem = i["file_stem"].as_str().unwrap();
        assert!(stem.starts_with(&format!("blecap-{slug}-r-c0123456789a-")), "{stem}");
        assert!(
            stem.bytes()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
        );
        assert_eq!(Path::new(stem).components().count(), 1);
        assert_eq!(i["device_name"]["raw_hex"], hex(name));
        assert_eq!(i["device_name"]["utf8"].as_str(), std::str::from_utf8(name).ok());
    }
}

#[test]
fn missing_names_and_unusable_gap_reads_fall_back_and_long_names_are_bounded() {
    assert!(identity(&probe(&[], &[]))["device_name"].is_null());
    for (name, result) in [
        (vec![], ReadResult::Ok),
        (vec![], ReadResult::Protected),
        (vec![b'X'; 200], ReadResult::Ok),
    ] {
        let mut p = probe(&[], &[]);
        gap_name(&mut p, &name, result);
        assert!(
            identity(&p)["file_stem"]
                .as_str()
                .unwrap()
                .starts_with("blecap-unnamed-")
        );
    }
    let mut p = probe(&[], &[]);
    gap_name(&mut p, &[b'A'; 40], ReadResult::Ok);
    let i = identity(&p);
    assert!(
        i["file_stem"]
            .as_str()
            .unwrap()
            .starts_with(&format!("blecap-{}-r-", "a".repeat(32)))
    );
    assert_eq!(i["device_name"]["utf8"].as_str().unwrap().len(), 40);
}

#[test]
fn every_name_byte_and_slug_boundary_remain_portable_and_lossless() {
    for byte in 0..=255u8 {
        let name = [b'A', byte, b'Z'];
        let i = identity(&probe(&ad(9, &name), &[]));
        let stem = i["file_stem"].as_str().unwrap();
        assert!(
            stem.bytes()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
        );
        assert_eq!(i["device_name"]["raw_hex"], hex(&name));
        assert_eq!(i["device_name"]["utf8"].as_str(), std::str::from_utf8(&name).ok());
    }
    let mut p = probe(&[], &[]);
    let name = [vec![b'A'; 31], b" /trailing".to_vec()].concat();
    gap_name(&mut p, &name, ReadResult::Ok);
    let stem = identity(&p)["file_stem"].as_str().unwrap().to_owned();
    assert!(stem.starts_with(&format!("blecap-{}-r-", "a".repeat(31))));
    assert!(!stem.contains("--"));
}

#[test]
fn equal_sanitized_names_do_not_alias_distinct_captures() {
    let a = identity(&probe(&ad(9, b"A/B"), &[]));
    let b = identity(&probe(&ad(9, b"A:B"), &[]));
    assert!(a["file_stem"].as_str().unwrap().starts_with("blecap-a-b-"));
    assert!(b["file_stem"].as_str().unwrap().starts_with("blecap-a-b-"));
    assert_ne!(a["file_stem"], b["file_stem"]);
    assert_ne!(a["capture_id"], b["capture_id"]);
}

#[test]
fn changed_bytes_and_observed_peer_type_change_identity_without_claiming_permanent_device_id() {
    let p = probe(&ad(9, b"Device"), &[]);
    let a = identity(&p);
    let mut p = p;
    p.inspection.link = Some(LinkInfo {
        connected_at_ms: 4,
        connect_elapsed_ms: 1,
        interval_us: 30000,
        latency: 0,
        supervision_timeout_us: 5000000,
        att_mtu: None,
    });
    let b = identity(&p);
    assert_eq!(a["device_key"], b["device_key"]);
    assert_ne!(a["capture_id"], b["capture_id"]);
    let mut public = Probe::new();
    public.catalog.observe(Advertisement::new(
        Peer {
            address: [0xC0, 0x12, 0x34, 0x56, 0x78, 0x9A],
            random: false,
        },
        ReportKind::Connectable,
        -44,
        &ad(9, b"Device"),
        1,
    ));
    assert_eq!(identity(&public)["device_key"], "p-c0123456789a");
    assert_ne!(a["capture_id"], identity(&public)["capture_id"]);
}

#[test]
fn frozen_legacy_v1_and_v2_captures_gain_identity_without_schema_rewrites() {
    for (schema, fixture) in [
        (1, include_str!("fixtures/ble-capture-v1-failure.hex")),
        (2, include_str!("fixtures/ble-capture-v2-failure.hex")),
    ] {
        let (pairs, remainder) = fixture.trim().as_bytes().as_chunks::<2>();
        assert!(remainder.is_empty());
        let bytes: Vec<u8> = pairs
            .iter()
            .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
            .collect();
        let v = decode_json(&bytes).unwrap();
        assert_eq!(v["schema"], schema);
        assert_eq!(v["export_schema"], 1);
        assert_eq!(v["identity"]["version"], 1);
        assert_eq!(
            v["identity"]["capture_id"],
            format!("sha256:{}", v["binary"]["sha256"].as_str().unwrap())
        );
    }
}

struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "ble-identity-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        // Only this test's newly created files and one-level output directories.
        for entry in fs::read_dir(&self.0).unwrap().map(Result::unwrap) {
            if entry.file_type().unwrap().is_dir() {
                for file in fs::read_dir(entry.path()).unwrap().map(Result::unwrap) {
                    fs::remove_file(file.path()).unwrap();
                }
                fs::remove_dir(entry.path()).unwrap();
            } else {
                fs::remove_file(entry.path()).unwrap();
            }
        }
        fs::remove_dir(&self.0).unwrap();
    }
}

#[test]
fn publication_uses_same_named_artifacts_across_slot_envelopes_and_offline_decode() {
    let temp = Temp::new();
    let data = bytes(&probe(&ad(9, b"NEEWER-TL21C"), &[]));
    let i = decode_json(&data).unwrap()["identity"].clone();
    let stem = i["file_stem"].as_str().unwrap();
    for (dir, slot, generation) in [
        ("usb", Some(1), Some(1)),
        ("later", Some(3), Some(12)),
        ("offline", None, None),
    ] {
        let output = temp.0.join(dir);
        publish(&output, &data, slot, generation).unwrap();
        let binary = format!("{stem}.blecap");
        let json_name = format!("{stem}.json");
        let txt = format!("{stem}.txt");
        assert_eq!(fs::read(output.join(&binary)).unwrap(), data);
        let v: Value = serde_json::from_slice(&fs::read(output.join(&json_name)).unwrap()).unwrap();
        assert_eq!(v["identity"], i);
        assert_eq!(v["export"]["artifacts"]["binary"], binary);
        assert_eq!(v["export"]["artifacts"]["json"], json_name);
        assert_eq!(v["export"]["artifacts"]["text"], txt);
        assert!(fs::read_to_string(output.join(&txt)).unwrap().contains(stem));
        let mut names: Vec<_> = fs::read_dir(&output)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        let mut expected = vec![binary, json_name, txt, "COMPLETE".into()];
        expected.sort();
        assert_eq!(names, expected);
        assert!(publish(&output, &data, None, None).is_err());
    }
}

fn cli(args: &[&Path]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_ble_probe_captures"))
        .arg("decode")
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn real_offline_cli_decodes_legacy_filename_and_reports_the_actual_named_outputs() {
    let temp = Temp::new();
    let input = temp.0.join("capture.blecap");
    let output = temp.0.join("output");
    let data = bytes(&probe(&ad(9, b"Light"), &[]));
    fs::write(&input, &data).unwrap();
    let result = cli(&[&input, &output]);
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    let v = decode_json(&data).unwrap();
    let stdout = String::from_utf8(result.stdout).unwrap();
    for ext in ["blecap", "json", "txt"] {
        assert!(stdout.contains(&format!("{}.{}", v["identity"]["file_stem"].as_str().unwrap(), ext)));
    }
    assert!(stdout.contains(v["identity"]["capture_id"].as_str().unwrap()));
    assert!(!cli(&[&input, &output]).status.success());
    assert_eq!(fs::read(&input).unwrap(), data);
}

#[test]
fn real_offline_cli_rejects_missing_corrupt_oversized_inputs_and_unwritable_destinations() {
    let temp = Temp::new();
    let input = temp.0.join("bad.blecap");
    let output = temp.0.join("output");
    assert!(!cli(&[&input, &output]).status.success());
    for bad in [
        b"invalid".to_vec(),
        vec![0; dashboard_core::discovery::capture_store::PAYLOAD_MAX + 1],
    ] {
        fs::write(&input, bad).unwrap();
        assert!(!cli(&[&input, &output]).status.success());
        assert!(!output.exists());
    }
    fs::write(&input, bytes(&probe(&[], &[]))).unwrap();
    assert!(!cli(&[&input, &temp.0.join("missing").join("child")]).status.success());
    fs::write(&output, b"existing-file").unwrap();
    assert!(!cli(&[&input, &output]).status.success());
    assert_eq!(fs::read(&output).unwrap(), b"existing-file");
}
