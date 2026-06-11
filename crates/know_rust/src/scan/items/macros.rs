use crate::*;

/// What: a small cursor over a `&[proc_macro2::TokenTree]` slice with the
/// lex-level primitives the macro-body walker needs (advance, balanced-
/// bracket skip, ident-with-`!`/`$`-skip, impl-header parse).
///
/// Why: macro_rules! template bodies aren't valid Rust until expansion,
/// so syn::visit::Visit can't enter them. The macro body walker has to
/// operate on raw TokenTrees; packaging the three reusable primitives
/// (skip_balanced, next_ident, parse_impl_header) as cursor methods
/// keeps the outer walker tidy.
///
/// Where: instantiated inside `scan_macro_body_tokens` once per
/// keyword-position-following slice (after each `impl` / `trait` /
/// `struct` / etc. token); the outer loop reads `cursor.pos()` after
/// the parse to advance its own index.
pub(crate) struct TokenCursor<'a> {
    trees: &'a [proc_macro2::TokenTree],
    pos: usize,
}

impl<'a> TokenCursor<'a> {
    pub(crate) fn new(trees: &'a [proc_macro2::TokenTree]) -> Self {
        Self { trees, pos: 0 }
    }

    pub(crate) fn pos(&self) -> usize {
        self.pos
    }

    fn current(&self) -> Option<&proc_macro2::TokenTree> {
        self.trees.get(self.pos)
    }

    fn advance(&mut self) {
        self.pos += 1;
    }

    /// Skip past a balanced `<open> ... <close>` punct pair starting at the
    /// current position (the current token must be the open punct). Advances
    /// past the matching close.
    fn skip_balanced(&mut self, open: char, close: char) {
        let mut depth = 0;
        while let Some(tree) = self.current() {
            if let proc_macro2::TokenTree::Punct(p) = tree {
                let c = p.as_char();
                if c == open {
                    depth += 1;
                } else if c == close {
                    depth -= 1;
                    if depth == 0 {
                        self.advance();
                        return;
                    }
                }
            }
            self.advance();
        }
    }

    /// Read the next identifier, skipping over `!` and `$` punctuation
    /// prefixes (which appear inside `macro_rules!` template tokens).
    /// Returns the ident string and advances past it; returns `None`
    /// when some non-skippable token blocks ident lookup.
    pub(crate) fn next_ident(&mut self) -> Option<String> {
        while let Some(tree) = self.current() {
            match tree {
                proc_macro2::TokenTree::Ident(id) => {
                    let nm = id.to_string();
                    self.advance();
                    let starts_alpha_or_underscore = nm.starts_with('_')
                        || nm.chars().next().map_or(false, |c| c.is_alphabetic());
                    if starts_alpha_or_underscore {
                        return Some(nm);
                    }
                    return None;
                }
                proc_macro2::TokenTree::Punct(p) if p.as_char() == '!' || p.as_char() == '$' => {
                    self.advance();
                }
                _ => return None,
            }
        }
        None
    }

    /// Parse the tokens following an `impl` keyword: optional `<G>`
    /// generics, optional `Trait for`, the self-type, stopping at the
    /// body brace or `where` clause. Returns `(trait_name, type_name)`.
    pub(crate) fn parse_impl_header(&mut self) -> (Option<String>, Option<String>) {
        if let Some(proc_macro2::TokenTree::Punct(p)) = self.current() {
            if p.as_char() == '<' {
                self.skip_balanced('<', '>');
            }
        }
        let mut first_idents: Vec<String> = Vec::new();
        let mut found_for = false;
        while let Some(tree) = self.current() {
            match tree {
                proc_macro2::TokenTree::Ident(id) => {
                    let nm = id.to_string();
                    if nm == "for" {
                        found_for = true;
                        self.advance();
                        break;
                    }
                    if nm == "where" {
                        break;
                    }
                    first_idents.push(nm);
                    self.advance();
                }
                proc_macro2::TokenTree::Group(g) if g.delimiter() == proc_macro2::Delimiter::Brace => break,
                proc_macro2::TokenTree::Punct(p) => {
                    if p.as_char() == '<' {
                        self.skip_balanced('<', '>');
                        continue;
                    }
                    self.advance();
                }
                _ => self.advance(),
            }
        }
        if !found_for {
            let type_name = first_idents
                .into_iter()
                .rfind(|s| !KEYWORDS.contains(&s.as_str()));
            return (None, type_name);
        }
        let trait_name = first_idents
            .into_iter()
            .rfind(|s| !KEYWORDS.contains(&s.as_str()));
        let mut type_idents: Vec<String> = Vec::new();
        while let Some(tree) = self.current() {
            match tree {
                proc_macro2::TokenTree::Ident(id) => {
                    let nm = id.to_string();
                    if nm == "where" {
                        break;
                    }
                    type_idents.push(nm);
                    self.advance();
                }
                proc_macro2::TokenTree::Group(g) if g.delimiter() == proc_macro2::Delimiter::Brace => break,
                proc_macro2::TokenTree::Punct(p) => {
                    if p.as_char() == '<' {
                        self.skip_balanced('<', '>');
                        continue;
                    }
                    self.advance();
                }
                _ => self.advance(),
            }
        }
        let type_name = type_idents
            .into_iter()
            .rfind(|s| !KEYWORDS.contains(&s.as_str()));
        (trait_name, type_name)
    }
}

/// What: walk the TokenStream of a macro body or `macro_rules!`
/// template, emitting facts for any nested impl / trait / struct /
/// enum / union / fn / mod / type / type_usage shape the token-level
/// pattern can recognize.
///
/// Why: syn::visit::Visit cannot enter unparsed macro bodies. The
/// python rustscan implementation walked these manually too; mirror
/// the coverage in rust by iterating the TokenStream and matching
/// keyword tokens. Each `impl` / `trait` / etc. emits an entry with
/// minimal context (line + name; cfg/visibility/doc left blank as the
/// template tokens carry no resolved attribute set).
///
/// Where: called from the walker's `visit_item_macro`, `visit_stmt`'s
/// macro arm, and `visit_expr`'s macro arm. Recurses into nested
/// `TokenTree::Group`s so braced / parenthesized template content is
/// fully covered.
pub(crate) fn scan_macro_body_tokens(
    facts: &mut FileLevelFacts,
    file: &str,
    is_example: bool,
    tokens: &proc_macro2::TokenStream,
    brace_depth: usize,
    emit_attrs: bool,
) {
    let trees: Vec<proc_macro2::TokenTree> = tokens.clone().into_iter().collect();
    let mut i = 0;
    while i < trees.len() {
        if emit_attrs {
            if let proc_macro2::TokenTree::Punct(p) = &trees[i] {
                if p.as_char() == '#' {
                    let mut bracket_idx = i + 1;
                    if let Some(proc_macro2::TokenTree::Punct(p2)) = trees.get(bracket_idx) {
                        if p2.as_char() == '!' {
                            bracket_idx += 1;
                        }
                    }
                    if let Some(proc_macro2::TokenTree::Group(g)) = trees.get(bracket_idx) {
                        if g.delimiter() == proc_macro2::Delimiter::Bracket {
                            let inner: Vec<proc_macro2::TokenTree> =
                                g.stream().clone().into_iter().collect();
                            let mut path_segs: Vec<String> = Vec::new();
                            let mut j = 0;
                            while j < inner.len() {
                                if let proc_macro2::TokenTree::Ident(id) = &inner[j] {
                                    path_segs.push(id.to_string());
                                    if let (
                                        Some(proc_macro2::TokenTree::Punct(c1)),
                                        Some(proc_macro2::TokenTree::Punct(c2)),
                                    ) = (inner.get(j + 1), inner.get(j + 2))
                                    {
                                        if c1.as_char() == ':' && c2.as_char() == ':' {
                                            j += 3;
                                            continue;
                                        }
                                    }
                                    break;
                                } else {
                                    break;
                                }
                            }
                            if !path_segs.is_empty() {
                                let name = path_segs.last().cloned().unwrap_or_default();
                                let path_str = path_segs.join("::");
                                if path_segs.len() == 1 && name == "derive" {
                                    if let Some(proc_macro2::TokenTree::Group(args_g)) =
                                        inner.get(j + 1)
                                    {
                                        if args_g.delimiter()
                                            == proc_macro2::Delimiter::Parenthesis
                                        {
                                            let args_text = args_g.stream().to_string();
                                            for piece in split_top_commas(&args_text) {
                                                let cleaned = last_segment(piece.trim());
                                                if !cleaned.is_empty() {
                                                    if cleaned == "Serialize"
                                                        || cleaned == "Deserialize"
                                                    {
                                                        *facts
                                                            .seams
                                                            .entry(SeamKind::SerdeSerialize)
                                                            .or_default() += 1;
                                                    }
                                                    facts.derives.push(DeriveEntry {
                                                        file: file.to_string(),
                                                        trait_name: cleaned,
                                                        line: p.span().start().line,
                                                    });
                                                }
                                            }
                                        }
                                    }
                                } else if !is_inert_attr(&name)
                                    && !is_noise_attr(&path_str, &name)
                                    && !is_noise_macro(&name)
                                {
                                    facts.macros.push(MacroEntry {
                                        file: file.to_string(),
                                        kind: MacroEntryKind::AttrMacro,
                                        name,
                                        line: p.span().start().line,
                                        expansion_unverified: true,
                                        args_count: None,
                                        arg_idents: None,
                                        brace_depth: Some(brace_depth),
                                    });
                                }
                            }
                            scan_macro_body_tokens(
                                facts,
                                file,
                                is_example,
                                &g.stream(),
                                brace_depth,
                                emit_attrs,
                            );
                            i = bracket_idx + 1;
                            continue;
                        }
                    }
                }
            }
        }
        match &trees[i] {
            proc_macro2::TokenTree::Ident(ident) => {
                let name = ident.to_string();
                let line = ident.span().start().line;
                // Bare `pub` immediately before an item keyword in a
                // macro INVOCATION's argument tokens: recover the
                // visibility the token walk otherwise drops (tokio's
                // cfg_*! { pub mod fs; } top-level mods gated the
                // whole decl channel). macro_rules! DEFINITION
                // templates (emit_attrs == false) stay blank - a
                // template fn is not a declaration until expanded.
                // pub(crate)/pub(super) forms stay blank either way.
                let prev_pub = emit_attrs
                    && i >= 1
                    && matches!(&trees[i - 1], proc_macro2::TokenTree::Ident(p) if p == "pub");
                if emit_attrs {
                    let mut path_segs: Vec<String> = vec![name.clone()];
                    let mut k = i + 1;
                    while k + 2 < trees.len() {
                        let c1 = match &trees[k] {
                            proc_macro2::TokenTree::Punct(p) => p,
                            _ => break,
                        };
                        let c2 = match &trees[k + 1] {
                            proc_macro2::TokenTree::Punct(p) => p,
                            _ => break,
                        };
                        if c1.as_char() != ':' || c2.as_char() != ':' {
                            break;
                        }
                        let id = match &trees[k + 2] {
                            proc_macro2::TokenTree::Ident(id) => id,
                            _ => break,
                        };
                        path_segs.push(id.to_string());
                        k += 3;
                    }
                    if k < trees.len() {
                        if let proc_macro2::TokenTree::Punct(bang) = &trees[k] {
                            if bang.as_char() == '!' {
                                if let Some(proc_macro2::TokenTree::Group(g)) = trees.get(k + 1) {
                                    let inv_name =
                                        path_segs.last().cloned().unwrap_or_default();
                                    if !is_noise_macro(&inv_name) {
                                        facts.macros.push(MacroEntry {
                                            file: file.to_string(),
                                            kind: MacroEntryKind::MacroInvocation,
                                            name: inv_name,
                                            line,
                                            expansion_unverified: true,
                                            args_count: None,
                                            arg_idents: None,
                                            brace_depth: Some(brace_depth),
                                        });
                                    }
                                    scan_macro_body_tokens(
                                        facts,
                                        file,
                                        is_example,
                                        &g.stream(),
                                        brace_depth,
                                        emit_attrs,
                                    );
                                    i = k + 2;
                                    continue;
                                }
                            }
                        }
                    }
                }
                match name.as_str() {
                    "impl" => {
                        let mut cursor = TokenCursor::new(&trees[i + 1..]);
                        let (trait_name, type_name) = cursor.parse_impl_header();
                        let next_i = i + 1 + cursor.pos();
                        facts.impls.push(ImplEntry {
                            file: file.to_string(),
                            trait_name,
                            type_name,
                            line,
                            end_line: line,
                            cfg_gated: false,
                            cfg: String::new(),
                        });
                        i = next_i;
                        continue;
                    }
                    "trait" => {
                        let mut cursor = TokenCursor::new(&trees[i + 1..]);
                        if let Some(nm) = cursor.next_ident() {
                            // Bare-pub recovery mirrors the fn/mod
                            // arms: a macro-INVOCATION-declared
                            // `pub trait` is a real pub declaration
                            // (bevy's define_label!{ pub trait
                            // ScheduleLabel } gated its is_pub and
                            // with it every public-set eligibility).
                            facts.traits.push(TraitEntry {
                                file: file.to_string(),
                                name: nm,
                                line,
                                cfg_gated: false,
                                doc: String::new(),
                                visibility: if prev_pub {
                                    "pub".to_string()
                                } else {
                                    String::new()
                                },
                                // Macro-token decls carry no module
                                // chain (the unit-7a seam): decl-
                                // channel ineligible.
                                module_path: None,
                                doc_hidden: false,
                            });
                            i = i + 1 + cursor.pos();
                            continue;
                        }
                    }
                    "struct" | "enum" | "union" => {
                        let mut cursor = TokenCursor::new(&trees[i + 1..]);
                        if let Some(nm) = cursor.next_ident() {
                            let kind = match name.as_str() {
                                "struct" => TypeEntryKind::Struct,
                                "enum" => TypeEntryKind::Enum,
                                "union" => TypeEntryKind::Union,
                                _ => unreachable!(),
                            };
                            facts.types.push(TypeEntry {
                                file: file.to_string(),
                                kind,
                                name: nm,
                                line,
                                cfg_gated: false,
                                doc: String::new(),
                                visibility: if prev_pub {
                                    "pub".to_string()
                                } else {
                                    String::new()
                                },
                                module_path: None,
                                doc_hidden: false,
                            });
                            i = i + 1 + cursor.pos();
                            continue;
                        }
                    }
                    "fn" => {
                        let mut cursor = TokenCursor::new(&trees[i + 1..]);
                        if let Some(nm) = cursor.next_ident() {
                            facts.fns.push(FnEntry {
                                file: file.to_string(),
                                name: nm,
                                line,
                                brace_depth,
                                doc: String::new(),
                                visibility: if prev_pub {
                                    "pub".to_string()
                                } else {
                                    String::new()
                                },
                                module_path: None,
                                doc_hidden: false,
                            });
                            i = i + 1 + cursor.pos();
                            continue;
                        }
                    }
                    "mod" => {
                        let mut cursor = TokenCursor::new(&trees[i + 1..]);
                        if let Some(nm) = cursor.next_ident() {
                            facts.mods.push(ModEntry {
                                file: file.to_string(),
                                name: nm,
                                line,
                                visibility: if prev_pub {
                                    "pub".to_string()
                                } else {
                                    String::new()
                                },
                                module_path: None,
                                doc_hidden: false,
                            });
                            i = i + 1 + cursor.pos();
                            continue;
                        }
                    }
                    "type" => {
                        let mut cursor = TokenCursor::new(&trees[i + 1..]);
                        if let Some(nm) = cursor.next_ident() {
                            facts.types.push(TypeEntry {
                                file: file.to_string(),
                                kind: TypeEntryKind::Type,
                                name: nm,
                                line,
                                cfg_gated: false,
                                doc: String::new(),
                                visibility: if prev_pub {
                                    "pub".to_string()
                                } else {
                                    String::new()
                                },
                                module_path: None,
                                doc_hidden: false,
                            });
                            i = i + 1 + cursor.pos();
                            continue;
                        }
                    }
                    _ => {
                        if let Some(entry) =
                            try_extract_macro_type_usage(&trees, i, file, brace_depth)
                        {
                            if is_example {
                                facts.example_type_usages.push(entry);
                            } else {
                                facts.type_usages.push(entry);
                            }
                        }
                    }
                }
            }
            proc_macro2::TokenTree::Group(g) => {
                scan_macro_body_tokens(facts, file, is_example, &g.stream(), brace_depth, emit_attrs);
            }
            _ => {}
        }
        i += 1;
    }
}

/// What: walk an attribute's argument tokens for inner macro
/// invocations (e.g., `document_features!()` inside `#[cfg_attr(..., doc = ...)]`).
/// Emits MacroEntry::MacroInvocation for each `IDENT(::IDENT)*!(...)` pattern.
///
/// Why: attribute arguments may contain real macro invocations whose
/// expansion lands in the surrounding compile-time context (e.g., the
/// doc = ... value in cfg_attr expands to actual doc lines). py
/// captures these via its token walker; mine missed them because
/// record_attribute_explicit only processed the attribute itself.
///
/// Where: called from `record_attribute_explicit` for each item-level
/// attribute, after the AttrEntry / AttrMacro emission.
pub(crate) fn scan_attr_meta_for_macros(
    facts: &mut FileLevelFacts,
    file: &str,
    tokens: &proc_macro2::TokenStream,
    brace_depth: usize,
) {
    let trees: Vec<proc_macro2::TokenTree> = tokens.clone().into_iter().collect();
    let mut i = 0;
    while i < trees.len() {
        if let proc_macro2::TokenTree::Ident(ident) = &trees[i] {
            let mut path_segs: Vec<String> = vec![ident.to_string()];
            let mut j = i + 1;
            while j + 1 < trees.len() {
                let c1 = match &trees[j] {
                    proc_macro2::TokenTree::Punct(p) => p,
                    _ => break,
                };
                let c2 = match trees.get(j + 1) {
                    Some(proc_macro2::TokenTree::Punct(p)) => p,
                    _ => break,
                };
                if c1.as_char() != ':' || c2.as_char() != ':' {
                    break;
                }
                let id = match trees.get(j + 2) {
                    Some(proc_macro2::TokenTree::Ident(id)) => id,
                    _ => break,
                };
                path_segs.push(id.to_string());
                j += 3;
            }
            if j < trees.len() {
                if let proc_macro2::TokenTree::Punct(p) = &trees[j] {
                    if p.as_char() == '!' {
                        if let Some(proc_macro2::TokenTree::Group(_)) = trees.get(j + 1) {
                            let name = path_segs.last().cloned().unwrap_or_default();
                            if !is_noise_macro(&name) {
                                facts.macros.push(MacroEntry {
                                    file: file.to_string(),
                                    kind: MacroEntryKind::MacroInvocation,
                                    name,
                                    line: ident.span().start().line,
                                    expansion_unverified: true,
                                    args_count: None,
                                    arg_idents: None,
                                    brace_depth: Some(brace_depth),
                                });
                            }
                            i = j + 2;
                            continue;
                        }
                    }
                }
            }
        }
        if let proc_macro2::TokenTree::Group(g) = &trees[i] {
            scan_attr_meta_for_macros(facts, file, &g.stream(), brace_depth);
        }
        i += 1;
    }
}

/// What: try to recognize the `Outer :: inner ( ... )` shape at index
/// `i` in `trees` (5 tokens consumed) and return a `TypeUsageEntry`
/// for it. Returns `None` when the shape doesn't match or when filter
/// constants reject the outer identifier.
///
/// Why: `syn::Expr::Call` parsing would resolve this in src code, but
/// inside `macro_rules!` template bodies the token-level pattern is
/// the only signal available. Mirrors the python rustscan token
/// detection exactly.
///
/// Where: called from `scan_macro_body_tokens`' default ident branch.
fn try_extract_macro_type_usage(
    trees: &[proc_macro2::TokenTree],
    i: usize,
    file: &str,
    brace_depth: usize,
) -> Option<TypeUsageEntry> {
    if i + 4 >= trees.len() {
        return None;
    }
    let outer_id = match &trees[i] {
        proc_macro2::TokenTree::Ident(id) => id,
        _ => return None,
    };
    let p1 = match &trees[i + 1] {
        proc_macro2::TokenTree::Punct(p) => p,
        _ => return None,
    };
    let p2 = match &trees[i + 2] {
        proc_macro2::TokenTree::Punct(p) => p,
        _ => return None,
    };
    let inner_id = match &trees[i + 3] {
        proc_macro2::TokenTree::Ident(id) => id,
        _ => return None,
    };
    let group = match &trees[i + 4] {
        proc_macro2::TokenTree::Group(g) => g,
        _ => return None,
    };
    if p1.as_char() != ':' || p2.as_char() != ':' {
        return None;
    }
    if group.delimiter() != proc_macro2::Delimiter::Parenthesis {
        return None;
    }
    let outer = outer_id.to_string();
    let inner = inner_id.to_string();
    if KEYWORDS.contains(&outer.as_str())
        || KEYWORDS.contains(&inner.as_str())
        || TYPE_USAGE_NOISE_TYPES.contains(&outer.as_str())
    {
        return None;
    }
    // Walk backward over `Ident :: ` triples to the path's ROOT so a
    // fully-qualified path inside a macro body keeps its written
    // origin (writeln!(std::io::stdout(), ..) -> qualifier "std").
    let mut root_idx = i;
    while root_idx >= 3 {
        let colons = matches!(&trees[root_idx - 1], proc_macro2::TokenTree::Punct(p) if p.as_char() == ':')
            && matches!(&trees[root_idx - 2], proc_macro2::TokenTree::Punct(p) if p.as_char() == ':');
        let prev_ident = matches!(&trees[root_idx - 3], proc_macro2::TokenTree::Ident(_));
        if colons && prev_ident {
            root_idx -= 3;
        } else {
            break;
        }
    }
    let qualifier = if root_idx < i {
        match &trees[root_idx] {
            proc_macro2::TokenTree::Ident(id) => Some(id.to_string()),
            _ => None,
        }
    } else {
        None
    };
    Some(TypeUsageEntry {
        file: file.to_string(),
        name: format!("{}::{}", outer, inner),
        kind_hint: TypeUsageKind::FactoryCall,
        line: outer_id.span().start().line,
        brace_depth,
        expansion_unverified: true,
        qualifier,
    })
}
