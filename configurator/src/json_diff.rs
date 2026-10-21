//! Read-only bounded comparison of strictly typed current configurations.
use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, Write};

use dashboard_config::{Config, ConfigError, MAX_JSON_BYTES, parse_current_json_candidate};
use serde_json::Value;

/// Maximum retained differences; traversal still counts omitted changes exactly.
const PATH_LIMIT: usize = 512;
/// Maximum UTF-8 prefix bytes retained for each before/after JSON value summary.
const SUMMARY_BYTES: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Structural change classification; ambiguous IDs force index-based comparison.
pub enum DiffKind {
    /// Candidate contains a value absent from accepted configuration.
    Added,
    /// Accepted configuration contains a value absent from candidate.
    Removed,
    /// Both sides exist but differ in scalar value or structural type.
    Changed,
    /// Shared uniquely identified views/widgets occur in a different order.
    Reordered,
    /// View/widget identities are missing or repeated, so indexed changes cannot be matched by ID.
    Ambiguous,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// JSON document in which a diff entry's indexed pointer must be resolved.
pub enum PointerSide {
    /// Removed value is addressed in accepted configuration.
    Accepted,
    /// Present candidate value, including explicit null, is addressed in candidate JSON.
    Candidate,
}
impl PointerSide {
    /// Returns the UI label naming the document that owns this pointer.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Accepted => "Pointer in accepted JSON",
            Self::Candidate => "Pointer in candidate JSON",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
/// Bounded serialized JSON prefix for presentation, never a parseable-value guarantee.
pub struct ValueSummary {
    /// At most 256 UTF-8 bytes; truncation can omit closing quotes/brackets.
    pub text: String,
    /// Serializer output exceeded byte capacity; `text` is an incomplete prefix.
    pub truncated: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
/// One indexed JSON change plus optional bounded before/after presentation values.
pub struct DiffEntry {
    /// Indexed pointer in pointer_side(), including ambiguous additions/removals.
    pub pointer: String,
    /// Human-readable view/widget ID and label; never used as a JSON pointer segment.
    pub object_label: Option<String>,
    /// Added/removed/changed/reordered or identity-ambiguous comparison result.
    pub kind: DiffKind,
    /// Accepted value summary; `None` means absent, distinct from JSON null.
    pub before: Option<ValueSummary>,
    /// Candidate value summary; `None` means absent and points at the accepted side.
    pub after: Option<ValueSummary>,
}
impl DiffEntry {
    /// Absent candidate values address accepted JSON. JSON null is a present
    /// value summary, so it still addresses candidate JSON when present there.
    /// Returns the document that owns this entry's indexed pointer.
    pub fn pointer_side(&self) -> PointerSide {
        if self.after.is_none() {
            PointerSide::Accepted
        } else {
            PointerSide::Candidate
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
/// At most 512 structural changes plus the exact number omitted by that limit.
pub struct DiffReport {
    /// Retained differences in deterministic traversal order.
    pub changes: Vec<DiffEntry>,
    /// Further differences counted after retained-entry capacity was reached.
    pub omitted: usize,
}

/// Escape one RFC6901 segment. Object identities never become path segments.
/// Returns `key` with `~` then `/` escaped, preserving all other characters.
/// See [JSON Pointer section 3](https://www.rfc-editor.org/rfc/rfc6901#section-3).
pub fn pointer_segment(key: &str) -> String { key.replace('~', "~0").replace('/', "~1") }

/// Original-byte typed parsing must precede any generic structural expansion.
/// Returns differences between `accepted` and strictly parsed candidate `bytes`.
/// Parse/encoding/length errors propagate; comparison does not approve semantic
/// validity or mutate the accepted configuration.
pub fn diff_json(
    accepted: &Config,
    bytes: &[u8],
) -> Result<DiffReport, ConfigError> {
    let candidate = parse_current_json_candidate(bytes)?;
    diff_configs(accepted, &candidate)
}

/// Returns a bounded structural comparison of typed `accepted` and `candidate`.
/// Each side must encode within `MAX_JSON_BYTES`; encoding/length/reparse failures
/// return `ConfigError`. Semantic invalidity is inspectable and is not validated here.
pub fn diff_configs(
    accepted: &Config,
    candidate: &Config,
) -> Result<DiffReport, ConfigError> {
    /// Returns generic JSON for bounded typed `config`; rejects serialization/length
    /// failure and preserves parser line/column diagnostics on an unexpected reparse error.
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

/// Serializer sink that keeps a fixed byte prefix and stops at its first overflow.
struct SummarySink {
    /// Raw prefix, possibly ending inside UTF-8 until [`summary`] trims it.
    bytes: Vec<u8>,
    /// At least one write exceeded remaining capacity.
    truncated: bool,
}
impl Write for SummarySink {
    /// Retain the fitting prefix of `bytes`; returns its full length only if all fit.
    /// Overflow sets the flag and returns an error after partial insertion to stop
    /// serialization. The sink invariant keeps accumulated bytes within capacity.
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

    /// Returns `Ok(())`; the memory-only sink has no deferred output to flush.
    fn flush(&mut self) -> io::Result<()> { Ok(()) }
}
/// Returns a ≤256-byte serialized prefix of `value`, trimming any final partial
/// UTF-8 scalar and exposing truncation. The prefix need not be complete JSON.
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

/// Returns unique unsigned `id` values mapped to indices in `array`; missing,
/// noninteger or duplicate IDs return `None` instead of inventing identity matches.
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
/// Returns a view/widget display label for `value` under `path`, appending an
/// indexed-identity warning when `ambiguous`. Missing ID/name uses zero/empty text;
/// only `/views` selects view wording, all other caller paths denote widgets.
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
    /// Append `kind` at indexed `path` with optional `object_label` and bounded
    /// `before`/`after` summaries. At capacity, increment omitted without allocation.
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

    /// Compare optional `before`/`after` values recursively at indexed `path`,
    /// retaining `object_label` and `ambiguous` provenance. Unique view/widget IDs
    /// match across reorder; ambiguous IDs compare by index. Missing values differ
    /// from null. Only bounded typed configuration trees may enter this recursion.
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
/// Pointer escaping, absent/null distinction and UTF-8 summary boundary regressions.
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    /// Assert escaped keys resolve on the correct side and scalar type/null/absence differ.
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
        assert_eq!(added.pointer_side(), PointerSide::Candidate);
        assert_eq!(added.pointer_side().label(), "Pointer in candidate JSON");
        assert!(before.pointer(&added.pointer).is_none());
        assert!(after.pointer(&added.pointer).unwrap().is_null());
        let removed = report.changes.iter().find(|entry| entry.pointer == "/null").unwrap();
        assert_eq!(removed.before.as_ref().unwrap().text, "null");
        assert!(removed.after.is_none());
        assert_eq!(removed.pointer_side(), PointerSide::Accepted);
        assert_eq!(removed.pointer_side().label(), "Pointer in accepted JSON");
        assert!(before.pointer(&removed.pointer).unwrap().is_null());
        assert!(after.pointer(&removed.pointer).is_none());
    }

    #[test]
    /// Assert truncating JSON inside a multibyte scalar retains only a valid UTF-8 prefix.
    fn utf8_summary_never_ends_inside_a_character() {
        let value = summary(&json!("ą".repeat(200)));
        assert_eq!(value.text.len(), 255); // opening quote + 127 two-byte chars
        assert!(value.truncated);
    }
}
