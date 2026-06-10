use crate::*;

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
    let file: syn::File = syn::parse_str(src)?;
    let mut facts = FileFacts::default();
    walk_items(&file.items, "", rel_path, &mut facts);
    Ok(facts)
}

fn walk_items(
    items: &[syn::Item],
    container_path: &str,
    file: &str,
    facts: &mut FileFacts,
) {
    for it in items {
        // Parity with the items walker's process_item_attrs: inline
        // `#[cfg(test)]` items (incl. whole `mod tests { ... }`
        // blocks) feed no usage signals. Without this the usages
        // scanner ingested test-mod fn sigs / fields / method refs.
        let attrs: Option<&[syn::Attribute]> = match it {
            syn::Item::Fn(f) => Some(&f.attrs),
            syn::Item::Impl(i) => Some(&i.attrs),
            syn::Item::Trait(t) => Some(&t.attrs),
            syn::Item::Struct(s) => Some(&s.attrs),
            syn::Item::Enum(e) => Some(&e.attrs),
            syn::Item::Union(u) => Some(&u.attrs),
            syn::Item::Type(ta) => Some(&ta.attrs),
            syn::Item::Mod(m) => Some(&m.attrs),
            _ => None,
        };
        if let Some(a) = attrs {
            if has_cfg_test_attr(a) {
                continue;
            }
        }
        match it {
            syn::Item::Fn(f) => walk_fn(f, container_path, file, facts),
            syn::Item::Impl(i) => walk_impl(i, container_path, file, facts),
            syn::Item::Trait(t) => walk_trait(t, container_path, file, facts),
            syn::Item::Struct(s) => walk_struct(s, container_path, file, facts),
            syn::Item::Enum(e) => walk_enum(e, container_path, file, facts),
            syn::Item::Union(u) => walk_union(u, container_path, file, facts),
            syn::Item::Type(ta) => walk_type_alias(ta, container_path, file, facts),
            syn::Item::Mod(m) => walk_mod(m, container_path, file, facts),
            _ => {}
        }
    }
}

fn walk_mod(
    m: &syn::ItemMod,
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
    f: &syn::ItemFn,
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
    let body_container = qualify(container_path, &fn_name);
    walk_fn_body(&f.block, &body_container, file, facts);
}

fn walk_impl(
    i: &syn::ItemImpl,
    container_path: &str,
    file: &str,
    facts: &mut FileFacts,
) {
    let target = type_string(&i.self_ty);
    let nested = qualify(container_path, &target);
    for item in &i.items {
        if let syn::ImplItem::Fn(f) = item {
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
            let body_container = qualify(&nested, &fn_name);
            walk_fn_body(&f.block, &body_container, file, facts);
        }
    }
}

fn walk_trait(
    t: &syn::ItemTrait,
    container_path: &str,
    file: &str,
    facts: &mut FileFacts,
) {
    let trait_name = t.ident.to_string();
    let nested = qualify(container_path, &trait_name);
    let trait_vis = visibility_string(&t.vis);
    for item in &t.items {
        if let syn::TraitItem::Fn(f) = item {
            let fn_name = f.sig.ident.to_string();
            collect_fn_sig(
                &f.sig,
                &fn_name,
                &nested,
                &trait_vis,
                file,
                facts,
            );
            if let Some(default_body) = &f.default {
                let body_container = qualify(&nested, &fn_name);
                walk_fn_body(default_body, &body_container, file, facts);
            }
        }
    }
}

/// What: walk a fn body block, recursively visiting all expressions,
/// and emit a MethodRefUsage for each multi-segment Expr::Path that
/// appears in argument position of Call or MethodCall.
///
/// Why: closes the iced Update gap (0.0.28 Phase 0 validated that
/// canonical iced examples use `Type::method` multi-segment refs).
/// Recurses into closure bodies, blocks, if/match/loop branches,
/// let initializers, etc. - argument-position Path detection happens
/// once per walk, then every sub-expression is recursed for nested
/// patterns.
///
/// Where: called from walk_fn after collect_fn_sig + from walk_impl
/// for each ImplItem::Fn + from walk_trait for each TraitItem::Fn
/// with a default body.
fn walk_fn_body(
    block: &syn::Block,
    container_path: &str,
    file: &str,
    facts: &mut FileFacts,
) {
    for stmt in &block.stmts {
        walk_stmt(stmt, container_path, file, facts);
    }
}

fn walk_stmt(
    stmt: &syn::Stmt,
    container_path: &str,
    file: &str,
    facts: &mut FileFacts,
) {
    match stmt {
        syn::Stmt::Local(local) => walk_local(local, container_path, file, facts),
        syn::Stmt::Expr(expr, _) => walk_expr(expr, container_path, file, facts),
        syn::Stmt::Item(_) => {}
        syn::Stmt::Macro(_) => {}
    }
}

fn walk_local(
    local: &syn::Local,
    container_path: &str,
    file: &str,
    facts: &mut FileFacts,
) {
    if let Some(init) = &local.init {
        walk_expr(&init.expr, container_path, file, facts);
        if let Some((_, diverge)) = &init.diverge {
            walk_expr(diverge, container_path, file, facts);
        }
    }
}

fn walk_expr(
    expr: &syn::Expr,
    container_path: &str,
    file: &str,
    facts: &mut FileFacts,
) {
    match expr {
        syn::Expr::Call(call) => walk_call(call, container_path, file, facts),
        syn::Expr::MethodCall(mc) => walk_method_call(mc, container_path, file, facts),
        syn::Expr::Closure(cl) => walk_expr(&cl.body, container_path, file, facts),
        syn::Expr::Block(b) => walk_fn_body(&b.block, container_path, file, facts),
        syn::Expr::If(i) => {
            walk_expr(&i.cond, container_path, file, facts);
            walk_fn_body(&i.then_branch, container_path, file, facts);
            if let Some((_, else_branch)) = &i.else_branch {
                walk_expr(else_branch, container_path, file, facts);
            }
        }
        syn::Expr::Match(m) => {
            walk_expr(&m.expr, container_path, file, facts);
            for arm in &m.arms {
                if let Some((_, guard)) = &arm.guard {
                    walk_expr(guard, container_path, file, facts);
                }
                walk_expr(&arm.body, container_path, file, facts);
            }
        }
        syn::Expr::Loop(l) => walk_fn_body(&l.body, container_path, file, facts),
        syn::Expr::While(w) => {
            walk_expr(&w.cond, container_path, file, facts);
            walk_fn_body(&w.body, container_path, file, facts);
        }
        syn::Expr::ForLoop(f) => {
            walk_expr(&f.expr, container_path, file, facts);
            walk_fn_body(&f.body, container_path, file, facts);
        }
        syn::Expr::Return(r) => {
            if let Some(e) = &r.expr {
                walk_expr(e, container_path, file, facts);
            }
        }
        syn::Expr::Tuple(t) => {
            for e in &t.elems {
                walk_expr(e, container_path, file, facts);
            }
        }
        syn::Expr::Array(a) => {
            for e in &a.elems {
                walk_expr(e, container_path, file, facts);
            }
        }
        syn::Expr::Binary(b) => {
            walk_expr(&b.left, container_path, file, facts);
            walk_expr(&b.right, container_path, file, facts);
        }
        syn::Expr::Unary(u) => walk_expr(&u.expr, container_path, file, facts),
        syn::Expr::Reference(r) => walk_expr(&r.expr, container_path, file, facts),
        syn::Expr::Paren(p) => walk_expr(&p.expr, container_path, file, facts),
        syn::Expr::Group(g) => walk_expr(&g.expr, container_path, file, facts),
        syn::Expr::Cast(c) => walk_expr(&c.expr, container_path, file, facts),
        syn::Expr::Field(f) => walk_expr(&f.base, container_path, file, facts),
        syn::Expr::Index(i) => {
            walk_expr(&i.expr, container_path, file, facts);
            walk_expr(&i.index, container_path, file, facts);
        }
        syn::Expr::Range(r) => {
            if let Some(s) = &r.start {
                walk_expr(s, container_path, file, facts);
            }
            if let Some(e) = &r.end {
                walk_expr(e, container_path, file, facts);
            }
        }
        syn::Expr::Try(t) => walk_expr(&t.expr, container_path, file, facts),
        syn::Expr::Await(a) => walk_expr(&a.base, container_path, file, facts),
        syn::Expr::Assign(a) => {
            walk_expr(&a.left, container_path, file, facts);
            walk_expr(&a.right, container_path, file, facts);
        }
        syn::Expr::Let(l) => walk_expr(&l.expr, container_path, file, facts),
        syn::Expr::Async(a) => walk_fn_body(&a.block, container_path, file, facts),
        syn::Expr::Unsafe(u) => walk_fn_body(&u.block, container_path, file, facts),
        syn::Expr::TryBlock(t) => walk_fn_body(&t.block, container_path, file, facts),
        syn::Expr::Struct(s) => {
            for fv in &s.fields {
                walk_expr(&fv.expr, container_path, file, facts);
            }
            if let Some(rest) = &s.rest {
                walk_expr(rest, container_path, file, facts);
            }
        }
        syn::Expr::Repeat(r) => {
            walk_expr(&r.expr, container_path, file, facts);
            walk_expr(&r.len, container_path, file, facts);
        }
        _ => {}
    }
}

fn walk_call(
    call: &syn::ExprCall,
    container_path: &str,
    file: &str,
    facts: &mut FileFacts,
) {
    if let syn::Expr::Path(p) = &*call.func {
        record_call_head(&p.path, container_path, file, facts);
    }
    walk_expr(&call.func, container_path, file, facts);
    for arg in &call.args {
        emit_method_ref_if_path(arg, container_path, file, facts);
        walk_expr(arg, container_path, file, facts);
    }
}

/// What: record a function call head (lowercase-initial callee) as an
/// FnCallUsage, and surface any turbofish type arguments on the path
/// as CallTurbofish fn-sig idents.
///
/// Why: standalone fns had no capture channel (the utilities FreeFn
/// gap); turbofish-only type usage (eval_block::<WithoutDebug>) was
/// invisible to every stream. Uppercase heads are tuple-struct /
/// variant constructors already covered by the items walker.
///
/// Where: called from `walk_call` for path-headed calls.
fn record_call_head(
    path: &syn::Path,
    container_path: &str,
    file: &str,
    facts: &mut FileFacts,
) {
    let segs: Vec<&syn::PathSegment> = path.segments.iter().collect();
    let last = match segs.last() {
        Some(s) => *s,
        None => return,
    };
    let name = last.ident.to_string();
    let line = last.ident.span().start().line;
    let lower_initial = name
        .chars()
        .next()
        .map(|c| c.is_lowercase() || c == '_')
        .unwrap_or(false);
    // An uppercase-initial penultimate segment means an ASSOCIATED
    // fn (Value::string(..)) - that is the items walker's type_usage
    // domain, not a free fn.
    let assoc_fn = segs
        .len()
        .checked_sub(2)
        .and_then(|i| segs.get(i))
        .map(|s| {
            s.ident
                .to_string()
                .chars()
                .next()
                .map(|c| c.is_uppercase())
                .unwrap_or(false)
        })
        .unwrap_or(false);
    if lower_initial && !assoc_fn {
        let qualifier = if segs.len() >= 2 {
            Some(segs[0].ident.to_string())
        } else {
            None
        };
        // Full-path calls keep the segment before the callee so the
        // demand side can look up `<module>::<fn>` pair picks; the
        // two-segment form's outer is already the qualifier.
        let parent = if segs.len() >= 3 {
            Some(segs[segs.len() - 2].ident.to_string())
        } else {
            None
        };
        facts.fn_call_usages.push(FnCallUsage {
            file: file.to_string(),
            name: name.clone(),
            line,
            qualifier,
            parent,
        });
    }
    for seg in &segs {
        if let syn::PathArguments::AngleBracketed(ab) = &seg.arguments {
            for a in &ab.args {
                if let syn::GenericArgument::Type(t) = a {
                    collect_idents(t, |ident, qualifier| {
                        facts.fn_sig_usages.push(FnSigUsage {
                            file: file.to_string(),
                            fn_name: name.clone(),
                            container: container_path.to_string(),
                            ident,
                            position: FnPosition::CallTurbofish,
                            line,
                            fn_visibility: String::new(),
                            qualifier,
                        });
                    });
                }
            }
        }
    }
}

fn walk_method_call(
    mc: &syn::ExprMethodCall,
    container_path: &str,
    file: &str,
    facts: &mut FileFacts,
) {
    walk_expr(&mc.receiver, container_path, file, facts);
    for arg in &mc.args {
        emit_method_ref_if_path(arg, container_path, file, facts);
        walk_expr(arg, container_path, file, facts);
    }
    // Method-call turbofish (`iter.collect::<Outer<T>>()`) carries
    // type arguments the signature walk cannot see.
    if let Some(ab) = &mc.turbofish {
        let line = mc.method.span().start().line;
        let callee = mc.method.to_string();
        for a in &ab.args {
            if let syn::GenericArgument::Type(t) = a {
                collect_idents(t, |ident, qualifier| {
                    facts.fn_sig_usages.push(FnSigUsage {
                        file: file.to_string(),
                        fn_name: callee.clone(),
                        container: container_path.to_string(),
                        ident,
                        position: FnPosition::CallTurbofish,
                        line,
                        fn_visibility: String::new(),
                        qualifier,
                    });
                });
            }
        }
    }
}

fn emit_method_ref_if_path(
    arg: &syn::Expr,
    container_path: &str,
    file: &str,
    facts: &mut FileFacts,
) {
    let path = match arg {
        syn::Expr::Path(p) => &p.path,
        _ => return,
    };
    let segs: Vec<String> = path
        .segments
        .iter()
        .map(|s| s.ident.to_string())
        .collect();
    if segs.len() < 2 {
        return;
    }
    let inner = segs.last().cloned().unwrap_or_default();
    let outer = segs[segs.len() - 2].clone();
    if inner.is_empty() || outer.is_empty() {
        return;
    }
    // Keep the explicit root of longer paths so resolution sees the
    // language-semantic origin (git2::Status::INDEX_NEW -> "git2").
    let qualifier = if segs.len() >= 3 {
        Some(segs[0].clone())
    } else {
        None
    };
    let line = arg.span().start().line;
    facts.method_ref_usages.push(MethodRefUsage {
        file: file.to_string(),
        container: container_path.to_string(),
        outer,
        inner,
        line,
        qualifier,
    });
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
        if let syn::FnArg::Typed(pt) = input {
            collect_idents(&pt.ty, |ident, qualifier| {
                facts.fn_sig_usages.push(FnSigUsage {
                    file: file.to_string(),
                    fn_name: fn_name.to_string(),
                    container: container_path.to_string(),
                    ident,
                    position: FnPosition::Param,
                    line,
                    fn_visibility: fn_vis.to_string(),
                    qualifier,
                });
            });
        }
    }
    if let syn::ReturnType::Type(_, ty) = &sig.output {
        collect_idents(ty, |ident, qualifier| {
            facts.fn_sig_usages.push(FnSigUsage {
                file: file.to_string(),
                fn_name: fn_name.to_string(),
                container: container_path.to_string(),
                ident,
                position: FnPosition::Return,
                line,
                fn_visibility: fn_vis.to_string(),
                qualifier,
            });
        });
    }
    for param in &sig.generics.params {
        if let syn::GenericParam::Type(tp) = param {
            for bound in &tp.bounds {
                if let syn::TypeParamBound::Trait(tb) = bound {
                    let mut emit = |ident: String, qualifier: Option<String>| {
                        facts.fn_sig_usages.push(FnSigUsage {
                            file: file.to_string(),
                            fn_name: fn_name.to_string(),
                            container: container_path.to_string(),
                            ident,
                            position: FnPosition::GenericBound,
                            line,
                            fn_visibility: fn_vis.to_string(),
                            qualifier,
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
    wc: &syn::WhereClause,
    fn_name: &str,
    container_path: &str,
    fn_vis: &str,
    file: &str,
    facts: &mut FileFacts,
) {
    for pred in &wc.predicates {
        if let syn::WherePredicate::Type(pt) = pred {
            let line = pt.bounded_ty.span().start().line;
            collect_idents(&pt.bounded_ty, |ident, qualifier| {
                facts.fn_sig_usages.push(FnSigUsage {
                    file: file.to_string(),
                    fn_name: fn_name.to_string(),
                    container: container_path.to_string(),
                    ident,
                    position: FnPosition::WhereClause,
                    line,
                    fn_visibility: fn_vis.to_string(),
                    qualifier,
                });
            });
            for bound in &pt.bounds {
                if let syn::TypeParamBound::Trait(tb) = bound {
                    let mut emit = |ident: String, qualifier: Option<String>| {
                        facts.fn_sig_usages.push(FnSigUsage {
                            file: file.to_string(),
                            fn_name: fn_name.to_string(),
                            container: container_path.to_string(),
                            ident,
                            position: FnPosition::WhereClause,
                            line,
                            fn_visibility: fn_vis.to_string(),
                            qualifier,
                        });
                    };
                    collect_idents_in_trait_bound(tb, &mut emit);
                }
            }
        }
    }
}

fn walk_struct(
    s: &syn::ItemStruct,
    container_path: &str,
    file: &str,
    facts: &mut FileFacts,
) {
    let name = s.ident.to_string();
    let nested = qualify(container_path, &name);
    let container_vis = visibility_string(&s.vis);
    match &s.fields {
        syn::Fields::Named(fields) => {
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
        syn::Fields::Unnamed(fields) => {
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
        syn::Fields::Unit => {}
    }
}

fn walk_enum(
    e: &syn::ItemEnum,
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
    v: &syn::Variant,
    container_path: &str,
    container_vis: &str,
    file: &str,
    facts: &mut FileFacts,
) {
    let variant_name = v.ident.to_string();
    let nested = qualify(container_path, &variant_name);
    match &v.fields {
        syn::Fields::Named(fields) => {
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
        syn::Fields::Unnamed(fields) => {
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
        syn::Fields::Unit => {}
    }
}

fn walk_union(
    u: &syn::ItemUnion,
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
    ta: &syn::ItemType,
    container_path: &str,
    file: &str,
    facts: &mut FileFacts,
) {
    let alias_name = ta.ident.to_string();
    let qualified = qualify(container_path, &alias_name);
    let alias_vis = visibility_string(&ta.vis);
    let line = ta.span().start().line;
    collect_idents(&ta.ty, |ident, qualifier| {
        facts.type_alias_usages.push(TypeAliasUsage {
            file: file.to_string(),
            alias_name: qualified.clone(),
            ident,
            line,
            alias_visibility: alias_vis.clone(),
            qualifier,
        });
    });
}

fn emit_field(
    f: &syn::Field,
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
    collect_idents(&f.ty, |ident, qualifier| {
        facts.field_usages.push(FieldUsage {
            file: file.to_string(),
            container: container_path.to_string(),
            field_name: field_name.clone(),
            ident,
            position,
            line,
            container_visibility: container_vis.to_string(),
            field_visibility: field_vis.clone(),
            qualifier,
        });
    });
}

fn emit_field_unnamed(
    f: &syn::Field,
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
    collect_idents(&f.ty, |ident, qualifier| {
        facts.field_usages.push(FieldUsage {
            file: file.to_string(),
            container: container_path.to_string(),
            field_name: field_name.clone(),
            ident,
            position,
            line,
            container_visibility: container_vis.to_string(),
            field_visibility: field_vis.clone(),
            qualifier,
        });
    });
}

/// What: the lowercase-initial ROOT segment of a multi-segment path -
/// the module / crate qualifier (`std::io::Error` -> "std",
/// `git2::Status` -> "git2"). `None` for single-segment or type-led
/// (uppercase-initial) paths.
///
/// Why: a written qualifier is explicit language semantics; recording
/// it lets the characterize resolution gate route the site to its
/// real origin instead of the unresolved crate-local fallback.
fn path_qualifier(path: &syn::Path) -> Option<String> {
    if path.segments.len() < 2 {
        return None;
    }
    let first = path.segments.first()?.ident.to_string();
    let lower_initial = first
        .chars()
        .next()
        .map(|c| c.is_lowercase() || c == '_')
        .unwrap_or(false);
    if lower_initial { Some(first) } else { None }
}

fn collect_idents<F: FnMut(String, Option<String>)>(
    ty: &syn::Type,
    mut emit: F,
) {
    collect_idents_inner(ty, &mut emit);
}

fn collect_idents_inner(
    ty: &syn::Type,
    emit: &mut dyn FnMut(String, Option<String>),
) {
    match ty {
        syn::Type::Path(tp) => collect_idents_in_path(tp, emit),
        syn::Type::Reference(syn::TypeReference { elem, .. }) => collect_idents_inner(elem, emit),
        syn::Type::Slice(syn::TypeSlice { elem, .. }) => collect_idents_inner(elem, emit),
        syn::Type::Array(syn::TypeArray { elem, .. }) => collect_idents_inner(elem, emit),
        syn::Type::Tuple(syn::TypeTuple { elems, .. }) => {
            for e in elems {
                collect_idents_inner(e, emit);
            }
        }
        syn::Type::Paren(p) => collect_idents_inner(&p.elem, emit),
        syn::Type::Group(g) => collect_idents_inner(&g.elem, emit),
        syn::Type::TraitObject(syn::TypeTraitObject { bounds, .. }) => {
            for b in bounds {
                if let syn::TypeParamBound::Trait(tb) = b {
                    collect_idents_in_trait_bound(tb, emit);
                }
            }
        }
        syn::Type::ImplTrait(syn::TypeImplTrait { bounds, .. }) => {
            for b in bounds {
                if let syn::TypeParamBound::Trait(tb) = b {
                    collect_idents_in_trait_bound(tb, emit);
                }
            }
        }
        syn::Type::BareFn(syn::TypeBareFn { inputs, output, .. }) => {
            for i in inputs {
                collect_idents_inner(&i.ty, emit);
            }
            if let syn::ReturnType::Type(_, ret) = output {
                collect_idents_inner(ret, emit);
            }
        }
        syn::Type::Ptr(p) => collect_idents_inner(&p.elem, emit),
        _ => {}
    }
}

fn collect_idents_in_path(
    tp: &syn::TypePath,
    emit: &mut dyn FnMut(String, Option<String>),
) {
    let qualifier = path_qualifier(&tp.path);
    for seg in &tp.path.segments {
        collect_idents_in_segment(seg, qualifier.as_deref(), emit);
    }
}

fn collect_idents_in_segment(
    seg: &syn::PathSegment,
    qualifier: Option<&str>,
    emit: &mut dyn FnMut(String, Option<String>),
) {
    let name = seg.ident.to_string();
    if !name.is_empty() {
        let first = name.chars().next().unwrap();
        if first.is_uppercase() || first == '_' {
            emit(name, qualifier.map(String::from));
        }
    }
    // Generic arguments are independent type contexts: each nested
    // path computes its own qualifier (Vec<git2::Status> gates Status
    // on "git2", not on the outer path's root).
    if let syn::PathArguments::AngleBracketed(syn::AngleBracketedGenericArguments { args, .. }) = &seg.arguments {
        for a in args {
            match a {
                syn::GenericArgument::Type(t) => collect_idents_inner(t, emit),
                syn::GenericArgument::AssocType(at) => collect_idents_inner(&at.ty, emit),
                _ => {}
            }
        }
    }
}

fn collect_idents_in_trait_bound(
    tb: &syn::TraitBound,
    emit: &mut dyn FnMut(String, Option<String>),
) {
    let qualifier = path_qualifier(&tb.path);
    for seg in &tb.path.segments {
        collect_idents_in_segment(seg, qualifier.as_deref(), emit);
    }
}

fn qualify(container: &str, child: &str) -> String {
    if container.is_empty() {
        child.to_string()
    } else {
        format!("{container}::{child}")
    }
}

fn type_string(ty: &syn::Type) -> String {
    if let syn::Type::Path(tp) = ty {
        if let Some(last) = tp.path.segments.last() {
            return last.ident.to_string();
        }
    }
    String::new()
}

