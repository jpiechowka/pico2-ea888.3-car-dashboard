//! Build storage report decoding and explicit cleanup authority regressions.

use dashboard_configurator::helper_client::BuildStorageView;
use dashboard_helper_protocol::AttemptId;

#[test]
/// Only known unprotected closed records yield cleanup IDs; unknown IDs/path-like input and incomplete records cannot.
fn cleanup_requires_a_current_unprotected_closed_record() {
    let id = "a".repeat(32);
    for (state, protected, allowed) in [
        ("failed", false, true),
        ("artifact", false, true),
        ("artifact", true, false),
        ("incomplete", false, false),
    ] {
        let bytes = format!(
            r#"{{"attempts":[{{"attempt_id":"{id}","state":"{state}","bytes":1024,"protected":{protected}}}],"reason":""}}"#
        );
        let report = BuildStorageView::decode(bytes.as_bytes()).unwrap();
        assert_eq!(report.cleanup_id(&id).is_some(), allowed);
        assert!(report.cleanup_id(&"b".repeat(32)).is_none());
        assert!(report.cleanup_id("../outside").is_none());
        if allowed {
            assert_eq!(report.cleanup_id(&id).unwrap(), AttemptId::new(&id).unwrap());
        }
    }
}

#[test]
/// Known storage conflict codes retain explanations but no cleanup authority; unknown schemas and excess bytes fail.
fn storage_conflicts_remain_inspectable_without_enabling_cleanup() {
    for code in ["BuildStorageBusy", "BuildStorageQuarantined", "BuildStorageUnavailable"] {
        let report = BuildStorageView::decode(format!(r#"{{"error":"{code}"}}"#).as_bytes()).unwrap();
        assert!(report.cleanup_id(&"a".repeat(32)).is_none());
        assert!(!report.explanation().is_empty());
    }
    for bytes in [
        br#"{"error":"anything"}"#.as_slice(),
        br#"{"attempts":[],"reason":"","extra":true}"#,
        b"{}",
        b"[]",
    ] {
        assert!(BuildStorageView::decode(bytes).is_err());
    }
    assert!(BuildStorageView::decode(&vec![b' '; 32769]).is_err());
}
