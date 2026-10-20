use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use dashboard_provenance::{verify_archive, verify_tree};
use flate2::Compression;
use flate2::write::GzEncoder;

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "dashboard-provenance-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn write(
        &self,
        name: &str,
        bytes: &[u8],
    ) {
        let path = self.0.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }
}
impl Drop for Fixture {
    fn drop(&mut self) { fs::remove_dir_all(&self.0).unwrap(); }
}
fn digest(bytes: &[u8]) -> String {
    sha2_const_stable::Sha256::new()
        .update(bytes)
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
fn maps(
    original: &[(&str, &[u8])],
    modified: &[(&str, &[u8])],
) -> Vec<u8> {
    let map = |pairs: &[(&str, &[u8])]| {
        pairs
            .iter()
            .map(|(p, b)| (p.to_string(), digest(b)))
            .collect::<BTreeMap<_, _>>()
    };
    serde_json::to_vec(&serde_json::json!({"original":map(original),"modified":map(modified)})).unwrap()
}
#[test]
fn exact_overlay_preserves_original_additions_and_rejects_changed_missing_extra_files() {
    let f = Fixture::new();
    f.write("src/lib.rs", b"patched\n");
    f.write("helper.rs", b"helper");
    let m = maps(
        &[("src/lib.rs", b"original\n")],
        &[("src/lib.rs", b"patched\n"), ("helper.rs", b"helper")],
    );
    verify_tree(&f.0, &m, false).unwrap();
    for bytes in [b"patched\r\n".as_slice(), b"changed"] {
        f.write("src/lib.rs", bytes);
        assert!(verify_tree(&f.0, &m, false).is_err());
    }
    f.write("src/lib.rs", b"patched\n");
    f.write("extra", b"extra");
    assert!(verify_tree(&f.0, &m, false).is_err());
    fs::remove_file(f.0.join("extra")).unwrap();
    fs::remove_file(f.0.join("helper.rs")).unwrap();
    assert!(verify_tree(&f.0, &m, false).is_err());
}
#[test]
fn only_real_vendor_top_level_target_directory_is_ignored() {
    let f = Fixture::new();
    f.write("src/lib.rs", b"source");
    f.write("target/build", b"generated");
    let m = maps(&[("src/lib.rs", b"source")], &[]);
    verify_tree(&f.0, &m, true).unwrap();
    assert!(verify_tree(&f.0, &m, false).is_err());
    f.write("src/target/extra", b"source");
    assert!(verify_tree(&f.0, &m, true).is_err());
    fs::remove_dir_all(f.0.join("src/target")).unwrap();
    fs::remove_dir_all(f.0.join("target")).unwrap();
    f.write("target", b"not a directory");
    assert!(verify_tree(&f.0, &m, true).is_err());
}
#[test]
fn manifest_rejects_noncanonical_paths_bad_hashes_duplicates_and_empty_original() {
    let f = Fixture::new();
    f.write("a", b"x");
    for path in [
        "../a",
        "/a",
        "a\\b",
        "a/../b",
        "a//b",
        "a/./b",
        "C:/a",
        ".",
        "",
        "a/",
        "target/file",
    ] {
        assert!(verify_tree(&f.0, &maps(&[(path, b"x")], &[]), true).is_err(), "{path}");
    }
    for hash in ["".to_string(), "0".repeat(63), "A".repeat(64), "g".repeat(64)] {
        let m = serde_json::to_vec(&serde_json::json!({"original":{"a":hash},"modified":{}})).unwrap();
        assert!(verify_tree(&f.0, &m, true).is_err());
    }
    assert!(verify_tree(&f.0, br#"{"original":{},"modified":{}}"#, true).is_err());
    let hash = digest(b"x");
    let duplicate = format!(r#"{{"original":{{"a":"{hash}","a":"{hash}"}},"modified":{{}}}}"#);
    assert!(verify_tree(&f.0, duplicate.as_bytes(), false).is_err());
}
#[test]
fn hard_linked_source_is_rejected_even_when_its_bytes_match() {
    let f = Fixture::new();
    f.write("a", b"x");
    fs::hard_link(f.0.join("a"), f.0.join("b")).unwrap();
    assert!(verify_tree(&f.0, &maps(&[("a", b"x"), ("b", b"x")], &[]), false).is_err());
}
#[cfg(unix)]
#[test]
fn symlink_root_ancestor_source_and_ignored_target_are_rejected() {
    use std::os::unix::fs::symlink;
    let outer = Fixture::new();
    outer.write("real/src/a", b"x");
    symlink(outer.0.join("real"), outer.0.join("link")).unwrap();
    let m = maps(&[("src/a", b"x")], &[]);
    assert!(verify_tree(&outer.0.join("link"), &m, true).is_err());
    assert!(verify_tree(&outer.0.join("link/src"), &maps(&[("a", b"x")], &[]), true).is_err());
    symlink(outer.0.join("real/src"), outer.0.join("real/target")).unwrap();
    assert!(verify_tree(&outer.0.join("real"), &m, true).is_err());
    fs::remove_file(outer.0.join("real/target")).unwrap();
    fs::remove_file(outer.0.join("real/src/a")).unwrap();
    symlink("../../outside", outer.0.join("real/src/a")).unwrap();
    assert!(verify_tree(&outer.0.join("real"), &m, false).is_err());
}
fn archive(
    path: &Path,
    names: &[(&str, &[u8])],
    link: bool,
) -> Vec<u8> {
    let encoder = GzEncoder::new(Vec::new(), Compression::default());
    let mut tar = tar::Builder::new(encoder);
    for (name, bytes) in names {
        let mut h = tar::Header::new_gnu();
        h.set_size(bytes.len() as u64);
        h.set_mode(0o644);
        if link {
            h.set_entry_type(tar::EntryType::Symlink);
            h.set_link_name("outside").unwrap();
            h.set_size(0);
        }
        h.set_cksum();
        tar.append_data(&mut h, name, if link { &[][..] } else { *bytes })
            .unwrap();
    }
    let bytes = tar.into_inner().unwrap().finish().unwrap();
    fs::write(path, &bytes).unwrap();
    bytes
}
#[test]
fn archive_checks_original_not_overlay_and_rejects_duplicates_foreign_roots_links_and_bad_gzip() {
    let f = Fixture::new();
    let path = f.0.join("source.crate");
    let m = maps(&[("src/a", b"original")], &[("src/a", b"patched")]);
    let bytes = archive(&path, &[("demo-1/src/a", b"original")], false);
    verify_archive(&path, "demo-1", &digest(&bytes), &m).unwrap();
    assert!(verify_archive(&path, "demo-1", &"0".repeat(64), &m).is_err());
    for (members, link) in [
        (
            vec![("demo-1/src/a", b"original".as_slice()), ("demo-1/src/a", b"original")],
            false,
        ),
        (vec![("other-1/src/a", b"original".as_slice())], false),
        (vec![("demo-1/src/a", b"patched".as_slice())], false),
        (vec![("demo-1/src/a", b"".as_slice())], true),
    ] {
        let bytes = archive(&path, &members, link);
        assert!(verify_archive(&path, "demo-1", &digest(&bytes), &m).is_err());
    }
    let bytes = archive(&path, &[("demo-1/src/a", b"original")], false);
    for trailing in [
        [bytes.as_slice(), b"extra"].concat(),
        [bytes.as_slice(), bytes.as_slice()].concat(),
    ] {
        fs::write(&path, &trailing).unwrap();
        assert!(verify_archive(&path, "demo-1", &digest(&trailing), &m).is_err());
    }
    let mut bytes = bytes;
    bytes.truncate(bytes.len() - 4);
    fs::write(&path, &bytes).unwrap();
    assert!(verify_archive(&path, "demo-1", &digest(&bytes), &m).is_err());
}

#[test]
fn reviewed_repository_manifests_and_complete_trees_are_accepted() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../firmware/vendor");
    dashboard_provenance::check_hal(&root, None).unwrap();
    dashboard_provenance::check_radio(&root).unwrap();
}
#[test]
fn pinned_release_and_overlay_inventory_cannot_be_changed_by_the_manifest() {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../firmware/vendor/embassy-rp-provenance.json");
    let baseline: serde_json::Value = serde_json::from_slice(&fs::read(source).unwrap()).unwrap();
    for field in ["crate", "version", "archive_sha256", "url"] {
        let f = Fixture::new();
        let mut changed = baseline.clone();
        changed[field] = "other".into();
        f.write("embassy-rp-provenance.json", &serde_json::to_vec(&changed).unwrap());
        assert!(
            dashboard_provenance::check_hal(&f.0, None)
                .unwrap_err()
                .contains("unexpected HAL source release")
        );
    }
    let f = Fixture::new();
    let mut changed = baseline;
    changed["patched_files_sha256"]
        .as_object_mut()
        .unwrap()
        .remove("src/flash.rs");
    f.write("embassy-rp-provenance.json", &serde_json::to_vec(&changed).unwrap());
    assert!(
        dashboard_provenance::check_hal(&f.0, None)
            .unwrap_err()
            .contains("unexpected HAL patch inventory")
    );
}

#[test]
fn resource_limits_fail_before_reading_unbounded_inputs() {
    let f = Fixture::new();
    f.write("a", b"x");
    let huge = vec![b' '; dashboard_provenance::MANIFEST_LIMIT + 1];
    assert!(
        verify_tree(&f.0, &huge, false)
            .unwrap_err()
            .contains("manifest size limit")
    );
    let file = fs::File::create(f.0.join("a")).unwrap();
    file.set_len(dashboard_provenance::FILE_LIMIT + 1).unwrap();
    drop(file);
    assert!(
        verify_tree(&f.0, &maps(&[("a", b"x")], &[]), false)
            .unwrap_err()
            .contains("size limit")
    );
}
#[cfg(windows)]
#[test]
fn windows_junction_root_ancestor_and_ignored_target_are_rejected() {
    use std::process::Command;
    let f = Fixture::new();
    f.write("real/src/a", b"x");
    let junction = |path: &Path, target: &Path| {
        let status = Command::new("powershell.exe")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "$ErrorActionPreference='Stop'; $null=New-Item -ItemType Junction -Path $env:PROVENANCE_LINK -Target \
                 $env:PROVENANCE_TARGET",
            ])
            .env("PROVENANCE_LINK", path)
            .env("PROVENANCE_TARGET", target)
            .status()
            .unwrap();
        assert!(status.success());
    };
    junction(&f.0.join("link"), &f.0.join("real"));
    let m = maps(&[("src/a", b"x")], &[]);
    assert!(verify_tree(&f.0.join("link"), &m, true).unwrap_err().contains("link"));
    assert!(
        verify_tree(&f.0.join("link/src"), &maps(&[("a", b"x")], &[]), true)
            .unwrap_err()
            .contains("link")
    );
    assert!(verify_tree(&f.0.join("link/../real"), &m, true).is_err());
    fs::remove_dir(f.0.join("link")).unwrap();
    junction(&f.0.join("real/target"), &f.0.join("real/src"));
    assert!(verify_tree(&f.0.join("real"), &m, true).unwrap_err().contains("link"));
    fs::remove_dir(f.0.join("real/target")).unwrap();
    assert_eq!(fs::read(f.0.join("real/src/a")).unwrap(), b"x");
}

#[cfg(unix)]
#[test]
fn a_native_backslash_filename_cannot_impersonate_a_manifest_directory() {
    let f = Fixture::new();
    f.write("a\\b", b"x");
    assert!(
        verify_tree(&f.0, &maps(&[("a/b", b"x")], &[]), false)
            .unwrap_err()
            .contains("noncanonical")
    );
}
#[test]
fn archive_paths_types_declared_sizes_and_end_blocks_are_validated_without_extraction() {
    let f = Fixture::new();
    let path = f.0.join("a.crate");
    let m = maps(&[("a", b"x")], &[]);
    for (name, kind, size) in [
        ("demo-1/../a", b'0', 1),
        ("demo-1/a", b'1', 0),
        ("demo-1/a", b'0', dashboard_provenance::FILE_LIMIT + 1),
        ("demo-1/a", b'x', 1),
    ] {
        let mut header = tar::Header::new_gnu();
        header.set_size(size);
        header.set_mode(0o644);
        header.as_mut_bytes()[..name.len()].copy_from_slice(name.as_bytes());
        header.set_entry_type(tar::EntryType::new(kind));
        header.set_cksum();
        let mut raw = header.as_bytes().to_vec();
        raw.extend([0; 1536]);
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        std::io::Write::write_all(&mut encoder, &raw).unwrap();
        let bytes = encoder.finish().unwrap();
        fs::write(&path, &bytes).unwrap();
        assert!(verify_archive(&path, "demo-1", &digest(&bytes), &m).is_err());
    }
    let bytes = archive(&path, &[("demo-1/a", b"x")], false);
    let mut decoder = flate2::read::GzDecoder::new(bytes.as_slice());
    let mut raw = Vec::new();
    std::io::Read::read_to_end(&mut decoder, &mut raw).unwrap();
    for bad in [raw[..1024].to_vec(), [raw.as_slice(), b"trailing"].concat()] {
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        std::io::Write::write_all(&mut encoder, &bad).unwrap();
        let bytes = encoder.finish().unwrap();
        fs::write(&path, &bytes).unwrap();
        assert!(verify_archive(&path, "demo-1", &digest(&bytes), &m).is_err());
    }
}

#[test]
fn zero_file_data_cannot_substitute_for_tar_end_blocks() {
    let f = Fixture::new();
    let path = f.0.join("zero.crate");
    let data = [0; 1024];
    let m = maps(&[("a", &data)], &[]);
    let bytes = archive(&path, &[("demo-1/a", &data)], false);
    let mut raw = Vec::new();
    std::io::Read::read_to_end(&mut flate2::read::GzDecoder::new(bytes.as_slice()), &mut raw).unwrap();
    for length in [1536, 2048] {
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        std::io::Write::write_all(&mut encoder, &raw[..length]).unwrap();
        let bytes = encoder.finish().unwrap();
        fs::write(&path, &bytes).unwrap();
        assert!(
            verify_archive(&path, "demo-1", &digest(&bytes), &m).is_err(),
            "accepted only {} end bytes",
            length - 1536
        );
    }
}
