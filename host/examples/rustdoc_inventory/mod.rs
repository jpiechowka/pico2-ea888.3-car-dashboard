//! Parsed item contracts; source fixtures exercise coverage without compiling them.

use proc_macro2::Span;
use syn::spanned::Spanned;
use syn::visit::{self, Visit};
use syn::{Attribute, Expr, FnArg, ImplItem, Item, Lit, Meta, ReturnType, Signature, TraitItem};

/// One source item and the structural gaps its documentation still contains.
#[derive(Debug)]
pub struct Entry {
    /// Stable category, such as `function`, `field` or `module`.
    pub kind: &'static str,
    /// Lexical path within the file; tuple fields use decimal indices.
    pub name: String,
    /// One-based line containing the item's defining identifier.
    pub line: usize,
    /// Joined doc attributes; ordinary comments are intentionally excluded.
    pub docs: String,
    /// Missing documentation or contract markers; empty means structurally covered.
    pub missing: Vec<String>,
}

/// Parse `source` and return all defining items, including private/disabled code.
///
/// Returns a syntax error for invalid Rust; macro expansion is not performed.
pub fn inspect_source(source: &str) -> syn::Result<Vec<Entry>> {
    let parsed = syn::parse_file(source)?;
    let mut inventory = Inventory::default();
    inventory.record("file", "<file>", &parsed.attrs, Span::call_site(), None);
    inventory.visit_file(&parsed);
    Ok(inventory.entries)
}

/// Ordered AST traversal state; scopes include nested block items and methods.
#[derive(Default)]
struct Inventory {
    /// Lexical owners; popped after visiting each owner's children.
    scopes: Vec<String>,
    /// Source-order entries accumulated without evaluating cfg predicates.
    entries: Vec<Entry>,
}

/// Join literal documentation in `attrs`; returns empty for comments/doc(hidden).
///
/// Nonliteral `doc = include_str!(...)` is recorded for rendered-doc review;
/// this source-only inventory cannot evaluate arbitrary attribute expressions.
fn documentation(attrs: &[Attribute]) -> String {
    attrs
        .iter()
        .filter_map(|attr| {
            match &attr.meta {
                Meta::NameValue(meta) if meta.path.is_ident("doc") => {
                    Some(match &meta.value {
                        Expr::Lit(value) => {
                            match &value.lit {
                                Lit::Str(value) => value.value(),
                                _ => String::new(),
                            }
                        }
                        _ => "[computed documentation: verify rendered output]".into(),
                    })
                }
                _ => None,
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Names bound inside a function argument pattern; `_` contributes no binding.
#[derive(Default)]
struct Bindings(
    /// Argument binding names in pattern traversal order; wildcards are omitted.
    Vec<String>,
);

impl<'ast> Visit<'ast> for Bindings {
    /// Record `node` and visit subpatterns; returns after collecting nested names.
    fn visit_pat_ident(
        &mut self,
        node: &'ast syn::PatIdent,
    ) {
        self.0.push(node.ident.to_string());
        visit::visit_pat_ident(self, node);
    }
}

impl Inventory {
    /// Append `kind`/`name` at `span`, using `attrs` and optional `signature`.
    ///
    /// Returns after recording gaps. Function receivers inherit the owner's
    /// contract; named pattern bindings need explicit backticked mentions.
    fn record(
        &mut self,
        kind: &'static str,
        name: &str,
        attrs: &[Attribute],
        span: Span,
        signature: Option<&Signature>,
    ) {
        let docs = documentation(attrs);
        let mut missing = Vec::new();
        if docs.trim().is_empty() {
            missing.push("docs".into());
        }
        if let Some(signature) = signature {
            for argument in &signature.inputs {
                if let FnArg::Typed(argument) = argument {
                    let mut bindings = Bindings::default();
                    bindings.visit_pat(&argument.pat);
                    for name in bindings.0 {
                        if !docs.contains(&format!("`{name}`")) {
                            missing.push(format!("argument:{name}"));
                        }
                    }
                }
            }
            if matches!(&signature.output, ReturnType::Type(_, ty) if !matches!(ty.as_ref(), syn::Type::Tuple(tuple) if tuple.elems.is_empty()))
                && !docs.to_ascii_lowercase().contains("return")
            {
                missing.push("returns".into());
            }
            if signature.unsafety.is_some() && !docs.contains("# Safety") {
                missing.push("safety".into());
            }
        }
        let path = if self.scopes.is_empty() {
            name.into()
        } else {
            format!("{}::{name}", self.scopes.join("::"))
        };
        self.entries.push(Entry {
            kind,
            name: path,
            line: span.start().line,
            docs,
            missing,
        });
    }
}

impl<'ast> Visit<'ast> for Inventory {
    /// Record `node` and recursively visit children; returns with scope restored.
    ///
    /// Imports and impl containers inherit referenced-item documentation. An
    /// out-of-line module's docs live in its separately inventoried source file.
    fn visit_item(
        &mut self,
        node: &'ast Item,
    ) {
        let details = match node {
            Item::Const(v) => Some(("constant", &v.ident, &v.attrs, None)),
            Item::Enum(v) => Some(("enum", &v.ident, &v.attrs, None)),
            Item::Fn(v) => Some(("function", &v.sig.ident, &v.attrs, Some(&v.sig))),
            Item::Macro(v) => v.ident.as_ref().map(|id| ("macro", id, &v.attrs, None)),
            Item::Mod(v) if v.content.is_some() => Some(("module", &v.ident, &v.attrs, None)),
            Item::Static(v) => Some(("static", &v.ident, &v.attrs, None)),
            Item::Struct(v) => Some(("struct", &v.ident, &v.attrs, None)),
            Item::Trait(v) => Some(("trait", &v.ident, &v.attrs, None)),
            Item::TraitAlias(v) => Some(("trait alias", &v.ident, &v.attrs, None)),
            Item::Type(v) => Some(("type alias", &v.ident, &v.attrs, None)),
            Item::Union(v) => Some(("union", &v.ident, &v.attrs, None)),
            _ => None,
        };
        let scope = if let Some((kind, ident, attrs, signature)) = details {
            let name = ident.to_string();
            self.record(kind, &name, attrs, ident.span(), signature);
            Some(name)
        } else if let Item::Impl(v) = node {
            Some(format!("impl@{}", v.impl_token.span.start().line))
        } else {
            None
        };
        if let Some(scope) = scope {
            self.scopes.push(scope);
            visit::visit_item(self, node);
            self.scopes.pop();
        } else {
            visit::visit_item(self, node);
        }
    }

    /// Record associated `node` under its trait; returns after visiting defaults.
    fn visit_trait_item(
        &mut self,
        node: &'ast TraitItem,
    ) {
        let details = match node {
            TraitItem::Const(v) => Some(("associated constant", &v.ident, &v.attrs, None)),
            TraitItem::Fn(v) => Some(("method", &v.sig.ident, &v.attrs, Some(&v.sig))),
            TraitItem::Type(v) => Some(("associated type", &v.ident, &v.attrs, None)),
            _ => None,
        };
        if let Some((kind, ident, attrs, signature)) = details {
            self.record(kind, &ident.to_string(), attrs, ident.span(), signature);
            self.scopes.push(ident.to_string());
            visit::visit_trait_item(self, node);
            self.scopes.pop();
        } else {
            visit::visit_trait_item(self, node);
        }
    }

    /// Record associated `node` under its impl; returns after visiting its body.
    fn visit_impl_item(
        &mut self,
        node: &'ast ImplItem,
    ) {
        let details = match node {
            ImplItem::Const(v) => Some(("associated constant", &v.ident, &v.attrs, None)),
            ImplItem::Fn(v) => Some(("method", &v.sig.ident, &v.attrs, Some(&v.sig))),
            ImplItem::Type(v) => Some(("associated type", &v.ident, &v.attrs, None)),
            _ => None,
        };
        if let Some((kind, ident, attrs, signature)) = details {
            self.record(kind, &ident.to_string(), attrs, ident.span(), signature);
            self.scopes.push(ident.to_string());
            visit::visit_impl_item(self, node);
            self.scopes.pop();
        } else {
            visit::visit_impl_item(self, node);
        }
    }

    /// Record `node` and any payload fields; returns with the enum scope restored.
    fn visit_variant(
        &mut self,
        node: &'ast syn::Variant,
    ) {
        self.record("variant", &node.ident.to_string(), &node.attrs, node.ident.span(), None);
        self.scopes.push(node.ident.to_string());
        visit::visit_variant(self, node);
        self.scopes.pop();
    }

    /// Record named fields in `node`; returns after visiting nested type syntax.
    fn visit_fields_named(
        &mut self,
        node: &'ast syn::FieldsNamed,
    ) {
        for field in &node.named {
            if let Some(ident) = &field.ident {
                self.record("field", &ident.to_string(), &field.attrs, ident.span(), None);
            }
            visit::visit_field(self, field);
        }
    }

    /// Record positional fields in `node`; returns after visiting each type.
    fn visit_fields_unnamed(
        &mut self,
        node: &'ast syn::FieldsUnnamed,
    ) {
        for (index, field) in node.unnamed.iter().enumerate() {
            self.record("field", &index.to_string(), &field.attrs, field.span(), None);
            visit::visit_field(self, field);
        }
    }

    /// Record external `node`; returns after visiting its declaration types.
    fn visit_foreign_item(
        &mut self,
        node: &'ast syn::ForeignItem,
    ) {
        let details = match node {
            syn::ForeignItem::Fn(v) => Some(("foreign function", &v.sig.ident, &v.attrs, Some(&v.sig))),
            syn::ForeignItem::Static(v) => Some(("foreign static", &v.ident, &v.attrs, None)),
            syn::ForeignItem::Type(v) => Some(("foreign type", &v.ident, &v.attrs, None)),
            _ => None,
        };
        if let Some((kind, ident, attrs, signature)) = details {
            self.record(kind, &ident.to_string(), attrs, ident.span(), signature);
        }
        visit::visit_foreign_item(self, node);
    }
}

#[cfg(test)]
mod tests {
    //! Handwritten syntax fixtures detect omitted items and false documentation.

    use super::inspect_source;

    /// Return missing-contract lists for every entry parsed from `source`.
    ///
    /// Panics only if the test fixture is invalid Rust.
    fn gaps(source: &str) -> Vec<(String, Vec<String>)> {
        inspect_source(source)
            .expect("valid source fixture")
            .into_iter()
            .filter(|entry| !entry.missing.is_empty())
            .map(|entry| (entry.name, entry.missing))
            .collect()
    }

    /// Private declarations and cfg-disabled code must not escape inventory.
    #[test]
    fn finds_private_fields_variants_and_disabled_functions() {
        let entries = inspect_source(
            "//! Fixture.\nstruct State { count: u8 }\nenum Event { Ready { id: u32 }, Off }\n#[cfg(any())] fn \
             hidden(value: u8) -> u8 { value }",
        )
        .unwrap();
        let names: Vec<_> = entries.iter().map(|entry| entry.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "<file>",
                "State",
                "State::count",
                "Event",
                "Event::Ready",
                "Event::Ready::id",
                "Event::Off",
                "hidden"
            ]
        );
        assert_eq!(entries.last().unwrap().line, 4);
        assert_eq!(entries.last().unwrap().missing, ["docs", "argument:value", "returns"]);
    }

    /// Plain comments, doc(hidden) and empty doc strings cannot satisfy coverage.
    #[test]
    fn rejects_non_documentation_and_empty_docs() {
        let missing =
            gaps("//! Fixture.\n// prose\n#[doc(hidden)]\nfn hidden() {}\n#[doc = \"   \" ]\nconst EMPTY: u8 = 0;");
        assert_eq!(
            missing,
            [
                ("hidden".into(), vec!["docs".into()]),
                ("EMPTY".into(), vec!["docs".into()])
            ]
        );
    }

    /// Argument and result contracts attach to functions, not primitive keywords.
    #[test]
    fn accepts_concise_input_return_contracts() {
        assert!(
            gaps(
                "//! Fixture.\n/// Convert `value` from milliseconds; returns elapsed seconds.\nfn seconds(value: \
                 u32) -> u32 { value / 1000 }"
            )
            .is_empty()
        );
        let missing = gaps("//! Fixture.\n/// Converts a time value.\nfn seconds(value: u32) -> u32 { value / 1000 }");
        assert_eq!(
            missing,
            [("seconds".into(), vec!["argument:value".into(), "returns".into()])]
        );
    }

    /// Methods, tuple fields and associated items are checked inside their owners.
    #[test]
    fn traverses_traits_impls_and_tuple_fields() {
        let entries = inspect_source(
            "//! Fixture.\nstruct Pair(u8, u8);\ntrait Read { type Error; const LIMIT: usize; fn read(&self, at: \
             usize) -> u8; }\nimpl Pair { fn get(&self) -> u8 { self.0 } }",
        )
        .unwrap();
        let names: Vec<_> = entries.iter().map(|entry| entry.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "<file>",
                "Pair",
                "Pair::0",
                "Pair::1",
                "Read",
                "Read::Error",
                "Read::LIMIT",
                "Read::read",
                "impl@4::get"
            ]
        );
    }

    /// Binding patterns require each named argument; ignored values require none.
    #[test]
    fn checks_pattern_arguments_and_safety_contract() {
        let missing = gaps(
            "//! Fixture.\n/// Returns the sum of `a` and `b`.\nunsafe fn sum((a, b): (u8, u8), _: ()) -> u8 { a + b }",
        );
        assert_eq!(missing, [("sum".into(), vec!["safety".into()])]);
    }

    /// Nested items and macro definitions remain visible without expanding tokens.
    #[test]
    fn visits_nested_items_and_macros() {
        let entries = inspect_source(
            "//! Fixture.\nfn outer() { const INNER: u8 = 0; }\nmacro_rules! make { () => { struct Generated; }; }",
        )
        .unwrap();
        let names: Vec<_> = entries.iter().map(|entry| entry.name.as_str()).collect();
        assert_eq!(names, ["<file>", "outer", "outer::INNER", "make"]);
    }

    /// A malformed file must fail, rather than reporting complete empty coverage.
    #[test]
    fn malformed_source_is_an_error() {
        assert!(inspect_source("fn broken(").is_err());
    }
}
