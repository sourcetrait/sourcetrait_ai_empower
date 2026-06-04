use crate::*;
use ext_syn::*;

/// What: parse a single .rs file's source text via syn, walk its AST,
/// and collect type-identifier occurrences in fn signatures, struct
/// fields, and type aliases. Returns FileFacts with file-relative
/// data (the caller attaches the file path before aggregating).
///
/// Why: this is the AST core of the supplemental signal. Each entry
/// has enough context (container name, position, visibility) for
/// characterize.py to filter to workspace-defined-pub items only.
///
/// Where: called from walk_workspace() once per .rs file.
pub(crate) fn scan_file(
    src: &str,
    rel_path: &str,
) -> std::result::Result<FileFacts, syn::Error> {
    let file: RsFile = syn::parse_str(src)?;
    let mut facts = FileFacts::default();
    walk_items(&file.items, "", rel_path, &mut facts);
    Ok(facts)
}

fn walk_items(
    items: &[Item],
    container_path: &str,
    file: &str,
    facts: &mut FileFacts,
) {
    for it in items {
        match it {
            Item::Fn(f) => walk_fn(f, container_path, file, facts),
            Item::Impl(i) => walk_impl(i, container_path, file, facts),
            Item::Trait(t) => walk_trait(t, container_path, file, facts),
            Item::Struct(s) => walk_struct(s, container_path, file, facts),
            Item::Enum(e) => walk_enum(e, container_path, file, facts),
            Item::Union(u) => walk_union(u, container_path, file, facts),
            Item::Type(ta) => walk_type_alias(ta, container_path, file, facts),
            Item::Mod(m) => walk_mod(m, container_path, file, facts),
            _ => {}
        }
    }
}

fn walk_mod(
    m: &ItemMod,
    container_path: &str,
    file: &str,
    facts: &mut FileFacts,
) {
    let nested = qualify(container_path, &m.ident.to_string());
    if let Some((_, inner)) = &m.content {
        walk_items(inner, &nested, file, facts);
    }
}

fn walk_fn(
    f: &ItemFn,
    container_path: &str,
    file: &str,
    facts: &mut FileFacts,
) {
    let fn_name = f.sig.ident.to_string();
    let vis = visibility_string(&f.vis);
    collect_fn_sig(
        &f.sig,
        &fn_name,
        container_path,
        &vis,
        file,
        facts,
    );
}

fn walk_impl(
    i: &ItemImpl,
    container_path: &str,
    file: &str,
    facts: &mut FileFacts,
) {
    let target = type_string(&i.self_ty);
    let nested = qualify(container_path, &target);
    for item in &i.items {
        if let ImplItem::Fn(f) = item {
            let fn_name = f.sig.ident.to_string();
            let vis = visibility_string(&f.vis);
            collect_fn_sig(
                &f.sig,
                &fn_name,
                &nested,
                &vis,
                file,
                facts,
            );
        }
    }
}

fn walk_trait(
    t: &ItemTrait,
    container_path: &str,
    file: &str,
    facts: &mut FileFacts,
) {
    let trait_name = t.ident.to_string();
    let nested = qualify(container_path, &trait_name);
    let trait_vis = visibility_string(&t.vis);
    for item in &t.items {
        if let TraitItem::Fn(f) = item {
            let fn_name = f.sig.ident.to_string();
            collect_fn_sig(
                &f.sig,
                &fn_name,
                &nested,
                &trait_vis,
                file,
                facts,
            );
        }
    }
}

fn collect_fn_sig(
    sig: &syn::Signature,
    fn_name: &str,
    container_path: &str,
    fn_vis: &str,
    file: &str,
    facts: &mut FileFacts,
) {
    let line = sig.span().start().line;
    for input in &sig.inputs {
        if let FnArg::Typed(pt) = input {
            collect_idents(&pt.ty, |ident| {
                facts.fn_sig_usages.push(FnSigUsage {
                    file: file.to_string(),
                    fn_name: fn_name.to_string(),
                    container: container_path.to_string(),
                    ident,
                    position: FnPosition::Param,
                    line,
                    fn_visibility: fn_vis.to_string(),
                });
            });
        }
    }
    if let ReturnType::Type(_, ty) = &sig.output {
        collect_idents(ty, |ident| {
            facts.fn_sig_usages.push(FnSigUsage {
                file: file.to_string(),
                fn_name: fn_name.to_string(),
                container: container_path.to_string(),
                ident,
                position: FnPosition::Return,
                line,
                fn_visibility: fn_vis.to_string(),
            });
        });
    }
    for param in &sig.generics.params {
        if let GenericParam::Type(tp) = param {
            for bound in &tp.bounds {
                if let TypeParamBound::Trait(tb) = bound {
                    let mut emit = |ident: String| {
                        facts.fn_sig_usages.push(FnSigUsage {
                            file: file.to_string(),
                            fn_name: fn_name.to_string(),
                            container: container_path.to_string(),
                            ident,
                            position: FnPosition::GenericBound,
                            line,
                            fn_visibility: fn_vis.to_string(),
                        });
                    };
                    collect_idents_in_trait_bound(tb, &mut emit);
                }
            }
        }
    }
    if let Some(where_clause) = &sig.generics.where_clause {
        collect_where_clause(
            where_clause,
            fn_name,
            container_path,
            fn_vis,
            file,
            facts,
        );
    }
}

fn collect_where_clause(
    wc: &WhereClause,
    fn_name: &str,
    container_path: &str,
    fn_vis: &str,
    file: &str,
    facts: &mut FileFacts,
) {
    for pred in &wc.predicates {
        if let WherePredicate::Type(pt) = pred {
            let line = pt.bounded_ty.span().start().line;
            collect_idents(&pt.bounded_ty, |ident| {
                facts.fn_sig_usages.push(FnSigUsage {
                    file: file.to_string(),
                    fn_name: fn_name.to_string(),
                    container: container_path.to_string(),
                    ident,
                    position: FnPosition::WhereClause,
                    line,
                    fn_visibility: fn_vis.to_string(),
                });
            });
            for bound in &pt.bounds {
                if let TypeParamBound::Trait(tb) = bound {
                    let mut emit = |ident: String| {
                        facts.fn_sig_usages.push(FnSigUsage {
                            file: file.to_string(),
                            fn_name: fn_name.to_string(),
                            container: container_path.to_string(),
                            ident,
                            position: FnPosition::WhereClause,
                            line,
                            fn_visibility: fn_vis.to_string(),
                        });
                    };
                    collect_idents_in_trait_bound(tb, &mut emit);
                }
            }
        }
    }
}

fn walk_struct(
    s: &ItemStruct,
    container_path: &str,
    file: &str,
    facts: &mut FileFacts,
) {
    let name = s.ident.to_string();
    let nested = qualify(container_path, &name);
    let container_vis = visibility_string(&s.vis);
    match &s.fields {
        Fields::Named(fields) => {
            for f in &fields.named {
                emit_field(
                    f,
                    &nested,
                    &container_vis,
                    FieldPosition::StructField,
                    file,
                    facts,
                );
            }
        }
        Fields::Unnamed(fields) => {
            for (i, f) in fields.unnamed.iter().enumerate() {
                emit_field_unnamed(
                    f,
                    &nested,
                    &container_vis,
                    FieldPosition::TupleStructField,
                    i,
                    file,
                    facts,
                );
            }
        }
        Fields::Unit => {}
    }
}

fn walk_enum(
    e: &ItemEnum,
    container_path: &str,
    file: &str,
    facts: &mut FileFacts,
) {
    let name = e.ident.to_string();
    let nested = qualify(container_path, &name);
    let container_vis = visibility_string(&e.vis);
    for v in &e.variants {
        walk_enum_variant(v, &nested, &container_vis, file, facts);
    }
}

fn walk_enum_variant(
    v: &Variant,
    container_path: &str,
    container_vis: &str,
    file: &str,
    facts: &mut FileFacts,
) {
    let variant_name = v.ident.to_string();
    let nested = qualify(container_path, &variant_name);
    match &v.fields {
        Fields::Named(fields) => {
            for f in &fields.named {
                emit_field(
                    f,
                    &nested,
                    container_vis,
                    FieldPosition::EnumVariantField,
                    file,
                    facts,
                );
            }
        }
        Fields::Unnamed(fields) => {
            for (i, f) in fields.unnamed.iter().enumerate() {
                emit_field_unnamed(
                    f,
                    &nested,
                    container_vis,
                    FieldPosition::EnumVariantTupleField,
                    i,
                    file,
                    facts,
                );
            }
        }
        Fields::Unit => {}
    }
}

fn walk_union(
    u: &ItemUnion,
    container_path: &str,
    file: &str,
    facts: &mut FileFacts,
) {
    let name = u.ident.to_string();
    let nested = qualify(container_path, &name);
    let container_vis = visibility_string(&u.vis);
    for f in &u.fields.named {
        emit_field(
            f,
            &nested,
            &container_vis,
            FieldPosition::UnionField,
            file,
            facts,
        );
    }
}

fn walk_type_alias(
    ta: &ItemType,
    container_path: &str,
    file: &str,
    facts: &mut FileFacts,
) {
    let alias_name = ta.ident.to_string();
    let qualified = qualify(container_path, &alias_name);
    let alias_vis = visibility_string(&ta.vis);
    let line = ta.span().start().line;
    collect_idents(&ta.ty, |ident| {
        facts.type_alias_usages.push(TypeAliasUsage {
            file: file.to_string(),
            alias_name: qualified.clone(),
            ident,
            line,
            alias_visibility: alias_vis.clone(),
        });
    });
}

fn emit_field(
    f: &Field,
    container_path: &str,
    container_vis: &str,
    position: FieldPosition,
    file: &str,
    facts: &mut FileFacts,
) {
    let field_name = f
        .ident
        .as_ref()
        .map(|i| i.to_string())
        .unwrap_or_default();
    let field_vis = visibility_string(&f.vis);
    let line = f.span().start().line;
    collect_idents(&f.ty, |ident| {
        facts.field_usages.push(FieldUsage {
            file: file.to_string(),
            container: container_path.to_string(),
            field_name: field_name.clone(),
            ident,
            position,
            line,
            container_visibility: container_vis.to_string(),
            field_visibility: field_vis.clone(),
        });
    });
}

fn emit_field_unnamed(
    f: &Field,
    container_path: &str,
    container_vis: &str,
    position: FieldPosition,
    index: usize,
    file: &str,
    facts: &mut FileFacts,
) {
    let field_vis = visibility_string(&f.vis);
    let line = f.span().start().line;
    let field_name = format!("_{}", index);
    collect_idents(&f.ty, |ident| {
        facts.field_usages.push(FieldUsage {
            file: file.to_string(),
            container: container_path.to_string(),
            field_name: field_name.clone(),
            ident,
            position,
            line,
            container_visibility: container_vis.to_string(),
            field_visibility: field_vis.clone(),
        });
    });
}

fn collect_idents<F: FnMut(String)>(
    ty: &Type,
    mut emit: F,
) {
    collect_idents_inner(ty, &mut emit);
}

fn collect_idents_inner(
    ty: &Type,
    emit: &mut dyn FnMut(String),
) {
    match ty {
        Type::Path(tp) => collect_idents_in_path(tp, emit),
        Type::Reference(TypeReference { elem, .. }) => collect_idents_inner(elem, emit),
        Type::Slice(TypeSlice { elem, .. }) => collect_idents_inner(elem, emit),
        Type::Array(TypeArray { elem, .. }) => collect_idents_inner(elem, emit),
        Type::Tuple(TypeTuple { elems, .. }) => {
            for e in elems {
                collect_idents_inner(e, emit);
            }
        }
        Type::Paren(p) => collect_idents_inner(&p.elem, emit),
        Type::Group(g) => collect_idents_inner(&g.elem, emit),
        Type::TraitObject(TypeTraitObject { bounds, .. }) => {
            for b in bounds {
                if let TypeParamBound::Trait(tb) = b {
                    collect_idents_in_trait_bound(tb, emit);
                }
            }
        }
        Type::ImplTrait(TypeImplTrait { bounds, .. }) => {
            for b in bounds {
                if let TypeParamBound::Trait(tb) = b {
                    collect_idents_in_trait_bound(tb, emit);
                }
            }
        }
        Type::BareFn(TypeBareFn { inputs, output, .. }) => {
            for i in inputs {
                collect_idents_inner(&i.ty, emit);
            }
            if let ReturnType::Type(_, ret) = output {
                collect_idents_inner(ret, emit);
            }
        }
        Type::Ptr(p) => collect_idents_inner(&p.elem, emit),
        _ => {}
    }
}

fn collect_idents_in_path(
    tp: &TypePath,
    emit: &mut dyn FnMut(String),
) {
    for seg in &tp.path.segments {
        collect_idents_in_segment(seg, emit);
    }
}

fn collect_idents_in_segment(
    seg: &PathSegment,
    emit: &mut dyn FnMut(String),
) {
    let name = seg.ident.to_string();
    if !name.is_empty() {
        let first = name.chars().next().unwrap();
        if first.is_uppercase() || first == '_' {
            emit(name);
        }
    }
    if let PathArguments::AngleBracketed(AngleBracketedGenericArguments { args, .. }) = &seg.arguments {
        for a in args {
            match a {
                GenericArgument::Type(t) => collect_idents_inner(t, emit),
                GenericArgument::AssocType(at) => collect_idents_inner(&at.ty, emit),
                _ => {}
            }
        }
    }
}

fn collect_idents_in_trait_bound(
    tb: &TraitBound,
    emit: &mut dyn FnMut(String),
) {
    for seg in &tb.path.segments {
        collect_idents_in_segment(seg, emit);
    }
}

fn qualify(container: &str, child: &str) -> String {
    if container.is_empty() {
        child.to_string()
    } else {
        format!("{container}::{child}")
    }
}

fn type_string(ty: &Type) -> String {
    if let Type::Path(tp) = ty {
        if let Some(last) = tp.path.segments.last() {
            return last.ident.to_string();
        }
    }
    String::new()
}

fn visibility_string(vis: &Visibility) -> String {
    match vis {
        Visibility::Public(_) => "pub".to_string(),
        Visibility::Restricted(r) => {
            let path = r.path.segments
                .iter()
                .map(|s| s.ident.to_string())
                .collect::<Vec<_>>()
                .join("::");
            format!("pub({})", path)
        }
        Visibility::Inherited => String::new(),
    }
}
