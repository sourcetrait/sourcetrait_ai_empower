use crate::*;

/// What: per-file walker that drives a syn::visit::Visit traversal,
/// emitting `*Entry` facts into a `FileLevelFacts` buffer.
///
/// Why: the python rustscan implementation it replaced did manual
/// regex / token-stream traversal; syn::visit::Visit lets the parser
/// drive recursion, which keeps the walker focused on per-node fact
/// emission and naturally handles new syntax variants as syn evolves.
/// Manual brace-depth tracking is preserved as a state field so the
/// downstream `fn_table` heuristic in characterize.py keeps the same
/// "free fn" semantics (brace_depth == 0).
///
/// Where: instantiated once per file in `items::workspace::scan_workspace`;
/// `walk_file` runs Visit over the parsed File, then `facts` is drained
/// into the workspace-level `ItemFacts`.
pub(crate) struct FileWalker {
    file: String,
    is_example: bool,
    brace_depth: usize,
    in_inner_attr_context: bool,
    current_trait_vis: Option<String>,
    current_trait_name: Option<String>,
    current_impl_type_name: Option<String>,
    facts: FileLevelFacts,
}

impl FileWalker {
    pub(crate) fn new(rel_path: String) -> Self {
        let is_example = is_example_file(&rel_path);
        Self {
            file: rel_path,
            is_example,
            brace_depth: 0,
            in_inner_attr_context: false,
            current_trait_vis: None,
            current_trait_name: None,
            current_impl_type_name: None,
            facts: FileLevelFacts::default(),
        }
    }

    /// What: record one carry name under a pattern key. The pattern
    /// key is the picked item's `Pattern::Display` form (e.g.
    /// `structure:Component`, `implementation_functions:World::new`).
    ///
    /// Why: refactor phase R2 - aggregates per-item carry into the
    /// per-file accumulator. Workspace.rs merges this into the
    /// workspace-level `ItemFacts::carries` map at scan_workspace exit.
    fn record_carry(&mut self, pattern_key: String, name: String) {
        self.facts
            .carries
            .entry(pattern_key)
            .or_default()
            .push(CarryEntry { name });
    }

    /// What: record many carry names from a `syn::Type` under one
    /// pattern key. Convenience wrapper around `record_carry` for the
    /// common struct-field / fn-signature extraction shape.
    fn record_carry_from_type(&mut self, pattern_key: &str, ty: &syn::Type) {
        for name in type_carry_names(ty) {
            self.record_carry(pattern_key.to_string(), name);
        }
    }

    /// What: record carry for every uppercase-starting trait bound in a
    /// `+`-punctuated bound list under `pattern_key`.
    ///
    /// Why: R2-expansion - supertype bounds, associated-type bounds, and
    /// generic-param bounds share one extraction (last path segment of
    /// each `TypeParamBound::Trait`, keep uppercase-starting idents to
    /// match `type_carry_names`' primitive/lifetime filter). Centralizing
    /// keeps the four call sites consistent.
    fn record_bound_carry(
        &mut self,
        pattern_key: &str,
        bounds: &syn::punctuated::Punctuated<syn::TypeParamBound, syn::token::Plus>,
    ) {
        for bound in bounds {
            if let syn::TypeParamBound::Trait(tb) = bound {
                if let Some(seg) = tb.path.segments.last() {
                    let nm = seg.ident.to_string();
                    if nm.chars().next().map(|c| c.is_uppercase()).unwrap_or(false) {
                        self.record_carry(pattern_key.to_string(), nm);
                    }
                }
            }
        }
    }

    /// What: record carry for all generic-param bounds + where-clause
    /// trait bounds on `generics` under `pattern_key`.
    ///
    /// Why: R2-expansion - `struct Foo<T: Bar>` / `impl<T: Bar> ...` /
    /// `where T: Bar` constrain the item's type parameters; the bound
    /// traits are reader context for the picked item, alongside the
    /// field-type / sig-type carry already recorded for the same key.
    fn record_generics_carry(&mut self, pattern_key: &str, generics: &syn::Generics) {
        for param in &generics.params {
            if let syn::GenericParam::Type(tp) = param {
                self.record_bound_carry(pattern_key, &tp.bounds);
            }
        }
        if let Some(wc) = &generics.where_clause {
            for pred in &wc.predicates {
                if let syn::WherePredicate::Type(pt) = pred {
                    self.record_bound_carry(pattern_key, &pt.bounds);
                }
            }
        }
    }

    /// What: parse the syn::File, walk it via Visit, and return the
    /// accumulated per-file facts.
    pub(crate) fn walk_file(mut self, file: &syn::File) -> FileLevelFacts {
        self.in_inner_attr_context = true;
        for attr in &file.attrs {
            self.record_attribute_explicit(attr);
        }
        self.in_inner_attr_context = false;
        for item in &file.items {
            self.visit_item(item);
        }
        self.facts
    }

    /// Record one attribute occurrence, plus the side effects keyed off
    /// its base name (derive list -> DeriveEntry, no_std -> seam, doc ->
    /// doc_count, non-inert attr -> MacroEntry of kind AttrMacro).
    fn record_attribute_explicit(&mut self, attr: &syn::Attribute) {
        let path_str = attribute_path_string(attr);
        let base = last_segment(&path_str);
        let args = attribute_args_string(attr);
        let line = attr_line(attr);
        if path_str == "doc" {
            self.facts.doc_count += 1;
        }
        self.facts.attrs.push(AttrEntry {
            file: self.file.clone(),
            path: path_str.clone(),
            base: base.clone(),
            args: args.clone(),
            inner: self.in_inner_attr_context,
            line,
        });
        if base == "no_std" {
            self.facts.seams.insert(SeamKind::NoStd, 1);
        }
        if base == "derive" {
            for piece in split_top_commas(&args) {
                let cleaned = last_segment(piece.trim());
                if !cleaned.is_empty() {
                    if cleaned == "Serialize" || cleaned == "Deserialize" {
                        self.bump_seam(SeamKind::SerdeSerialize, 1);
                    }
                    // R2 carry extraction: derive carries the derived
                    // trait so the reader has impl-shape context.
                    // Pattern key shape: derives:<trait>.
                    let pat = format!("derives:{}", cleaned);
                    self.record_carry(pat, cleaned.clone());
                    self.facts.derives.push(DeriveEntry {
                        file: self.file.clone(),
                        trait_name: cleaned,
                        line,
                    });
                }
            }
        } else if !is_inert_attr(&base) && !is_noise_attr(&path_str, &base) {
            let args_count = split_top_commas(&args)
                .iter()
                .filter(|s| !s.trim().is_empty())
                .count();
            self.facts.macros.push(MacroEntry {
                file: self.file.clone(),
                kind: MacroEntryKind::AttrMacro,
                name: path_str.clone(),
                line,
                expansion_unverified: true,
                args_count: Some(args_count),
                arg_idents: None,
                brace_depth: None,
            });
        }
        if let syn::Meta::List(list) = &attr.meta {
            scan_attr_meta_for_macros(
                &mut self.facts,
                &self.file,
                &list.tokens,
                self.brace_depth,
            );
        }
    }

    /// Process an item's attrs list, extracting cfg-gating + the
    /// macro_export flag along the way. Returns `(cfg_gated, cfg_expr,
    /// has_macro_export)` or `None` when `cfg_expr.trim() == "test"`,
    /// signaling that the caller should skip the item entirely.
    fn process_item_attrs(
        &mut self,
        attrs: &[syn::Attribute],
    ) -> Option<(bool, String, bool)> {
        let mut cfg_gated = false;
        let mut cfg_expr = String::new();
        let mut has_macro_export = false;
        for attr in attrs {
            let path_str = attribute_path_string(attr);
            let base = last_segment(&path_str);
            if base == "cfg" {
                cfg_gated = true;
                cfg_expr = attribute_args_string(attr);
                if cfg_expr.trim() == "test" {
                    return None;
                }
            }
            if base == "macro_export" {
                has_macro_export = true;
            }
            self.record_attribute_explicit(attr);
        }
        Some((cfg_gated, cfg_expr, has_macro_export))
    }

    fn bump_seam(&mut self, kind: SeamKind, n: usize) {
        if n == 0 {
            return;
        }
        *self.facts.seams.entry(kind).or_default() += n;
    }

    /// If `path` matches the `Outer::inner` shape with no generics on
    /// either segment and the outer ident passes the type-usage filter,
    /// emit a `TypeUsageEntry` at the current brace depth.
    fn maybe_record_type_usage(&mut self, path: &syn::Path) {
        let segments: Vec<&syn::PathSegment> = path.segments.iter().collect();
        if segments.len() < 2 {
            return;
        }
        let inner_seg = segments[segments.len() - 1];
        let outer_seg = segments[segments.len() - 2];
        if !matches!(outer_seg.arguments, syn::PathArguments::None) {
            return;
        }
        if !matches!(inner_seg.arguments, syn::PathArguments::None) {
            return;
        }
        let outer = outer_seg.ident.to_string();
        let inner = inner_seg.ident.to_string();
        if KEYWORDS.contains(&outer.as_str()) || KEYWORDS.contains(&inner.as_str()) {
            return;
        }
        if TYPE_USAGE_NOISE_TYPES.contains(&outer.as_str()) {
            return;
        }
        let entry = TypeUsageEntry {
            file: self.file.clone(),
            name: format!("{}::{}", outer, inner),
            kind_hint: TypeUsageKind::FactoryCall,
            line: outer_seg.ident.span().start().line,
            brace_depth: self.brace_depth,
            expansion_unverified: false,
        };
        if self.is_example {
            self.facts.example_type_usages.push(entry);
        } else {
            self.facts.type_usages.push(entry);
        }
    }

    fn scan_path_for_seams(&mut self, path: &syn::Path) {
        for seg in &path.segments {
            match seg.ident.to_string().as_str() {
                "libc" | "syscall" => self.bump_seam(SeamKind::SyscallLibc, 1),
                "Serialize" | "Deserialize" => self.bump_seam(SeamKind::SerdeSerialize, 1),
                "stdin" | "stdout" | "stderr" => self.bump_seam(SeamKind::StdIoStream, 1),
                "Command" => self.bump_seam(SeamKind::ProcessSpawn, 1),
                _ => {}
            }
        }
    }

    /// Iterate the stmts of a block at the CURRENT brace depth (no
    /// further increment). Used by impl-item / trait-item fn handlers,
    /// which fold the impl/trait brace and the fn body brace into one
    /// depth level to match the python rustscan convention.
    fn walk_block_stmts(&mut self, block: &syn::Block) {
        for stmt in &block.stmts {
            self.visit_stmt(stmt);
        }
    }

    /// Process attrs on every field in a struct / enum-variant / union;
    /// the field types themselves aren't emitted (the AST scanner in
    /// `crate::scan` handles cross-item type-reference signals
    /// separately).
    fn process_fields_attrs(&mut self, fields: &syn::Fields) {
        match fields {
            syn::Fields::Named(named) => {
                for f in &named.named {
                    self.process_item_attrs(&f.attrs);
                    self.visit_type(&f.ty);
                }
            }
            syn::Fields::Unnamed(unnamed) => {
                for f in &unnamed.unnamed {
                    self.process_item_attrs(&f.attrs);
                    self.visit_type(&f.ty);
                }
            }
            syn::Fields::Unit => {}
        }
    }

    /// Process attrs of a trait item; visibility flows from the trait
    /// definition since trait items inherit visibility from the trait.
    fn visit_trait_item_with_vis(&mut self, item: &syn::TraitItem, trait_vis: &str) {
        match item {
            syn::TraitItem::Fn(f) => {
                if self.process_item_attrs(&f.attrs).is_none() {
                    return;
                }
                self.brace_depth += 1;
                syn::visit::visit_signature(self, &f.sig);
                self.facts.fns.push(FnEntry {
                    file: self.file.clone(),
                    name: f.sig.ident.to_string(),
                    line: f.sig.ident.span().start().line,
                    brace_depth: self.brace_depth,
                    doc: extract_doc(&f.attrs),
                    visibility: trait_vis.to_string(),
                });
                // R2-expansion: trait method sig carry under
                // `trait_functions:<trait>::<method>`. Mirrors the
                // impl-fn carry pattern; bridges the trait's API
                // surface to the reader's dependent-type set.
                if let Some(tname) = self.current_trait_name.clone() {
                    if !tname.is_empty() {
                        let pat = format!(
                            "trait_functions:{}::{}",
                            tname,
                            f.sig.ident
                        );
                        for input in &f.sig.inputs {
                            if let syn::FnArg::Typed(pt) = input {
                                self.record_carry_from_type(&pat, &pt.ty);
                            }
                        }
                        if let syn::ReturnType::Type(_, ty) = &f.sig.output {
                            self.record_carry_from_type(&pat, ty);
                        }
                    }
                }
                if f.sig.unsafety.is_some() {
                    self.bump_seam(SeamKind::Unsafe, 1);
                }
                if let Some(b) = &f.default {
                    self.walk_block_stmts(b);
                }
                self.brace_depth -= 1;
            }
            syn::TraitItem::Type(ty) => {
                if self.process_item_attrs(&ty.attrs).is_none() {
                    return;
                }
                self.facts.types.push(TypeEntry {
                    file: self.file.clone(),
                    kind: TypeEntryKind::Type,
                    name: ty.ident.to_string(),
                    line: ty.ident.span().start().line,
                    cfg_gated: false,
                    doc: String::new(),
                    visibility: trait_vis.to_string(),
                });
                // R2-expansion: associated-type bounds (`type X: Bound;`)
                // carry under traits:<trait> - part of the trait's
                // interface contract, like its supertype bounds. No
                // assoc-type pick exists, so the bound attaches to the
                // trait pick, not a trait_functions:<trait>::X key.
                if let Some(tname) = self.current_trait_name.clone() {
                    if !tname.is_empty() {
                        let trait_pat = format!("traits:{}", tname);
                        self.record_bound_carry(&trait_pat, &ty.bounds);
                    }
                }
                for bound in &ty.bounds {
                    syn::visit::visit_type_param_bound(self, bound);
                }
                if let Some((_, default_ty)) = &ty.default {
                    self.visit_type(default_ty);
                }
            }
            syn::TraitItem::Const(c) => {
                if self.process_item_attrs(&c.attrs).is_none() {
                    return;
                }
                self.visit_type(&c.ty);
                if let Some((_, expr)) = &c.default {
                    self.brace_depth += 1;
                    self.visit_expr(expr);
                    self.brace_depth -= 1;
                }
            }
            syn::TraitItem::Macro(im) => {
                if self.process_item_attrs(&im.attrs).is_none() {
                    return;
                }
                let name_segs: Vec<String> = im
                    .mac
                    .path
                    .segments
                    .iter()
                    .map(|s| s.ident.to_string())
                    .collect();
                let name = name_segs.last().cloned().unwrap_or_default();
                let line = im
                    .mac
                    .path
                    .segments
                    .last()
                    .map(|s| s.ident.span().start().line)
                    .unwrap_or(0);
                if !is_noise_macro(&name) {
                    let arg_idents = extract_macro_arg_idents(&im.mac.tokens);
                    let args_count = count_top_commas_plus_one(&im.mac.tokens);
                    self.facts.macros.push(MacroEntry {
                        file: self.file.clone(),
                        kind: MacroEntryKind::MacroInvocation,
                        name,
                        line,
                        expansion_unverified: true,
                        args_count: Some(args_count),
                        arg_idents: Some(arg_idents.into_iter().take(64).collect()),
                        brace_depth: Some(self.brace_depth),
                    });
                }
                scan_macro_body_tokens(
                    &mut self.facts,
                    &self.file,
                    self.is_example,
                    &im.mac.tokens,
                    self.brace_depth,
                    true,
                );
            }
            other => {
                eprintln!(
                    "[know_rust] unhandled syn::TraitItem variant in {} (variant: {:?})",
                    self.file,
                    std::mem::discriminant(other),
                );
            }
        }
    }
}

impl<'ast> syn::visit::Visit<'ast> for FileWalker {
    fn visit_item_impl(&mut self, i: &'ast syn::ItemImpl) {
        let Some((cfg_gated, cfg_expr, _)) = self.process_item_attrs(&i.attrs) else {
            return;
        };
        let line = i.impl_token.span.start().line;
        let end_line = i.brace_token.span.close().start().line;
        let trait_name = i
            .trait_
            .as_ref()
            .and_then(|(_, path, _)| path.segments.last().map(|s| s.ident.to_string()));
        let type_name = type_base_name(&i.self_ty);
        self.facts.impls.push(ImplEntry {
            file: self.file.clone(),
            trait_name,
            type_name: Some(type_name.clone()),
            line,
            end_line,
            cfg_gated,
            cfg: cfg_expr,
        });
        if i.unsafety.is_some() {
            self.bump_seam(SeamKind::Unsafe, 1);
        }
        syn::visit::visit_generics(self, &i.generics);
        if let Some((_, path, _)) = &i.trait_ {
            self.visit_path(path);
        }
        self.visit_type(&i.self_ty);
        // R2-expansion: impl generic-param + where-clause bounds
        // (impl<T: Bar> ... for Foo) carry under structure:<impl_target>,
        // recorded structurally for any non-empty target. Workspace-
        // origin filtering of carry - the design rule that all forms of
        // picks (Picked + Carried) exclude types defined outside the
        // workspace - happens downstream at characterize time, where the
        // full workspace type+trait set is known; the walker is per-file
        // and has no cross-workspace knowledge. See
        // notes/know_rust/working/02_picks_data.md.
        if !type_name.is_empty() {
            let struct_pat = format!("structure:{}", type_name);
            self.record_generics_carry(&struct_pat, &i.generics);
        }
        // R2 carry extraction: track the impl-target type so each impl
        // method visit keys its carry under
        // `implementation_functions:<type>::<method>`.
        let saved_impl = self.current_impl_type_name.take();
        self.current_impl_type_name = Some(type_name);
        for item in &i.items {
            self.visit_impl_item(item);
        }
        self.current_impl_type_name = saved_impl;
    }

    fn visit_item_trait(&mut self, t: &'ast syn::ItemTrait) {
        let Some((cfg_gated, _, _)) = self.process_item_attrs(&t.attrs) else {
            return;
        };
        let line = t.ident.span().start().line;
        let trait_name = t.ident.to_string();
        self.facts.traits.push(TraitEntry {
            file: self.file.clone(),
            name: trait_name.clone(),
            line,
            cfg_gated,
            doc: extract_doc(&t.attrs),
            visibility: visibility_string(&t.vis),
        });
        if t.unsafety.is_some() {
            self.bump_seam(SeamKind::Unsafe, 1);
        }
        syn::visit::visit_generics(self, &t.generics);
        // R2-expansion: supertype bounds carry under `traits:<trait>`.
        // The trait's interface includes the constraints it composes
        // with; readers need them to understand its scope.
        let trait_pat = format!("traits:{}", trait_name);
        self.record_bound_carry(&trait_pat, &t.supertraits);
        for bound in &t.supertraits {
            syn::visit::visit_type_param_bound(self, bound);
        }
        let saved_vis = self.current_trait_vis.take();
        let saved_name = self.current_trait_name.take();
        self.current_trait_vis = Some(visibility_string(&t.vis));
        self.current_trait_name = Some(trait_name);
        let trait_vis = visibility_string(&t.vis);
        for item in &t.items {
            self.visit_trait_item_with_vis(item, &trait_vis);
        }
        self.current_trait_vis = saved_vis;
        self.current_trait_name = saved_name;
    }

    fn visit_item_struct(&mut self, s: &'ast syn::ItemStruct) {
        let Some((cfg_gated, _, _)) = self.process_item_attrs(&s.attrs) else {
            return;
        };
        let struct_name = s.ident.to_string();
        self.facts.types.push(TypeEntry {
            file: self.file.clone(),
            kind: TypeEntryKind::Struct,
            name: struct_name.clone(),
            line: s.ident.span().start().line,
            cfg_gated,
            doc: extract_doc(&s.attrs),
            visibility: visibility_string(&s.vis),
        });
        syn::visit::visit_generics(self, &s.generics);
        // R2 carry extraction: field types -> carry under structure:<name>
        let pat = format!("structure:{}", struct_name);
        match &s.fields {
            syn::Fields::Named(named) => {
                for f in &named.named {
                    self.record_carry_from_type(&pat, &f.ty);
                }
            }
            syn::Fields::Unnamed(unnamed) => {
                for f in &unnamed.unnamed {
                    self.record_carry_from_type(&pat, &f.ty);
                }
            }
            syn::Fields::Unit => {}
        }
        // R2-expansion: generic-param + where-clause trait bounds
        // (struct Foo<T: Bar>) carry under structure:<name>.
        self.record_generics_carry(&pat, &s.generics);
        self.process_fields_attrs(&s.fields);
    }

    fn visit_item_enum(&mut self, e: &'ast syn::ItemEnum) {
        let Some((cfg_gated, _, _)) = self.process_item_attrs(&e.attrs) else {
            return;
        };
        let enum_name = e.ident.to_string();
        self.facts.types.push(TypeEntry {
            file: self.file.clone(),
            kind: TypeEntryKind::Enum,
            name: enum_name.clone(),
            line: e.ident.span().start().line,
            cfg_gated,
            doc: extract_doc(&e.attrs),
            visibility: visibility_string(&e.vis),
        });
        syn::visit::visit_generics(self, &e.generics);
        // R2 carry extraction: variant payload types -> carry under structure:<name>
        let pat = format!("structure:{}", enum_name);
        for v in &e.variants {
            self.process_item_attrs(&v.attrs);
            match &v.fields {
                syn::Fields::Named(named) => {
                    for f in &named.named {
                        self.record_carry_from_type(&pat, &f.ty);
                    }
                }
                syn::Fields::Unnamed(unnamed) => {
                    for f in &unnamed.unnamed {
                        self.record_carry_from_type(&pat, &f.ty);
                    }
                }
                syn::Fields::Unit => {}
            }
            self.process_fields_attrs(&v.fields);
            if let Some((_, expr)) = &v.discriminant {
                // Quirk preserved from the python rustscan: enum
                // discriminants walk at brace_depth 0 regardless of
                // the enclosing context.
                let saved = self.brace_depth;
                self.brace_depth = 0;
                self.visit_expr(expr);
                self.brace_depth = saved;
            }
        }
        // R2-expansion: generic-param + where-clause trait bounds
        // (enum Either<L: Display, R>) carry under structure:<name>.
        self.record_generics_carry(&pat, &e.generics);
    }

    fn visit_item_union(&mut self, u: &'ast syn::ItemUnion) {
        let Some((cfg_gated, _, _)) = self.process_item_attrs(&u.attrs) else {
            return;
        };
        self.facts.types.push(TypeEntry {
            file: self.file.clone(),
            kind: TypeEntryKind::Union,
            name: u.ident.to_string(),
            line: u.ident.span().start().line,
            cfg_gated,
            doc: extract_doc(&u.attrs),
            visibility: visibility_string(&u.vis),
        });
        syn::visit::visit_generics(self, &u.generics);
        for f in &u.fields.named {
            self.process_item_attrs(&f.attrs);
            self.visit_type(&f.ty);
        }
    }

    fn visit_item_type(&mut self, ta: &'ast syn::ItemType) {
        if self.process_item_attrs(&ta.attrs).is_none() {
            return;
        }
        self.facts.types.push(TypeEntry {
            file: self.file.clone(),
            kind: TypeEntryKind::Type,
            name: ta.ident.to_string(),
            line: ta.ident.span().start().line,
            cfg_gated: false,
            doc: String::new(),
            visibility: visibility_string(&ta.vis),
        });
        syn::visit::visit_generics(self, &ta.generics);
        self.visit_type(&ta.ty);
    }

    fn visit_item_fn(&mut self, f: &'ast syn::ItemFn) {
        if self.process_item_attrs(&f.attrs).is_none() {
            return;
        }
        syn::visit::visit_signature(self, &f.sig);
        self.facts.fns.push(FnEntry {
            file: self.file.clone(),
            name: f.sig.ident.to_string(),
            line: f.sig.ident.span().start().line,
            brace_depth: self.brace_depth,
            doc: extract_doc(&f.attrs),
            visibility: visibility_string(&f.vis),
        });
        if f.sig.unsafety.is_some() {
            self.bump_seam(SeamKind::Unsafe, 1);
        }
        self.visit_block(&f.block);
    }

    fn visit_item_mod(&mut self, m: &'ast syn::ItemMod) {
        if self.process_item_attrs(&m.attrs).is_none() {
            return;
        }
        self.facts.mods.push(ModEntry {
            file: self.file.clone(),
            name: m.ident.to_string(),
            line: m.ident.span().start().line,
            visibility: visibility_string(&m.vis),
        });
        if let Some((_, items)) = &m.content {
            self.brace_depth += 1;
            for item in items {
                self.visit_item(item);
            }
            self.brace_depth -= 1;
        }
    }

    fn visit_item_macro(&mut self, mc: &'ast syn::ItemMacro) {
        let Some((_, _, has_macro_export)) = self.process_item_attrs(&mc.attrs) else {
            return;
        };
        let name_segs: Vec<String> = mc
            .mac
            .path
            .segments
            .iter()
            .map(|s| s.ident.to_string())
            .collect();
        let path_name = name_segs.join("::");
        let last_name = name_segs.last().cloned().unwrap_or_default();
        let is_macro_rules = path_name == "macro_rules" || mc.ident.is_some();
        if is_macro_rules {
            if let Some(ident) = &mc.ident {
                self.facts.macro_defs.push(MacroDefEntry {
                    file: self.file.clone(),
                    name: ident.to_string(),
                    line: ident.span().start().line,
                    visibility: String::new(),
                    macro_exported: has_macro_export,
                });
            }
        } else {
            let line = mc
                .mac
                .path
                .segments
                .last()
                .map(|s| s.ident.span().start().line)
                .unwrap_or(0);
            let arg_idents = extract_macro_arg_idents(&mc.mac.tokens);
            let args_count = count_top_commas_plus_one(&mc.mac.tokens);
            if !is_noise_macro(&last_name) {
                self.facts.macros.push(MacroEntry {
                    file: self.file.clone(),
                    kind: MacroEntryKind::MacroInvocation,
                    name: last_name,
                    line,
                    expansion_unverified: true,
                    args_count: Some(args_count),
                    arg_idents: Some(arg_idents.into_iter().take(64).collect()),
                    brace_depth: Some(self.brace_depth),
                });
            }
        }
        scan_macro_body_tokens(
            &mut self.facts,
            &self.file,
            self.is_example,
            &mc.mac.tokens,
            self.brace_depth,
            !is_macro_rules,
        );
    }

    fn visit_item_use(&mut self, u: &'ast syn::ItemUse) {
        let _ = self.process_item_attrs(&u.attrs);
        self.facts.uses.push(UseEntry {
            file: self.file.clone(),
            reexport: matches!(u.vis, syn::Visibility::Public(_)),
            path: flatten_use_tree(&u.tree),
            line: u.use_token.span.start().line,
        });
    }

    fn visit_item_extern_crate(&mut self, ec: &'ast syn::ItemExternCrate) {
        let _ = self.process_item_attrs(&ec.attrs);
        self.bump_seam(SeamKind::Extern, 1);
    }

    fn visit_item_foreign_mod(&mut self, fm: &'ast syn::ItemForeignMod) {
        let _ = self.process_item_attrs(&fm.attrs);
        self.bump_seam(SeamKind::Extern, 1);
        if fm.unsafety.is_some() {
            self.bump_seam(SeamKind::Unsafe, 1);
        }
        for it in &fm.items {
            match it {
                syn::ForeignItem::Fn(ff) => {
                    let _ = self.process_item_attrs(&ff.attrs);
                    syn::visit::visit_signature(self, &ff.sig);
                    self.facts.fns.push(FnEntry {
                        file: self.file.clone(),
                        name: ff.sig.ident.to_string(),
                        line: ff.sig.ident.span().start().line,
                        brace_depth: self.brace_depth,
                        doc: extract_doc(&ff.attrs),
                        visibility: visibility_string(&ff.vis),
                    });
                }
                syn::ForeignItem::Static(s) => {
                    let _ = self.process_item_attrs(&s.attrs);
                    self.visit_type(&s.ty);
                }
                syn::ForeignItem::Type(t) => {
                    let _ = self.process_item_attrs(&t.attrs);
                    self.facts.types.push(TypeEntry {
                        file: self.file.clone(),
                        kind: TypeEntryKind::Type,
                        name: t.ident.to_string(),
                        line: t.ident.span().start().line,
                        cfg_gated: false,
                        doc: extract_doc(&t.attrs),
                        visibility: visibility_string(&t.vis),
                    });
                }
                syn::ForeignItem::Macro(m) => {
                    let _ = self.process_item_attrs(&m.attrs);
                    syn::visit::visit_macro(self, &m.mac);
                }
                other => {
                    eprintln!(
                        "[know_rust] unhandled syn::ForeignItem variant in {} (variant: {:?})",
                        self.file,
                        std::mem::discriminant(other),
                    );
                }
            }
        }
    }

    fn visit_item_const(&mut self, c: &'ast syn::ItemConst) {
        if self.process_item_attrs(&c.attrs).is_none() {
            return;
        }
        self.visit_type(&c.ty);
        self.visit_expr(&c.expr);
    }

    fn visit_item_static(&mut self, s: &'ast syn::ItemStatic) {
        if self.process_item_attrs(&s.attrs).is_none() {
            return;
        }
        self.visit_type(&s.ty);
        self.visit_expr(&s.expr);
    }

    fn visit_item(&mut self, i: &'ast syn::Item) {
        if let syn::Item::Verbatim(tokens) = i {
            eprintln!(
                "[know_rust] unhandled syn::Item::Verbatim in {} (tokens: {})",
                self.file,
                tokens.to_string().chars().take(64).collect::<String>(),
            );
            scan_macro_body_tokens(
                &mut self.facts,
                &self.file,
                self.is_example,
                tokens,
                self.brace_depth,
                true,
            );
            return;
        }
        syn::visit::visit_item(self, i);
    }

    fn visit_item_trait_alias(&mut self, ta: &'ast syn::ItemTraitAlias) {
        eprintln!(
            "[know_rust] unhandled syn::Item::TraitAlias in {} (ident: {})",
            self.file,
            ta.ident,
        );
    }

    fn visit_impl_item_fn(&mut self, f: &'ast syn::ImplItemFn) {
        if self.process_item_attrs(&f.attrs).is_none() {
            return;
        }
        // Fold the impl block's brace into the fn body's brace: emit the
        // FnEntry at brace_depth+1 AND iterate body stmts at the same
        // brace_depth+1 (no further increment via visit_block).
        self.brace_depth += 1;
        syn::visit::visit_signature(self, &f.sig);
        self.facts.fns.push(FnEntry {
            file: self.file.clone(),
            name: f.sig.ident.to_string(),
            line: f.sig.ident.span().start().line,
            brace_depth: self.brace_depth,
            doc: extract_doc(&f.attrs),
            visibility: visibility_string(&f.vis),
        });
        // R2 carry extraction: when inside an impl block with a
        // resolvable Self-type ident, record carry for this method's
        // parameter types + return type under
        // `implementation_functions:<impl_type>::<method>`. Generic
        // blanket impls like `impl<T> Foo for T` resolve to an empty
        // type_base_name; skip them to avoid noise keys of the shape
        // `implementation_functions::<method>` (R2-expansion).
        if let Some(impl_type) = self.current_impl_type_name.clone() {
            if !impl_type.is_empty() {
                let pat = format!(
                    "implementation_functions:{}::{}",
                    impl_type,
                    f.sig.ident
                );
                for input in &f.sig.inputs {
                    if let syn::FnArg::Typed(pt) = input {
                        self.record_carry_from_type(&pat, &pt.ty);
                    }
                }
                if let syn::ReturnType::Type(_, ty) = &f.sig.output {
                    self.record_carry_from_type(&pat, ty);
                }
            }
        }
        if f.sig.unsafety.is_some() {
            self.bump_seam(SeamKind::Unsafe, 1);
        }
        self.walk_block_stmts(&f.block);
        self.brace_depth -= 1;
    }

    fn visit_impl_item_type(&mut self, ty: &'ast syn::ImplItemType) {
        if self.process_item_attrs(&ty.attrs).is_none() {
            return;
        }
        self.visit_type(&ty.ty);
        self.facts.types.push(TypeEntry {
            file: self.file.clone(),
            kind: TypeEntryKind::Type,
            name: ty.ident.to_string(),
            line: ty.ident.span().start().line,
            cfg_gated: false,
            doc: String::new(),
            visibility: visibility_string(&ty.vis),
        });
    }

    fn visit_impl_item_const(&mut self, c: &'ast syn::ImplItemConst) {
        if self.process_item_attrs(&c.attrs).is_none() {
            return;
        }
        self.visit_type(&c.ty);
        self.brace_depth += 1;
        self.visit_expr(&c.expr);
        self.brace_depth -= 1;
    }

    fn visit_impl_item_macro(&mut self, im: &'ast syn::ImplItemMacro) {
        if self.process_item_attrs(&im.attrs).is_none() {
            return;
        }
        let name_segs: Vec<String> = im
            .mac
            .path
            .segments
            .iter()
            .map(|s| s.ident.to_string())
            .collect();
        let name = name_segs.last().cloned().unwrap_or_default();
        let line = im
            .mac
            .path
            .segments
            .last()
            .map(|s| s.ident.span().start().line)
            .unwrap_or(0);
        if !is_noise_macro(&name) {
            let arg_idents = extract_macro_arg_idents(&im.mac.tokens);
            let args_count = count_top_commas_plus_one(&im.mac.tokens);
            self.facts.macros.push(MacroEntry {
                file: self.file.clone(),
                kind: MacroEntryKind::MacroInvocation,
                name,
                line,
                expansion_unverified: true,
                args_count: Some(args_count),
                arg_idents: Some(arg_idents.into_iter().take(64).collect()),
                brace_depth: Some(self.brace_depth),
            });
        }
        scan_macro_body_tokens(
            &mut self.facts,
            &self.file,
            self.is_example,
            &im.mac.tokens,
            self.brace_depth,
            true,
        );
    }

    fn visit_impl_item(&mut self, i: &'ast syn::ImplItem) {
        if let syn::ImplItem::Verbatim(tokens) = i {
            eprintln!(
                "[know_rust] unhandled syn::ImplItem::Verbatim in {} (tokens: {})",
                self.file,
                tokens.to_string().chars().take(64).collect::<String>(),
            );
            self.brace_depth += 1;
            scan_macro_body_tokens(
                &mut self.facts,
                &self.file,
                self.is_example,
                tokens,
                self.brace_depth,
                true,
            );
            self.brace_depth -= 1;
            return;
        }
        syn::visit::visit_impl_item(self, i);
    }

    fn visit_type_macro(&mut self, tm: &'ast syn::TypeMacro) {
        let name_segs: Vec<String> = tm
            .mac
            .path
            .segments
            .iter()
            .map(|s| s.ident.to_string())
            .collect();
        let name = name_segs.last().cloned().unwrap_or_default();
        let line = tm
            .mac
            .path
            .segments
            .last()
            .map(|s| s.ident.span().start().line)
            .unwrap_or(0);
        if !is_noise_macro(&name) {
            let arg_idents = extract_macro_arg_idents(&tm.mac.tokens);
            let args_count = count_top_commas_plus_one(&tm.mac.tokens);
            self.facts.macros.push(MacroEntry {
                file: self.file.clone(),
                kind: MacroEntryKind::MacroInvocation,
                name,
                line,
                expansion_unverified: true,
                args_count: Some(args_count),
                arg_idents: Some(arg_idents.into_iter().take(64).collect()),
                brace_depth: Some(self.brace_depth),
            });
        }
        scan_macro_body_tokens(
            &mut self.facts,
            &self.file,
            self.is_example,
            &tm.mac.tokens,
            self.brace_depth,
            true,
        );
    }

    fn visit_block(&mut self, b: &'ast syn::Block) {
        self.brace_depth += 1;
        self.walk_block_stmts(b);
        self.brace_depth -= 1;
    }

    fn visit_fn_arg(&mut self, arg: &'ast syn::FnArg) {
        match arg {
            syn::FnArg::Typed(pt) => {
                for attr in &pt.attrs {
                    self.record_attribute_explicit(attr);
                }
                self.visit_pat(&pt.pat);
                self.visit_type(&pt.ty);
            }
            syn::FnArg::Receiver(r) => {
                for attr in &r.attrs {
                    self.record_attribute_explicit(attr);
                }
                self.visit_type(&r.ty);
            }
        }
    }

    fn visit_expr_call(&mut self, c: &'ast syn::ExprCall) {
        if let syn::Expr::Path(p) = &*c.func {
            self.maybe_record_type_usage(&p.path);
        }
        self.visit_expr(&c.func);
        for a in &c.args {
            self.visit_expr(a);
        }
    }

    fn visit_expr_method_call(&mut self, mc: &'ast syn::ExprMethodCall) {
        if mc.method == "spawn" {
            self.bump_seam(SeamKind::ProcessSpawn, 1);
        }
        self.visit_expr(&mc.receiver);
        for a in &mc.args {
            self.visit_expr(a);
        }
        if let Some(tf) = &mc.turbofish {
            syn::visit::visit_angle_bracketed_generic_arguments(self, tf);
        }
    }

    fn visit_path(&mut self, p: &'ast syn::Path) {
        self.scan_path_for_seams(p);
        syn::visit::visit_path(self, p);
    }

    fn visit_type_trait_object(&mut self, t: &'ast syn::TypeTraitObject) {
        self.bump_seam(SeamKind::DynTraitObject, 1);
        syn::visit::visit_type_trait_object(self, t);
    }

    fn visit_expr_path(&mut self, p: &'ast syn::ExprPath) {
        self.visit_path(&p.path);
    }

    fn visit_expr_unsafe(&mut self, u: &'ast syn::ExprUnsafe) {
        self.bump_seam(SeamKind::Unsafe, 1);
        self.visit_block(&u.block);
    }

    fn visit_expr_macro(&mut self, m: &'ast syn::ExprMacro) {
        let name_segs: Vec<String> = m
            .mac
            .path
            .segments
            .iter()
            .map(|s| s.ident.to_string())
            .collect();
        let name = name_segs.last().cloned().unwrap_or_default();
        let line = m
            .mac
            .path
            .segments
            .last()
            .map(|s| s.ident.span().start().line)
            .unwrap_or(0);
        if !is_noise_macro(&name) {
            let arg_idents = extract_macro_arg_idents(&m.mac.tokens);
            let args_count = count_top_commas_plus_one(&m.mac.tokens);
            self.facts.macros.push(MacroEntry {
                file: self.file.clone(),
                kind: MacroEntryKind::MacroInvocation,
                name,
                line,
                expansion_unverified: true,
                args_count: Some(args_count),
                arg_idents: Some(arg_idents.into_iter().take(64).collect()),
                brace_depth: Some(self.brace_depth),
            });
        }
        scan_macro_body_tokens(
            &mut self.facts,
            &self.file,
            self.is_example,
            &m.mac.tokens,
            self.brace_depth,
            true,
        );
    }

    fn visit_stmt_macro(&mut self, sm: &'ast syn::StmtMacro) {
        let name_segs: Vec<String> = sm
            .mac
            .path
            .segments
            .iter()
            .map(|s| s.ident.to_string())
            .collect();
        let name = name_segs.last().cloned().unwrap_or_default();
        let line = sm
            .mac
            .path
            .segments
            .last()
            .map(|s| s.ident.span().start().line)
            .unwrap_or(0);
        if !is_noise_macro(&name) {
            let arg_idents = extract_macro_arg_idents(&sm.mac.tokens);
            let args_count = count_top_commas_plus_one(&sm.mac.tokens);
            self.facts.macros.push(MacroEntry {
                file: self.file.clone(),
                kind: MacroEntryKind::MacroInvocation,
                name,
                line,
                expansion_unverified: true,
                args_count: Some(args_count),
                arg_idents: Some(arg_idents.into_iter().take(64).collect()),
                brace_depth: Some(self.brace_depth),
            });
        }
        scan_macro_body_tokens(
            &mut self.facts,
            &self.file,
            self.is_example,
            &sm.mac.tokens,
            self.brace_depth,
            true,
        );
    }

    fn visit_pat_tuple_struct(&mut self, ts: &'ast syn::PatTupleStruct) {
        self.maybe_record_type_usage(&ts.path);
        for elem in &ts.elems {
            self.visit_pat(elem);
        }
    }
}
