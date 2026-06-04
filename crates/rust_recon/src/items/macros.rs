use crate::*;
use ext_proc_macro2::*;
use super::*;

/// What: a small cursor over a `&[TokenTree]` slice with the lex-level
/// primitives the macro-body walker needs (advance, balanced-bracket
/// skip, ident-with-`!`/`$`-skip, impl-header parse).
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
    trees: &'a [TokenTree],
    pos: usize,
}

impl<'a> TokenCursor<'a> {
    pub(crate) fn new(trees: &'a [TokenTree]) -> Self {
        Self { trees, pos: 0 }
    }

    pub(crate) fn pos(&self) -> usize {
        self.pos
    }

    fn current(&self) -> Option<&TokenTree> {
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
            if let TokenTree::Punct(p) = tree {
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
                TokenTree::Ident(id) => {
                    let nm = id.to_string();
                    self.advance();
                    let starts_alpha_or_underscore = nm.starts_with('_')
                        || nm.chars().next().map_or(false, |c| c.is_alphabetic());
                    if starts_alpha_or_underscore {
                        return Some(nm);
                    }
                    return None;
                }
                TokenTree::Punct(p) if p.as_char() == '!' || p.as_char() == '$' => {
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
        if let Some(TokenTree::Punct(p)) = self.current() {
            if p.as_char() == '<' {
                self.skip_balanced('<', '>');
            }
        }
        let mut first_idents: Vec<String> = Vec::new();
        let mut found_for = false;
        while let Some(tree) = self.current() {
            match tree {
                TokenTree::Ident(id) => {
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
                TokenTree::Group(g) if g.delimiter() == Delimiter::Brace => break,
                TokenTree::Punct(p) => {
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
                TokenTree::Ident(id) => {
                    let nm = id.to_string();
                    if nm == "where" {
                        break;
                    }
                    type_idents.push(nm);
                    self.advance();
                }
                TokenTree::Group(g) if g.delimiter() == Delimiter::Brace => break,
                TokenTree::Punct(p) => {
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
    tokens: &TokenStream,
    brace_depth: usize,
) {
    let trees: Vec<TokenTree> = tokens.clone().into_iter().collect();
    let mut i = 0;
    while i < trees.len() {
        match &trees[i] {
            TokenTree::Ident(ident) => {
                let name = ident.to_string();
                let line = ident.span().start().line;
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
                            facts.traits.push(TraitEntry {
                                file: file.to_string(),
                                name: nm,
                                line,
                                cfg_gated: false,
                                doc: String::new(),
                                visibility: String::new(),
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
                                visibility: String::new(),
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
                                visibility: String::new(),
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
                                visibility: String::new(),
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
                                visibility: String::new(),
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
            TokenTree::Group(g) => {
                scan_macro_body_tokens(facts, file, is_example, &g.stream(), brace_depth);
            }
            _ => {}
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
    trees: &[TokenTree],
    i: usize,
    file: &str,
    brace_depth: usize,
) -> Option<TypeUsageEntry> {
    if i + 4 >= trees.len() {
        return None;
    }
    let outer_id = match &trees[i] {
        TokenTree::Ident(id) => id,
        _ => return None,
    };
    let p1 = match &trees[i + 1] {
        TokenTree::Punct(p) => p,
        _ => return None,
    };
    let p2 = match &trees[i + 2] {
        TokenTree::Punct(p) => p,
        _ => return None,
    };
    let inner_id = match &trees[i + 3] {
        TokenTree::Ident(id) => id,
        _ => return None,
    };
    let group = match &trees[i + 4] {
        TokenTree::Group(g) => g,
        _ => return None,
    };
    if p1.as_char() != ':' || p2.as_char() != ':' {
        return None;
    }
    if group.delimiter() != Delimiter::Parenthesis {
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
    Some(TypeUsageEntry {
        file: file.to_string(),
        name: format!("{}::{}", outer, inner),
        kind_hint: TypeUsageKind::FactoryCall,
        line: outer_id.span().start().line,
        brace_depth,
        expansion_unverified: true,
    })
}
