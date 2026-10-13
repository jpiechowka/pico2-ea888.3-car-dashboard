//! Read-only bounded comparison of strictly typed current configurations.
use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, Write};

use dashboard_config::{Config, ConfigError, MAX_JSON_BYTES, parse_current_json_candidate};
use serde_json::Value;

const PATH_LIMIT: usize = 512;
const SUMMARY_BYTES: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiffKind {
    Added,
    Removed,
    Changed,
    Reordered,
    Ambiguous,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValueSummary {
    pub text: String,
    pub truncated: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiffEntry {
    /// Added/changed: candidate index. Removed: accepted index. Reorder: array.
    pub pointer: String,
    pub object_label: Option<String>,
    pub kind: DiffKind,
    pub before: Option<ValueSummary>,
    pub after: Option<ValueSummary>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DiffReport {
    pub changes: Vec<DiffEntry>,
    pub omitted: usize,
}

/// Escape one RFC6901 segment. Object identities never become path segments.
pub fn pointer_segment(key: &str) -> String { key.replace('~', "~0").replace('/', "~1") }

/// Original-byte typed parsing must precede any generic structural expansion.
pub fn diff_json(
    accepted: &Config,
    bytes: &[u8],
) -> Result<DiffReport, ConfigError> {
    let candidate = parse_current_json_candidate(bytes)?;
    diff_configs(accepted, &candidate)
}

pub fn diff_configs(
    accepted: &Config,
    candidate: &Config,
) -> Result<DiffReport, ConfigError> {
    fn value(config: &Config) -> Result<Value, ConfigError> {
        let bytes = serde_json::to_vec(config).map_err(|_| ConfigError::Length)?;
        if bytes.len() > MAX_JSON_BYTES {
            return Err(ConfigError::Length);
        }
        serde_json::from_slice(&bytes).map_err(|error| {
            ConfigError::Json {
                line: error.line(),
                column: error.column(),
            }
        })
    }
    let before = value(accepted)?;
    let after = value(candidate)?;
    let mut report = DiffReport::default();
    report.walk(Some(&before), Some(&after), "", None, false);
    Ok(report)
}

struct SummarySink {
    bytes: Vec<u8>,
    truncated: bool,
}
impl Write for SummarySink {
    fn write(
        &mut self,
        bytes: &[u8],
    ) -> io::Result<usize> {
        let available = SUMMARY_BYTES - self.bytes.len();
        self.bytes.extend_from_slice(&bytes[..bytes.len().min(available)]);
        if bytes.len() > available {
            self.truncated = true;
            return Err(io::Error::other("summary capacity"));
        }
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> { Ok(()) }
}
fn summary(value: &Value) -> ValueSummary {
    let mut sink = SummarySink {
        bytes: Vec::with_capacity(SUMMARY_BYTES),
        truncated: false,
    };
    let _ = serde_json::to_writer(&mut sink, value);
    // A serializer write can end inside a multibyte scalar; retain only UTF-8.
    let len = std::str::from_utf8(&sink.bytes).map_or_else(|error| error.valid_up_to(), str::len);
    sink.bytes.truncate(len);
    ValueSummary {
        text: String::from_utf8(sink.bytes).expect("checked UTF-8 prefix"),
        truncated: sink.truncated,
    }
}

fn ids(array: &[Value]) -> Option<BTreeMap<u64, usize>> {
    let mut ids = BTreeMap::new();
    for (index, object) in array.iter().enumerate() {
        let id = object.get("id")?.as_u64()?;
        if ids.insert(id, index).is_some() {
            return None;
        }
    }
    Some(ids)
}
fn label(
    path: &str,
    value: &Value,
    ambiguous: bool,
) -> String {
    let kind = if path == "/views" { "view" } else { "widget" };
    let id = value.get("id").and_then(Value::as_u64).unwrap_or(0);
    let name = value
        .get(if kind == "view" { "name" } else { "label" })
        .and_then(Value::as_str)
        .unwrap_or("");
    format!(
        "{kind} {id}: {name}{}",
        if ambiguous { " (ambiguous ID; indexed)" } else { "" }
    )
}
impl DiffReport {
    fn add(
        &mut self,
        before: Option<&Value>,
        after: Option<&Value>,
        path: &str,
        object_label: Option<&str>,
        kind: DiffKind,
    ) {
        if self.changes.len() == PATH_LIMIT {
            // Traversal is bounded by two 128 KiB inputs: this count is exact.
            self.omitted += 1;
            return;
        }
        self.changes.push(DiffEntry {
            pointer: path.to_owned(),
            object_label: object_label.map(str::to_owned),
            kind,
            before: before.map(summary),
            after: after.map(summary),
        });
    }

    fn walk(
        &mut self,
        before: Option<&Value>,
        after: Option<&Value>,
        path: &str,
        object_label: Option<&str>,
        ambiguous: bool,
    ) {
        if before == after {
            return;
        }
        match (before, after) {
            (Some(Value::Object(left)), Some(Value::Object(right))) => {
                let keys = left.keys().chain(right.keys()).collect::<BTreeSet<_>>();
                for key in keys {
                    self.walk(
                        left.get(key),
                        right.get(key),
                        &format!("{path}/{}", pointer_segment(key)),
                        object_label,
                        ambiguous,
                    );
                }
            }
            (Some(Value::Array(left)), Some(Value::Array(right))) => {
                let stable = matches!(path, "/views" | "/widgets");
                if stable && let (Some(old_ids), Some(new_ids)) = (ids(left), ids(right)) {
                    let old_order = left
                        .iter()
                        .filter_map(|v| v.get("id").and_then(Value::as_u64))
                        .filter(|id| new_ids.contains_key(id))
                        .collect::<Vec<_>>();
                    let new_order = right
                        .iter()
                        .filter_map(|v| v.get("id").and_then(Value::as_u64))
                        .filter(|id| old_ids.contains_key(id))
                        .collect::<Vec<_>>();
                    if old_order != new_order {
                        self.add(before, after, path, None, DiffKind::Reordered);
                    }
                    for (index, value) in right.iter().enumerate() {
                        let id = value["id"].as_u64().expect("unique schema ID");
                        let old = old_ids.get(&id).map(|index| &left[*index]);
                        self.walk(
                            old,
                            Some(value),
                            &format!("{path}/{index}"),
                            Some(&label(path, value, false)),
                            false,
                        );
                    }
                    for (index, value) in left.iter().enumerate() {
                        let id = value["id"].as_u64().expect("unique schema ID");
                        if !new_ids.contains_key(&id) {
                            self.walk(
                                Some(value),
                                None,
                                &format!("{path}/{index}"),
                                Some(&label(path, value, false)),
                                false,
                            );
                        }
                    }
                } else {
                    for index in 0..left.len().max(right.len()) {
                        let old = left.get(index);
                        let new = right.get(index);
                        let display = stable.then(|| label(path, new.or(old).expect("array index exists"), true));
                        self.walk(
                            old,
                            new,
                            &format!("{path}/{index}"),
                            display.as_deref().or(object_label),
                            ambiguous || stable,
                        );
                    }
                }
            }
            _ => {
                self.add(
                    before,
                    after,
                    path,
                    object_label,
                    if ambiguous {
                        DiffKind::Ambiguous
                    } else if before.is_none() {
                        DiffKind::Added
                    } else if after.is_none() {
                        DiffKind::Removed
                    } else {
                        DiffKind::Changed
                    },
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn nested_keys_types_null_and_missing_have_real_distinct_entries() {
        let before = json!({"a~/b":{"~1/": "1"}, "null": null});
        let after = json!({"a~/b":{"~1/": 1}, "added": null});
        let mut report = DiffReport::default();
        report.walk(Some(&before), Some(&after), "", None, false);
        assert_eq!(report.changes.len(), 3);
        let changed = report
            .changes
            .iter()
            .find(|entry| entry.pointer == "/a~0~1b/~01~1")
            .unwrap();
        assert_eq!(changed.before.as_ref().unwrap().text, "\"1\"");
        assert_eq!(changed.after.as_ref().unwrap().text, "1");
        let added = report.changes.iter().find(|entry| entry.pointer == "/added").unwrap();
        assert!(added.before.is_none());
        assert_eq!(added.after.as_ref().unwrap().text, "null");
        let removed = report.changes.iter().find(|entry| entry.pointer == "/null").unwrap();
        assert_eq!(removed.before.as_ref().unwrap().text, "null");
        assert!(removed.after.is_none());
    }

    #[test]
    fn utf8_summary_never_ends_inside_a_character() {
        let value = summary(&json!("ą".repeat(200)));
        assert_eq!(value.text.len(), 255); // opening quote + 127 two-byte chars
        assert!(value.truncated);
    }
}
