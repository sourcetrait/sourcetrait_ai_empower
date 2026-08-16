/// What: extract one-hop carry names from a `syn::Type`. Walks every
/// `TypePath` in the type subtree (handles generics, references,
/// tuples, arrays, etc.) and collects the last-segment ident of each.
/// Filters to identifiers starting with an uppercase letter so
/// primitives (`u8`, `bool`, etc.) and lifetime parameters are
/// excluded.
///
/// Why: refactor phase R2 (per
/// `notes/know_rust/tasks/picks-data-model-refactor.md`). The carry
/// concept needs a syntactic extraction that doesn't require
/// semantic resolution; pulling type-path leaf idents from each
/// referenced type gives a stable, walker-only signal that the
/// reader can grep against the existing per-crate facts.
///
/// Where: called by walker visit methods on struct fields, enum
/// variants, impl-method signatures, and trait supertypes when
/// recording carry under the parent's pattern key.
pub(crate) fn type_carry_names(ty: &syn::Type) -> Vec<String> {
    use syn::visit::Visit;
    struct CarryCollector {
        names: Vec<String>,
    }
    impl<'ast> Visit<'ast> for CarryCollector {
        fn visit_type_path(&mut self, tp: &'ast syn::TypePath) {
            if let Some(last) = tp.path.segments.last() {
                let name = last.ident.to_string();
                if name
                    .chars()
                    .next()
                    .map(|c| c.is_uppercase())
                    .unwrap_or(false)
                {
                    self.names.push(name);
                }
            }
            syn::visit::visit_type_path(self, tp);
        }
    }
    let mut c = CarryCollector { names: Vec::new() };
    c.visit_type(ty);
    c.names
}

/// What: format an attribute's path as a `::`-joined string (the same
/// shape rustdoc and the AttrEntry wire schema use).
///
/// Why: attributes ride through the walker as syn::Path objects;
/// downstream code matches on the joined-segment string form.
///
/// Where: called from `FileWalker::record_attribute` and the
/// cfg / macro_export detection logic.
pub(crate) fn attribute_path_string(attr: &syn::Attribute) -> String {
    let path = attr.path();
    path.segments
        .iter()
        .map(|s| s.ident.to_string())
        .collect::<Vec<_>>()
        .join("::")
}

/// What: the final `::`-separated segment of a path string.
///
/// Why: many filter checks (cfg detection, attribute-name lookup) want
/// the bare leaf identifier without the qualifying segments.
///
/// Where: called against `attribute_path_string` output, against type
/// idents in derive lists, and against macro-path strings.
pub(crate) fn last_segment(path: &str) -> String {
    path.rsplit("::").next().unwrap_or(path).trim().to_string()
}

/// What: the literal text between the parentheses of a `#[name(...)]`
/// attribute, or the empty string for path-only / name-value attrs.
///
/// Why: cfg-expression text + derive-list text + attr-macro arg-count
/// estimation all read this same raw form.
///
/// Where: called from `FileWalker::record_attribute` for cfg expression
/// capture and derive-list splitting.
pub(crate) fn attribute_args_string(attr: &syn::Attribute) -> String {
    match &attr.meta {
        syn::Meta::Path(_) => String::new(),
        syn::Meta::List(list) => list.tokens.to_string(),
        syn::Meta::NameValue(_) => String::new(),
    }
}

/// What: the source line of the `#` token opening an attribute.
pub(crate) fn attr_line(attr: &syn::Attribute) -> usize {
    attr.pound_token.span.start().line
}

/// What: true when an attribute list carries `#[doc(hidden)]`.
///
/// Why: a hidden item/binding is excluded from the decl-driven API
/// channel - the publisher's explicit "not the public face" signal.
///
/// Where: called from the walker's fn / mod / use visitors when
/// populating the `doc_hidden` wire flags.
pub(crate) fn is_doc_hidden(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|a| {
        attribute_path_string(a) == "doc"
            && matches!(&a.meta, syn::Meta::List(list)
                if list.tokens.to_string().split(',').any(|t| t.trim() == "hidden"))
    })
}

/// What: join all `///` / `#[doc = "..."]` strings on an item into a
/// single space-separated docstring.
///
/// Why: characterize.py's downstream picker consumes the joined form;
/// the prior python implementation used the same heuristic.
///
/// Where: called from `FileWalker` when emitting `TraitEntry`,
/// `TypeEntry`, `FnEntry` and friends.
pub(crate) fn extract_doc(attrs: &[syn::Attribute]) -> String {
    let mut parts: Vec<String> = Vec::new();
    for attr in attrs {
        if attribute_path_string(attr) != "doc" {
            continue;
        }
        if let syn::Meta::NameValue(nv) = &attr.meta {
            if let syn::Expr::Lit(lit) = &nv.value {
                if let syn::Lit::Str(s) = &lit.lit {
                    parts.push(clean_doc_line(&s.value()));
                }
            }
        }
    }
    parts
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// What: strip leading `*` (from `/** */` blocks) and surrounding
/// whitespace from a single docstring line.
pub(crate) fn clean_doc_line(s: &str) -> String {
    s.trim().trim_start_matches('*').trim().to_string()
}

/// What: extract the leaf identifier of a type expression (`Foo` from
/// `Foo<T>`, `Foo` from `&mut Foo`, etc.).
///
/// Why: impl-target names are emitted as a single bare identifier;
/// generics + lifetimes + references are stripped.
///
/// Where: called when emitting `ImplEntry::type_name`.
pub(crate) fn type_base_name(ty: &syn::Type) -> String {
    match ty {
        syn::Type::Path(tp) => tp
            .path
            .segments
            .last()
            .map(|s| s.ident.to_string())
            .unwrap_or_default(),
        syn::Type::Reference(r) => type_base_name(&r.elem),
        syn::Type::Paren(p) => type_base_name(&p.elem),
        syn::Type::Group(g) => type_base_name(&g.elem),
        _ => String::new(),
    }
}

/// What: split a delimited string on top-level commas (ignoring commas
/// nested inside parens / brackets / braces / angle brackets).
///
/// Why: derive lists and macro arg lists are flat at depth 0 but may
/// contain commas inside generic arguments; the python tokenizer used
/// the same depth-tracked split.
///
/// Where: called for `#[derive(A, B<C, D>)]` -> `["A", "B<C, D>"]`
/// splits and for the macro-call arg-count + arg-ident extraction.
pub(crate) fn split_top_commas(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth: i32 = 0;
    let mut cur = String::new();
    for ch in s.chars() {
        match ch {
            '(' | '[' | '{' | '<' => {
                depth += 1;
                cur.push(ch);
            }
            ')' | ']' | '}' | '>' => {
                depth = (depth - 1).max(0);
                cur.push(ch);
            }
            ',' if depth == 0 => {
                if !cur.trim().is_empty() {
                    out.push(cur.clone());
                }
                cur.clear();
            }
            _ => cur.push(ch),
        }
    }
    if !cur.trim().is_empty() {
        out.push(cur);
    }
    out
}

/// What: the number of top-level comma-separated chunks in a macro
/// argument TokenStream, or `0` for empty.
pub(crate) fn count_top_commas_plus_one(tokens: &proc_macro2::TokenStream) -> usize {
    let s = tokens.to_string();
    if s.trim().is_empty() {
        return 0;
    }
    split_top_commas(&s).len().max(1)
}

/// What: extract identifier-shaped arguments from a macro-call
/// TokenStream (one per top-level chunk; chunks that aren't bare
/// identifiers are dropped).
///
/// Why: agents reading the orientation use these as the registered-item
/// names for `bind_command!(A, B, C)`-style macros without having to
/// expand the macro.
pub(crate) fn extract_macro_arg_idents(tokens: &proc_macro2::TokenStream) -> Vec<String> {
    let s = tokens.to_string();
    let pieces = split_top_commas(&s);
    let mut out = Vec::new();
    for piece in pieces {
        let trimmed = piece.trim();
        let last = trimmed
            .rsplit("::")
            .next()
            .unwrap_or(trimmed)
            .split('<')
            .next()
            .unwrap_or(trimmed)
            .trim()
            .to_string();
        if is_ident(&last) {
            out.push(last);
        }
    }
    out
}

/// What: true when `s` is a syntactically valid Rust identifier
/// (starts with letter / underscore; rest alphanumeric / underscore).
pub(crate) fn is_ident(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_alphabetic() || c == '_' => {
            chars.all(|c| c.is_alphanumeric() || c == '_')
        }
        _ => false,
    }
}

/// What: render a `use ...;` tree as a flat string (e.g. `foo::{Bar, Baz}`,
/// `foo::Bar as Qux`, `foo::*`).
///
/// Why: characterize.py downstream pulls capitalized identifiers from
/// the joined-path form to count cross-crate type imports; preserving
/// the python output shape keeps the merge transparent.
pub(crate) fn flatten_use_tree(tree: &syn::UseTree) -> String {
    let mut buf = String::new();
    flatten_use_tree_inner(tree, &mut buf);
    buf
}

fn flatten_use_tree_inner(tree: &syn::UseTree, buf: &mut String) {
    match tree {
        syn::UseTree::Path(p) => {
            buf.push_str(&p.ident.to_string());
            buf.push_str("::");
            flatten_use_tree_inner(&p.tree, buf);
        }
        syn::UseTree::Name(n) => {
            buf.push_str(&n.ident.to_string());
        }
        syn::UseTree::Rename(r) => {
            buf.push_str(&r.ident.to_string());
            buf.push_str(" as ");
            buf.push_str(&r.rename.to_string());
        }
        syn::UseTree::Glob(_) => {
            buf.push('*');
        }
        syn::UseTree::Group(g) => {
            buf.push('{');
            let mut first = true;
            for item in &g.items {
                if !first {
                    buf.push_str(", ");
                }
                first = false;
                flatten_use_tree_inner(item, buf);
            }
            buf.push('}');
        }
    }
}
