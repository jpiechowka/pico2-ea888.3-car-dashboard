//! Literal item-include ownership; generated or unknown expansion never grants coverage.

use std::collections::{BTreeMap, BTreeSet};

use syn::parse::Parser;
use syn::spanned::Spanned;
use syn::visit::{self, Visit};

/// One directly documented owner of a literal included source fragment.
#[derive(Clone, Debug)]
pub struct Owner {
    /// Repository-relative includer path; never an external file.
    pub path: String,
    /// One-based literal include invocation line.
    pub line: usize,
    /// File and enclosing named-scope contracts, without ordinary comments.
    pub docs: String,
}

/// Resolves literal item includes in `sources`, returning documented owners per fragment.
/// Every direct includer must have file and enclosing-scope documentation. Only the
/// fragment file contract may inherit; its individual items remain independently checked.
/// Dynamic includes grant nothing. Missing/escaping paths, syntax errors, cycles and
/// nesting beyond128 files return Err; no filesystem access or macro expansion occurs.
pub fn resolve(sources: &BTreeMap<String, String>) -> Result<BTreeMap<String, Vec<Owner>>, String> {
    let mut incoming: BTreeMap<String, Vec<Owner>> = BTreeMap::new();
    let mut outgoing: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (path, source) in sources {
        let file = syn::parse_file(source).map_err(|e| format!("{path}: {e}"))?;
        let mut visitor = Includes {
            contracts: vec![super::documentation(&file.attrs)],
            found: Vec::new(),
        };
        visitor.visit_file(&file);
        for (literal, line, docs) in visitor.found {
            let target = relative_target(path, &literal)?;
            if !sources.contains_key(&target) {
                return Err(format!("{path}:{line}: literal include is not inventoried: {target}"));
            }
            outgoing.entry(path.clone()).or_default().push(target.clone());
            incoming.entry(target).or_default().push(Owner {
                path: path.clone(),
                line,
                docs,
            });
        }
    }
    let mut complete = BTreeMap::new();
    for path in sources.keys() {
        acyclic(path, &outgoing, &mut BTreeSet::new(), &mut complete)?;
    }
    incoming.retain(|_, owners| owners.iter().all(|owner| !owner.docs.trim().is_empty()));
    Ok(incoming)
}

/// Returns the inventoried relative target of `literal` within `caller`'s directory.
/// Rejects platform-absolute/backslash paths and traversal beyond the repository root.
fn relative_target(
    caller: &str,
    literal: &str,
) -> Result<String, String> {
    if literal.is_empty() || literal.starts_with('/') || literal.contains(['\\', ':']) {
        return Err(format!("{caller}: unsafe literal include path: {literal}"));
    }
    let mut parts: Vec<_> = caller.split('/').collect();
    parts.pop();
    for part in literal.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                if parts.pop().is_none() {
                    return Err(format!("{caller}: literal include escapes root: {literal}"));
                }
            }
            part => parts.push(part),
        }
    }
    Ok(parts.join("/"))
}

/// Checks `path`'s `edges`, sharing `active` ancestors and `complete` longest suffix depths.
/// Returns the longest path's file count, including `path`; cycles or any complete
/// path beyond128 files return Err, including paths through previously checked tails.
fn acyclic(
    path: &str,
    edges: &BTreeMap<String, Vec<String>>,
    active: &mut BTreeSet<String>,
    complete: &mut BTreeMap<String, usize>,
) -> Result<usize, String> {
    if let Some(&depth) = complete.get(path) {
        return if active.len() + depth <= 128 {
            Ok(depth)
        } else {
            Err(format!("literal include depth limit at {path}"))
        };
    }
    if active.len() >= 128 || !active.insert(path.into()) {
        return Err(format!("literal include cycle or depth limit at {path}"));
    }
    let mut depth = 1;
    if let Some(children) = edges.get(path) {
        for child in children {
            depth = depth.max(1 + acyclic(child, edges, active, complete)?);
        }
    }
    active.remove(path);
    complete.insert(path.into(), depth);
    Ok(depth)
}

/// Syntax visitor retaining direct scope contracts and literal item include sites.
struct Includes {
    /// File followed by enclosing named-item documentation; empty scopes disqualify.
    contracts: Vec<String>,
    /// Literal path, one-based line and qualifying owner docs, empty if undocumented.
    found: Vec<(String, usize, String)>,
}
impl<'ast> Visit<'ast> for Includes {
    /// Records literal built-in `node` invocations in any syntax position, without expansion.
    /// Returns after visiting arguments; dynamic or custom-qualified macros grant nothing.
    fn visit_macro(
        &mut self,
        node: &'ast syn::Macro,
    ) {
        let segments: Vec<_> = node.path.segments.iter().map(|s| s.ident.to_string()).collect();
        let builtin = segments == ["include"] || segments == ["core", "include"] || segments == ["std", "include"];
        let literal_argument = |input: syn::parse::ParseStream<'_>| -> syn::Result<syn::LitStr> {
            let literal = input.parse()?;
            let _comma = input.parse::<Option<syn::Token![,]>>()?;
            Ok(literal)
        };
        if builtin && let Ok(literal) = literal_argument.parse2(node.tokens.clone()) {
            let docs = if self.contracts.iter().any(|s| s.trim().is_empty()) {
                String::new()
            } else {
                self.contracts.join("\n\n")
            };
            self.found.push((literal.value(), node.span().start().line, docs));
        }
        visit::visit_macro(self, node);
    }

    /// Records literal `node` include sites and visits children under their own contracts.
    /// Returns after restoring the enclosing scope; computed include arguments are ignored.
    fn visit_item(
        &mut self,
        node: &'ast syn::Item,
    ) {
        let attrs = match node {
            syn::Item::Mod(v) if v.content.is_some() => Some(&v.attrs),
            syn::Item::Fn(v) => Some(&v.attrs),
            syn::Item::Impl(v) => Some(&v.attrs),
            syn::Item::Trait(v) => Some(&v.attrs),
            _ => None,
        };
        if let Some(attrs) = attrs {
            self.contracts.push(super::documentation(attrs));
        }
        visit::visit_item(self, node);
        if attrs.is_some() {
            self.contracts.pop();
        }
    }
}

#[cfg(test)]
mod tests {
    //! Include graphs must retain every source/item gap without a documented owner.
    use std::collections::BTreeMap;

    use super::resolve;

    /// Returns owned fixture sources from `pairs`, without filesystem access.
    fn sources(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs.iter().map(|(p, s)| ((*p).into(), (*s).into())).collect()
    }

    /// Both actual root contracts must qualify, with their distinct source locations retained.
    #[test]
    fn two_documented_roots_cover_only_fragment_file_contract() {
        let input = sources(&[
            ("src/main.rs", "//! Standard owner.\ninclude!(\"application.rs\");"),
            (
                "src/bin/info.rs",
                "//! Identity owner.\ninclude!(\"../application.rs\");",
            ),
            ("src/application.rs", "fn missing() {}"),
        ]);
        let inherited = resolve(&input).unwrap();
        let owners = &inherited["src/application.rs"];
        assert_eq!(owners.len(), 2);
        assert!(owners.iter().all(|o| o.line == 2 && !o.docs.is_empty()));
        let items = super::super::inspect_source(&input["src/application.rs"]).unwrap();
        assert!(items.iter().any(|e| e.name == "missing" && e.missing == ["docs"]));
    }

    /// An undocumented caller blocks inherited coverage even when another caller has prose.
    #[test]
    fn every_includer_must_be_documented() {
        let input = sources(&[
            ("a.rs", "//! Owner.\ninclude!(\"part.rs\");"),
            ("b.rs", "include!(\"part.rs\");"),
            ("part.rs", "fn f() {}"),
        ]);
        assert!(!resolve(&input).unwrap().contains_key("part.rs"));
    }

    /// Orphan and computed-include fragments cannot gain a manufactured contract.
    #[test]
    fn dynamic_and_orphan_sources_remain_uncovered() {
        let input = sources(&[
            ("a.rs", "//! Owner.\ninclude!(concat!(\"part\", \".rs\"));"),
            ("part.rs", "fn f() {}"),
            ("orphan.rs", "fn g() {}"),
        ]);
        assert!(resolve(&input).unwrap().is_empty());
    }

    /// Unsafe or missing literal edges fail instead of disappearing from ownership.
    #[test]
    fn escaping_missing_and_platform_paths_are_rejected() {
        for path in ["../outside.rs", "/outside.rs", "C:/outside.rs", "a\\b.rs", "missing.rs"] {
            let source = format!("//! Owner.\ninclude!({path:?});");
            assert!(resolve(&sources(&[("a.rs", &source)])).is_err(), "{path}");
        }
    }

    /// Documented nodes cannot hide cycles, including a direct self-include.
    #[test]
    fn cycles_are_rejected() {
        for input in [
            sources(&[("a.rs", "//! Owner.\ninclude!(\"a.rs\");")]),
            sources(&[
                ("a.rs", "//! Owner.\ninclude!(\"b.rs\");"),
                ("b.rs", "include!(\"a.rs\");"),
            ]),
        ] {
            assert!(resolve(&input).is_err());
        }
    }

    /// Tail-first lexical visitation must still count every file on the longest path.
    #[test]
    fn cached_tail_depth_enforces_exact_file_limit() {
        let mut input = BTreeMap::from([("f000.rs".into(), "//! Leaf.".into())]);
        for index in 1..128 {
            input.insert(
                format!("f{index:03}.rs"),
                format!("//! Owner.\ninclude!(\"f{:03}.rs\");", index - 1),
            );
        }
        assert!(resolve(&input).is_ok(), "128 files must be accepted");
        input.insert("f128.rs".into(), "//! Owner.\ninclude!(\"f127.rs\");".into());
        assert!(resolve(&input).is_err(), "129 files must be rejected");
    }

    /// A valid optional trailing comma retains both documented and undocumented owners.
    #[test]
    fn trailing_comma_preserves_every_owner() {
        let mut input = sources(&[
            ("a.rs", "//! Owner.\ninclude!(\"part.rs\",);"),
            ("part.rs", "fn f() {}"),
        ]);
        assert_eq!(resolve(&input).unwrap()["part.rs"].len(), 1);
        input.insert("b.rs".into(), "include!(\"part.rs\",);".into());
        assert!(!resolve(&input).unwrap().contains_key("part.rs"));
    }

    /// Nested item includes require a documented enclosing module as well as its file owner.
    #[test]
    fn undocumented_nested_scope_cannot_borrow_unrelated_file_prose() {
        let input = sources(&[
            ("a.rs", "//! Owner.\nmod hidden { include!(\"part.rs\"); }"),
            ("part.rs", "fn f() {}"),
        ]);
        assert!(!resolve(&input).unwrap().contains_key("part.rs"));
    }

    /// Qualified and cfg-disabled includers still participate in the every-owner rule.
    #[test]
    fn qualified_and_disabled_includers_are_not_skipped() {
        let input = sources(&[
            ("a.rs", "//! Owner.\ncore::include!(\"part.rs\");"),
            ("b.rs", "#[cfg(any())] std::include!(\"part.rs\");"),
            ("part.rs", "fn f() {}"),
        ]);
        assert!(!resolve(&input).unwrap().contains_key("part.rs"));
    }
}
